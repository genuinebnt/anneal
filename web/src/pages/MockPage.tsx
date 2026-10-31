import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type Grade, type MockConfig, type MockData } from "../api";
import { Header, MOCK_FRESH } from "../components/Header";
import { Segmented, Stepper } from "../components/ui";
import { HowFound, Page3, Panel, Tile, PageLoading, LoadFailed } from "../components/layout";
import { GradeGlyph } from "../components/icons";
import { DIFF, leetcode, niceDate, statusOf } from "../dsa";
import {
    BANDS,
    FAVOUR,
    MOCK_LISTS,
    MOCK_REACH,
    MOCK_STATUS,
    PRESETS,
    clock,
    countBy,
    currentIndex,
    defaultConfig,
    draw,
    elapsed,
    isLogged,
    limitOf,
    mmss,
    newRound,
    onProblem,
    paceMinutes,
    poolOf,
    shortfall,
    slotsOf,
    suggested,
    summary,
    warning,
    type Round,
} from "../mock";

const ROUND_KEY = "anneal-mock-round";

function loadRound(): Round | null {
    try {
        const raw = localStorage.getItem(ROUND_KEY);
        return raw ? (JSON.parse(raw) as Round) : null;
    } catch {
        return null;
    }
}

function storeRound(r: Round | null) {
    try {
        if (r) localStorage.setItem(ROUND_KEY, JSON.stringify(r));
        else localStorage.removeItem(ROUND_KEY);
    } catch {
        // Private windows can refuse storage; the round still runs for this visit.
    }
}

/** A timed round of random LeetCode problems, set up the way you choose (docs/DSA.md, decision 25). */
export function MockPage() {
    const q = useQuery({ queryKey: ["dsa-mock"], queryFn: api.mock, staleTime: MOCK_FRESH });
    return (
        <>
            <Header area="dsa" />
            <div style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                {q.data ? (
                    <MockApp data={q.data} />
                ) : (
                    <PageLoading crumb={["DSA", "Mock interview"]} title="Mock interview" error={q.isError ? <LoadFailed what="the mock interview" error={q.error} retry={() => void q.refetch()} /> : undefined} />
                )}
            </div>
        </>
    );
}

function MockApp({ data }: { data: MockData }) {
    const qc = useQueryClient();
    const [round, setRound] = useState<Round | null>(loadRound);
    const [config, setConfig] = useState<MockConfig>(() => ({ ...defaultConfig(), ...data.saved?.last }));
    const [drawError, setDrawError] = useState<string | null>(null);
    const finishing = useRef(false);
    const remember = useMutation({ mutationFn: api.saveMock });

    const update = useCallback((fn: (r: Round) => Round) => {
        setRound((cur) => {
            if (!cur) return cur;
            const next = fn(cur);
            storeRound(next);
            return next;
        });
    }, []);

    const begin = (c: MockConfig) => {
        const pool = poolOf(data.problems, c, data.today);
        const picked = draw(pool, c, data.today);
        if (!picked) {
            setDrawError(shortfall(pool, c) ?? "Couldn't draw problems for this setup.");
            return false;
        }
        setDrawError(null);
        setConfig(c);
        finishing.current = false;
        const r = newRound(c, picked.map((p) => p.id), Date.now());
        storeRound(r);
        setRound(r);
        remember.mutate({ ...data.saved, last: c });
        return true;
    };

    const finish = (r: Round) => {
        if (r.ended != null || finishing.current) return;
        finishing.current = true;
        const now = Date.now();
        const ended = { ...r, ended: elapsed(r, now), pausedAt: null };
        const body = summary(ended, now);
        storeRound({ ...ended, saved: true });
        setRound({ ...ended, saved: true });
        api
            .finishMock({ config: r.config, ...body })
            .catch(() => undefined)
            .finally(() => {
                void qc.invalidateQueries({ queryKey: ["dsa-mock"] });
                void qc.invalidateQueries({ queryKey: ["dsa"] });
                void qc.invalidateQueries({ queryKey: ["tracks"] });
            });
    };

    const leave = (keepConfig: boolean) => {
        if (keepConfig && round) setConfig(round.config);
        storeRound(null);
        setRound(null);
        finishing.current = false;
    };

    if (round && round.ended == null) return <main className="page"><Live data={data} round={round} update={update} onFinish={finish} /></main>;
    if (round) {
        return (
            <main className="page"><Result
                data={data}
                round={round}
                onAgain={() => {
                    if (!begin(round.config)) leave(true);
                }}
                onChange={() => leave(true)}
            /></main>
        );
    }
    return <Setup data={data} initial={config} onStart={begin} error={drawError} />;
}

// ---------------------------------------------------------------- setup

const toggle = (list: string[], v: string) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
type Tab = "pool" | "topics" | "companies" | "clock";

function Switch({ on, onClick, title, hint, disabled }: { on: boolean; onClick: () => void; title: string; hint?: string; disabled?: boolean }) {
    return (
        <button className={`mi-tog${on ? " on" : ""}`} onClick={onClick} disabled={disabled} role="switch" aria-checked={on}>
            <i className="sw" />
            <span>
                <b>{title}</b>
                {hint && <small>{hint}</small>}
            </span>
        </button>
    );
}

function Seg<T extends string>({ value, options, onPick, label }: { value: T; options: [T, string][]; onPick: (v: T) => void; label: string }) {
    return <Segmented value={value} options={options} onChange={onPick} label={label} />;
}

/** The shape of a round as small bars, one per problem: green easy, amber medium, red hard. */
function Shape({ c }: { c: MockConfig }) {
    const bars = c.anyDiff ? Array.from({ length: c.anyCount }, () => "a") : c.mix.flatMap((k, i) => Array.from({ length: k }, () => "ewh"[i]!));
    return <span className="mi-shape">{bars.map((b, i) => <i key={i} className={b} />)}</span>;
}

