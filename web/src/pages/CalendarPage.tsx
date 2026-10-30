import { Link } from "@tanstack/react-router";
import { Skeleton } from "../components/Skeleton";
import { ConfirmButton, DateField } from "../components/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { api, type StreakDay, type Streaks, type DayKind, type PlanDay, type PlanDiff, type PlanPreview, type PlanRules, type PlanTopic, type PlanView } from "../api";
import { Header } from "../components/Header";
import { MarkChoices, useStreaks } from "../components/Streaks";

/* ---------- dates ---------- */
const DAY = 86400000;
const ts = (s: string) => Date.parse(`${s}T00:00:00Z`);
const iso = (t: number) => new Date(t).toISOString().slice(0, 10);
const add = (s: string, n: number) => iso(ts(s) + n * DAY);
const dow = (s: string) => (new Date(ts(s)).getUTCDay() + 6) % 7; // Monday = 0
const fmt = (d: string, o: Intl.DateTimeFormatOptions) => new Date(ts(d)).toLocaleDateString("en-GB", { timeZone: "UTC", ...o });
const short = (d: string) => fmt(d, { day: "numeric", month: "short" });
const long = (d: string) => fmt(d, { weekday: "short", day: "numeric", month: "short" });
const full = (d: string) => fmt(d, { weekday: "short", day: "numeric", month: "short", year: "numeric" });
const days = (a: string, b: string) => Math.round((ts(b) - ts(a)) / DAY);

/** A topic's colour, from its track code (D7 is the seventh). */
const hue = (code: string) => `oklch(0.74 0.13 ${(Number.parseInt(code.slice(1), 10) * 137) % 360})`;
const GLYPH = { good: "✓", easy: "✓", hard: "½", again: "!" } as const;
const KIND_LABEL: Record<DayKind, string> = { solve: "SOLVE", practice: "PRACTICE", break: "BREAK" };

type Overrides = Record<string, DayKind>;
interface Snapshot {
    rules: PlanRules;
    overrides: Overrides;
}
type Tab = "rules" | "day" | "history";
type View = "calendar" | "timeline";

/** The calendar edits that turn `from` into `to`, for a preview: a date to its kind, or null where it was cleared. */
function patch(from: Overrides, to: Overrides): Record<string, DayKind | null> {
    const out: Record<string, DayKind | null> = {};
    for (const d of new Set([...Object.keys(from), ...Object.keys(to)])) if (from[d] !== to[d]) out[d] = to[d] ?? null;
    return out;
}

function impact(diff: PlanPreview["diff"], finish: string | null, late: number): string[] {
    const bits: string[] = [];
    if (diff.added) bits.push(`${diff.added} problem${diff.added > 1 ? "s" : ""} added`);
    if (diff.dropped) bits.push(`${diff.dropped} left out`);
    if (diff.moved) bits.push(`${diff.moved} move`);
    if (diff.finish_days) bits.push(`finish ${diff.finish_days > 0 ? "+" : ""}${diff.finish_days} days${finish ? ` to ${fmt(finish, { day: "numeric", month: "short", year: "numeric" })}` : ""}`);
    if (diff.reviews_moved) bits.push(`${diff.reviews_moved} of your reviews re-placed`);
    if (late) bits.push(`${late} target${late > 1 ? "s" : ""} late`);
    return bits;
}

