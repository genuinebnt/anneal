import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { ACCENTS, useAppearance, usePageWidth } from "../settings";
import { isRustSection } from "../curriculum";
import { StatsBar } from "./StatsBar";
import { toast } from "./toasts";
import { THEMES, applyTheme, storedTheme, type Theme } from "../theme";

// The header has two tiers (docs/mockups/navbar-v2.html). The first answers "where in the product am I": the three areas as one switcher
// with their progress, a search field, Continue, and the tools. The second answers "what can I do here": the pages of the current area,
// a breadcrumb and a few counters. On a phone the areas move to a bar at the bottom.

type AreaId = "dsa" | "rust" | "courses";

const AREAS: { id: AreaId; label: string; to: "/dsa" | "/rust" | "/courses"; color: string }[] = [
    { id: "dsa", label: "DSA", to: "/dsa", color: "var(--acc)" },
    { id: "rust", label: "Rust", to: "/rust", color: "var(--vio)" },
    { id: "courses", label: "Courses", to: "/courses", color: "var(--grn)" },
];

type Tab = { to: "/dsa" | "/dsa/plan" | "/dsa/review" | "/dsa/calendar" | "/dsa/mock" | "/progress" | "/rust" | "/courses"; label: string; match: (path: string) => boolean; badge?: "due" };

const TABS: Record<AreaId, Tab[]> = {
    dsa: [
        { to: "/dsa", label: "Problems", match: (p) => p === "/dsa" || p.startsWith("/dsa/patterns") || p.startsWith("/dsa/practice") || p.startsWith("/d/") },
        { to: "/dsa/plan", label: "Plan", match: (p) => p.startsWith("/dsa/plan") },
        { to: "/dsa/review", label: "Review", match: (p) => p.startsWith("/dsa/review"), badge: "due" },
        { to: "/dsa/calendar", label: "Calendar", match: (p) => p.startsWith("/dsa/calendar") },
        { to: "/dsa/mock", label: "Mock interview", match: (p) => p.startsWith("/dsa/mock") },
        { to: "/progress", label: "Progress", match: (p) => p.startsWith("/progress") },
    ],
    rust: [
        { to: "/rust", label: "Tracks", match: (p) => p.startsWith("/rust") || p.startsWith("/t/") || p.startsWith("/p/") },
        { to: "/progress", label: "Progress", match: (p) => p.startsWith("/progress") },
    ],
    courses: [
        { to: "/courses", label: "BusTub", match: (p) => p.startsWith("/courses") },
        { to: "/progress", label: "Progress", match: (p) => p.startsWith("/progress") },
    ],
};

/** Pages that are designed but not built yet; dimmed, in the area they belong to. */
const LATER: Record<AreaId, string[]> = { dsa: [], rust: ["Library"], courses: ["Readiness"] };

const AREA_KEY = "anneal.area";
const areaFromPath = (path: string): AreaId | null => {
    if (path.startsWith("/courses")) return "courses";
    if (path.startsWith("/rust") || path.startsWith("/t/") || path.startsWith("/p/")) return "rust";
    if (path.startsWith("/dsa") || path.startsWith("/d/")) return "dsa";
    return null;
};

/** Centred or wide pages: a two-state switch beside the theme button. The same setting is in the account menu. */
function WidthToggle() {
    const { width, set } = usePageWidth();
    const wide = width === "wide";
    return (
        <button className={`hd-ib wdt${wide ? " wide" : ""}`} onClick={() => set(wide ? "narrow" : "wide")} aria-pressed={wide} title={wide ? "Wide pages: click for centred" : "Centred pages: click for wide"} aria-label="Page width">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <rect x="1.5" y="3" width="13" height="10" rx="2" />
                <rect className="pane" x="5" y="5.5" width="6" height="5" rx="1" fill="currentColor" stroke="none" />
            </svg>
        </button>
    );
}

/** A small ring: how far along an area is. */
function Ring({ p, color }: { p: number; color: string }) {
    return (
        <svg className="hd-ring" viewBox="0 0 20 20" style={{ "--c": color, "--p": Math.max(0, Math.min(1, p)) } as React.CSSProperties} aria-hidden="true">
            <circle className="bg" cx="10" cy="10" r="8" />
            <circle className="fg" cx="10" cy="10" r="8" />
        </svg>
    );
}