function Setup({ data, initial, onStart, error }: { data: MockData; initial: MockConfig; onStart: (c: MockConfig) => void; error: string | null }) {
    const qc = useQueryClient();
    const [c, setC] = useState<MockConfig>(initial);
    const [preset, setPreset] = useState("custom");
    const [tab, setTab] = useState<Tab>("pool");
    const [coQuery, setCoQuery] = useState("");
    const [allCo, setAllCo] = useState(false);
    const [naming, setNaming] = useState(false);
    const [name, setName] = useState("");
    const presets = data.saved?.presets ?? [];
    const savePresets = useMutation({
        mutationFn: (next: typeof presets) => api.saveMock({ ...data.saved, presets: next }),
        onSuccess: () => void qc.invalidateQueries({ queryKey: ["dsa-mock"] }),
    });

    const edit = (fn: (c: MockConfig) => MockConfig) => {
        setC(fn);
        setPreset("custom");
    };

    const pool = useMemo(() => poolOf(data.problems, c, data.today), [data, c]);
    const have = countBy(pool);
    const problem = shortfall(pool, c);
    const need = slotsOf(c);

    // Counts shown beside each choice: what the other settings leave.
    const byTopic = useMemo(() => {
        const m = new Map<string, number>();
        for (const p of poolOf(data.problems, { ...c, topics: {} }, data.today)) m.set(p.pattern, (m.get(p.pattern) ?? 0) + 1);
        return m;
    }, [data, c]);
    const byCompany = useMemo(() => {
        const m = new Map<string, number>();
        const g = new Map<string, number>();
        for (const p of poolOf(data.problems, { ...c, groups: [], companies: [], recent: false }, data.today)) {
            const seenGroups = new Set<string>();
            for (const co of p.companies) {
                m.set(co.name, (m.get(co.name) ?? 0) + 1);
                seenGroups.add(co.group);
            }
            for (const n of seenGroups) g.set(n, (g.get(n) ?? 0) + 1);
        }
        return { companies: m, groups: g };
    }, [data, c]);
    const listCount = (key: string) => data.problems.filter((p) => !p.premium && p.lists.includes(key as never)).length;

    const allCompanies = useMemo(
        () =>
            [...new Set(data.company_groups.flatMap((g) => g.companies))]
                .map((n) => ({ name: n, n: byCompany.companies.get(n) ?? 0 }))
                .filter((x) => x.n > 0 || c.companies.includes(x.name))
                .sort((a, b) => b.n - a.n || a.name.localeCompare(b.name)),
        [data.company_groups, byCompany, c.companies],
    );
    const q = coQuery.trim().toLowerCase();
    const matching = q ? allCompanies.filter((x) => x.name.toLowerCase().includes(q)) : allCompanies;
    const shownCo = q || allCo ? matching : matching.slice(0, 8);

    const included = Object.values(c.topics).filter((v) => v === 1).length;
    const excluded = Object.values(c.topics).filter((v) => v === -1).length;
    const cycleTopic = (code: string) =>
        edit((cur) => {
            const topics = { ...cur.topics };
            const v = topics[code];
            if (v === undefined) topics[code] = 1;
            else if (v === 1) topics[code] = -1;
            else delete topics[code];
            return { ...cur, topics };
        });

    const slots = c.anyDiff ? Array.from({ length: c.anyCount }, () => 1) : c.mix.flatMap((k, i) => Array.from({ length: k }, () => i));
    const perTotal = paceMinutes(c);
    const warnings: { cls: string; text: string }[] = [];
    if (problem) warnings.push({ cls: "mi-bad", text: problem });
    if (!problem && error) warnings.push({ cls: "mi-bad", text: error });
    if (c.format === "total" && perTotal > c.total * 1.25) warnings.push({ cls: "mi-warn", text: `At the suggested pace the round takes ${perTotal} min, but the clock is ${c.total}. Lengthen the round or ask for fewer problems.` });
    if (c.favour === "weak" && !pool.some((p) => p.state.last_grade === "again" || p.state.last_grade === "hard" || (p.state.due && p.state.due <= data.today))) {
        warnings.push({ cls: "mi-warn", text: "Nothing here is marked weak yet, so this will draw uniformly." });
    }
    const timerName = c.format === "total" ? `Countdown, ${c.total}:00` : c.format === "per" ? "Countdown per problem" : "Stopwatch";

    // What the pool is drawn from, each removable.
    const listName = (k: string) => MOCK_LISTS.find(([key]) => key === k)?.[1] ?? k;
    const filters: { key: string; label: string; exc?: boolean; remove: () => void }[] = [
        ...c.lists.map((l) => ({ key: `list:${l}`, label: listName(l), remove: () => edit((cur) => (cur.lists.length > 1 ? { ...cur, lists: toggle(cur.lists, l) } : cur)) })),
        ...((c.reach ?? "all") !== "all" ? [{ key: "reach", label: MOCK_REACH.find(([k]) => k === c.reach)?.[1] ?? "", remove: () => edit((cur) => ({ ...cur, reach: "all" as const })) }] : []),
        ...(c.status !== "all" ? [{ key: "status", label: MOCK_STATUS.find(([k]) => k === c.status)?.[1] ?? c.status, remove: () => edit((cur) => ({ ...cur, status: "all" as const })) }] : []),
        ...Object.entries(c.topics).map(([code, v]) => ({ key: `topic:${code}`, label: data.patterns.find((p) => p.code === code)?.name ?? code, exc: v === -1, remove: () => cycleTopic(code) })),
        ...c.groups.map((g) => ({ key: `group:${g}`, label: g, remove: () => edit((cur) => ({ ...cur, groups: toggle(cur.groups, g) })) })),
        ...c.companies.map((n) => ({ key: `co:${n}`, label: n, remove: () => edit((cur) => ({ ...cur, companies: toggle(cur.companies, n) })) })),
        ...(c.recent ? [{ key: "recent", label: "last 6 months", remove: () => edit((cur) => ({ ...cur, recent: false })) }] : []),
    ];
    const removeSlot = (band: number) => {
        if (c.anyDiff) edit((cur) => ({ ...cur, anyCount: Math.max(1, cur.anyCount - 1) }));
        else edit((cur) => ({ ...cur, mix: cur.mix.map((x, j) => (j === band ? Math.max(0, x - 1) : x)) as MockConfig["mix"] }));
    };
    const addSlot = (band: number) => edit((cur) => ({ ...cur, mix: cur.mix.map((x, j) => (j === band ? x + 1 : x)) as MockConfig["mix"] }));
    const tabs: [Tab, string, string | number][] = [
        ["pool", "Pool", pool.length],
        ["topics", "Topics", included || excluded ? `${included + excluded}` : "all"],
        ["companies", "Companies", c.groups.length + c.companies.length || "any"],
        ["clock", "Clock", c.format === "up" ? "∞" : c.format === "per" ? "each" : `${c.total}m`],
    ];

    // counts beside each choice: what the other settings leave
    const reachN = (reach: NonNullable<MockConfig["reach"]>) => poolOf(data.problems, { ...c, reach }, data.today).length;
    const statusN = (status: MockConfig["status"]) => poolOf(data.problems, { ...c, status }, data.today).length;
    const listMax = Math.max(1, ...MOCK_LISTS.map(([k]) => listCount(k)));
    const histN = (k: "new" | "retry" | "solved" | "due") => pool.filter((p) => statusOf(p, data.today) === k).length;
    const dots = pool.map((p) => p.difficulty);
    const weights: Record<MockConfig["favour"], number[]> = { random: [50, 50, 50, 50, 50, 50, 50], weak: [90, 72, 55, 40, 30, 22, 16], asked: [30, 85, 45, 95, 25, 60, 40] };
    const favourIcon: Record<MockConfig["favour"], ReactNode> = {
        random: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" /><rect x="9" y="2.5" width="4.5" height="4.5" rx="1" /><rect x="2.5" y="9" width="4.5" height="4.5" rx="1" /><rect x="9" y="9" width="4.5" height="4.5" rx="1" /></svg>,
        weak: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="8" cy="8" r="5.5" /><circle cx="8" cy="8" r="2.5" /></svg>,
        asked: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M5 2.5h6v3a3 3 0 0 1-6 0zM8 8.5v3M5.5 13.5h5" /></svg>,
    };
    const reachIcon: Record<NonNullable<MockConfig["reach"]>, ReactNode> = {
        all: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true"><path d="M5.5 4h8M5.5 8h8M5.5 12h8M2.5 4h.01M2.5 8h.01M2.5 12h.01" /></svg>,
        done: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m3 8.5 3 3 7-7" /></svg>,
        similar: <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true"><circle cx="4" cy="4" r="2" /><circle cx="12" cy="5" r="2" /><circle cx="7" cy="12" r="2" /><path d="M5.8 4.4 10 4.8M5 5.8l1.4 4.4M10.8 6.6 8.3 10.6" /></svg>,
    };
    const reachText: Record<NonNullable<MockConfig["reach"]>, string> = { all: "every problem in the sources below", done: "only what you have attempted or solved", similar: "what you have done and its close neighbours" };
    const Sh = ({ n, t, sub, extra }: { n: ReactNode; t: string; sub?: string; extra?: ReactNode }) => (
        <div className="mk-sh"><span className="mk-n">{n}</span><b>{t}</b>{sub && <small>{sub}</small>}<span className="ly-sp" />{extra}</div>
    );
    const C = 2 * Math.PI * 84;
    const arcs = c.format === "up" ? [] : slots.map((band) => c.per[c.anyDiff ? 1 : band] ?? 0);
    const arcTotal = Math.max(1, arcs.reduce((x, y) => x + y, 0));
    const dial = (
        <svg viewBox="0 0 200 200" className="mk-dial" aria-hidden="true">
            <circle cx="100" cy="100" r="84" className="tr" />
            {(() => {
                let at = -90;
                return arcs.map((m, i) => {
                    const len = (m / arcTotal) * C - (arcs.length > 1 ? 10 : 0);
                    const el = <circle key={i} cx="100" cy="100" r="84" className={`s${(i % 2) + 1} b${c.anyDiff ? 1 : slots[i]}`} strokeDasharray={`${Math.max(2, len).toFixed(1)} ${C.toFixed(1)}`} transform={`rotate(${at + (arcs.length > 1 ? 1.7 : 0)} 100 100)`} />;
                    at += (m / arcTotal) * 360;
                    return el;
                });
            })()}
            {Array.from({ length: 12 }, (_, k) => <line key={k} x1="100" y1={k % 3 ? 30 : 28} x2="100" y2="34" className="tk" transform={`rotate(${k * 30} 100 100)`} />)}
        </svg>
    );

    const tiles = (
        <>
            <Tile em={data.rounds.length === 1 ? "Round taken" : "Rounds taken"} strong={data.rounds.length} small={data.rounds.length ? `last ${new Date(data.rounds[0]!.finished_at).toLocaleDateString(undefined, { day: "numeric", month: "short" })}` : "none yet"} />
            <Tile em="In the pool" hint="Problems a round can draw, after the filters." strong={pool.length} small="after the filters" />
            <Tile em={need === 1 ? "Problem a round" : "Problems a round"} strong={need} small={c.format === "total" ? `${c.total} min` : c.format === "per" ? "a clock each" : "no limit"} />
        </>
    );

    const left = (
        <>
            <Panel title="Presets">
                <div className="mi-presets" role="group" aria-label="Presets">
                    {PRESETS.map((p) => (
                        <button key={p.key} className={`mi-pc${preset === p.key ? " on" : ""}`} aria-pressed={preset === p.key} onClick={() => { setC(p.apply(c)); setPreset(p.key); }}>
                            <b>{p.name}</b>
                            <Shape c={p.apply(c)} />
                            <small>{p.detail}</small>
                        </button>
                    ))}
                    {presets.map((p) => (
                        <span key={p.name} className={`mi-pc${preset === `saved:${p.name}` ? " on" : ""}`}>
                            <button className="body" aria-pressed={preset === `saved:${p.name}`} onClick={() => { setC(p.config); setPreset(`saved:${p.name}`); }}>
                                <b>{p.name}</b>
                                <Shape c={p.config} />
                                <small>{describe(p.config, data)}</small>
                            </button>
                            <button className="x" aria-label={`Delete preset ${p.name}`} onClick={() => savePresets.mutate(presets.filter((x) => x.name !== p.name))}>×</button>
                        </span>
                    ))}
                </div>
            </Panel>
            <Panel title="Drawing from" aside={<span className="mk-cnt">{pool.length} can be drawn</span>}>
                <div className="mi-sum">
                    {filters.map((f) => (
                        <span key={f.key} className={`mi-fc${f.exc ? " exc" : ""}`}>
                            {f.exc ? "not " : ""}{f.label}
                            <button aria-label={`Remove ${f.label}`} onClick={f.remove}>✕</button>
                        </span>
                    ))}
                </div>
            </Panel>
            <Panel title="Your rounds" aside={<span className="mk-cnt">{data.rounds.length} taken</span>}>
                {data.rounds.length === 0 ? (
                    <p>No rounds yet. Your rounds show here with a trend: time used, solved, and a Repeat for each.</p>
                ) : (
                    <>
                        {(() => {
                            const last = [...data.rounds].slice(0, 10).reverse();
                            const share = last.map((r) => (r.items.length ? r.items.filter((i) => i.grade === "good" || i.grade === "easy").length / r.items.length : 0));
                            if (last.length < 2) return null;
                            const avg = Math.round((100 * share.reduce((x, y) => x + y, 0)) / share.length);
                            const mins = Math.round(last.reduce((n, r) => n + r.seconds, 0) / last.length / 60);
                            return (
                                <div className="mi-trend" aria-label="Trend over your last rounds">
                                    <span className="bars" aria-hidden="true">{share.map((v, k) => <i key={k} style={{ height: `${8 + v * 28}px` }} />)}</span>
                                    <span>Last {last.length}: <b>{avg}%</b> on your own, <b>{mins} min</b> on average.</span>
                                </div>
                            );
                        })()}
                        <div className="mk-rounds">
                            {data.rounds.slice(0, 12).map((r) => (
                                <div key={r.id} className="mk-rd">
                                    <span className="d">{new Date(r.finished_at).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
                                    <span className="res">{r.items.map((i, k) => <i key={k} className={`dot ${i.grade === "good" || i.grade === "easy" ? "g" : i.grade === "hard" ? "h" : i.grade === "again" ? "f" : "s"}`}>{glyphOf(i.grade)}</i>)}</span>
                                    <span className="t">{mmss(r.seconds)}</span>
                                    <button className="mi-ghost" title={describe(r.config, data)} onClick={() => { setC(r.config); setPreset("custom"); }}>Repeat</button>
                                </div>
                            ))}
                        </div>
                    </>
                )}
            </Panel>
        </>
    );

    const center = (
        <>
            <div className="mi-tabs mk-tabs" role="tablist">
                {tabs.map(([k, label, n]) => (
                    <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{label}<small>{n}</small></button>
                ))}
            </div>

            {tab === "pool" && (
                <div className="mk-pool">
                    <section className="mk-sec">
                        <Sh n="1" t="Which problems" sub="the starting set" />
                        <div className="mk-opts three" role="radiogroup" aria-label="Which problems">
                            {MOCK_REACH.map(([k, label]) => (
                                <button key={k} role="radio" aria-checked={(c.reach ?? "all") === k} aria-pressed={(c.reach ?? "all") === k} className="mk-opt" onClick={() => edit((cur) => ({ ...cur, reach: k }))}>
                                    <span className="mk-oi">{reachIcon[k]}</span>
                                    <span className="mk-ot"><b>{label}</b><small>{reachText[k]}</small></span>
                                    <em>{reachN(k)}</em>
                                </button>
                            ))}
                        </div>
                    </section>
                    <section className="mk-sec">
                        <Sh n="2" t="Where problems come from" sub="pick one or more" extra={<span className="mk-cnt">{c.lists.length} selected</span>} />
                        <div className="mk-srcs">
                            {MOCK_LISTS.map(([key, label]) => {
                                const on = c.lists.includes(key);
                                return (
                                    <button key={key} className="mk-src" aria-pressed={on} onClick={() => edit((cur) => ({ ...cur, lists: cur.lists.includes(key) ? (cur.lists.length > 1 ? toggle(cur.lists, key) : cur.lists) : [...cur.lists, key] }))}>
                                        <span className="mk-cb"><Check /></span>
                                        <span className="mk-ot"><b>{label}</b></span>
                                        <span className="mk-sb"><i style={{ "--w": `${(listCount(key) / listMax) * 100}%` } as CSSProperties} /></span>
                                        <em>{listCount(key)}</em>
                                    </button>
                                );
                            })}
                        </div>
                    </section>
                    <section className="mk-sec">
                        <Sh n="3" t="Your history" sub="which of them you have touched" />
                        <div className="mk-pills" role="radiogroup" aria-label="History">
                            {MOCK_STATUS.map(([k, label, hint]) => {
                                const n = statusN(k);
                                return (
                                    <button key={k} role="radio" aria-checked={c.status === k} aria-pressed={c.status === k} className="mk-pill" data-empty={n === 0 ? "" : undefined} title={hint} onClick={() => edit((cur) => ({ ...cur, status: k }))}>{label}<em>{n}</em></button>
                                );
                            })}
                        </div>
                    </section>
                    <section className="mk-sec">
                        <Sh n="4" t="How to draw" sub="the odds of each problem" />
                        <div className="mk-opts three" role="radiogroup" aria-label="How to draw">
                            {FAVOUR.map(([k, label, hint]) => (
                                <button key={k} role="radio" aria-checked={c.favour === k} aria-pressed={c.favour === k} className="mk-opt" onClick={() => edit((cur) => ({ ...cur, favour: k }))}>
                                    <span className="mk-oi">{favourIcon[k]}</span>
                                    <span className="mk-ot"><b>{label}</b><small>{hint}</small></span>
                                    <span className="mk-wb">{weights[k].map((h, i) => <i key={i} style={{ "--h": `${h}%` } as CSSProperties} />)}</span>
                                </button>
                            ))}
                        </div>
                    </section>
                    <section className="mk-sec mk-prev">
                        <div className="mk-sh"><span className="mk-n"><Eye /></span><b>The pool</b><small>each dot is a problem that can be drawn</small><span className="ly-sp" /><b className="mk-big">{pool.length}</b></div>
                        <PoolDots kinds={dots} />
                        <div className="mk-foot">
                            <div className="mi-mix" role="img" aria-label={`${have[0]} easy, ${have[1]} medium, ${have[2]} hard`}>
                                {(["Easy", "Medium", "Hard"] as const).map((l, i) => <span key={l}><i className={"ewh"[i]} />{l} <b>{have[i]}</b></span>)}
                            </div>
                            <span className="ly-sp" />
                            {(["new", "retry", "solved", "due"] as const).map((k) => <span key={k} className="mk-st"><b>{histN(k)}</b>{{ new: "not attempted", retry: "attempted", solved: "solved", due: "due" }[k]}</span>)}
                        </div>
                    </section>
                </div>
            )}

            {tab === "topics" && (
                <section className="mk-sec">
                    <Sh n={<Layers />} t="Topics" sub={included || excluded ? `${included} included · ${excluded} left out` : "a click includes, a second leaves out, a third clears"} extra={(included || excluded) ? <button className="mi-ghost" onClick={() => edit((cur) => ({ ...cur, topics: {} }))}>Clear</button> : undefined} />
                    <div className="mk-pats">
                        {data.patterns.map((pt, i) => {
                            const v = c.topics[pt.code];
                            const n = byTopic.get(pt.code) ?? 0;
                            const total = data.problems.filter((p) => p.pattern === pt.code && !p.premium && p.lists.includes("neetcode150")).length;
                            const solved = data.problems.filter((p) => p.pattern === pt.code && !p.premium && p.lists.includes("neetcode150") && p.state.solved).length;
                            const state = v === 1 ? "in" : v === -1 ? "out" : included ? "off" : "any";
                            return (
                                <button key={pt.code} className={`mk-pat ${state}`} style={{ "--c": PCOL[i % PCOL.length], "--i": i } as CSSProperties} onClick={() => cycleTopic(pt.code)} aria-label={`${pt.name}: ${v === 1 ? "included" : v === -1 ? "excluded" : "allowed"}`}>
                                    <span className="mk-cb">{v === -1 ? <Cross /> : <Check />}</span>
                                    <span className="mk-pn"><b>{pt.name}</b><small>{n} in the pool · {solved}/{total} solved</small></span>
                                    <span className="mk-pb" style={{ "--p": `${total ? (solved / total) * 100 : 0}%` } as CSSProperties} />
                                </button>
                            );
                        })}
                    </div>
                </section>
            )}

            {tab === "companies" && (
                <section className="mk-sec">
                    <Sh n={<Target />} t="Only problems these companies ask" sub={c.groups.length + c.companies.length ? [...c.groups, ...c.companies].slice(0, 3).join(", ") + (c.groups.length + c.companies.length > 3 ? ` +${c.groups.length + c.companies.length - 3}` : "") : "none picked: any company"} />
                    <div className="mk-row">
                        <label className="cm-find mk-find">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" /></svg>
                            <input value={coQuery} onChange={(e) => setCoQuery(e.target.value)} placeholder="Find a company" aria-label="Find a company" />
                        </label>
                        <Seg label="When asked" value={c.recent ? "recent" : "any"} options={[["any", "Asked any time"], ["recent", "Asked in the last 6 months"]]} onPick={(v) => edit((cur) => ({ ...cur, recent: v === "recent" }))} />
                    </div>
                    <span className="ly-lab">Groups</span>
                    <div className="mk-pills">
                        {data.company_groups.map((g) => (
                            <button key={g.name} className="mk-pill" aria-pressed={c.groups.includes(g.name)} onClick={() => edit((cur) => ({ ...cur, groups: toggle(cur.groups, g.name) }))}>{g.name}<em>{byCompany.groups.get(g.name) ?? 0}</em></button>
                        ))}
                    </div>
                    {c.groups.length + c.companies.length === 0 && <div className="mk-empty"><Target /><span>No company picked, so the round can draw from any of them. Pick a group or companies to narrow the pool.</span></div>}
                    <div className="mk-cos">
                        {shownCo.map((x, i) => {
                            const on = c.companies.includes(x.name);
                            const group = data.company_groups.find((g) => g.companies.includes(x.name))?.name ?? "";
                            return (
                                <button key={x.name} className="mk-co" aria-pressed={on} style={{ "--i": Math.min(i, 12) } as CSSProperties} onClick={() => edit((cur) => ({ ...cur, companies: toggle(cur.companies, x.name) }))}>
                                    <span className="mk-av">{x.name[0]}</span>
                                    <span className="mk-ot"><b>{x.name}</b><small>{group}</small></span>
                                    <span className="mk-cn"><b>{x.n}</b><small>in the pool</small></span>
                                    <span className="mk-add"><Plus /></span>
                                </button>
                            );
                        })}
                    </div>
                    {!q && matching.length > 8 && <button className="mi-ghost" style={{ justifySelf: "start" }} onClick={() => setAllCo(!allCo)}>{allCo ? "Show fewer" : `Show ${matching.length - 8} more`}</button>}
                </section>
            )}

            {tab === "clock" && (
                <section className="mk-sec mk-clock">
                    <div className="mk-dialw">
                        {dial}
                        <div className="mk-dc"><b>{c.format === "up" ? "∞" : c.format === "total" ? `${c.total}:00` : `${arcTotal}:00`}</b><small>{need} problem{need === 1 ? "" : "s"} · {c.format === "total" ? `${perTotal} min at the suggested pace` : c.format === "per" ? "a clock for each" : "the clock counts up"}</small></div>
                    </div>
                    <div className="mk-cset">
                        <Sh n={<Clock />} t="Format" />
                        <div className="mk-opts three" role="radiogroup" aria-label="Timer format">
                            {([["total", "Countdown for the round", "one clock for every problem"], ["per", "Countdown per problem", "a clock each, at the minutes below"], ["up", "Stopwatch only", "no limit; you end the round"]] as const).map(([k, label, hint]) => (
                                <button key={k} role="radio" aria-checked={c.format === k} aria-pressed={c.format === k} className="mk-opt" onClick={() => edit((cur) => ({ ...cur, format: k }))}>
                                    <span className="mk-ot"><b>{label}</b><small>{hint}</small></span>
                                </button>
                            ))}
                        </div>
                        {c.format === "up" ? (
                            <p className="mi-hint">No limit. The clock counts up ({c.per.join(" / ")} min suggested).</p>
                        ) : (
                            <div className="mk-mins">
                                {c.format === "total" && <div><span className="ly-lab">Round length</span><Stepper label="minutes" value={c.total} min={10} max={180} step={5} onChange={(total) => edit((cur) => ({ ...cur, total }))} /></div>}
                                {BANDS.map((band, i) => (
                                    <div key={band}><span className="ly-lab">{c.format === "total" ? "Suggested, " : ""}{band}</span><Stepper label={`${band} minutes`} value={c.per[i] ?? 0} min={3} max={90} onChange={(n) => edit((cur) => ({ ...cur, per: cur.per.map((x, j) => (j === i ? n : x)) as MockConfig["per"] }))} /></div>
                                ))}
                            </div>
                        )}
                        <div className="mk-sws">
                            <Switch on={c.strict} onClick={() => edit((cur) => ({ ...cur, strict: !cur.strict }))} title="Strict: no pause" hint="The clock keeps running, like a real call. Turn off to allow Pause." />
                            <Switch on={c.blind} onClick={() => edit((cur) => ({ ...cur, blind: !cur.blind }))} title="Blind mode" hint="Hide topic, tags and companies until you log the problem." />
                            <Switch on={c.auto} disabled={c.format === "up"} onClick={() => edit((cur) => ({ ...cur, auto: !cur.auto }))} title={c.format === "per" ? "Move on at 0:00" : "End the round at 0:00"} hint={c.format === "per" ? "Off: the problem's clock turns red and counts overtime." : "Off: the clock turns red and counts overtime, and you decide when to stop."} />
                            <Switch on={c.warns} disabled={c.format === "up"} onClick={() => edit((cur) => ({ ...cur, warns: !cur.warns }))} title="Time warnings" hint="A banner at 10, 5 and 1 minute left." />
                        </div>
                    </div>
                </section>
            )}
        </>
    );

    const right = (
        <>
            <aside className="mi-ticket">
                <button className="mi-go" disabled={!!problem} onClick={() => onStart(c)}>Draw and start the clock →</button>
                <div className="top">
                    <div className="big"><b>{need}</b><span>{need === 1 ? "problem" : "problems"} · {c.format === "total" ? `${c.total} min` : c.format === "per" ? "per problem" : "no limit"}</span></div>
                    <div className="bands">
                        {BANDS.map((band, i) => <span key={band} className={`band ${band[0]}`}>{DIFF[band][0]} {have[i]}</span>)}
                        <span className="note">can be drawn</span>
                    </div>
                </div>
                <div className="slots">
                    {slots.length === 0 && <p className="mi-hint" style={{ textAlign: "center", padding: "10px 0" }}>Add at least one problem.</p>}
                    {slots.map((band, j) => (
                        <div key={j} className="slot">
                            <i>{j + 1}</i>
                            <span className="ti">{c.anyDiff ? <b className="a">Any</b> : <b className={BANDS[band]![0]}>{DIFF[BANDS[band]!][0]}</b>}</span>
                            <span className="tm">~{c.per[c.anyDiff ? 1 : band]}m</span>
                            <button aria-label={`Remove problem ${j + 1}`} disabled={c.anyDiff && c.anyCount <= 1} onClick={() => removeSlot(band)}>✕</button>
                        </div>
                    ))}
                </div>
                <div className="adds">
                    {c.anyDiff ? (
                        <button style={{ gridColumn: "1 / -1" }} disabled={c.anyCount >= 6} onClick={() => edit((cur) => ({ ...cur, anyCount: cur.anyCount + 1 }))}>+ Any difficulty<small>{pool.length} available</small></button>
                    ) : (
                        BANDS.map((band, i) => (
                            <button key={band} disabled={(c.mix[i] ?? 0) >= have[i]! || (c.mix[i] ?? 0) >= 4} onClick={() => addSlot(i)}>+ {DIFF[band][0]}<small>{have[i]} available</small></button>
                        ))
                    )}
                </div>
                <div className="anyrow"><Switch on={c.anyDiff} onClick={() => edit((cur) => ({ ...cur, anyDiff: !cur.anyDiff }))} title="Any difficulty" /></div>
                <dl className="kv">
                    <div><dt>Time at suggested pace</dt><dd>{perTotal} min</dd></div>
                    <div><dt>Timer</dt><dd>{timerName}</dd></div>
                    <div><dt>Pause</dt><dd>{c.strict ? "not allowed" : "allowed"}</dd></div>
                    <div><dt>Topic and tags</dt><dd>{c.blind ? "hidden" : "shown"}</dd></div>
                </dl>
                {warnings.map((w) => <div key={w.text} className={w.cls}>{w.text}</div>)}
                <div className="foot">
                    {naming ? (
                        <form
                            onSubmit={(e) => {
                                e.preventDefault();
                                const n = name.trim();
                                if (!n) return;
                                savePresets.mutate([...presets.filter((p) => p.name !== n), { name: n, config: c }]);
                                setPreset(`saved:${n}`);
                                setNaming(false);
                                setName("");
                            }}
                        >
                            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name this preset" aria-label="Preset name" maxLength={40} autoFocus />
                            <div className="row"><button className="mi-ghost" type="submit" disabled={!name.trim()}>Save</button><button className="mi-ghost" type="button" onClick={() => setNaming(false)}>Cancel</button></div>
                        </form>
                    ) : (
                        <button className="mi-ghost" onClick={() => setNaming(true)}>Save as a preset</button>
                    )}
                </div>
            </aside>
            <HowFound
                title="How a round is drawn"
                text={`From the pool, ${c.favour === "random" ? "at random" : c.favour === "weak" ? "weighted towards your weak spots" : "weighted by how often the chosen companies ask"}. The same problem never comes twice in a round.`}
                rows={[["Pool", pool.length], ["Weighting", c.favour === "random" ? "uniform" : c.favour === "weak" ? "weak ×3" : "often asked"], ["Repeats", "never in a round"], ["Timer", c.format === "total" ? `${c.total}:00` : c.format === "per" ? "per problem" : "stopwatch"]]}
            />
        </>
    );

    return (
        <div className="mi-wrap m-setup mk-3">
            <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Mock interview"]} title={<>Build your <span style={{ color: "var(--acc)" }}>interview.</span></>} sub="Pick the pool, the problems and the clock, then draw a round." tiles={tiles} left={left} center={center} right={right} label="Presets and rounds" />
            <div className="m-dock">
                <span><b>{pool.length}</b> can be drawn · {need} problem{need === 1 ? "" : "s"}</span>
                <button className="mi-go" disabled={!!problem} onClick={() => onStart(c)}>Start →</button>
            </div>
        </div>
    );
}

const PCOL = ["var(--grn)", "var(--fn)", "var(--acc)", "var(--vio)", "var(--bad)", "var(--warn)"];
const Check = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m3 8.5 3 3 7-7" /></svg>;
const Cross = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>;
const Plus = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><path d="M8 3.5v9M3.5 8h9" /></svg>;
const Eye = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z" /><circle cx="8" cy="8" r="2" /></svg>;
const Layers = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" aria-hidden="true"><path d="M8 2 1.5 5.5 8 9l6.5-3.5zM1.5 9 8 12.5 14.5 9" /></svg>;
const Target = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="8" cy="8" r="5.5" /><circle cx="8" cy="8" r="2.5" /></svg>;
const Clock = () => <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true"><circle cx="8" cy="8" r="6" /><path d="M8 4.5V8l2.5 1.5" /></svg>;

/** The pool as dots, one per problem, coloured by difficulty; two light up in turn, the way a draw might pick them. */
function PoolDots({ kinds }: { kinds: string[] }) {
    const [pick, setPick] = useState<number[]>([]);
    useEffect(() => {
        if (kinds.length < 2) return;
        const t = window.setInterval(() => setPick([Math.floor(Math.random() * kinds.length), Math.floor(Math.random() * kinds.length)]), 1600);
        return () => window.clearInterval(t);
    }, [kinds.length]);
    const order = { easy: 0, medium: 1, hard: 2 } as Record<string, number>;
    const sorted = [...kinds].sort((a, b) => (order[a] ?? 0) - (order[b] ?? 0));
    return (
        <div className="mk-dots" aria-hidden="true">
            {sorted.slice(0, 900).map((k, i) => <i key={i} className={`${k[0]}${pick.includes(i) ? " pick" : ""}`} />)}
        </div>
    );
}

/** One line for a round's settings: "2 medium · Trees, Graphs · NeetCode 150". */
function describe(c: MockConfig, data: MockData): string {
    const parts: string[] = [];
    parts.push(c.anyDiff ? `${c.anyCount} problem${c.anyCount === 1 ? "" : "s"}` : BANDS.flatMap((b, i) => ((c.mix[i] ?? 0) > 0 ? [`${c.mix[i]} ${b}`] : [])).join(", ") || "no problems");
    const topics = Object.entries(c.topics).filter(([, v]) => v === 1).map(([code]) => data.patterns.find((p) => p.code === code)?.name ?? code);
    parts.push(topics.length ? topics.join(", ") : "any topic");
    parts.push(c.lists.map((l) => MOCK_LISTS.find(([k]) => k === l)?.[1] ?? l).join(" + "));
    if (c.groups.length + c.companies.length) parts.push([...c.groups, ...c.companies].join(", "));
    return parts.join(" · ");
}

const glyphOf = (g: Grade | null) => (g === "good" ? "✓" : g === "easy" ? "⚡" : g === "hard" ? "½" : g === "again" ? "!" : "–");

// ---------------------------------------------------------------- live

function useNow(running: boolean): number {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        setNow(Date.now());
        if (!running) return;
        const t = window.setInterval(() => setNow(Date.now()), 250);
        return () => window.clearInterval(t);
    }, [running]);
    return now;
}

