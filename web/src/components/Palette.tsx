import { useNavigate } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { api, type DsaOverview, type DsaProblem, type Grade } from "../api";
import { counted, leetcode, newOn, niceDate, statusOf, WEEKDAYS } from "../dsa";
import { getPref, setPref } from "../prefs";
import { THEMES, applyTheme, storedTheme } from "../theme";
import { ACCENTS, useAppearance } from "../settings";
import { toggleTimer } from "../focusStore";
import { useShowIdeas } from "../spoilers";
import { toast } from "./toasts";
import { useDialog } from "./useDialog";

/** Something a result can do: the first runs on ↵, the second on ⌘↵, all of them from Tab. */
type Act = { label: string; run: () => void };
type Hit = {
    key: string;
    label: string;
    kind: string;
    /** The tile's glyph and colour. */
    glyph: string;
    color: string;
    go: () => void;
    sub?: string;
    rows?: [string, string][];
    acts?: Act[];
    problem?: DsaProblem;
};
/** A view inside the bar that replaces the search: everything about one thing, what means the same as a problem, or this session's logs. */
type Lens = { type: "about"; kind: "pro" | "tec" | "co"; id: string; title: string } | { type: "similar"; id: string; title: string } | { type: "did" };
type Did = { id: string; title: string; label: string; at: number; undone?: boolean };

/** The order the groups are shown in. */
const GROUPS = ["Read", "Now", "Did", "Recent", "Alike", "Matching", "Log", "Action", "Page", "Problem", "Concept", "Drill", "Technique", "Company", "Stage", "Challenge", "Track", "About"];
const HEADINGS: Record<string, string> = { Read: "READ AS", Alike: "LOOKS LIKE IT", Matching: "MATCHING", Did: "WHAT I DID", Technique: "TECHNIQUES", Company: "COMPANIES", Now: "DO NEXT", Recent: "RECENT", Log: "LOG A PROBLEM", Action: "ACTIONS", Page: "PAGES", Problem: "PROBLEMS", Concept: "SYSTEM DESIGN PAGES", Drill: "SYSTEM DESIGN DRILLS", Stage: "COURSE STAGES", Challenge: "COURSE CHALLENGES", Track: "RUST TRACKS" };
const KIND_WORD: Record<string, string> = { Problem: "problem", Technique: "technique", Company: "company", Page: "page", Action: "action", Concept: "page", Drill: "drill", Stage: "stage", Challenge: "challenge", Track: "track", Log: "log", Alike: "problem", Matching: "problem", Now: "next", Did: "log", Read: "list", About: "" };
const DIFF_COL: Record<string, string> = { easy: "var(--grn)", medium: "var(--warn)", hard: "var(--bad)" };
const OUTCOMES: [Grade, string][] = [["good", "on my own"], ["hard", "with help"], ["again", "attempted"], ["easy", "instant"]];
const REFRESH = ["dsa", "activity", "tracks", "progress", "stats", "reviews", "dsa-preview", "practice", "dsa-mock"];
const DID_KEY = "anneal-palette-did";
/** A log can be undone from here for this long, and only while it is the latest one made here for that problem. */
const UNDO_WINDOW = 10 * 60_000;

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

function readDid(): Did[] {
    try {
        return JSON.parse(sessionStorage.getItem(DID_KEY) ?? "[]") as Did[];
    } catch {
        return [];
    }
}

function writeDid(d: Did[]) {
    try {
        sessionStorage.setItem(DID_KEY, JSON.stringify(d.slice(0, 20)));
    } catch {
        // storage can be refused; the list then lasts as long as the bar is open
    }
}

const ago = (t: number) => {
    const m = Math.round((Date.now() - t) / 60_000);
    return m < 1 ? "just now" : m < 60 ? `${m} min ago` : `${Math.round(m / 60)} h ago`;
};