export function Header({ area, compact = false, stats = true }: { area?: AreaId; compact?: boolean; stats?: boolean }) {
    const path = useRouterState({ select: (s) => s.location.pathname });
    const [theme, setTheme] = useState<Theme>(storedTheme);
    const cycleTheme = () => {
        const next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]!;
        applyTheme(next);
        setTheme(next);
    };
    const detected = areaFromPath(path);
    const stored = (() => {
        try {
            return localStorage.getItem(AREA_KEY) as AreaId | null;
        } catch {
            return null;
        }
    })();
    const current: AreaId = area ?? detected ?? (stored && TABS[stored] ? stored : "dsa");
    useEffect(() => {
        try {
            localStorage.setItem(AREA_KEY, current);
        } catch {
            // storage may be refused; the area is then picked from the page each time
        }
    }, [current]);

    const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const course = useQuery({ queryKey: ["course", "bustub"], queryFn: () => api.course("bustub") });
    const progress = useQuery({ queryKey: ["progress"], queryFn: api.progress });
    const activity = useQuery({ queryKey: ["activity", "header"], queryFn: () => api.activity(["D"]), enabled: current === "dsa" });
    const [paletteOpen, setPaletteOpen] = useState(false);

    const rust = useMemo(() => {
        const t = (tracks.data ?? []).filter((x) => isRustSection(x.section)); // the DSA tracks live in the DSA area
        const total = t.reduce((n, x) => n + x.total, 0);
        const solved = t.reduce((n, x) => n + x.solved, 0);
        return { total, solved };
    }, [tracks.data]);
    const dsaDone = dsa.data?.plan.goal_done ?? 0;
    const dsaTotal = dsa.data?.plan.goal_total ?? 0;
    const due = dsa.data?.plan.due ?? 0;
    const counts: Record<AreaId, { done: number | undefined; total: number }> = {
        dsa: { done: dsa.data ? dsaDone : undefined, total: dsaTotal },
        rust: { done: tracks.data ? rust.solved : undefined, total: rust.total },
        courses: { done: course.data?.done, total: course.data?.total ?? 0 },
    };

    // The page measures itself against the header: layouts that stick below it read --hdr-h.
    const box = useRef<HTMLElement>(null);
    useLayoutEffect(() => {
        const el = box.current;
        if (!el) return;
        const set = () => document.documentElement.style.setProperty("--hdr-h", `${el.offsetHeight}px`);
        set();
        const ro = new ResizeObserver(set);
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    useEffect(() => {
        const open = (e: KeyboardEvent) => {
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
                e.preventDefault();
                setPaletteOpen(true);
            }
        };
        addEventListener("keydown", open);
        return () => removeEventListener("keydown", open);
    }, []);

    const meta = AREAS.find((a) => a.id === current)!;
    return (
        <>
            <header className={`hdr${compact ? " compact" : ""}`} ref={box} data-area={current}>
                <div className="hd-t1">
                    <div className="hd-l">
                    <Link className="logo" to="/" aria-label="anneal home">
                        <span className="mk">
                            <span style={{ background: "var(--acc)" }} />
                            <span style={{ background: "var(--vio)" }} />
                            <span style={{ background: "var(--grn)" }} />
                            <span style={{ boxShadow: "inset 0 0 0 1px var(--line)" }} />
                        </span>
                        <b>
                            anneal<i>.genuinebasil.dev</i>
                        </b>
                    </Link>
                    <AreaSwitcher current={current} counts={counts} />
                    </div>
                    <button className="hd-srch" onClick={() => setPaletteOpen(true)} aria-label="Search" aria-keyshortcuts="Control+K Meta+K">
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
                            <circle cx="7" cy="7" r="4.5" />
                            <path d="M10.5 10.5 14 14" />
                        </svg>
                        <span>Search tracks, stages, concepts, problems…</span>
                        <kbd>⌘K</kbd>
                    </button>
                    <div className="hdr-r">
                        <Continue current={current} course={course.data} dsa={dsa.data} />
                        <WidthToggle />
                        <button className="hd-ib thm" onClick={cycleTheme} title={`Theme: ${theme}. Click for ${THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]}`} aria-label={`Theme: ${theme}`}>
                            <svg className="moon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                                <path d="M13.5 9.6A6 6 0 0 1 6.4 2.5a6 6 0 1 0 7.1 7.1Z" />
                            </svg>
                            <svg className="sun" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
                                <circle cx="8" cy="8" r="3" />
                                <path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" />
                            </svg>
                        </button>
                        {theme === "system" && <span className="thm-auto" title="Following your device">auto</span>}
                        <Account />
                    </div>
                </div>
                {!compact && (
                    <div className="hd-t2">
                        <Crumb area={meta.label} path={path} tabs={TABS[current]} />
                        <SubTabs area={current} path={path} due={due} />
                        <span className="hd-sp" />
                        <div className="hd-chips">
                            {current === "dsa" && (
                                <>
                                    <span className="hd-chip">
                                        solved <span><b>{dsa.data ? dsaDone : "–"}</b>/{dsa.data ? dsaTotal : "–"}</span>
                                    </span>
                                    <span className="hd-chip">
                                        streak <b>{progress.data?.streak ?? "–"}</b> {progress.data?.streak === 1 ? "day" : "days"}
                                    </span>
                                    {dsa.data && activity.data && (() => {
                                        const plan = dsa.data.plan;
                                        const doneToday = activity.data.week[activity.data.week.length - 1]?.solved ?? 0;
                                        const target = plan.solve_day ? dsa.data.settings.new_per_day : 0;
                                        const full = target > 0 ? doneToday >= target : plan.due === 0;
                                        return (
                                            <Link to="/dsa/review" className={`hd-chip hd-today${full ? " full" : ""}`} title={plan.solve_day ? "New problems done today against today's plan, and the reviews still due" : "A practice or rest day: reviews only"}>
                                                today {target > 0 ? <span><b>{doneToday}</b>/{target} new</span> : <span>no new</span>}{plan.due > 0 ? <> · <b>{plan.due}</b> due</> : null}
                                            </Link>
                                        );
                                    })()}
                                </>
                            )}
                            {current === "rust" && (
                                <span className="hd-chip">
                                    problems <span><b>{tracks.data ? rust.solved : "–"}</b>/{tracks.data ? rust.total : "–"}</span>
                                </span>
                            )}
                            {current === "courses" && (
                                <>
                                    <span className="hd-chip">
                                        stages <span><b>{course.data?.done ?? "–"}</b>/{course.data?.total ?? "–"}</span>
                                    </span>
                                    <span className="hd-chip hd-beyond" title="Optional challenges, which are not part of BusTub">
                                        challenges <span><b>{course.data?.challenges_done ?? "–"}</b>/{course.data?.challenges ?? "–"}</span>
                                    </span>
                                </>
                            )}
                        </div>
                    </div>
                )}
            </header>
            {stats && !compact && <StatsBar area={current} />}
            <nav className="hd-dock" aria-label="Areas">
                {AREAS.map((a) => (
                    <Link key={a.id} to={a.to} className={a.id === current ? "on" : ""} style={{ "--c": a.color } as React.CSSProperties} aria-current={a.id === current ? "page" : undefined}>
                        <Ring p={ratio(counts[a.id])} color={a.color} />
                        {a.label}
                    </Link>
                ))}
            </nav>
            {paletteOpen && <Palette onClose={() => setPaletteOpen(false)} />}
        </>
    );
}

