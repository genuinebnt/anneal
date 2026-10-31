import { useNavigate } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api } from "../api";
import { counted, newOn, niceDate, statusOf, WEEKDAYS } from "../dsa";
import { getPref, setPref } from "../prefs";
import { THEMES, applyTheme, storedTheme } from "../theme";
import { ACCENTS, useAppearance } from "../settings";
import { toggleTimer } from "../focusStore";
import { toast } from "./toasts";
import { useDialog } from "./useDialog";

type Hit = { key: string; label: string; kind: string; color: string; go: () => void; meta?: string; rows?: [string, string][] };

/** The order the groups are shown in. */
const GROUPS = ["Now", "Recent", "Alike", "Log", "Action", "Page", "Problem", "Technique", "Company", "Stage", "Challenge", "Track"];
const HEADINGS: Record<string, string> = { Alike: "LOOKS LIKE IT (SHARED TAGS)", Technique: "TECHNIQUES", Company: "COMPANIES", Now: "DO NEXT", Recent: "RECENT", Log: "LOG A PROBLEM", Action: "ACTIONS", Page: "PAGES", Problem: "PROBLEMS", Stage: "COURSE STAGES", Challenge: "COURSE CHALLENGES", Track: "RUST TRACKS" };

/** 0 for no match. Every word must be found: at the start of the text or of a word scores most, anywhere inside less, and as scattered
 *  letters (a typo or an abbreviation, "twsm" for "two sum") least. */
function score(text: string, words: string[]): number {
    const t = text.toLowerCase();
    let total = 0;
    for (const w of words) {
        const at = t.indexOf(w);
        if (at >= 0) total += at === 0 ? 8 : t[at - 1] === " " ? 6 : 3;
        else {
            let pos = 0;
            for (const ch of w) {
                pos = t.indexOf(ch, pos);
                if (pos < 0) return 0;
                pos++;
            }
            total += 1;
        }
    }
    return total + Math.max(0, 3 - text.length / 30);
}

/** The label with the words that were typed marked. */
function marked(label: string, words: string[]): ReactNode {
    const found = words.filter(Boolean).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    if (!found.length) return label;
    return label.split(new RegExp(`(${found.join("|")})`, "ig")).map((part, i) => (i % 2 ? <mark key={i}>{part}</mark> : part));
}

/** ⌘K: pages, actions, problems, techniques, companies, course stages and tracks. Fuzzy matching, grouped results, what you chose last on
 *  top, and a few shortcuts: `>` for pages and actions only, a number for a LeetCode problem, `log two sum` to log one, `~ two sum` for the
 *  problems that look like it (shared tags). It reads what the app already loads. */