/** Words the bar understands as filters: "medium graphs google unsolved". Null when the words hold no filter. */
export function readAs(raw: string, o: DsaOverview) {
    const text = ` ${raw.toLowerCase().replace(/\s+/g, " ").trim()} `;
    const parts: [string, string][] = [];
    let rest = text;
    const take = (re: RegExp) => {
        const m = re.exec(rest);
        if (!m) return null;
        rest = rest.replace(m[0], " ");
        return m;
    };
    const diff = take(/ (easy|medium|hard) /);
    if (diff) parts.push(["difficulty", diff[1]!]);
    const status = take(/ (unsolved|not solved|to do|todo|solved|due|tried|attempted|untouched|not started) /);
    const statuses: string[] = !status ? [] : /unsolved|not solved|to ?do/.test(status[1]!) ? ["new", "retry", "due"] : status[1] === "solved" ? ["solved"] : status[1] === "due" ? ["due"] : /tried|attempted/.test(status[1]!) ? ["retry"] : ["new"];
    if (status) parts.push(["status", { new: "not started", retry: "tried", due: "review due", solved: "solved" }[statuses.length > 1 ? "x" : statuses[0]!] ?? "not solved"]);
    const list = take(/ (blind ?75|blind|neetcode ?150|150|neetcode ?250|250) /);
    const listKey = !list ? null : list[1]!.startsWith("blind") ? "blind75" : list[1]!.includes("150") ? "neetcode150" : "neetcode250";
    if (listKey) parts.push(["list", { blind75: "Blind 75", neetcode150: "NeetCode 150", neetcode250: "NeetCode 250" }[listKey]!]);
    // patterns by any word of their name ("graphs", "heap", "intervals"), plus "dp" for both DP patterns
    const pats = new Set<string>();
    for (const p of o.patterns) {
        const words = p.name.toLowerCase().replace(/[^a-z0-9 ]/g, " ").split(" ").filter((w) => w.length > 3 && !["dynamic", "programming", "advanced", "priority", "queue", "search"].includes(w));
        for (const w of words) {
            const singular = w.endsWith("s") ? w.slice(0, -1) : w;
            if (take(new RegExp(` (${w}|${singular}) `))) pats.add(p.code);
        }
        if (/dynamic programming/i.test(p.name) && / dp /.test(rest)) pats.add(p.code);
    }
    take(/ dp /);
    if (pats.size) parts.push(["pattern", [...pats].map((c) => o.patterns.find((p) => p.code === c)?.name).join(", ")]);
    const names = [...new Set(o.problems.flatMap((p) => p.companies.map((c) => c.name)))].sort((a, b) => b.length - a.length);
    const companies: string[] = [];
    for (const n of names) if (take(new RegExp(` ${n.toLowerCase().replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} `))) companies.push(n);
    if (companies.length) parts.push(["company", companies.join(", ")]);
    const leftover = rest.trim();
    // one filter only counts when nothing else was typed; two or more always do
    if (!parts.length || (parts.length === 1 && leftover)) return null;
    const match = o.problems
        .filter((p) => counted(o, p))
        .filter((p) => (listKey ? p.lists.includes(listKey) : true))
        .filter((p) => !diff || p.difficulty === diff[1])
        .filter((p) => !statuses.length || statuses.includes(statusOf(p, o.today)))
        .filter((p) => !pats.size || pats.has(p.pattern))
        .filter((p) => !companies.length || p.companies.some((c) => companies.includes(c.name)))
        .sort((a, b) => a.order - b.order);
    const q = new URLSearchParams();
    q.set("list", listKey ?? "all");
    if (diff) q.set("diff", diff[1]!);
    if (statuses.length) q.set("status", statuses.join("~"));
    if (pats.size) q.set("patterns", [...pats].join("~"));
    if (companies.length) q.set("companies", companies.join("~"));
    return { parts, match, search: q.toString(), leftover };
}

/** ⌘K: pages, actions, problems, techniques, companies, course stages and tracks, in two panes: results, and the selected one with what
 *  it can do (Tab). Fuzzy matching, groups with counts, what you chose last on top, and shortcuts: `>` for pages and actions, a number
 *  for a LeetCode problem, `log two sum` to log one, `~ words` to search by meaning, and plain words read as filters ("medium graphs
 *  google unsolved"). "Everything about" a result, "Find similar" and "What I did" (this session's logs, with Undo) open inside it. */