const ratio = (c: { done: number | undefined; total: number }) => (c.total > 0 && c.done !== undefined ? c.done / c.total : 0);

/** The three areas as one control: a solid thumb slides to the current one. */
function AreaSwitcher({ current, counts }: { current: AreaId; counts: Record<AreaId, { done: number | undefined; total: number }> }) {
    const box = useRef<HTMLElement>(null);
    const [thumb, setThumb] = useState<{ x: number; w: number } | null>(null);
    useLayoutEffect(() => {
        const place = () => {
            const on = box.current?.querySelector<HTMLElement>("a.on");
            if (on) setThumb({ x: on.offsetLeft, w: on.offsetWidth });
        };
        place();
        addEventListener("resize", place);
        return () => removeEventListener("resize", place);
    }, [current, counts.dsa.done, counts.rust.done, counts.courses.done]);
    const color = AREAS.find((a) => a.id === current)!.color;
    return (
        <nav className="hd-areas" ref={box} aria-label="Areas">
            {thumb && <span className="hd-thumb" style={{ transform: `translateX(${thumb.x}px)`, width: thumb.w, "--tc": color } as React.CSSProperties} />}
            {AREAS.map((a) => (
                <Link key={a.id} to={a.to} className={a.id === current ? "on" : ""} style={{ "--c": a.color } as React.CSSProperties} aria-current={a.id === current ? "page" : undefined}>
                    <Ring p={ratio(counts[a.id])} color={a.color} />
                    {a.label}
                    <small>{counts[a.id].done ?? "–"}</small>
                </Link>
            ))}
        </nav>
    );
}