const WARN_TEXT: Record<number, string> = { 600: "10 minutes left.", 300: "5 minutes left.", 60: "1 minute left." };

function Live({ data, round, update, onFinish }: { data: MockData; round: Round; update: (fn: (r: Round) => Round) => void; onFinish: (r: Round) => void }) {
    const now = useNow(round.pausedAt == null);
    const byId = useMemo(() => new Map(data.problems.map((p) => [p.id, p])), [data.problems]);
    const c = round.config;
    const index = currentIndex(round);
    const problem = byId.get(round.ids[index] ?? "");
    const logged = isLogged(round);
    const band = problem?.difficulty ?? null;
    const time = clock(round, band, now);
    const limit = limitOf(c, band);
    const [banner, setBanner] = useState<string | null>(null);
    const [confirmEnd, setConfirmEnd] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const last = useRef(time.value);
    const lastKey = useRef(`${index}`);

    const advance = useCallback(
        (fn: (r: Round) => Round) => {
            update(fn);
        },
        [update],
    );

    // Warnings as the clock crosses 10, 5 and 1 minute, and the moves at 0:00.
    useEffect(() => {
        const key = `${index}`;
        if (lastKey.current !== key) {
            lastKey.current = key;
            last.current = time.value;
            setBanner(null);
            return;
        }
        if (c.format !== "up" && c.warns) {
            const w = warning(last.current, time.value, limit);
            if (w != null) {
                setBanner(WARN_TEXT[w] ?? null);
                const t = window.setTimeout(() => setBanner(null), 9000);
                last.current = time.value;
                return () => window.clearTimeout(t);
            }
        }
        last.current = time.value;
    }, [time.value, index, c.format, c.warns, limit]);

    useEffect(() => {
        if (!c.auto || c.format === "up" || time.value > 0 || round.pausedAt != null) return;
        if (c.format === "total") {
            onFinish(round);
        } else if (!logged) {
            const to = elapsed(round, Date.now());
            if (index + 1 < round.ids.length) advance((r) => ({ ...r, marks: [...r.marks, { grade: null, to, due: null }], starts: [...r.starts, to] }));
            else onFinish({ ...round, marks: [...round.marks, { grade: null, to, due: null }] });
        }
    }, [time.value, c.auto, c.format, round, logged, index, advance, onFinish]);

    useEffect(() => {
        const before = document.title;
        document.title = `${mmss(time.value)} · Mock round`;
        return () => {
            document.title = before;
        };
    }, [time.value]);

    if (!problem) {
        return (
            <div className="wrap" style={{ paddingBlock: 28 }}>
                <p className="rempty">A problem in this round is no longer in the catalog.</p>
                <button className="mi-ghost" onClick={() => onFinish(round)}>End the round</button>
            </div>
        );
    }

    const log = async (grade: Grade) => {
        if (logged || busy) return;
        setBusy(true);
        setError(null);
        const to = elapsed(round, Date.now());
        try {
            const out = await api.logDsa(problem.id, grade);
            advance((r) => ({ ...r, marks: [...r.marks, { grade, to, due: out.due }] }));
        } catch (e) {
            setError((e as Error).message);
        } finally {
            setBusy(false);
        }
    };
    const skip = () => {
        const to = elapsed(round, Date.now());
        if (index + 1 < round.ids.length) advance((r) => ({ ...r, marks: [...r.marks, { grade: null, to, due: null }], starts: [...r.starts, to] }));
        else onFinish({ ...round, marks: [...round.marks, { grade: null, to, due: null }] });
    };
    const next = () => {
        const to = elapsed(round, Date.now());
        if (index + 1 < round.ids.length) advance((r) => ({ ...r, starts: [...r.starts, to] }));
        else onFinish(round);
    };
    const togglePause = () =>
        update((r) => {
            const t = Date.now();
            return r.pausedAt == null ? { ...r, pausedAt: t } : { ...r, pausedMs: r.pausedMs + (t - r.pausedAt), pausedAt: null };
        });

    const total = elapsed(round, now);
    const sug = suggested(c, problem.difficulty);
    const spent = onProblem(round, now);
    const mark = round.marks[index];
    const overall = time.value < 0 && time.counting === "down";
    const showMeta = logged || !c.blind;
    const patternName = data.patterns.find((p) => p.code === problem.pattern)?.name ?? "";
    const lastOne = index + 1 >= round.ids.length;
    const clockLabel = c.format === "per" ? `PROBLEM ${index + 1} · ${time.value < 0 ? "OVERTIME" : "LEFT"}` : c.format === "up" ? "ELAPSED" : overall ? "OVERTIME" : "MOCK ROUND · LIVE";


    const C = 2 * Math.PI * 46;
    const frac = c.format === "total" ? time.value / (c.total * 60) : c.format === "per" ? time.value / Math.max(1, sug) : spent / Math.max(1, sug);
    const ringCls = time.counting === "down" && time.value < 0 ? " over" : time.counting === "down" && time.value <= 300 && limit > 300 ? " low" : "";
    const paceAll = round.ids.reduce((n, id) => n + suggested(c, byId.get(id)?.difficulty ?? "medium"), 0);
    const clockText = mmss(time.value);

    return (
        <div className="mi-wrap mi-room">
            <div className="mi-rbar">
                <div className={`mi-ring${ringCls}`} aria-live="off">
                    <svg viewBox="0 0 108 108" aria-hidden="true"><circle className="bg" cx="54" cy="54" r="46" /><circle className="fg" cx="54" cy="54" r="46" strokeDasharray={C} strokeDashoffset={C * (1 - Math.min(1, Math.max(0, frac)))} /></svg>
                    <div className="c"><b className={clockText.length > 5 ? "long" : ""}>{clockText}</b><small>{clockLabel.replace("MOCK ROUND · LIVE", "LEFT")}</small></div>
                </div>
                <div className="mi-segbar">
                    <div className="lab"><span>MOCK ROUND · LIVE</span><span>{mmss(total)} used · {Math.round(paceAll / 60)} min suggested</span></div>
                    <div className="segs" aria-label="Problems">
                        {round.ids.map((id, i) => {
                            const m = round.marks[i];
                            const b = byId.get(id)?.difficulty ?? "medium";
                            const tone = !m ? "" : m.grade === "again" ? " no" : m.grade === "hard" ? " help" : m.grade ? " ok" : " skip";
                            const here = i === index && !m;
                            const w = here ? Math.min(100, (spent / Math.max(1, suggested(c, b))) * 100) : 0;
                            return (
                                <div key={id} className={`sg${here ? " cur" : ""}${m ? " done" : ""}${tone}`} style={{ flex: suggested(c, b) }}>
                                    <span className="fill" style={{ width: `${w}%` }} />
                                    <b className={`m-${b[0]}`}>{i + 1} · {DIFF[b][0]}</b>
                                    <small>{m ? glyphOf(m.grade) : `~${Math.round(suggested(c, b) / 60)}m`}</small>
                                </div>
                            );
                        })}
                    </div>
                </div>
                <div className="mi-rctl">
                    {!c.strict && <button className="mi-ghost" onClick={togglePause}>{round.pausedAt == null ? "Pause" : "Resume"}</button>}
                    {confirmEnd ? (
                        <>
                            <button className="mi-ghost" onClick={() => onFinish(round)}>End now</button>
                            <button className="mi-ghost" onClick={() => setConfirmEnd(false)}>Keep going</button>
                        </>
                    ) : (
                        <button className="mi-ghost" onClick={() => setConfirmEnd(true)}>End round</button>
                    )}
                </div>
            </div>
            {banner && <div className="mi-banner" role="status">{banner}</div>}
            {round.pausedAt != null && <div className="mi-banner" role="status">Paused. The clock is stopped until you resume.</div>}

            <RoundKeys logged={logged} onLog={(g) => void log(g)} onNext={next} onSkip={skip} />
            <div className="mi-stage">
                <div className="mi-main">
                <section className="mi-card mi-prob">
                    <span className="lab">PROBLEM {index + 1} OF {round.ids.length} · {DIFF[problem.difficulty][0].toUpperCase()} · SUGGESTED {Math.round(sug / 60)} MIN</span>
                    <h2 className="pt">{showMeta ? `#${problem.number} ` : ""}{problem.title}</h2>
                    <a className="open" href={leetcode(problem.slug)} target="_blank" rel="noopener noreferrer">Open on LeetCode ↗</a>
                    <div className="ptime">
                        <div className="row"><span><b>{mmss(spent)}</b> on this problem</span><span>{mmss(sug)} suggested</span></div>
                        <div className="split"><i className={spent > sug ? "over" : ""} style={{ width: `${Math.min(100, (spent / Math.max(1, sug)) * 100)}%` }} /></div>
                    </div>
                    {showMeta ? (
                        <div className="meta">
                            <span>{patternName}</span>
                            {problem.tags.slice(0, 5).map((t) => <span key={t}>{t}</span>)}
                            {problem.companies.length > 0 && <span className="co">Asked by {problem.companies.slice(0, 4).map((x) => x.name).join(", ")}{problem.companies.length > 4 ? ` +${problem.companies.length - 4}` : ""}</span>}
                        </div>
                    ) : (
                        <div className="blind"><span className="lock" aria-hidden="true">🔒</span><span><b>Blind mode.</b> Number, topic and companies stay hidden until you log it.</span></div>
                    )}
                </section>
                    <section className="mi-card">
                        <h3 className="mi-h3"><span>THIS ROUND</span><em>{round.marks.filter((m) => m.grade).length} of {round.ids.length} logged</em></h3>
                        <ol className="mi-rr">
                            {round.ids.map((id, i) => {
                                const p = byId.get(id);
                                const m = round.marks[i];
                                const here = i === index;
                                const upcoming = i > index;
                                const took = m ? m.to - (round.starts[i] ?? 0) : null;
                                const tone = !m ? "" : m.grade === "again" ? "no" : m.grade === "hard" ? "half" : m.grade ? "ok" : "skip";
                                return (
                                    <li key={id} className={`rr${here ? " cur" : ""}${upcoming ? " later" : ""}`}>
                                        <i>{i + 1}</i>
                                        <span>
                                            {upcoming || !p ? (
                                                <><b className={`m-${p?.difficulty[0] ?? "m"}`}>{p ? DIFF[p.difficulty][0] : "Problem"}</b> <span className="dim">· hidden until you get there</span></>
                                            ) : here && !m && c.blind ? "Working on it now" : p.title}
                                        </span>
                                        <span className={`m ${tone}`}>{m ? (m.grade ? `${glyphOf(m.grade)} ${mmss(took ?? 0)}` : "skipped") : here ? "now" : `~${p ? Math.round(suggested(c, p.difficulty) / 60) : "?"}m`}</span>
                                    </li>
                                );
                            })}
                        </ol>
                    </section>
                </div>

                <div className="mi-side">
                    <section className="mi-card">
                        <h3 className="mi-h3"><span>LOG PROBLEM {index + 1}</span><em>same as a normal attempt</em></h3>
                        <div className="mi-log3">
                            {([["good", "✓", "on my own", "ok"], ["hard", "½", "with help", "half"], ["again", "!", "attempted", "no"], ["easy", "⚡", "instant", "fast"]] as const).map(([g, , label, cls]) => (
                                <button key={g} className={`lg ${cls}${mark?.grade === g && logged ? " on" : ""}`} disabled={logged || busy} onClick={() => void log(g)}>
                                    <b><GradeGlyph grade={g} /></b><span>{label}</span><kbd>{["good", "hard", "again", "easy"].indexOf(g) + 1}</kbd>
                                </button>
                            ))}
                        </div>
                        {error && <div className="mi-bad">{error}</div>}
                        {logged && mark && mark.grade && (
                            <div className="mi-logged">Logged {glyphOf(mark.grade)}. {mark.due ? `Review ${niceDate(mark.due, data.today)}.` : "Practice problems schedule no reviews."}</div>
                        )}
                        <button className="mi-go" disabled={!logged} onClick={next}>{lastOne ? "Finish the round →" : "Next problem →"}</button>
                        {!logged && <button className="mi-ghost" onClick={skip}>Skip this problem <kbd>S</kbd></button>}
                    </section>
                </div>
            </div>
        </div>
    );
}