export function Palette({ onClose }: { onClose: () => void }) {
    const nav = useNavigate();
    const qc = useQueryClient();
    const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const course = useQuery({ queryKey: ["course", "bustub"], queryFn: () => api.course("bustub") });
    const [q, setQ] = useState("");
    const [sel, setSel] = useState(0);
    const input = useRef<HTMLInputElement>(null);
    const list = useRef<HTMLUListElement>(null);
    const dialog = useRef<HTMLDivElement>(null);
    useDialog(dialog, true, onClose);
    const [recent] = useState<string[]>(() => getPref<string[]>("palette.recent", []));
    const appearance = useAppearance();
    useEffect(() => input.current?.focus(), []);

    const all = useMemo<Hit[]>(() => {
        const go = (key: string, fn: () => unknown) => () => {
            setPref("palette.recent", [key, ...recent.filter((k) => k !== key)].slice(0, 8));
            onClose();
            void fn();
        };
        const hits: Hit[] = [
            { key: "p-dsa", label: "DSA problems", kind: "Page", color: "var(--acc)", rows: dsa.data ? [["Goal", `${dsa.data.plan.goal_done} of ${dsa.data.plan.goal_total} done`], ["Reviews due", String(dsa.data.plan.due)]] : undefined, go: go("p-dsa", () => nav({ to: "/dsa" })) },
            { key: "p-plan", label: "Plan", kind: "Page", color: "var(--acc)", rows: dsa.data ? [["Today", dsa.data.plan.solve_day ? "a solve day" : "no new problem"], ["New a week", String(dsa.data.settings.new_days.length * dsa.data.settings.new_per_day)], ["Finish by", dsa.data.settings.target_date ? niceDate(dsa.data.settings.target_date, dsa.data.today) : "not set"]] : undefined, go: go("p-plan", () => nav({ to: "/dsa/plan" })) },
            { key: "p-review", label: "Review", kind: "Page", color: "var(--acc)", rows: dsa.data ? [["Due", String(dsa.data.plan.due)], ["Overdue", String(dsa.data.plan.overdue)], ["Room today", String(dsa.data.plan.capacity)]] : undefined, go: go("p-review", () => nav({ to: "/dsa/review" })) },
            { key: "p-cal", label: "Calendar", kind: "Page", color: "var(--acc)", go: go("p-cal", () => nav({ to: "/dsa/calendar" })) },
            { key: "p-company", label: "Interview mode (a company and a day)", kind: "Page", color: "var(--acc)", go: go("p-company", () => nav({ to: "/dsa/company" })) },
            { key: "p-mock", label: "Mock interview", kind: "Page", color: "var(--acc)", go: go("p-mock", () => nav({ to: "/dsa/mock" })) },
            { key: "p-rust", label: "Rust tracks", kind: "Page", color: "var(--vio)", go: go("p-rust", () => nav({ to: "/rust" })) },
            { key: "p-courses", label: "BusTub course", kind: "Page", color: "var(--grn)", go: go("p-courses", () => nav({ to: "/courses" })) },
            { key: "p-progress", label: "Progress", kind: "Page", color: "var(--fg)", go: go("p-progress", () => nav({ to: "/progress" })) },
            { key: "p-graph", label: "Graph: where to start, looks like, path", kind: "Page", color: "var(--acc)", go: go("p-graph", () => nav({ to: "/dsa/graph" })) },
            { key: "p-analytics", label: "Analytics: the VPS and the app", kind: "Page", color: "var(--fg)", go: go("p-analytics", () => nav({ to: "/analytics" })) },
            { key: "a-timer", label: "Start or pause the focus timer", kind: "Action", color: "var(--fg)", meta: "the same timer as the header", go: go("a-timer", () => toggleTimer()) },
            ...ACCENTS.filter((a) => a.id !== "custom").map((a): Hit => ({ key: `a-accent-${a.id}`, label: `Accent: ${a.label}`, kind: "Action", color: "var(--fg)", go: go(`a-accent-${a.id}`, () => appearance.set(a.id)) })),
            { key: "p-log", label: "Quick log (phone)", kind: "Page", color: "var(--acc)", go: go("p-log", () => nav({ to: "/log" })) },
            { key: "a-settings", label: "Settings", kind: "Action", color: "var(--fg)", go: go("a-settings", () => nav({ to: "/settings" })) },
            { key: "a-help", label: "Keyboard shortcuts", kind: "Action", color: "var(--fg)", go: go("a-help", () => window.dispatchEvent(new Event("anneal:help"))) },
            { key: "a-backup", label: "Download a backup of everything", kind: "Action", color: "var(--fg)", go: go("a-backup", () => window.location.assign("/api/export")) },
            { key: "a-theme", label: "Change the theme (dark, light, system)", kind: "Action", color: "var(--fg)", go: go("a-theme", () => applyTheme(THEMES[(THEMES.indexOf(storedTheme()) + 1) % THEMES.length]!)) },
        ];
        // what the plan asks for today, on top of an empty search
        const o = dsa.data;
        if (o) {
            const p = o.plan;
            const next = o.problems.find((x) => x.id === p.next_up[0]);
            if (p.due > 0) hits.unshift({ key: "n-review", label: `Review ${p.due} due problem${p.due === 1 ? "" : "s"}`, kind: "Now", color: "var(--vio)", meta: p.overdue ? `${p.overdue} overdue` : "today", rows: [["Due", String(p.due)], ["Overdue", String(p.overdue)], ["Room today", String(p.capacity)]], go: go("n-review", () => nav({ to: "/dsa/review" })) });
            if (p.solve_day && next && counted(o, next)) {
                const n = newOn(o.settings, WEEKDAYS[(new Date(`${o.today}T00:00`).getDay() + 6) % 7] ?? "mon");
                hits.unshift({ key: "n-next", label: `Today's problem: ${next.number} · ${next.title}`, kind: "Now", color: "var(--acc)", meta: `${next.difficulty} · ${n} today`, rows: [["Difficulty", next.difficulty], ["New today", String(n)], ["Asked by", next.companies.slice(0, 3).map((c) => c.name).join(", ") || "–"]], go: go("n-next", () => nav({ to: "/d/$slug", params: { slug: next.slug } })) });
            }
        }
        for (const t of tracks.data ?? []) hits.push({ key: `t-${t.slug}`, label: `${t.code} · ${t.name}`, kind: "Track", color: "var(--vio)", go: go(`t-${t.slug}`, () => nav({ to: "/t/$track", params: { track: t.slug } })) });
        for (const p of course.data?.projects ?? [])
            for (const m of p.modules)
                for (const s of m.stages)
                    hits.push({ key: `s-${s.id}`, label: `${s.id} · ${s.title}`, kind: s.kind === "challenge" ? "Challenge" : "Stage", color: "var(--grn)", go: go(`s-${s.id}`, () => nav({ to: "/courses/$course/$stage", params: { course: "bustub", stage: s.id } })) });
        // techniques open their card on the Learn tab; companies open Interview mode
        const od = dsa.data;
        if (od) {
            for (const t of od.techniques) {
                const pat = od.patterns.find((x) => x.name === t.pattern);
                if (!pat) continue;
                const ps = od.problems.filter((x) => x.technique === t.id && x.list_tag);
                hits.push({ key: `te-${t.id}`, label: t.name, kind: "Technique", color: "var(--vio)", meta: `${t.pattern} · ${ps.filter((x) => x.state.solved).length}/${ps.length}`, go: go(`te-${t.id}`, () => nav({ to: "/dsa/patterns/$code", params: { code: pat.code }, hash: `t-${t.id.split(":")[1] ?? t.id}` })) });
            }
            const asked = new Map<string, number>();
            for (const x of od.problems) for (const c of x.companies) asked.set(c.name, (asked.get(c.name) ?? 0) + 1);
            for (const [name, n] of [...asked.entries()].sort((a, b) => b[1] - a[1]).slice(0, 60))
                hits.push({ key: `co-${name}`, label: name, kind: "Company", color: "var(--fg)", meta: `${n} asked`, go: go(`co-${name}`, () => nav({ to: "/dsa/company" })) });
        }
        const today = dsa.data?.today ?? "";
        for (const p of dsa.data?.problems ?? []) {
            const st = today ? statusOf(p, today) : "new";
            hits.push({ key: `d-${p.id}`, label: `${p.number} · ${p.title}`, kind: "Problem", color: st === "solved" ? "var(--grn)" : "var(--acc)", meta: `${st === "solved" ? "solved · " : st === "due" ? "review due · " : st === "retry" ? "attempted · " : ""}${p.difficulty}`,
                rows: [["Difficulty", p.difficulty], ["Status", { new: "not attempted", retry: "attempted", due: "review due", solved: "solved" }[st]], ["Last tried", p.state.last_review ? niceDate(p.state.last_review.slice(0, 10), today) : "never"], ["Next review", p.state.due ? niceDate(p.state.due, today) : "none"], ...(p.companies.length ? [["Asked by", p.companies.slice(0, 3).map((c) => c.name).join(", ")] as [string, string]] : []), ...(p.premium ? [["Premium", "needs a subscription"] as [string, string]] : [])], go: go(`d-${p.id}`, () => nav({ to: "/d/$slug", params: { slug: p.slug } })) });
        }
        return hits;
    }, [tracks.data, dsa.data, course.data, nav, onClose, recent, appearance]);

    const shown = useMemo<Hit[]>(() => {
        const raw = q.trim().toLowerCase();
        if (!raw) {
            const byKey = new Map(all.map((h) => [h.key, h]));
            const rec = recent.map((k) => byKey.get(k)).filter((h): h is Hit => !!h).map((h) => ({ ...h, kind: "Recent" }));
            const now = all.filter((h) => h.kind === "Now");
            return [...now, ...rec, ...all.filter((h) => (h.kind === "Page" || h.kind === "Action") && !rec.some((r) => r.key === h.key))].slice(0, 18);
        }
        // "log two sum": the four outcomes for the best matches, logged straight from here
        const words0 = raw.split(/\s+/);
        if (words0[0] === "log" && words0.length > 1) {
            const rest = words0.slice(1);
            const hits: Hit[] = [];
            const matches = (dsa.data?.problems ?? []).map((p) => ({ p, s: score(`${p.title} ${p.number}`, rest) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).slice(0, 2);
            for (const { p } of matches)
                for (const [grade, label] of [["good", "on my own"], ["hard", "with help"], ["again", "attempted"], ["easy", "instant"]] as const)
                    hits.push({
                        key: `log-${p.id}-${grade}`,
                        label: `Log ${p.title}: ${label}`,
                        kind: "Log",
                        color: "var(--acc)",
                        go: () => {
                            onClose();
                            void api.logDsa(p.id, grade).then(
                                () => {
                                    for (const key of ["dsa", "activity", "tracks", "progress", "stats", "reviews", "dsa-preview", "practice"]) void qc.invalidateQueries({ queryKey: [key] });
                                    toast("ok", `${p.title}: ${label}`, "Logged. Open its page to undo.");
                                },
                                (e: Error) => toast("er", "Couldn't log it", e.message),
                            );
                        },
                    });
            if (hits.length) return hits;
        }
        // "~ two sum": the problems that share the most LeetCode tags with the best match
        if (raw.startsWith("~")) {
            const rest = raw.slice(1).trim().split(/\s+/).filter(Boolean);
            const ps = dsa.data?.problems ?? [];
            const base = rest.length ? ps.map((p) => ({ p, s: score(`${p.title} ${p.number}`, rest) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s)[0]?.p : undefined;
            if (base) {
                return ps
                    .filter((p) => p.id !== base.id)
                    .map((p) => ({ p, k: p.tags.filter((t) => base.tags.includes(t)) }))
                    .filter((x) => x.k.length > 0)
                    .sort((a, b) => b.k.length - a.k.length || a.p.order - b.p.order)
                    .slice(0, 12)
                    .map(({ p, k }): Hit => ({ key: `al-${p.id}`, label: `${p.number} · ${p.title}`, kind: "Alike", color: "var(--acc)", meta: `${k.length} shared with ${base.title}`, rows: [["Shared tags", k.join(", ")], ["Difficulty", p.difficulty]], go: () => { onClose(); void nav({ to: "/d/$slug", params: { slug: p.slug } }); } }));
            }
        }
        const actionsOnly = raw.startsWith(">");
        const text = actionsOnly ? raw.slice(1).trim() : raw.replace(/^#/, "");
        const words = text.split(/\s+/).filter(Boolean);
        const pool = actionsOnly ? all.filter((h) => h.kind === "Page" || h.kind === "Action") : all;
        if (!words.length) return pool.slice(0, 16);
        const asNumber = /^\d+$/.test(text) ? text : null;
        const scored = pool
            .map((h) => ({ h, base: score(`${h.label} ${h.kind}`, words) }))
            // the bonuses only rank things that matched; they never make a non-match appear
            .filter((x) => x.base > 0)
            .map((x) => ({ h: x.h, s: x.base + (asNumber && x.h.kind === "Problem" && x.h.label.startsWith(`${asNumber} ·`) ? 50 : 0) + (x.h.kind === "Page" || x.h.kind === "Action" ? 1.5 : 0) + (recent.includes(x.h.key) ? 2 : 0) }))
            .sort((a, b) => b.s - a.s)
            .slice(0, 30)
            .map((x) => x.h);
        // grouped, each group keeping the best-first order inside it
        return GROUPS.flatMap((g) => scored.filter((h) => h.kind === g));
    }, [all, q, recent, dsa.data, qc, onClose]);
    useEffect(() => setSel(0), [q]);
    useEffect(() => list.current?.querySelector<HTMLElement>("li.hd-sel")?.scrollIntoView({ block: "nearest" }), [sel, shown]);
    const words = q.trim().toLowerCase().replace(/^[>#]/, "").split(/\s+/).filter(Boolean);

    return (
        <div className="hd-pal-wrap" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="hd-pal" ref={dialog} role="dialog" aria-label="Search" aria-modal="true">
                <input
                    ref={input}
                    value={q}
                    onChange={(e) => setQ(e.target.value)}
                    placeholder="Search, or type  > for actions, a number, log two sum, ~ two sum…"
                    aria-label="Search"
                    autoComplete="off"
                    spellCheck={false}
                    onKeyDown={(e) => {
                        if (e.key === "Escape") onClose();
                        else if (e.key === "ArrowDown") {
                            e.preventDefault();
                            setSel((s) => Math.min(s + 1, shown.length - 1));
                        } else if (e.key === "ArrowUp") {
                            e.preventDefault();
                            setSel((s) => Math.max(s - 1, 0));
                        } else if (e.key === "PageDown") {
                            e.preventDefault();
                            setSel((s) => Math.min(s + 6, shown.length - 1));
                        } else if (e.key === "PageUp") {
                            e.preventDefault();
                            setSel((s) => Math.max(s - 6, 0));
                        } else if (e.key === "Enter") shown[sel]?.go();
                    }}
                />
                <div className="hd-main">
                <ul role="listbox" ref={list}>
                    {shown.map((h, i) => (
                        <li key={`${h.kind}-${h.key}`} role="presentation">
                            {(i === 0 || shown[i - 1]!.kind !== h.kind) && <span className="hd-grp">{HEADINGS[h.kind] ?? h.kind.toUpperCase()}</span>}
                            <div role="option" aria-selected={i === sel} className={`hd-opt${i === sel ? " hd-sel" : ""}`} style={{ "--c": h.color } as CSSProperties} onMouseEnter={() => setSel(i)} onMouseDown={(e) => e.preventDefault()} onClick={h.go}>
                                <span className="d" />
                                <span className="hd-lab">{marked(h.label, words)}</span>
                                {h.meta && <small>{h.meta}</small>}
                                {words.length > 0 && ["Page", "Action", "Track", "Stage", "Challenge"].includes(h.kind) && <em className="hd-kind">{h.kind.toLowerCase()}</em>}
                                {i === sel && <kbd className="hd-enter" aria-hidden="true">↵</kbd>}
                            </div>
                        </li>
                    ))}
                    {shown.length === 0 && <li className="hd-none">No match. Try fewer letters, a LeetCode number, or <b>&gt;</b> for actions.</li>}
                </ul>
                {shown[sel] && (
                    <aside className="hd-prev" aria-label="Preview" style={{ "--c": shown[sel]!.color } as CSSProperties}>
                        <span className="k"><i />{(HEADINGS[shown[sel]!.kind] ?? shown[sel]!.kind).toLowerCase()}</span>
                        <b className="t">{shown[sel]!.label}</b>
                        {shown[sel]!.rows && (
                            <dl>
                                {shown[sel]!.rows!.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}
                            </dl>
                        )}
                        <span className="go"><kbd>↵</kbd> open</span>
                    </aside>
                )}
                </div>
                <div className="hd-hint">
                    <span>↑↓ move</span>
                    <span>↵ open</span>
                    <span>esc close</span>
                    <span>&gt; actions</span>
                    <span>log two sum</span>
                </div>
            </div>
        </div>
    );
}