/** Where you are: the area, the page, and for a course the stage or concept. */
function Crumb({ area, path }: { area: string; path: string; tabs: Tab[] }) {
    const parts = [area];
    const stage = path.match(/^\/courses\/[^/]+\/concept\/(.+)$/);
    const concept = stage?.[1];
    const st = path.match(/^\/courses\/[^/]+\/([^/]+)$/);
    if (concept) parts.push(concept);
    else if (st?.[1] && st[1] !== "concept") parts.push(st[1]);
    if (parts.length < 2) return null; // the area is already lit in the switcher, the tab below says the page
    return (
        <div className="hd-crumb" aria-label="You are here">
            {parts.map((p, i) => (
                <span key={i}>
                    {i > 0 && <i>/</i>}
                    {i === parts.length - 1 ? <b>{p}</b> : p}
                </span>
            ))}
        </div>
    );
}

/** The pages of the current area: an ink line glides under the current one. */
function SubTabs({ area, path, due }: { area: AreaId; path: string; due: number }) {
    const box = useRef<HTMLDivElement>(null);
    const [ink, setInk] = useState<{ x: number; w: number } | null>(null);
    const color = AREAS.find((a) => a.id === area)!.color;
    useLayoutEffect(() => {
        const place = () => {
            const on = box.current?.querySelector<HTMLElement>("a.on");
            setInk(on ? { x: on.offsetLeft, w: on.offsetWidth } : null);
        };
        place();
        addEventListener("resize", place);
        return () => removeEventListener("resize", place);
    }, [area, path, due]);
    return (
        <div className="hd-tabs" ref={box}>
            {ink && <span className="hd-ink" style={{ transform: `translateX(${ink.x}px)`, width: ink.w, background: color }} />}
            {TABS[area].map((t) => {
                const on = t.match(path);
                return (
                    <Link key={t.label} to={t.to} className={on ? "on" : ""} aria-current={on ? "page" : undefined}>
                        {t.label}
                        {t.badge === "due" && due > 0 && <span className="hd-n hd-hot">{due}</span>}
                    </Link>
                );
            })}
            {LATER[area].map((l) => (
                <span key={l} className="hd-later" title="Designed; not built yet">
                    {l}
                </span>
            ))}
        </div>
    );
}

/** One button for "take me back to what I was doing": the current course stage, or the next DSA problem. */
function Continue({ current, course, dsa }: { current: AreaId; course: Awaited<ReturnType<typeof api.course>> | undefined; dsa: Awaited<ReturnType<typeof api.dsa>> | undefined }) {
    if (current === "courses" && course?.current) {
        return (
            <Link className="hd-cta" to="/courses/$course/$stage" params={{ course: "bustub", stage: course.current }}>
                <span className="lbl">Continue</span>
                <small>{course.current}</small>
                <span aria-hidden="true">→</span>
            </Link>
        );
    }
    if (current === "dsa" && dsa) {
        if (dsa.plan.due > 0) {
            return (
                <Link className="hd-cta" to="/dsa/review">
                    <span className="lbl">Review</span>
                    <small>{dsa.plan.due} due</small>
                    <span aria-hidden="true">→</span>
                </Link>
            );
        }
        const id = dsa.plan.next_up[0];
        const next = id ? dsa.problems.find((p) => p.id === id) : undefined;
        if (next) {
            return (
                <Link className="hd-cta" to="/d/$slug" params={{ slug: next.slug }}>
                    <span className="lbl">Continue</span>
                    <small>{next.title}</small>
                    <span aria-hidden="true">→</span>
                </Link>
            );
        }
    }
    return null;
}