export function CalendarPage() {
    const qc = useQueryClient();
    // the month is in the address bar (#2026-11), so a refresh, Back and a shared link keep it
    const [month, setMonth] = useState(() => (/^#\d{4}-\d{2}$/.test(location.hash) ? location.hash.slice(1) : ""));
    useEffect(() => {
        if (month) window.history.replaceState(window.history.state, "", `#${month}`);
    }, [month]);
    const [view, setView] = useState<View>("calendar");
    const [tab, setTab] = useState<Tab>("rules");
    const [open, setOpen] = useState({ scope: false, order: false, targets: true });
    const [sel, setSel] = useState<string[]>([]);
    const [anchor, setAnchor] = useState<string | null>(null);
    const [undo, setUndo] = useState<Snapshot[]>([]);
    const [note, setNote] = useState<string | null>(null);
    const [hover, setHover] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [naming, setNaming] = useState<string | null>(null);
    const drag = useRef<{ a: string; last: string; moved: boolean } | null>(null);
    const suppress = useRef(false);

    // The first fetch has no `today` yet, so it asks for a window around it and learns the date from the answer.
    const first = useQuery({ queryKey: ["plan", "first"], queryFn: () => api.plan(add(new Date().toISOString().slice(0, 10), -3), add(new Date().toISOString().slice(0, 10), 1)), staleTime: Infinity });
    const today = first.data?.today;
    const m = month || today?.slice(0, 7) || "";
    const gridStart = m ? add(`${m}-01`, -dow(`${m}-01`)) : "";
    const planQ = useQuery({ queryKey: ["plan", gridStart], queryFn: () => api.plan(gridStart, add(gridStart, 41)), enabled: !!gridStart, placeholderData: (prev) => prev });
    const p = planQ.data;
    const streaks = useStreaks();
    const stByDate = new Map((streaks.data?.days ?? []).map((d) => [d.date, d]));

    const refresh = useCallback(async () => {
        await Promise.all([qc.invalidateQueries({ queryKey: ["plan"] }), qc.invalidateQueries({ queryKey: ["dsa"] }), qc.invalidateQueries({ queryKey: ["reviews"] })]);
    }, [qc]);

    /** Saves new rules and calendar edits, with what changed as the note. */
    const commit = async (rules: PlanRules, overrides: Overrides, label: string) => {
        if (!p) return;
        setBusy(true);
        setError(null);
        try {
            const r = await api.planPreview({ rules, overrides: patch(p.overrides, overrides) });
            setUndo((u) => [...u, { rules: p.rules, overrides: p.overrides }]);
            await api.savePlanState({ rules, overrides });
            const bits = impact(r.diff, r.finish, r.late);
            setNote(`${label}: ${bits.join(" · ") || "no change to the plan"}`);
            await refresh();
        } catch (e) {
            setError((e as Error).message);
        } finally {
            setBusy(false);
        }
    };

    const doUndo = async () => {
        const prev = undo[undo.length - 1];
        if (!prev) return;
        setBusy(true);
        try {
            await api.savePlanState(prev);
            setUndo((u) => u.slice(0, -1));
            setNote(null);
            await refresh();
        } catch (e) {
            setError((e as Error).message);
        } finally {
            setBusy(false);
        }
    };

    const kindOfRoutine = (d: string): DayKind => (p?.solve_days.includes(dow(d)) ? "solve" : "practice");
    /** The calendar edits that would result from making the selected days `kind`. */
    const withKind = (kind: DayKind | "routine"): Overrides => {
        const o = { ...(p?.overrides ?? {}) };
        for (const d of sel) {
            if (kind === "routine" || kind === kindOfRoutine(d)) delete o[d];
            else o[d] = kind;
        }
        return o;
    };
    const setKind = (kind: DayKind | "routine") => p && void commit(p.rules, withKind(kind), "Calendar");
    const setRules = (fn: (r: PlanRules) => PlanRules, label: string) => p && void commit(fn(structuredClone(p.rules)), p.overrides, label);

    const range = (a: string, b: string) => {
        const [x, y] = a <= b ? [a, b] : [b, a];
        const out: string[] = [];
        for (let d = x; d <= y; d = add(d, 1)) if (today && d >= today) out.push(d);
        return out;
    };
    const quick = (what: "wp" | "wb" | "we") => {
        if (!p || !today) return;
        const nextMon = add(today, 7 - dow(today));
        const ds = what === "we" ? range(add(today, Math.max(0, 5 - dow(today))), add(today, 6 - dow(today))) : range(nextMon, add(nextMon, 6));
        setSel(ds);
        setTab("day");
        const o = { ...p.overrides };
        for (const d of ds) o[d] = what === "wp" ? "practice" : "break";
        void commit(p.rules, o, what === "wp" ? "Next week as practice" : what === "wb" ? "Next week off" : "Weekend off");
    };

    // Shortcuts: S, P, B, R set the selected days; Z undoes; Esc clears.
    const keys = useRef<(e: KeyboardEvent) => void>(() => {});
    keys.current = (e) => {
        if ((e.target as HTMLElement | null)?.closest?.("input,textarea,select") || e.metaKey || e.ctrlKey || e.altKey) return;
        const k = e.key.toLowerCase();
        if (k === "escape") setSel([]);
        else if (k === "z") void doUndo();
        else if (view === "calendar" && (k === "arrowleft" || k === "arrowright")) setMonth(iso(Date.UTC(+m.slice(0, 4), +m.slice(5) - 1 + (k === "arrowright" ? 1 : -1), 1)).slice(0, 7));
        else if (view === "calendar" && k === "t" && today) { setMonth(today.slice(0, 7)); setSel([today]); setTab("day"); }
        else if (sel.length && (k === "s" || k === "p" || k === "b" || k === "r")) setKind({ s: "solve", p: "practice", b: "break", r: "routine" }[k] as DayKind | "routine");
    };
    useEffect(() => {
        const f = (e: KeyboardEvent) => keys.current(e);
        const up = () => {
            if (drag.current?.moved) {
                suppress.current = true;
                window.setTimeout(() => (suppress.current = false), 0);
            }
            drag.current = null;
        };
        window.addEventListener("keydown", f);
        window.addEventListener("mouseup", up);
        return () => {
            window.removeEventListener("keydown", f);
            window.removeEventListener("mouseup", up);
        };
    }, []);

    const pickDay = (d: string, e: React.MouseEvent) => {
        if (suppress.current) return;
        if (today && d < today) {
            setSel([d]);
            setAnchor(null);
            setTab("day");
            return;
        }
        if (e.shiftKey && anchor) setSel(range(anchor, d));
        else if (e.metaKey || e.ctrlKey) setSel((s) => (s.includes(d) ? s.filter((x) => x !== d) : [...s, d]));
        else setSel((s) => (s.length === 1 && s[0] === d ? [] : [d]));
        setAnchor(d);
        if (tab === "history") setTab("day");
    };
    const overDay = (d: string) => {
        const g = drag.current;
        if (g && d !== g.last && today && d >= today) {
            g.last = d;
            g.moved = true;
            setSel(range(g.a, d));
            setAnchor(g.a);
        }
    };

    const showImpact = async (kind: DayKind | "routine") => {
        if (!p) return;
        setHover("…");
        try {
            const r = await api.planPreview({ overrides: patch(p.overrides, withKind(kind)) });
            setHover(impact(r.diff, r.finish, r.late).join(" · ") || "No change to the plan.");
        } catch {
            setHover(null);
        }
    };

    if (!p || !today) {
        return (
            <>
                <Header area="dsa" />
                <main className="page">
                    <div className="wrap" style={{ paddingBlock: 32 }}>
                        {planQ.isError || first.isError ? <p className="rempty">Couldn't reach the API.</p> : <Skeleton lines={6} />}
                    </div>
                </main>
            </>
        );
    }

    const byDate = new Map(p.days.map((d) => [d.date, d]));
    const selDays = [...sel].sort();
    const s = p.summary;
    const spare = s.finish && s.finish_by ? days(s.finish, s.finish_by) : null;
    const dd = s.finish && s.baseline_finish ? days(s.baseline_finish, s.finish) : 0;
    const explicitTargets = p.topics.filter((t) => p.rules.targets[t.id] && t.status);
    const topicName = (code: string) => p.topics.find((t) => t.id === code)?.name ?? code;
    const planWord = p.plans.find((x) => x.id === p.active);

    return (
        <>
            <Header area="dsa" />
            <main className="page c-page">
                <div className="c-wrap" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                    <div className="c-title">
                        <div>
                            <div className="eyebrow"><Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link><span>/</span><span>CALENDAR</span></div>
                            <h1 className="c-h1">Plan</h1>
                        </div>
                        <Link className="c-link" to="/dsa/plan">Routine and pace ›</Link>
                    </div>

                    <div className="c-plans">
                        {p.plans.map((x) => (
                            <button key={x.id} className={`c-pl${x.id === p.active ? " on" : ""}`} onClick={() => x.id !== p.active && api.setActivePlan(x.id).then(refresh).then(() => setSel([]))}>
                                <b>{x.name}</b>
                                <small>{x.problems} problems · {x.finish ? short(x.finish) : "—"}{x.late ? <em> · ⚠ {x.late} late</em> : null}</small>
                            </button>
                        ))}
                        {naming === null ? (
                            <button className="c-pl add" onClick={() => setNaming("")}><b>+ Save as new plan</b><small>keeps these rules and days</small></button>
                        ) : (
                            <form className="c-pl add on" onSubmit={(e) => { e.preventDefault(); if (naming.trim()) void api.createPlan(naming.trim()).then(refresh).then(() => setNaming(null)).catch((er) => setError((er as Error).message)); }}>
                                <input autoFocus placeholder="Name, e.g. Amazon loop" value={naming} maxLength={60} onChange={(e) => setNaming(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setNaming(null)} />
                                <small>Enter to save · Esc to cancel</small>
                            </form>
                        )}
                    </div>

                    <div className="c-sum">
                        <div className="c-tile big">
                            <small>YOU FINISH</small>
                            <b>{s.finish ? full(s.finish) : "beyond the horizon"}</b>
                            <span>
                                {dd ? <em className={dd > 0 ? "up" : "dn"}>{dd > 0 ? "+" : ""}{dd} days vs the default plan</em> : null}
                                {dd && spare !== null ? " · " : ""}
                                {spare === null ? "" : spare >= 0 ? `${spare >= 14 ? `${Math.round(spare / 7)} weeks` : `${spare} days`} before ${short(s.finish_by!)}` : `${-spare} days after ${short(s.finish_by!)}`}
                            </span>
                        </div>
                        <div className="c-tile">
                            <small>STILL TO DO</small>
                            <b>{s.left}<i> problems</i></b>
                            <span>{s.main} main{s.practice ? ` · ${s.practice} practice` : ""} · Premium {s.premium ? "in" : "out"}</span>
                        </div>
                        <div className="c-tile">
                            <small>PACE</small>
                            <b>{s.per_week.toFixed(1)}<i> a week</i></b>
                            <span>{s.solve_days_28} problem days in the next four weeks</span>
                        </div>
                        <div className="c-tile wide">
                            <small>TARGETS</small>
                            {explicitTargets.length || p.rules.finish_by || s.finish_by ? (
                                <div className="c-tcs">
                                    {explicitTargets.map((t) => (
                                        <span key={t.id} className={`c-tc ${t.status!.late ? "bad" : "ok"}`} title={`${t.name} by ${short(p.rules.targets[t.id] ?? "")}`}>
                                            <i style={{ background: hue(t.id) }} />{t.name}<b>{t.status!.late ? (t.status!.late >= 9999 ? "never" : `${t.status!.late}d late`) : `${t.status!.slack}d spare`}</b>
                                        </span>
                                    ))}
                                    <span className={`c-tc ${spare !== null && spare < 0 ? "bad" : "ok"}`}><i style={{ background: "var(--dim)" }} />Everything<b>{spare === null ? "—" : spare >= 0 ? `${spare}d spare` : `${-spare}d late`}</b></span>
                                </div>
                            ) : <span>No targets yet. Add one in Rules.</span>}
                        </div>
                    </div>

                    <div className="c-top">
                        <h2>{view === "calendar" ? fmt(`${m}-01`, { month: "long", year: "numeric" }) : "Topics"}</h2>
                        {view === "calendar" && (() => {
                            let n = 0, r = 0, off = 0;
                            for (let i = 0; i < new Date(Date.UTC(+m.slice(0, 4), +m.slice(5), 0)).getUTCDate(); i++) {
                                const d = byDate.get(add(`${m}-01`, i));
                                if (!d) continue;
                                n += d.new?.length ?? d.past?.problems?.length ?? 0;
                                r += d.reviews?.length ?? d.past?.reviews ?? 0;
                                if (d.kind === "break") off++;
                            }
                            return <span className="c-msum" title="Problems and reviews planned or done this month">{n} problem{n === 1 ? "" : "s"} · {r} review{r === 1 ? "" : "s"}{off ? ` · ${off} day${off === 1 ? "" : "s"} off` : ""}</span>;
                        })()}
                        {view === "calendar" && (
                            <span className="c-nav">
                                <button className="c-nb" aria-label="Previous month" onClick={() => setMonth(iso(Date.UTC(+m.slice(0, 4), +m.slice(5) - 2, 1)).slice(0, 7))}>‹</button>
                                <button className="c-nb" aria-label="Next month" onClick={() => setMonth(iso(Date.UTC(+m.slice(0, 4), +m.slice(5), 1)).slice(0, 7))}>›</button>
                                <button className="c-tb" onClick={() => { setMonth(today.slice(0, 7)); setSel([today]); setTab("day"); }}>Today</button>
                            </span>
                        )}
                        <div className="c-views">
                            <button className={view === "calendar" ? "on" : ""} onClick={() => setView("calendar")}>Calendar</button>
                            <button className={view === "timeline" ? "on" : ""} onClick={() => setView("timeline")}>Timeline</button>
                        </div>
                        <span className="sp" />
                        <span className="c-hint"><kbd>←</kbd><kbd>→</kbd> month <kbd>T</kbd> today <kbd>Z</kbd> undo</span>
                        <button className="c-tb" disabled={!undo.length || busy} onClick={() => void doUndo()}>↶ Undo</button>
                        <button className="c-tb" disabled={!Object.keys(p.overrides).length || busy} onClick={() => void commit(p.rules, {}, "Calendar")}>Clear day edits</button>
                    </div>
                    {error && <p className="notice bad">{error}</p>}

                    <div className="c-layout">
                        <div className="c-main">
                            {view === "calendar" ? (
                                <>
                                    <Bar p={p} sel={selDays} note={tab === "rules" ? null : note} hover={hover} busy={busy} onKind={setKind} onHover={(k) => (k ? void showImpact(k) : setHover(null))} onQuick={quick} kindOfRoutine={kindOfRoutine} />
                                    <Grid p={p} byDate={byDate} st={stByDate} month={m} gridStart={gridStart} today={today} sel={sel} onPick={pickDay} onDown={(d) => { if (d >= today) drag.current = { a: d, last: d, moved: false }; }} onOver={overDay} onWeek={(ws) => { const ds = range(ws, add(ws, 6)); setSel(ds); setAnchor(ds[0] ?? null); }} />
                                    <Load p={p} byDate={byDate} month={m} today={today} />
                                </>
                            ) : (
                                <Timeline p={p} today={today} />
                            )}
                        </div>
                        <aside className="c-rail">
                            <div className="c-card">
                                <div className="c-tabs" role="tablist">
                                    {(["rules", "day", "history"] as Tab[]).map((t) => (
                                        <button key={t} role="tab" className={tab === t ? "on" : ""} onClick={() => setTab(t)}>{t === "rules" ? "Rules" : t === "day" ? (sel.length ? "Day" : "Today") : "History"}</button>
                                    ))}
                                </div>
                                {tab === "rules" && <Rules p={p} note={note} open={open} setOpen={setOpen} setRules={setRules} topicName={topicName} commit={commit} busy={busy} onDelete={planWord && planWord.id !== "default" ? () => api.deletePlan(planWord.id).then(refresh) : undefined} />}
                                {tab === "day" && <DayDetail p={p} date={selDays[0] ?? today} today={today} byDate={byDate} streaks={streaks.data} />}
                                {tab === "history" && <History p={p} />}
                            </div>
                        </aside>
                    </div>
                </div>
            </main>
        </>
    );
}

/* ---------- the month ---------- */

function Grid({ p, byDate, st: stByDate, month, gridStart, today, sel, onPick, onDown, onOver, onWeek }: { p: PlanView; byDate: Map<string, PlanDay>; st: Map<string, StreakDay>; month: string; gridStart: string; today: string; sel: string[]; onPick: (d: string, e: React.MouseEvent) => void; onDown: (d: string) => void; onOver: (d: string) => void; onWeek: (ws: string) => void }) {
    const rows: string[] = [];
    for (let w = 0; w < 6; w++) {
        const ws = add(gridStart, w * 7);
        if (w >= 4 && ws.slice(0, 7) !== month && add(ws, 6).slice(0, 7) !== month) break;
        rows.push(ws);
    }
    return (
        <div className="c-grid">
            <div className="c-row"><span />{["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"].map((x) => <div key={x} className="c-dh">{x}</div>)}</div>
            {rows.map((ws) => {
                let n = 0, r = 0;
                for (let i = 0; i < 7; i++) {
                    const d = byDate.get(add(ws, i));
                    n += d?.new?.length ?? d?.past?.problems?.length ?? 0;
                    r += d?.reviews?.length ?? d?.past?.reviews ?? 0;
                }
                const wn = Math.ceil(((ts(ws) - Date.UTC(new Date(ts(ws)).getUTCFullYear(), 0, 1)) / DAY + 4) / 7);
                return (
                    <div key={ws} className="c-row">
                        <button className="c-wk" title={`Select this week: ${n} new problem${n === 1 ? "" : "s"}, ${r} review${r === 1 ? "" : "s"}`} onClick={() => onWeek(ws)}><b>W{wn}</b><span>{n}<i className="g" /></span><span>{r}<i className="r" /></span></button>
                        {Array.from({ length: 7 }, (_, i) => {
                            const date = add(ws, i);
                            const d = byDate.get(date);
                            return d ? <Cell key={date} d={d} p={p} out={date.slice(0, 7) !== month} today={today} st={stByDate.get(date)} selected={sel.includes(date)} onPick={onPick} onDown={onDown} onOver={onOver} /> : <span key={date} />;
                        })}
                    </div>
                );
            })}
        </div>
    );
}

function Cell({ d, p, out, today, st, selected, onPick, onDown, onOver }: { st?: StreakDay; d: PlanDay; p: PlanView; out: boolean; today: string; selected: boolean; onPick: (d: string, e: React.MouseEvent) => void; onDown: (d: string) => void; onOver: (d: string) => void }) {
    const past = d.date < today;
    const cls = ["c-day", out ? "out" : "", selected ? "sel" : "", d.date === today ? "today" : "", past ? "past" : `up ${d.kind}`];
    let kind: string = KIND_LABEL[d.kind], title = "", sub: React.ReactNode = null, tag = "", topic: string | null = null;
    if (past) {
        const x = d.past!;
        if (st && (st.state === "frozen" || st.state === "holiday" || st.state === "sick")) {
            cls.push(st.state);
            kind = { frozen: "FROZEN", holiday: "HOLIDAY", sick: "SICK" }[st.state];
            title = { frozen: "Freeze used", holiday: "Holiday", sick: "Sick day" }[st.state];
        } else if (x.state === "missed") { cls.push("missed"); kind = "MISSED"; title = "Planned problem not done"; }
        else if (x.state === "break") { cls.push("break"); kind = "BREAK"; }
        else if (x.state === "solved") {
            const first = x.problems![0]!;
            const g = first.grade === "again" ? "fail" : first.grade === "hard" ? "help" : "solo";
            cls.push("done", g);
            kind = `${GLYPH[first.grade]} SOLVED`;
            title = first.problem.title + (x.problems!.length > 1 ? ` +${x.problems!.length - 1}` : "");
            topic = first.problem.topic;
            if (x.reviews) sub = `↻ ${x.reviews} done`;
        } else if (x.state === "reviews") { cls.push("done", "rev"); kind = "REVIEWS"; sub = `↻ ${x.reviews} done`; }
        else kind = "REST";
    } else {
        const nw = d.new?.[0];
        if (d.kind === "solve" && nw) { title = nw.title + (d.new!.length > 1 ? ` +${d.new!.length - 1}` : ""); topic = nw.topic; tag = `${p.topics.find((t) => t.id === nw.topic)?.name ?? nw.topic}${nw.practice ? " · practice" : ""}`; cls.push("has"); }
        else if (d.kind === "solve") title = "Nothing left in scope";
        else if (d.kind === "practice" && !d.reviews?.length) title = "Practice day";
        const rs = d.reviews?.length ?? 0;
        const doneN = (d.done?.problems.length ?? 0) + (d.done?.reviews ?? 0);
        if (doneN) cls.push("did");
        if (rs || doneN) sub = <>{doneN > 0 && <b className="dn">✓ {doneN} done</b>}{Array.from({ length: Math.min(rs, 12) }, (_, i) => <i key={i} className={i >= d.capacity ? "over" : ""} />)}{rs > 0 && <span>{rs}</span>}</>;
    }
    return (
        <button className={cls.filter(Boolean).join(" ")} style={topic ? ({ "--th": hue(topic) } as CSSProperties) : undefined} onClick={(e) => onPick(d.date, e)} onMouseDown={(e) => e.button === 0 && !e.shiftKey && onDown(d.date)} onMouseOver={() => onOver(d.date)}>
            <div className="n"><span>{Number(d.date.slice(8))}</span><span className="k">{d.edited && <i className="pin" title="Your change">✎</i>} {d.date === today ? "TODAY" : kind}</span></div>
            {title && <div className="t">{title}</div>}
            {tag && <div className="tg"><i />{tag}</div>}
            {d.flags?.length ? <div className="flag">⚑ {d.flags[0]} due</div> : null}
            {sub && <div className="rv">{sub}</div>}
        </button>
    );
}

/** Reviews per day this month against what each day can take, so a crowded day shows before you reach it. */
function Load({ p, byDate, month, today }: { p: PlanView; byDate: Map<string, PlanDay>; month: string; today: string }) {
    const n = new Date(Date.UTC(+month.slice(0, 4), +month.slice(5), 0)).getUTCDate();
    let over = 0;
    const bars = Array.from({ length: n }, (_, i) => {
        const date = add(`${month}-01`, i);
        const d = byDate.get(date);
        const past = date < today;
        const v = past ? (d?.past?.reviews ?? 0) : (d?.reviews?.length ?? 0);
        const cap = d?.capacity ?? 0;
        const bad = !past && v > Math.max(cap, 0) && d?.kind !== "break";
        if (bad) over++;
        const cls = past ? "past" : bad ? "bad" : v ? "ok" : "";
        return <i key={date} className={`${cls}${d?.kind === "break" && !past ? " brk" : ""}`} style={{ height: `${Math.min(100, (v / 12) * 100) + 4}%` }} title={`${long(date)}: ${v} review${v === 1 ? "" : "s"}${past ? " done" : ` of ${cap}`}`} />;
    });
    void p;
    return (
        <div className="c-load">
            <div className="lh"><span>REVIEWS PER DAY</span><em>{over ? <b>{over} day{over > 1 ? "s" : ""} over capacity</b> : "every day within capacity"}</em></div>
            <div className="lb">{bars}</div>
        </div>
    );
}

function Bar({ p, sel, note, hover, busy, onKind, onHover, onQuick, kindOfRoutine }: { p: PlanView; sel: string[]; note: string | null; hover: string | null; busy: boolean; onKind: (k: DayKind | "routine") => void; onHover: (k: DayKind | "routine" | null) => void; onQuick: (w: "wp" | "wb" | "we") => void; kindOfRoutine: (d: string) => DayKind }) {
    void kindOfRoutine;
    const noteEl = note ? <div className="c-note">{note}</div> : null;
    if (!sel.length) {
        return (
            <div className="c-bar">
                {noteEl}
                <div className="ln">
                    <div className="c-seg q">
                        <button disabled={busy} onClick={() => onQuick("wp")}>Next week: practice</button>
                        <button disabled={busy} onClick={() => onQuick("wb")}>Next week: break</button>
                        <button disabled={busy} onClick={() => onQuick("we")}>This weekend off</button>
                    </div>
                </div>
            </div>
        );
    }
    const kinds = new Set(sel.map((d) => p.days.find((x) => x.date === d)?.kind));
    const cur = kinds.size === 1 ? [...kinds][0] : null;
    const label = sel.length === 1 ? long(sel[0]!) : `${long(sel[0]!)} – ${long(sel[sel.length - 1]!)}`;
    const btn = (k: DayKind | "routine", g: string, name: string, key: string) => (
        <button key={k} className={cur === k ? "on" : ""} disabled={busy} onClick={() => onKind(k)} onMouseEnter={() => onHover(k)} onMouseLeave={() => onHover(null)} onFocus={() => onHover(k)} onBlur={() => onHover(null)}>
            <i>{g}</i>{name}<kbd>{key}</kbd>
        </button>
    );
    return (
        <div className="c-bar">
            {noteEl}
            <div className="ln">
                <div className="what">{sel.length} day{sel.length > 1 ? "s" : ""}<small>{label}</small></div>
                <div className="c-seg">{btn("solve", "●", "Problem day", "S")}{btn("practice", "↻", "Practice day", "P")}{btn("break", "✕", "Break", "B")}{btn("routine", "↺", "Routine", "R")}</div>
            </div>
            <div className={`impact${hover ? "" : " empty"}`}>{hover ?? <>Hover a choice to see what it moves. <kbd>Z</kbd> undoes, <kbd>Esc</kbd> clears.</>}</div>
        </div>
    );
}

/* ---------- the timeline ---------- */

function Timeline({ p, today }: { p: PlanView; today: string }) {
    const rows = p.topics.filter((t) => t.status?.start);
    const ends = rows.flatMap((t) => [t.status!.end, p.rules.targets[t.id] || t.covered ? t.status!.due : null]).filter((x): x is string => !!x).sort();
    const end = ends[ends.length - 1] ?? add(today, 60);
    const span = Math.max(30, days(today, end) + 7);
    const pct = (d: string) => Math.max(0, Math.min(100, (days(today, d) / span) * 100));
    const ticks: string[] = [];
    for (let d = iso(Date.UTC(+today.slice(0, 4), +today.slice(5, 7), 1)); d < add(today, span); d = iso(Date.UTC(+d.slice(0, 4), +d.slice(5, 7), 1))) ticks.push(d);
    if (!rows.length) return <div className="c-tl"><p className="rempty">Nothing is in scope. Loosen the rules.</p></div>;
    return (
        <div className="c-tl">
            <div className="tlh"><span /><div className="tlax">{ticks.map((d) => <em key={d} style={{ left: `${pct(d)}%` }}>{fmt(d, { month: "short" })}</em>)}</div></div>
            <div className="tlbody">
                <div className="tlgrid">{ticks.map((d) => <u key={d} style={{ left: `${pct(d)}%` }} />)}<b className="tnow" style={{ left: 0 }}><em>today</em></b></div>
                {rows.map((t) => {
                    const s = t.status!;
                    const a = pct(s.start!), b = pct(s.end ?? s.start!);
                    const hasT = !!(s.due && (p.rules.targets[t.id] || t.covered));
                    const cls = s.late ? "late" : hasT ? "ok" : "free";
                    const lateFrom = s.late && hasT && s.due ? pct(s.due) : null;
                    return (
                        <div key={t.id} className="tlr">
                            <div className="tlt"><i style={{ background: hue(t.id) }} /><div><b>{t.name}</b><small>{s.n} problems{hasT ? ` · due ${short(s.due!)}` : ""}</small></div></div>
                            <div className="tlb" title={`${t.name}: ${s.n} problems, ${short(s.start!)} to ${s.end ? short(s.end) : "—"}${hasT ? `, due ${short(s.due!)}` : ""}`}>
                                <i className={`bar ${cls}`} style={{ "--th": hue(t.id), left: `${a}%`, width: `${Math.max(1.4, b - a)}%` } as CSSProperties}><span>{s.n}</span></i>
                                {lateFrom !== null && <i className="over" style={{ left: `${lateFrom}%`, width: `${Math.max(0.8, b - lateFrom)}%` }} />}
                                {hasT && <u className="dlm" style={{ left: `${pct(s.due!)}%` }}><em>{short(s.due!)}</em></u>}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

/* ---------- the rail ---------- */

function Seg<T extends string | boolean>({ value, options, onPick }: { value: T; options: [T, string][]; onPick: (v: T) => void }) {
    return (
        <div className="c-seg s" role="group">
            {options.map(([v, l]) => <button key={String(v)} className={v === value ? "on" : ""} onClick={() => onPick(v)}>{l}</button>)}
        </div>
    );
}

function Acc({ id, title, summary, open, setOpen, children }: { id: "scope" | "order" | "targets"; title: string; summary: string; open: { scope: boolean; order: boolean; targets: boolean }; setOpen: (o: { scope: boolean; order: boolean; targets: boolean }) => void; children: React.ReactNode }) {
    return (
        <div className={`c-acc${open[id] ? " open" : ""}`}>
            <button className="ah" onClick={() => setOpen({ ...open, [id]: !open[id] })}><b>{title}</b><span>{summary}</span><i>▾</i></button>
            {open[id] && <div className="ab">{children}</div>}
        </div>
    );
}

function Rules({ p, note, open, setOpen, setRules, topicName, commit, busy, onDelete }: { p: PlanView; note: string | null; open: { scope: boolean; order: boolean; targets: boolean }; setOpen: (o: { scope: boolean; order: boolean; targets: boolean }) => void; setRules: (fn: (r: PlanRules) => PlanRules, label: string) => void; topicName: (c: string) => string; commit: (r: PlanRules, o: Overrides, label: string) => Promise<void>; busy: boolean; onDelete?: () => void }) {
    const r = p.rules;
    const premium = r.premium ?? !p.free_only;
    const order = r.topic_order ?? p.topics.map((t) => t.id);
    const rows: PlanTopic[] = order.map((id) => p.topics.find((t) => t.id === id)).filter((t): t is PlanTopic => !!t);
    const nt = Object.keys(r.targets).length;
    const finishBy = r.finish_by ?? p.summary.finish_by ?? "";
    const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
    const practiceSum = { none: "no practice", targeted: "practice for targeted topics", all: "all practice" }[r.practice];
    const orderName = { curriculum: "Curriculum", ramp: "Easy → hard", company: "Most asked", important: "Most important" }[r.order];
    const sg = p.suggestion;
    const move = (id: string, by: number) => setRules((x) => { const o = [...order]; const i = o.indexOf(id); [o[i], o[i + by]] = [o[i + by]!, o[i]!]; return { ...x, topic_order: o }; }, "Topic order");
    // dragging a row by its grip: it lands where the row it is dropped on is, the rows between shift by one
    const [dragging, setDragging] = useState<string | null>(null);
    const [over, setOver] = useState<string | null>(null);
    const drop = (to: string) => {
        const from = dragging;
        setDragging(null);
        setOver(null);
        if (!from || from === to) return;
        setRules((x) => { const o = [...order]; const at = o.indexOf(to); o.splice(o.indexOf(from), 1); o.splice(at, 0, from); return { ...x, topic_order: o }; }, "Topic order");
    };
    return (
        <>
            {note && <div className="c-note">{note}</div>}
            <Acc id="scope" title="Scope" summary={`Premium ${premium ? "in" : "out"} · ${practiceSum}${r.companies.length ? ` · ${r.companies.join(", ")}${r.recent_only ? " (recent)" : ""}` : ""}${r.difficulty.length < 3 ? ` · ${r.difficulty.join("/")}` : ""}`} open={open} setOpen={setOpen}>
                <div className="fl"><span>Premium problems</span><Seg value={premium} options={[[false, "Left out"], [true, "Included"]]} onPick={(v) => setRules((x) => ({ ...x, premium: v }), v ? "Premium included" : "Premium left out")} /></div>
                <div className="fl"><span>LeetCode practice</span><Seg value={r.practice} options={[["none", "None"], ["targeted", "Topics with a target"], ["all", "All"]]} onPick={(v) => setRules((x) => ({ ...x, practice: v }), "Practice")} /></div>
                <div className="fl"><span>Difficulty</span><div className="c-chs">{(["E", "M", "H"] as PlanDiff[]).map((d) => <button key={d} className={`c-ch${r.difficulty.includes(d) ? " on" : ""}`} onClick={() => r.difficulty.length > 1 || !r.difficulty.includes(d) ? setRules((x) => ({ ...x, difficulty: toggle(x.difficulty, d) }), "Difficulty") : undefined}>{{ E: "Easy", M: "Medium", H: "Hard" }[d]}</button>)}</div></div>
                <div className="fl"><span>Asked by</span><div className="c-chs">{p.companies.map((c) => <button key={c} className={`c-ch${r.companies.includes(c) ? " on" : ""}`} onClick={() => setRules((x) => ({ ...x, companies: toggle(x.companies, c) }), "Companies")}>{c}</button>)}</div>
                    <label className="c-chk"><input type="checkbox" checked={r.recent_only} onChange={(e) => setRules((x) => ({ ...x, recent_only: e.target.checked }), "Last six months")} /> last six months only</label></div>
            </Acc>
            <Acc id="order" title="Order" summary={orderName} open={open} setOpen={setOpen}>
                <Seg value={r.order} options={[["curriculum", "Curriculum"], ["ramp", "Easy → hard"], ["company", "Most asked"], ["important", "Most important"]]} onPick={(v) => setRules((x) => ({ ...x, order: v }), "Order")} />
            </Acc>
            <Acc id="targets" title="Targets" summary={`${nt ? `${nt} topic target${nt > 1 ? "s" : ""} · ` : ""}all by ${finishBy ? short(finishBy) : "—"}`} open={open} setOpen={setOpen}>
                <div className="fl row1"><span>Everything done by</span><DateField label="Everything done by" className="compact" value={finishBy} onChange={(v) => setRules((x) => ({ ...x, finish_by: v || null }), "Final target")} /></div>
                <div className="c-trs">
                    {rows.map((t, i) => (
                        <div key={t.id} className={`c-tr${t.status ? "" : " dim"}${dragging === t.id ? " drag" : ""}${over === t.id && dragging !== t.id ? " over" : ""}`} data-topic={t.id}
                            onDragOver={(e) => { if (dragging) { e.preventDefault(); setOver(t.id); } }}
                            onDrop={(e) => { e.preventDefault(); drop(t.id); }}>
                            <span className="mv"><span className="grip" role="img" aria-label={`Drag ${t.name} to reorder`} title="Drag to reorder" draggable
                                onDragStart={(e) => { setDragging(t.id); e.dataTransfer.effectAllowed = "move"; e.dataTransfer.setData("text/plain", t.id); const row = (e.currentTarget as HTMLElement).closest(".c-tr"); if (row) e.dataTransfer.setDragImage(row, 16, 16); }}
                                onDragEnd={() => { setDragging(null); setOver(null); }}>⠿</span><button disabled={i === 0} aria-label="Move up" onClick={() => move(t.id, -1)}>↑</button><button disabled={i === rows.length - 1} aria-label="Move down" onClick={() => move(t.id, 1)}>↓</button></span>
                            <span className="nm"><i style={{ background: hue(t.id) }} /><b title={t.name}>{t.name}</b><small>{t.status ? `${t.status.n} left` : "none in scope"}</small></span>
                            {!t.status ? <span className="st muted">—</span> : t.status.due && (t.target || t.covered) ? (t.status.late ? <span className="st bad">⚠ {t.status.late >= 9999 ? "not in time" : `${t.status.late}d late`}</span> : <span className="st ok">✓ {t.status.slack}d spare</span>) : null}
                            <span className="dt">
                                <DateField label={`${t.name} target`} className="compact" value={t.target ?? ""} onChange={(v) => setRules((x) => { const targets = { ...x.targets }; if (v) targets[t.id] = v; else delete targets[t.id]; return { ...x, targets }; }, `${t.name} target`)} />
                                {t.covered && <small>due with a later topic</small>}
                            </span>
                        </div>
                    ))}
                </div>
                {sg && (
                    <div className="c-sg">
                        <span><b>{sg.name}</b> is {sg.late >= 9999 ? "out of reach" : `${sg.late} days late`}. {sg.remedy.kind === "add_days" ? <>Make {sg.remedy.days.map(long).join(" and ")} problem day{sg.remedy.days.length > 1 ? "s" : ""} and it lands <b>{short(sg.remedy.end)}</b>.</> : <>The days you have free aren't enough.{sg.remedy.end && sg.late < 9999 ? <> At this pace it lands <b>{short(sg.remedy.end)}</b>.</> : null}</>}</span>
                        <div className="two">
                            {sg.remedy.kind === "add_days" && <button disabled={busy} onClick={() => { const o = { ...p.overrides }; for (const d of (sg.remedy as { days: string[] }).days) o[d] = "solve"; void commit(p.rules, o, "Calendar"); }}>Add the days</button>}
                            {sg.remedy.end && sg.late < 9999 && <button disabled={busy} onClick={() => setRules((x) => ({ ...x, targets: { ...x.targets, [sg.topic]: sg.remedy.end! } }), `${topicName(sg.topic)} target`)}>Move target to {short(sg.remedy.end)}</button>}
                        </div>
                    </div>
                )}
            </Acc>
            {onDelete && <ConfirmButton label="Delete this plan" confirm="Yes, delete it" note="The plan and its calendar edits go. A copy is kept." onConfirm={onDelete} />}
        </>
    );
}

function DayDetail({ p, date, today, byDate, streaks }: { p: PlanView; date: string; today: string; byDate: Map<string, PlanDay>; streaks?: Streaks }) {
    const d = byDate.get(date);
    if (!d) return <p className="rempty">That day is outside this month's view.</p>;
    const past = date < today;
    const x = d.past;
    const marked = streaks?.days.find((q) => q.date === date && (q.state === "frozen" || q.state === "holiday" || q.state === "sick"));
    const label = marked ? marked.state.toUpperCase() : past ? (x?.state === "missed" ? "MISSED" : x?.state === "break" ? "BREAK" : x?.state === "reviews" ? "REVIEW DAY" : x?.state === "rest" ? "REST" : "PROBLEM DAY") : `${{ solve: "PROBLEM DAY", practice: "PRACTICE DAY", break: "BREAK" }[d.kind]}${d.edited ? " · EDITED" : ""}`;
    return (
        <div className="c-dd">
            <div className="big">{long(date)}</div>
            <span className="chip">{label}</span>
            {streaks && date <= today && streaks.days.some((q) => q.date === date) && (
                <div className="sk-box">
                    <MarkChoices s={streaks} date={date} repair={streaks.repairs.find((r) => r.date === date)} />
                </div>
            )}
            {past ? (
                <>
                    {x?.problems?.map((q) => (
                        <Link key={q.problem.id} className={`li ${q.grade === "again" ? "f" : q.grade === "hard" ? "h" : "g"}`} to="/d/$slug" params={{ slug: q.problem.slug }}><b>{GLYPH[q.grade]}</b><span>{q.problem.title}</span><small>{(p.topics.find((t) => t.id === q.problem.topic)?.name ?? "").toUpperCase()}</small></Link>
                    ))}
                    {x?.reviews ? <div className="cap">{x.reviews} review{x.reviews > 1 ? "s" : ""} done</div> : null}
                </>
            ) : (
                <>
                    {d.new?.map((q) => (
                        <div key={q.id}>
                            <Link className="li" to="/d/$slug" params={{ slug: q.slug }}><b>●</b><span>{q.title}</span><small>{q.practice ? "PRACTICE" : (p.topics.find((t) => t.id === q.topic)?.name ?? "").toUpperCase()}</small></Link>
                            {q.companies.length > 0 && <div className="cap">Asked at {q.companies.map((c) => `${c.name}${c.recent ? "*" : ""}`).join(", ")}{q.companies.some((c) => c.recent) ? " (* last six months)" : ""}</div>}
                        </div>
                    ))}
                    {d.reviews?.slice(0, 8).map((r) => (
                        <Link key={`${r.problem.id}-${r.n}`} className="li" to="/d/$slug" params={{ slug: r.problem.slug }}><b>↻</b><span>{r.problem.title}</span><small>REVIEW {r.n + 1}</small></Link>
                    ))}
                    {(d.reviews?.length ?? 0) > 8 && <div className="cap">+{d.reviews!.length - 8} more</div>}
                    <div className="cap">{d.kind === "break" ? "Nothing planned." : `${d.reviews?.length ?? 0} of ${d.capacity} reviews${(d.reviews?.length ?? 0) > d.capacity ? " · over capacity" : ""}`}</div>
                </>
            )}
        </div>
    );
}

function History({ p }: { p: PlanView }) {
    const h = p.history;
    const maxR = Math.max(1, ...h.weeks.map((w) => w.reviews));
    return (
        <div className="c-hist">
            <div className="c-stats">
                <div className="stat"><b>{h.problems}</b><small>problems in 8 weeks</small></div>
                <div className="stat" title="Days in a row that kept the plan: every planned solve day had a solve. Rest and practice days neither break it nor add to it."><b>{h.streak}d</b><small>days kept in a row</small></div>
                <div className="stat"><b>{h.kept_percent === null ? "—" : `${h.kept_percent}%`}</b><small>of planned solve days</small></div>
                <div className="stat"><b>{h.clean_percent === null ? "—" : `${h.clean_percent}%`}</b><small>solved without failing</small></div>
            </div>
            <div className="weeks">{h.weeks.map((w) => <div key={w.start} className="wb" title={`${w.problems} problems, ${w.reviews} reviews`}><i className="r" style={{ height: `${(w.reviews / maxR) * 36}px` }} /><i className="p" style={{ height: `${(Math.min(w.problems, 6) / 6) * 44}px` }} /></div>)}</div>
            <div className="wl">{h.weeks.map((w) => <span key={w.start}>{Number(w.start.slice(8))}</span>)}</div>
            <div className="cap"><span style={{ color: "var(--grn)" }}>■</span> problems &nbsp;<span style={{ color: "var(--acc)" }}>■</span> reviews, per week (by Monday)</div>
            <div className="c-chs">{Object.entries(h.mix).sort((a, b) => b[1] - a[1]).map(([n, c]) => <span key={n} className="c-ch mix">{n} {c}</span>)}</div>
            <div className="c-key">
                <i className="sw solo" /><span>Solved on my own</span><i className="sw help" /><span>Solved with help</span><i className="sw fail" /><span>Attempted</span>
                <i className="sw miss" /><span>Planned, not done</span><i className="sw pr" /><span>Practice day (upcoming)</span><i className="sw br" /><span>Break</span><i className="sw td" /><span>Today</span>
            </div>
        </div>
    );
}