export function Palette({ onClose }: { onClose: () => void }) {
    const nav = useNavigate();
    const qc = useQueryClient();
    const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const course = useQuery({ queryKey: ["course", "bustub"], queryFn: () => api.course("bustub") });
    const sd = useQuery({ queryKey: ["sd"], queryFn: api.sd });
    const [q, setQ] = useState("");
    const [sel, setSel] = useState(0);
    const [lens, setLens] = useState<Lens | null>(null);
    const [did, setDid] = useState<Did[]>(readDid);
    const [confirmUndo, setConfirmUndo] = useState<string | null>(null);
    const [showIdeas] = useShowIdeas();
    const input = useRef<HTMLInputElement>(null);
    const list = useRef<HTMLUListElement>(null);
    const acts = useRef<HTMLDivElement>(null);
    const dialog = useRef<HTMLDivElement>(null);
    // Escape is the dialog's (it listens first): inside a lens it goes back to the search, otherwise it closes
    const lensOpen = useRef(false);
    lensOpen.current = lens !== null;
    useDialog(dialog, true, () => (lensOpen.current ? setLens(null) : onClose()));
    const [recent] = useState<string[]>(() => getPref<string[]>("palette.recent", []));
    const appearance = useAppearance();
    useEffect(() => {
        input.current?.focus();
    }, []);
    const o = dsa.data;

    // `~ words`: by meaning, once the typing pauses
    const raw = q.trim();
    const tilde = raw.startsWith("~") ? raw.slice(1).trim() : "";
    const [meaningText, setMeaningText] = useState("");
    useEffect(() => {
        const t = window.setTimeout(() => setMeaningText(tilde), 220);
        return () => window.clearTimeout(t);
    }, [tilde]);
    const meaning = useQuery({ queryKey: ["meaning", { q: meaningText }], queryFn: () => api.meaning({ q: meaningText, k: 30 }), enabled: meaningText.length > 1 && !lens, staleTime: Infinity, retry: false });
    const similarId = lens?.type === "similar" ? lens.id : lens?.type === "about" && lens.kind === "pro" ? lens.id : null;
    const similar = useQuery({ queryKey: ["meaning", { like: similarId }], queryFn: () => api.meaning({ like: similarId!, k: 30 }), enabled: !!similarId, staleTime: Infinity, retry: false });

    const remember = (key: string) => setPref("palette.recent", [key, ...recent.filter((k) => k !== key)].slice(0, 8));
    const close = (fn: () => unknown, key?: string) => () => {
        if (key) remember(key);
        onClose();
        void fn();
    };
    const refresh = () => {
        for (const key of REFRESH) void qc.invalidateQueries({ queryKey: [key] });
    };
    const log = (p: DsaProblem, grade: Grade, label: string) => {
        onClose();
        void api.logDsa(p.id, grade).then(
            () => {
                refresh();
                const next = [{ id: p.id, title: p.title, label, at: Date.now() }, ...readDid()];
                writeDid(next);
                setDid(next);
                toast("ok", `${p.title}: ${label}`, "Logged. ⌘K → What I did can undo it.");
            },
            (e: Error) => toast("er", "Couldn't log it", e.message),
        );
    };
    const undo = (d: Did) => {
        void api.undoDsa(d.id).then(
            (r) => {
                refresh();
                const next = readDid().map((x) => (x.id === d.id && x.at === d.at ? { ...x, undone: true } : x));
                writeDid(next);
                setDid(next);
                setConfirmUndo(null);
                toast(r.undone ? "ok" : "er", r.undone ? `Undone: ${d.title}` : "Nothing to undo", r.undone ? "The log and its review are back as they were." : "That log can't be undone any more.");
            },
            (e: Error) => toast("er", "Couldn't undo it", e.message),
        );
    };
    const copy = (text: string) => {
        void navigator.clipboard?.writeText(text).then(
            () => toast("ok", "Link copied", text),
            () => toast("er", "Couldn't copy", text),
        );
    };

    // the actions every kind of result offers
    const problemHit = (p: DsaProblem, kind = "Problem", extra: Partial<Hit> = {}): Hit => {
        const st = o ? statusOf(p, o.today) : "new";
        const pat = o?.patterns.find((x) => x.code === p.pattern);
        const open = () => nav({ to: "/d/$slug", params: { slug: p.slug } });
        return {
            key: `d-${p.id}`,
            label: `${p.number} · ${p.title}`,
            kind,
            glyph: p.difficulty[0]!.toUpperCase(),
            color: DIFF_COL[p.difficulty] ?? "var(--acc)",
            sub: `${pat?.name ?? ""} · ${{ new: "not started", retry: "attempted", due: "review due", solved: "solved" }[st]}`,
            problem: p,
            rows: [["Status", { new: "not attempted", retry: "attempted", due: "review due", solved: "solved" }[st]], ["Last tried", p.state.last_review ? niceDate(p.state.last_review.slice(0, 10), o?.today ?? "") : "never"], ["Next review", p.state.due ? niceDate(p.state.due, o?.today ?? "") : "none"], ...(p.companies.length ? [["Asked by", p.companies.slice(0, 3).map((c) => c.name).join(", ") + (p.companies.length > 3 ? ` +${p.companies.length - 3}` : "")] as [string, string]] : [])],
            go: close(open, `d-${p.id}`),
            acts: [
                { label: "Open the problem page", run: close(open, `d-${p.id}`) },
                { label: "Open on LeetCode", run: close(() => window.open(leetcode(p.slug), "_blank", "noopener")) },
                { label: "Log how it went…", run: () => { setLens(null); setQ(`log ${p.title}`); input.current?.focus(); } },
                { label: "Everything about it", run: () => { setLens({ type: "about", kind: "pro", id: p.id, title: p.title }); setQ(""); input.current?.focus(); } },
                { label: "Find similar", run: () => { setLens({ type: "similar", id: p.id, title: p.title }); setQ(""); input.current?.focus(); } },
                { label: "Show in the graph", run: close(() => nav({ to: "/dsa/graph", search: { node: `pro:${p.id}` } as never })) },
                { label: "Copy link", run: () => copy(`${location.origin}/d/${p.slug}`) },
            ],
            ...extra,
        };
    };

    const all = useMemo<Hit[]>(() => {
        const page = (key: string, label: string, to: string, sub?: string, glyph = "▦", rows?: [string, string][]): Hit => ({ key, label, kind: "Page", glyph, color: "var(--acc)", sub, rows, go: close(() => nav({ to: to as never }), key), acts: [{ label: "Open", run: close(() => nav({ to: to as never }), key) }, { label: "Open in a new tab", run: () => window.open(to, "_blank", "noopener") }] });
        const action = (key: string, label: string, run: () => unknown, sub?: string): Hit => ({ key, label, kind: "Action", glyph: ">", color: "var(--mut)", sub, go: close(run, key), acts: [{ label: "Run", run: close(run, key) }] });
        const hits: Hit[] = [
            page("p-dsa", "DSA problems", "/dsa", o ? `${o.plan.goal_done} of ${o.plan.goal_total} done` : undefined, "▦", o ? [["Goal", `${o.plan.goal_done} of ${o.plan.goal_total} done`], ["Reviews due", String(o.plan.due)]] : undefined),
            page("p-plan", "Plan", "/dsa/plan", o?.settings.target_date ? `finish by ${niceDate(o.settings.target_date, o.today)}` : undefined, "▦", o ? [["Today", o.plan.solve_day ? "a solve day" : "no new problem"], ["New a week", String(o.settings.new_days.length * o.settings.new_per_day)], ["Finish by", o.settings.target_date ? niceDate(o.settings.target_date, o.today) : "not set"]] : undefined),
            page("p-review", "Review", "/dsa/review", o ? `${o.plan.due} due` : undefined, "↺", o ? [["Due", String(o.plan.due)], ["Overdue", String(o.plan.overdue)], ["Room today", String(o.plan.capacity)]] : undefined),
            page("p-cal", "Calendar", "/dsa/calendar", "the plan by day, streaks"),
            page("p-company", "Interview mode (a company and a day)", "/dsa/company"),
            page("p-mock", "Mock interview", "/dsa/mock", "a timed round"),
            page("p-graph", "Graph: where to start, looks like, path", "/dsa/graph", "map, where to start, looks like, path", "◎"),
            page("p-read", "Read: problems to remember", "/dsa/read", "the problem, your recall, then the idea"),
            page("p-sd", "System design", "/sd", sd.data ? `${sd.data.due} cards due` : undefined),
            page("p-sd-learn", "System design: Learn", "/sd/learn", sd.data ? `${sd.data.concepts.length} pages` : undefined),
            page("p-sd-problems", "System design: Problems", "/sd/problems", sd.data ? `${sd.data.problems.length} drills` : undefined),
            page("p-sd-review", "System design: Review", "/sd/review", sd.data ? `${sd.data.due} due` : undefined, "↺"),
            page("p-rust", "Rust tracks", "/rust"),
            page("p-courses", "BusTub course", "/courses"),
            page("p-progress", "Progress", "/progress"),
            page("p-analytics", "Analytics: the VPS and the app", "/analytics", "CPU, memory, containers, the database"),
            page("p-log", "Quick log (phone)", "/log"),
            action("a-timer", "Start or pause the focus timer", () => toggleTimer(), "the same timer as the header"),
            ...ACCENTS.filter((a) => a.id !== "custom").map((a) => action(`a-accent-${a.id}`, `Accent: ${a.label}`, () => appearance.set(a.id))),
            action("a-settings", "Settings", () => nav({ to: "/settings" })),
            action("a-help", "Keyboard shortcuts", () => window.dispatchEvent(new Event("anneal:help"))),
            action("a-backup", "Download a backup of everything", () => window.location.assign("/api/export")),
            action("a-theme", "Change the theme (dark, light, system)", () => applyTheme(THEMES[(THEMES.indexOf(storedTheme()) + 1) % THEMES.length]!)),
            { key: "a-did", label: "What I did", kind: "Action", glyph: ">", color: "var(--mut)", sub: "this session's logs, with Undo", go: () => { setLens({ type: "did" }); setQ(""); }, acts: [{ label: "Show", run: () => { setLens({ type: "did" }); setQ(""); } }] },
        ];
        if (o) {
            const p = o.plan;
            const next = o.problems.find((x) => x.id === p.next_up[0]);
            if (p.due > 0) hits.unshift({ key: "n-review", label: `Review ${p.due} due problem${p.due === 1 ? "" : "s"}`, kind: "Now", glyph: "↺", color: "var(--vio)", sub: p.overdue ? `${p.overdue} overdue · the most forgotten first` : "today", rows: [["Due", String(p.due)], ["Overdue", String(p.overdue)], ["Room today", String(p.capacity)]], go: close(() => nav({ to: "/dsa/review" }), "n-review") });
            if (p.solve_day && next && counted(o, next)) {
                const n = newOn(o.settings, WEEKDAYS[(new Date(`${o.today}T00:00`).getDay() + 6) % 7] ?? "mon");
                hits.unshift({ ...problemHit(next, "Now"), key: "n-next", label: `Today's problem: ${next.number} · ${next.title}`, glyph: "→", color: "var(--acc)", sub: `next up in your plan · ${next.difficulty} · ${n} today` });
            }
            for (const t of o.techniques) {
                const pat = o.patterns.find((x) => x.name === t.pattern);
                if (!pat) continue;
                const ps = o.problems.filter((x) => x.technique === t.id && x.list_tag);
                const open = () => nav({ to: "/dsa/patterns/$code", params: { code: pat.code }, hash: `t-${t.id.split(":")[1] ?? t.id}` });
                hits.push({
                    key: `te-${t.id}`,
                    label: t.name,
                    kind: "Technique",
                    glyph: "◆",
                    color: "var(--vio)",
                    sub: `${t.pattern} · ${ps.filter((x) => x.state.solved).length}/${ps.length} solved`,
                    rows: [["Pattern", t.pattern], ["Problems", String(ps.length)], ["Solved", String(ps.filter((x) => x.state.solved).length)]],
                    go: close(open, `te-${t.id}`),
                    acts: [
                        { label: "Open its card", run: close(open, `te-${t.id}`) },
                        ...(pat.has_idea ? [{ label: "Watch it run (Idea)", run: close(() => nav({ to: "/dsa/patterns/$code/idea", params: { code: pat.code } })) }] : []),
                        { label: "Everything about it", run: () => { setLens({ type: "about", kind: "tec", id: t.id, title: t.name }); setQ(""); } },
                        { label: "Find similar", run: () => { setLens(null); setQ(`~ ${t.name}`); input.current?.focus(); } },
                        { label: "Show in the graph", run: close(() => nav({ to: "/dsa/graph", search: { node: `tec:${t.id}` } as never })) },
                    ],
                });
            }
            const asked = new Map<string, DsaProblem[]>();
            for (const x of o.problems) for (const c of x.companies) asked.set(c.name, [...(asked.get(c.name) ?? []), x]);
            for (const [name, ps] of [...asked.entries()].sort((a, b) => b[1].length - a[1].length).slice(0, 60)) {
                const done = ps.filter((x) => x.state.solved).length;
                hits.push({
                    key: `co-${name}`,
                    label: name,
                    kind: "Company",
                    glyph: "▲",
                    color: "var(--fg)",
                    sub: `${ps.length} asked · ${Math.round((done / ps.length) * 100)}% done`,
                    rows: [["Asked", String(ps.length)], ["Solved", String(done)], ["Last six months", String(ps.filter((x) => x.companies.some((c) => c.name === name && c.recent)).length)]],
                    go: close(() => nav({ to: "/dsa/company" }), `co-${name}`),
                    acts: [
                        { label: "Open interview mode", run: close(() => nav({ to: "/dsa/company" }), `co-${name}`) },
                        { label: "Everything about it", run: () => { setLens({ type: "about", kind: "co", id: name, title: name }); setQ(""); } },
                        { label: "Show in the graph", run: close(() => nav({ to: "/dsa/graph", search: { node: `co:${name}` } as never })) },
                    ],
                });
            }
            for (const x of o.problems) hits.push(problemHit(x));
        }
        for (const c of sd.data?.concepts ?? []) {
            const go = close(() => nav({ to: "/sd/learn/$id", params: { id: c.id } }), `sd-${c.id}`);
            hits.push({ key: `sd-${c.id}`, label: c.title, kind: "Concept", glyph: "◆", color: "var(--acc)", sub: `system design · ${c.minutes} min · ${c.checked}/${c.checks} checks`, rows: [["Reading", `${c.minutes} min`], ["Checks marked", `${c.checked} of ${c.checks}`]], go, acts: [{ label: "Open", run: go }, { label: "Open in a new tab", run: () => window.open(`/sd/learn/${c.id}`, "_blank", "noopener") }] });
        }
        for (const d of sd.data?.problems ?? []) {
            const go = close(() => nav({ to: "/sd/practice/$id", params: { id: d.id } }), `sdp-${d.id}`);
            hits.push({ key: `sdp-${d.id}`, label: d.title, kind: "Drill", glyph: "▶", color: "var(--vio)", sub: `system design · ${d.minutes} min${d.best != null ? ` · best ${Math.round(d.best * 100)}%` : ""}`, rows: [["Rubric points", String(d.points)], ["Attempts", String(d.attempts)], ["Asked at", d.companies.slice(0, 3).join(", ")]], go, acts: [{ label: "Start the drill", run: go }] });
        }
        for (const t of tracks.data ?? []) hits.push({ key: `t-${t.slug}`, label: `${t.code} · ${t.name}`, kind: "Track", glyph: "R", color: "var(--vio)", go: close(() => nav({ to: "/t/$track", params: { track: t.slug } }), `t-${t.slug}`) });
        for (const p of course.data?.projects ?? [])
            for (const m of p.modules)
                for (const s of m.stages)
                    hits.push({ key: `s-${s.id}`, label: `${s.id} · ${s.title}`, kind: s.kind === "challenge" ? "Challenge" : "Stage", glyph: s.kind === "challenge" ? "★" : "§", color: "var(--grn)", go: close(() => nav({ to: "/courses/$course/$stage", params: { course: "bustub", stage: s.id } }), `s-${s.id}`) });
        return hits;
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [tracks.data, o, course.data, sd.data, recent, appearance]);

    const didHits = (rows: Did[]): Hit[] => {
        const latest = new Map<string, number>();
        for (const d of rows) if (!latest.has(d.id)) latest.set(d.id, d.at);
        return rows.map((d) => {
            const key = `did-${d.id}-${d.at}`;
            const can = !d.undone && latest.get(d.id) === d.at && Date.now() - d.at < UNDO_WINDOW;
            const asking = confirmUndo === key;
            const ask = () => (asking ? undo(d) : setConfirmUndo(key));
            return { key, label: `${d.undone ? "Undone" : "Logged"} ${d.title}: ${d.label}`, kind: "Did", glyph: d.undone ? "↶" : "✓", color: d.undone ? "var(--dim)" : "var(--grn)", sub: asking ? "press ↵ again to undo it" : `${ago(d.at)}${can ? " · ↵ to undo" : ""}`, go: can ? ask : () => {}, acts: can ? [{ label: asking ? "Yes, undo it" : "Undo", run: ask }] : [] };
        });
    };

    const lensHits = useMemo<Hit[] | null>(() => {
        if (!lens) return null;
        if (lens.type === "did") return didHits(did);
        if (!o) return [];
        const meaningHits = (data: typeof similar.data, skip?: string) =>
            (data?.hits ?? [])
                .map((h) => ({ h, p: o.problems.find((x) => x.id === h.id) }))
                .filter((x): x is { h: { id: string; score: number }; p: DsaProblem } => !!x.p && counted(o, x.p) && x.p.id !== skip)
                .slice(0, 8)
                .map(({ h, p }) => problemHit(p, "Alike", { key: `al-${p.id}`, glyph: "≈", sub: `${h.score.toFixed(2)} · ${p.difficulty} · ${o.patterns.find((x) => x.code === p.pattern)?.name ?? ""}` }));
        if (lens.type === "similar") return meaningHits(similar.data, lens.id);
        const out: Hit[] = [];
        const as = (h: Hit, kind: string) => ({ ...h, kind, key: `${kind}-${h.key}` });
        if (lens.kind === "pro") {
            const p = o.problems.find((x) => x.id === lens.id);
            if (!p) return [];
            const tec = all.find((h) => h.key === `te-${p.technique}`);
            if (tec) out.push(as(tec, "Technique"));
            for (const c of p.companies.slice(0, 8)) {
                const h = all.find((x) => x.key === `co-${c.name}`);
                if (h) out.push(as(h, "Company"));
            }
            out.push(...meaningHits(similar.data, p.id).slice(0, 5));
            const tagged = o.problems
                .filter((x) => x.id !== p.id && counted(o, x))
                .map((x) => ({ x, k: x.tags.filter((t) => p.tags.includes(t)) }))
                .filter((v) => v.k.length > 0)
                .sort((a, b) => b.k.length - a.k.length || a.x.order - b.x.order)
                .slice(0, 5);
            for (const { x, k } of tagged) out.push(problemHit(x, "Matching", { key: `tg-${x.id}`, sub: `${k.length} shared tags: ${k.join(", ")}` }));
        } else if (lens.kind === "tec") {
            for (const x of o.problems.filter((x) => x.technique === lens.id && counted(o, x)).sort((a, b) => a.order - b.order)) out.push(problemHit(x));
        } else {
            for (const x of o.problems
                .filter((x) => counted(o, x) && x.companies.some((c) => c.name === lens.id))
                .sort((a, b) => (b.companies.find((c) => c.name === lens.id)?.frequency ?? 0) - (a.companies.find((c) => c.name === lens.id)?.frequency ?? 0))
                .slice(0, 25))
                out.push(problemHit(x));
        }
        return out;
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [lens, did, o, all, similar.data, confirmUndo]);

    const read = useMemo(() => (o && raw && !/^[>~#]/.test(raw) && !raw.toLowerCase().startsWith("log ") ? readAs(raw, o) : null), [o, raw]);

    const shown = useMemo<Hit[]>(() => {
        if (lensHits) return lensHits;
        const low = raw.toLowerCase();
        if (!low) {
            const byKey = new Map(all.map((h) => [h.key, h]));
            const rec = recent.map((k) => byKey.get(k)).filter((h): h is Hit => !!h).map((h) => ({ ...h, kind: "Recent" }));
            const now = all.filter((h) => h.kind === "Now");
            const mine = didHits(did).slice(0, 3);
            return [...now, ...mine, ...rec, ...all.filter((h) => (h.kind === "Page" || h.kind === "Action") && !rec.some((r) => r.key === h.key))].slice(0, 20);
        }
        // "log two sum": the four outcomes for the best matches, logged straight from here
        const words0 = low.split(/\s+/);
        if (words0[0] === "log" && words0.length > 1 && o) {
            const rest = words0.slice(1);
            const hits: Hit[] = [];
            const matches = o.problems.map((p) => ({ p, s: score(`${p.title} ${p.number}`, rest) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).slice(0, 2);
            for (const { p } of matches)
                for (const [grade, label] of OUTCOMES) hits.push({ key: `log-${p.id}-${grade}`, label: `Log ${p.title}: ${label}`, kind: "Log", glyph: "✓", color: "var(--acc)", sub: `${p.number} · ${p.difficulty}`, go: () => log(p, grade, label) });
            if (hits.length) return hits;
        }
        // "~ words": by meaning; while the model isn't there, by the tags of the best title match
        if (low.startsWith("~")) {
            if (!tilde) return [];
            if (meaning.data?.phase === "ready" && meaningText === tilde) {
                return meaning.data.hits
                    .map((h) => ({ h, p: o?.problems.find((x) => x.id === h.id) }))
                    .filter((x): x is { h: { id: string; score: number }; p: DsaProblem } => !!x.p && !!o && counted(o, x.p))
                    .slice(0, 12)
                    .map(({ h, p }) => problemHit(p, "Alike", { key: `al-${p.id}`, glyph: "≈", sub: `${h.score.toFixed(2)} · ${p.difficulty} · ${o!.patterns.find((x) => x.code === p.pattern)?.name ?? ""}` }));
            }
            if (meaning.isError || (meaning.data && meaning.data.phase !== "ready")) {
                const rest = tilde.toLowerCase().split(/\s+/);
                const ps = o?.problems ?? [];
                const base = ps.map((p) => ({ p, s: score(`${p.title} ${p.number}`, rest) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s)[0]?.p;
                if (base)
                    return ps
                        .filter((p) => p.id !== base.id)
                        .map((p) => ({ p, k: p.tags.filter((t) => base.tags.includes(t)) }))
                        .filter((x) => x.k.length > 0)
                        .sort((a, b) => b.k.length - a.k.length || a.p.order - b.p.order)
                        .slice(0, 12)
                        .map(({ p, k }) => problemHit(p, "Alike", { key: `al-${p.id}`, glyph: "≈", sub: `${k.length} tags shared with ${base.title}` }));
            }
            return [];
        }
        const actionsOnly = low.startsWith(">");
        const text = actionsOnly ? low.slice(1).trim() : low.replace(/^#/, "");
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
        // read as filters: the matching problems first, and a way to open them in the list
        const readHits: Hit[] = read
            ? [
                  { key: "read-open", label: `Open these ${read.match.length} in the problem list`, kind: "Read", glyph: "≡", color: "var(--acc)", sub: read.parts.map(([, v]) => v).join(" · "), go: close(() => { window.location.assign(`/dsa?${read.search}`); }), acts: [{ label: "Open the list with these filters", run: close(() => window.location.assign(`/dsa?${read.search}`)) }] },
                  ...read.match.slice(0, 8).map((p) => problemHit(p, "Matching", { key: `m-${p.id}` })),
              ]
            : [];
        // grouped, each group keeping the best-first order inside it
        return [...readHits, ...GROUPS.filter((g) => g !== "Read" && g !== "Matching").flatMap((g) => scored.filter((h) => h.kind === g))];
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [all, raw, recent, o, lensHits, read, meaning.data, meaning.isError, meaningText, tilde, did]);

    useEffect(() => {
        setSel(0);
    }, [q, lens]);
    useEffect(() => {
        list.current?.querySelector<HTMLElement>("li .hd-sel")?.scrollIntoView({ block: "nearest" });
    }, [sel, shown]);
    const words = raw.toLowerCase().replace(/^[>#~]/, "").split(/\s+/).filter(Boolean);
    const counts = new Map<string, number>();
    for (const h of shown) counts.set(h.kind, (counts.get(h.kind) ?? 0) + 1);
    const cur = shown[sel];
    const run = (h: Hit | undefined, i = 0) => {
        if (!h) return;
        const a = h.acts?.[i];
        if (a) a.run();
        else if (i === 0) h.go();
    };

    // the line under the input: how the words were read
    const readLine: ReactNode = lens ? (
        lens.type === "did" ? <>What I did this session: <b>logs made here</b>, with Undo for ten minutes. <kbd>esc</kbd> goes back.</>
        : lens.type === "similar" ? <>Problems that mean the same as <b>{lens.title}</b>{similar.data?.phase === "ready" ? <>: {similar.data.compared.toLocaleString()} compared by a small local model, exact.</> : similar.isError || similar.data ? <>: meaning search is off on this server.</> : "…"} <kbd>esc</kbd> goes back.</>
        : <>Everything about <b>{lens.title}</b>. <kbd>esc</kbd> goes back.</>
    ) : raw.startsWith("~") ? (
        tilde ? <>Searching by meaning for <b>“{tilde}”</b>{meaning.data?.phase === "ready" ? <>: vectors from a small local model, every one of {meaning.data.compared.toLocaleString()} problems compared.</> : meaning.isError || meaning.data ? <>: the model isn't loaded, so this ranks by the tags of the best title match.</> : "…"}</> : <>Type what a problem is about: <b>~ two numbers that add up to a target</b></>
    ) : read ? (
        <>Read as: <b>Problems</b>{read.parts.map(([k, v]) => <span key={k}> · {k} <b>{v}</b></span>)} → {read.match.length} problem{read.match.length === 1 ? "" : "s"}.{read.leftover ? "" : " Press ↵ to open the list with these filters."}</>
    ) : null;

    const onActsKey = (e: ReactKeyboardEvent<HTMLDivElement>) => {
        const btns = [...(acts.current?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
        const at = btns.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === "ArrowDown") {
            e.preventDefault();
            btns[Math.min(at + 1, btns.length - 1)]?.focus();
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            if (at <= 0) input.current?.focus();
            else btns[at - 1]?.focus();
        } else if ((e.key === "Tab" && e.shiftKey) || e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            input.current?.focus();
        }
    };

    const p = cur?.problem;
    return (
        <div className="hd-pal-wrap" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="hd-pal pk" ref={dialog} role="dialog" aria-label="Search" aria-modal="true">
                <div className="pk-in">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" /></svg>
                    {lens && <button className="pk-lens" onClick={() => setLens(null)} title="Back to the search">{lens.type === "did" ? "What I did" : lens.type === "similar" ? `≈ ${lens.title}` : lens.title} ×</button>}
                    <input
                        ref={input}
                        value={q}
                        onChange={(e) => { setQ(e.target.value); if (lens) setLens(null); }}
                        placeholder={lens ? "" : "Search or ask: problems, techniques, companies, pages, actions…"}
                        aria-label="Search"
                        autoComplete="off"
                        spellCheck={false}
                        onKeyDown={(e) => {
                            if (e.key === "Escape") {
                                e.preventDefault();
                                if (lens) setLens(null);
                                else onClose();
                            } else if (e.key === "Backspace" && !q && lens) setLens(null);
                            else if (e.key === "ArrowDown") {
                                e.preventDefault();
                                setSel((s) => Math.min(s + 1, shown.length - 1));
                                setConfirmUndo(null);
                            } else if (e.key === "ArrowUp") {
                                e.preventDefault();
                                setSel((s) => Math.max(s - 1, 0));
                                setConfirmUndo(null);
                            } else if (e.key === "PageDown") {
                                e.preventDefault();
                                setSel((s) => Math.min(s + 6, shown.length - 1));
                            } else if (e.key === "PageUp") {
                                e.preventDefault();
                                setSel((s) => Math.max(s - 6, 0));
                            } else if (e.key === "Tab" && !e.shiftKey && cur?.acts?.length) {
                                e.preventDefault();
                                acts.current?.querySelector<HTMLButtonElement>("button")?.focus();
                            } else if (e.key === "Enter") {
                                e.preventDefault();
                                run(cur, e.metaKey || e.ctrlKey ? 1 : 0);
                            }
                        }}
                    />
                    <kbd>esc</kbd>
                </div>
                {readLine && <div className="pk-read" role="status">{readLine}</div>}
                <div className="hd-main pk-body">
                    <ul role="listbox" ref={list} aria-label="Results">
                        {shown.map((h, i) => (
                            <li key={`${h.kind}-${h.key}-${i}`} role="presentation">
                                {(i === 0 || shown[i - 1]!.kind !== h.kind) && h.kind !== "About" && (
                                    <span className="hd-grp">{HEADINGS[h.kind] ?? h.kind.toUpperCase()}<em>{counts.get(h.kind)}</em></span>
                                )}
                                <div role="option" aria-selected={i === sel} className={`hd-opt pk-opt${i === sel ? " hd-sel" : ""}`} style={{ "--c": h.color } as CSSProperties} onMouseEnter={() => setSel(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => run(h)}>
                                    <span className="pk-ico" aria-hidden="true">{h.glyph}</span>
                                    <span className="pk-txt">
                                        <span className="hd-lab">{marked(h.label, words)}</span>
                                        {h.sub && <small>{h.sub}</small>}
                                    </span>
                                    <em className="pk-kind">{KIND_WORD[h.kind] ?? h.kind.toLowerCase()}</em>
                                    {i === sel && <kbd className="hd-enter" aria-hidden="true">↵</kbd>}
                                </div>
                            </li>
                        ))}
                        {shown.length === 0 && (
                            <li className="hd-none">
                                {raw.startsWith("~") && tilde && (meaning.isFetching || meaningText !== tilde) ? "Comparing every problem…" : lens?.type === "did" ? "Nothing logged from here this session." : lens?.type === "similar" && similar.isFetching ? "Comparing every problem…" : <>No match. Try fewer letters, a LeetCode number, <b>&gt;</b> for actions or <b>~</b> to search by meaning.</>}
                            </li>
                        )}
                    </ul>
                    {cur && (
                        <aside className="hd-prev pk-prev" aria-label="Preview" style={{ "--c": cur.color } as CSSProperties}>
                            <span className="pk-pk">{KIND_WORD[cur.kind] || cur.kind.toLowerCase()}</span>
                            <b className="t">{cur.label}</b>
                            {cur.sub && <p className="pk-sub">{cur.sub}</p>}
                            {p && (
                                <div className="pk-chips">
                                    <span style={{ color: DIFF_COL[p.difficulty] }}>{p.difficulty}</span>
                                    {p.priority === "must" && <span>must learn</span>}
                                    {p.list_tag && p.list_tag !== "all" && <span>{{ blind75: "Blind 75", neetcode150: "NeetCode 150", neetcode250: "NeetCode 250" }[p.list_tag] ?? p.list_tag}</span>}
                                    {p.recent && <span>asked recently</span>}
                                    {p.premium && <span>premium</span>}
                                </div>
                            )}
                            {p && showIdeas && p.insight && (
                                <div className="pk-idea">
                                    <span>Key idea</span>
                                    {p.insight}
                                </div>
                            )}
                            {cur.rows && (
                                <dl>
                                    {cur.rows.map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}
                                </dl>
                            )}
                            {cur.acts && cur.acts.length > 0 && (
                                <div className="pk-acts" ref={acts} onKeyDown={onActsKey} role="group" aria-label="Actions">
                                    {cur.acts.map((a, i) => (
                                        <button key={a.label} onMouseDown={(e) => e.preventDefault()} onClick={a.run}>
                                            {a.label}
                                            {i === 0 ? <kbd>↵</kbd> : i === 1 ? <kbd>⌘↵</kbd> : null}
                                        </button>
                                    ))}
                                </div>
                            )}
                            {cur.acts && cur.acts.length > 1 && <span className="go"><kbd>Tab</kbd> opens these actions</span>}
                        </aside>
                    )}
                </div>
                <div className="hd-hint pk-foot">
                    <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
                    <span><kbd>↵</kbd> open</span>
                    <span><kbd>Tab</kbd> actions</span>
                    <span><kbd>&gt;</kbd> commands</span>
                    <span><kbd>#</kbd> number</span>
                    <span><kbd>~</kbd> by meaning</span>
                    <span>log two sum</span>
                </div>
            </div>
        </div>
    );
}