/** The avatar menu: accent colour, and Sign out when login is on. */
function Account() {
    const session = useQuery({ queryKey: ["session"], queryFn: api.session, staleTime: Infinity });
    const { accent, set } = useAppearance();
    const { width, set: setWidth } = usePageWidth();
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);
    useEffect(() => {
        if (!open) return;
        const close = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
        const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
        document.addEventListener("mousedown", close);
        document.addEventListener("keydown", esc);
        return () => {
            document.removeEventListener("mousedown", close);
            document.removeEventListener("keydown", esc);
        };
    }, [open]);
    return (
        <div className="acct" ref={box}>
            <button className="av" aria-haspopup="menu" aria-expanded={open} aria-label="Settings" onClick={() => setOpen(!open)} title="Settings">
                gb
            </button>
            {open && (
                <div className="acct-menu" role="menu">
                    <span className="acct-lab">ACCENT</span>
                    <div className="swatches" role="radiogroup" aria-label="Accent colour">
                        {ACCENTS.map((a) => (
                            <button key={a.id} role="radio" aria-checked={accent === a.id} className={`swatch${accent === a.id ? " on" : ""}`} data-swatch={a.id} onClick={() => set(a.id)} title={a.label}>
                                <span />
                                {a.label}
                            </button>
                        ))}
                    </div>
                    <span className="acct-lab">PAGE WIDTH</span>
                    <div className="acct-seg" role="radiogroup" aria-label="Page width">
                        {(["narrow", "wide"] as const).map((w) => (
                            <button key={w} role="radio" aria-checked={width === w} className={width === w ? "on" : ""} onClick={() => setWidth(w)} title={w === "narrow" ? "Main screens at a comfortable reading width" : "Main screens use more of the window"}>
                                {w === "narrow" ? "Narrow" : "Wide"}
                            </button>
                        ))}
                    </div>
                    <Link role="menuitem" className="acct-out" to="/settings" onClick={() => setOpen(false)}>
                        All settings
                    </Link>
                    {session.data?.required && (
                        <button
                            role="menuitem"
                            className="acct-out"
                            onClick={async () => {
                                await api.logout().catch(() => undefined);
                                location.assign("/login");
                            }}
                        >
                            Sign out
                        </button>
                    )}
                </div>
            )}
        </div>
    );
}

type Hit = { key: string; label: string; kind: string; color: string; go: () => void };