/** Keys in a round: 1 to 4 log how it went, N moves on once it is logged, S skips. Ignored while typing. */
function RoundKeys({ logged, onLog, onNext, onSkip }: { logged: boolean; onLog: (g: Grade) => void; onNext: () => void; onSkip: () => void }) {
    useEffect(() => {
        const f = (e: KeyboardEvent) => {
            if (e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement | null)?.closest?.("input,textarea,select,[contenteditable]")) return;
            const grade = (["good", "hard", "again", "easy"] as const)[Number(e.key) - 1];
            if (grade && !logged) onLog(grade);
            else if ((e.key === "n" || e.key === "N" || e.key === "Enter") && logged) onNext();
            else if ((e.key === "s" || e.key === "S") && !logged) onSkip();
        };
        window.addEventListener("keydown", f);
        return () => window.removeEventListener("keydown", f);
    }, [logged, onLog, onNext, onSkip]);
    return null;
}

// ---------------------------------------------------------------- result

function Result({ data, round, onAgain, onChange }: { data: MockData; round: Round; onAgain: () => void; onChange: () => void }) {
    const byId = useMemo(() => new Map(data.problems.map((p) => [p.id, p])), [data.problems]);
    const c = round.config;
    const { items, seconds } = summary(round, 0);
    const own = items.filter((i) => i.grade === "good" || i.grade === "easy").length;
    const help = items.filter((i) => i.grade === "hard").length;
    const failed = items.filter((i) => i.grade === "again").length;
    const skipped = items.filter((i) => !i.grade).length;
    const pace = items.reduce((n, it) => n + suggested(c, byId.get(it.id)?.difficulty ?? "medium"), 0);
    const diff = seconds - pace;
    const spare = c.format === "total" ? c.total * 60 - seconds : null;
    const parts = [own ? `${own} solved on your own` : "", help ? `${help} with help` : "", failed ? `${failed} attempted` : "", skipped ? `${skipped} skipped` : ""].filter(Boolean);
    const C = 2 * Math.PI * 62;
    return (
        <div className="mi-wrap">
            <div className="eyebrow">
                <Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link>
                <span>/</span>
                <span style={{ color: "var(--ca)" }}>MOCK INTERVIEW</span>
                <span>/</span>
                <span>ROUND FINISHED</span>
            </div>
            <div className="mi-hero">
                <div className="mi-score">
                    <svg viewBox="0 0 140 140" aria-hidden="true"><circle className="bg" cx="70" cy="70" r="62" /><circle className="fg" cx="70" cy="70" r="62" strokeDasharray={C} strokeDashoffset={C * (1 - (items.length ? own / items.length : 0))} /></svg>
                    <div className="c"><b>{own}/{items.length}</b><small>ON MY OWN</small></div>
                </div>
                <div>
                    <h1 className="mi-h2">{parts.length ? `${parts.join(", ")}.` : "Round ended."}</h1>
                    <p className="mi-sub" style={{ marginTop: 8 }}>
                        {spare != null && <b style={{ color: spare >= 0 ? "var(--grn)" : "var(--warn)" }}>{spare >= 0 ? `Finished with ${mmss(spare)} to spare. ` : `${mmss(-spare)} over the time. `}</b>}
                        {describe(c, data)}
                    </p>
                </div>
            </div>

            <div className="mi-stats">
                <div className="stat"><small>TIME USED</small><b>{mmss(seconds)}</b><span>{c.format === "total" ? `of ${mmss(c.total * 60)}` : c.format === "per" ? "per-problem countdowns" : "stopwatch"}</span></div>
                <div className="stat"><small>SOLVED ON MY OWN</small><b>{own} / {items.length}</b><span>{help} with help, {failed} attempted{skipped ? `, ${skipped} skipped` : ""}</span></div>
                <div className="stat"><small>AGAINST SUGGESTED</small><b style={{ color: diff <= 0 ? "var(--grn)" : "var(--warn)" }}>{diff <= 0 ? "−" : "+"}{mmss(Math.abs(diff))}</b><span>{mmss(pace)} suggested in all</span></div>
                <div className="stat"><small>SETTINGS</small><b style={{ fontSize: 16, paddingTop: 6 }}>{c.strict ? "strict" : "pausable"} · {c.blind ? "blind" : "open"}</b><span>{c.favour === "random" ? "uniform draw" : c.favour === "weak" ? "favoured weak ones" : "favoured often-asked"}</span></div>
            </div>

            <section className="mi-card">
                <h3 className="mi-h3"><span>THE PROBLEMS</span><em>open any to read it on LeetCode</em></h3>
                {items.map((it, i) => {
                    const p = byId.get(it.id);
                    if (!p) return null;
                    const sug = suggested(c, p.difficulty);
                    const cls = it.grade === "good" || it.grade === "easy" ? "ok" : it.grade === "hard" ? "half" : it.grade === "again" ? "no" : "skip";
                    const mark = round.marks[i];
                    const practice = p.lists.every((l) => l === "practice");
                    return (
                        <div key={it.id} className="mi-prow">
                            <span className={`g ${cls}`}>{glyphOf(it.grade)}</span>
                            <div>
                                <a className="t1" href={leetcode(p.slug)} target="_blank" rel="noopener noreferrer">#{p.number} {p.title} ↗</a>
                                <small>{data.patterns.find((x) => x.code === p.pattern)?.name} · <span className={`m-${p.difficulty[0]}`}>{DIFF[p.difficulty][0]}</span>{p.companies.length ? ` · ${p.companies.slice(0, 3).map((x) => x.name).join(", ")}` : ""}</small>
                            </div>
                            <div className="tm">
                                <div className="row"><span><b>{it.seconds ? mmss(it.seconds) : "–"}</b> {it.seconds ? "used" : "not opened"}</span><span>{mmss(sug)} suggested</span></div>
                                <div className="split"><i className={it.seconds > sug ? "over" : ""} style={{ width: `${Math.min(100, (it.seconds / Math.max(1, sug)) * 100)}%` }} /></div>
                            </div>
                            <span className="next">{!it.grade ? "Not logged" : practice ? "No review (practice)" : mark?.due ? `Review ${niceDate(mark.due, data.today)}` : "Logged"}</span>
                        </div>
                    );
                })}
            </section>

            <div className="mi-acts">
                <button className="mi-go wide" onClick={onAgain}>Another round, same settings →</button>
                <button className="mi-ghost tall" onClick={onChange}>Change settings</button>
            </div>
        </div>
    );
}