/** Search: pages, tracks, course stages and DSA problems by name. It reads what the app already loads. */
function Palette({ onClose }: { onClose: () => void }) {
    const nav = useNavigate();
    const qc = useQueryClient();
    const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const course = useQuery({ queryKey: ["course", "bustub"], queryFn: () => api.course("bustub") });
    const [q, setQ] = useState("");
    const [sel, setSel] = useState(0);
    const input = useRef<HTMLInputElement>(null);
    useEffect(() => input.current?.focus(), []);

    const all = useMemo<Hit[]>(() => {
        const go = (fn: () => unknown) => () => {
            onClose();
            void fn();
        };
        const hits: Hit[] = [
            { key: "p-dsa", label: "DSA problems", kind: "Page", color: "var(--acc)", go: go(() => nav({ to: "/dsa" })) },
            { key: "p-plan", label: "Plan", kind: "Page", color: "var(--acc)", go: go(() => nav({ to: "/dsa/plan" })) },
            { key: "p-review", label: "Review", kind: "Page", color: "var(--acc)", go: go(() => nav({ to: "/dsa/review" })) },
            { key: "p-cal", label: "Calendar", kind: "Page", color: "var(--acc)", go: go(() => nav({ to: "/dsa/calendar" })) },
            { key: "p-mock", label: "Mock interview", kind: "Page", color: "var(--acc)", go: go(() => nav({ to: "/dsa/mock" })) },
            { key: "p-rust", label: "Rust tracks", kind: "Page", color: "var(--vio)", go: go(() => nav({ to: "/rust" })) },
            { key: "p-courses", label: "BusTub course", kind: "Page", color: "var(--grn)", go: go(() => nav({ to: "/courses" })) },
            { key: "p-progress", label: "Progress", kind: "Page", color: "var(--fg)", go: go(() => nav({ to: "/progress" })) },
        ];
        hits.push(
            { key: "a-settings", label: "Settings", kind: "Action", color: "var(--fg)", go: go(() => nav({ to: "/settings" })) },
            { key: "a-help", label: "Keyboard shortcuts", kind: "Action", color: "var(--fg)", go: go(() => window.dispatchEvent(new Event("anneal:help"))) },
            { key: "a-backup", label: "Download a backup of everything", kind: "Action", color: "var(--fg)", go: go(() => window.location.assign("/api/export")) },
            { key: "a-theme", label: "Change the theme (dark, light, system)", kind: "Action", color: "var(--fg)", go: go(() => { const next = THEMES[(THEMES.indexOf(storedTheme()) + 1) % THEMES.length]!; applyTheme(next); }) },
        );
        for (const t of tracks.data ?? []) hits.push({ key: `t-${t.slug}`, label: `${t.code} · ${t.name}`, kind: "Track", color: "var(--vio)", go: go(() => nav({ to: "/t/$track", params: { track: t.slug } })) });
        for (const p of course.data?.projects ?? [])
            for (const m of p.modules)
                for (const s of m.stages)
                    hits.push({ key: `s-${s.id}`, label: `${s.id} · ${s.title}`, kind: s.kind === "challenge" ? "Challenge" : "Stage", color: "var(--grn)", go: go(() => nav({ to: "/courses/$course/$stage", params: { course: "bustub", stage: s.id } })) });
        for (const p of dsa.data?.problems ?? []) hits.push({ key: `d-${p.id}`, label: `${p.number} · ${p.title}`, kind: "Problem", color: "var(--acc)", go: go(() => nav({ to: "/d/$slug", params: { slug: p.slug } })) });
        return hits;
    }, [tracks.data, dsa.data, course.data, nav, onClose]);

    const shown = useMemo(() => {
        const needle = q.trim().toLowerCase();
        if (!needle) return all.slice(0, 10);
        const words = needle.split(/\s+/);
        // "log two sum": the four outcomes for the best matches, logged straight from here
        if (words[0] === "log" && words.length > 1) {
            const rest = words.slice(1);
            const hits: Hit[] = [];
            for (const p of (dsa.data?.problems ?? []).filter((x) => rest.every((w) => `${x.title} ${x.number}`.toLowerCase().includes(w))).slice(0, 2))
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
        return all.filter((h) => words.every((w) => `${h.label} ${h.kind}`.toLowerCase().includes(w))).slice(0, 14);
    }, [all, q]);
    useEffect(() => setSel(0), [q]);

    return (
        <div className="hd-pal-wrap" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="hd-pal" role="dialog" aria-label="Search" aria-modal="true">
                <input
                    ref={input}
                    value={q}
                    onChange={(e) => setQ(e.target.value)}
                    placeholder="Search tracks, stages, concepts, problems…"
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
                        } else if (e.key === "Enter") shown[sel]?.go();
                    }}
                />
                <ul role="listbox">
                    {shown.map((h, i) => (
                        <li key={h.key} role="option" aria-selected={i === sel} className={i === sel ? "hd-sel" : ""} style={{ "--c": h.color } as React.CSSProperties} onMouseEnter={() => setSel(i)} onMouseDown={(e) => e.preventDefault()} onClick={h.go}>
                            <span className="d" />
                            {h.label}
                            <small>{h.kind}</small>
                        </li>
                    ))}
                    {shown.length === 0 && <li className="hd-none">No match</li>}
                </ul>
                <div className="hd-hint">
                    <span>↑↓ move</span>
                    <span>↵ open</span>
                    <span>esc close</span>
                </div>
            </div>
        </div>
    );
}
