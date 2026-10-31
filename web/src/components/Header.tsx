import { Link, useRouterState } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api } from "../api";
import { ACCENTS, useAppearance } from "../settings";
import { fresh, mmss, setTimer, toggleTimer, useFocus } from "../focusStore";
import { isRustSection } from "../curriculum";
import { StatsBar } from "./StatsBar";
import { Palette } from "./Palette";
import { useDialog } from "./useDialog";
import { NotificationBell } from "./Notifications";
import { fmt, useStreaks } from "./Streaks";
import { THEMES, applyTheme, storedTheme, type Theme } from "../theme";

// The header has two tiers (docs/mockups/navbar-v2.html, refined in the 2026-10-31 redesign). The first answers "where in the product am I":
// the three areas as one switcher with their progress, a search field, the date and time, the focus timer, the accent, and the tools. The second answers "what can I do here": the pages of the current area,
// a breadcrumb and a few counters. On a phone the areas move to a bar at the bottom.

type AreaId = "dsa" | "rust" | "courses" | "sd";

const AREAS: { id: AreaId; label: string; to: "/dsa" | "/rust" | "/courses" | "/sd"; color: string }[] = [
    { id: "dsa", label: "DSA", to: "/dsa", color: "var(--acc)" },
    { id: "rust", label: "Rust", to: "/rust", color: "var(--vio)" },
    { id: "courses", label: "Courses", to: "/courses", color: "var(--grn)" },
    { id: "sd", label: "System design", to: "/sd", color: "var(--warn)" },
];

/** How long the Mock page's data counts as fresh, so coming back to it within a minute shows it at once. */
export const MOCK_FRESH = 60_000;

type Tab = { to: "/dsa" | "/dsa/plan" | "/dsa/review" | "/dsa/calendar" | "/dsa/company" | "/dsa/mock" | "/dsa/graph" | "/dsa/read" | "/progress" | "/analytics" | "/settings" | "/log" | "/rust" | "/courses" | "/sd" | "/sd/learn" | "/sd/problems" | "/sd/review"; label: string; match: (path: string) => boolean; badge?: "due" };

/** Pages about everything, not one area: Progress covers DSA, Rust and Courses; it has its own place beside the areas. */
const GLOBAL: Tab[] = [
    { to: "/progress", label: "Progress", match: (p) => p.startsWith("/progress") },
    { to: "/analytics", label: "Analytics", match: (p) => p.startsWith("/analytics") },
    { to: "/settings", label: "Settings", match: (p) => p.startsWith("/settings") },
    { to: "/log", label: "Quick log", match: (p) => p.startsWith("/log") },
];
const isGlobal = (path: string) => GLOBAL.some((t) => t.match(path));

const TABS: Record<AreaId, Tab[]> = {
    dsa: [
        { to: "/dsa", label: "Problems", match: (p) => p === "/dsa" || p.startsWith("/dsa/patterns") || p.startsWith("/dsa/practice") || p.startsWith("/d/") },
        { to: "/dsa/plan", label: "Plan", match: (p) => p.startsWith("/dsa/plan") },
        { to: "/dsa/review", label: "Review", match: (p) => p.startsWith("/dsa/review"), badge: "due" },
        { to: "/dsa/read", label: "Read", match: (p) => p.startsWith("/dsa/read") },
        { to: "/dsa/calendar", label: "Calendar", match: (p) => p.startsWith("/dsa/calendar") },
        { to: "/dsa/company", label: "Company", match: (p) => p.startsWith("/dsa/company") },
        { to: "/dsa/mock", label: "Mock interview", match: (p) => p.startsWith("/dsa/mock") },
        { to: "/dsa/graph", label: "Graph", match: (p) => p.startsWith("/dsa/graph") },
    ],
    rust: [
        { to: "/rust", label: "Tracks", match: (p) => p.startsWith("/rust") || p.startsWith("/t/") || p.startsWith("/p/") },
    ],
    courses: [
        { to: "/courses", label: "BusTub", match: (p) => p.startsWith("/courses") },
    ],
    sd: [
        { to: "/sd", label: "Home", match: (p) => p === "/sd" },
        { to: "/sd/learn", label: "Learn", match: (p) => p.startsWith("/sd/learn") },
        { to: "/sd/problems", label: "Problems", match: (p) => p.startsWith("/sd/problems") || p.startsWith("/sd/practice") },
        { to: "/sd/review", label: "Review", match: (p) => p.startsWith("/sd/review") },
    ],
};

/** Pages that are designed but not built yet; dimmed, in the area they belong to. */
const LATER: Record<AreaId, string[]> = { dsa: [], rust: ["Library"], courses: ["Readiness"], sd: ["Stories", "Mock interview"] };

const AREA_KEY = "anneal.area";
const areaFromPath = (path: string): AreaId | null => {
    if (path.startsWith("/courses")) return "courses";
    if (path.startsWith("/sd")) return "sd";
    if (path.startsWith("/rust") || path.startsWith("/t/") || path.startsWith("/p/")) return "rust";
    if (path.startsWith("/dsa") || path.startsWith("/d/")) return "dsa";
    return null;
};

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
    const sd = useQuery({ queryKey: ["sd"], queryFn: api.sd, staleTime: 60_000 });
    const streaks = useStreaks();
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
        // system design: concept checks answered and problems drilled, over all there are
        sd: { done: sd.data ? sd.data.concepts.reduce((n, c) => n + Math.min(c.checked, c.checks), 0) + sd.data.problems.filter((p) => p.attempts > 0).length : undefined, total: sd.data ? sd.data.concepts.reduce((n, c) => n + c.checks, 0) + sd.data.problems.length : 0 },
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
    const global = isGlobal(path);
    return (
        <>
            <header className={`hdr${compact ? " compact" : ""}`} ref={box} data-area={current}>
                <ScrollProgress />
                <div className="hd-t1">
                    <div className="hd-l">
                    <Link className="logo" to="/" aria-label="anneal home">
                        <span className="mk">
                            <span style={{ background: "var(--acc)" }} />
                            <span style={{ background: "var(--vio)" }} />
                            <span style={{ background: "var(--grn)" }} />
                            <span style={{ boxShadow: "inset 0 0 0 1px var(--line)" }} />
                        </span>
                        <b>anneal</b>
                    </Link>
                    <AreaSwitcher current={global ? null : current} counts={counts} progress={path.startsWith("/progress")} />
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
                        <Clock />
                        <MiniTimer />
                        <span className="hd-vsep" aria-hidden="true" />
                        <AccentPicker />
                        <NotificationBell />
                        <Link className="hd-ib" to="/analytics" title="Analytics: the VPS and the app" aria-label="Analytics">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true"><path d="M2.5 13.5h11M4.5 11V7M8 11V3.5M11.5 11V8.5" /></svg>
                        </Link>
                        <Link className="hd-ib set" to="/settings" title="Settings" aria-label="Settings">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                                <path d="M6.9 1.5h2.2l.4 1.8 1.2.5 1.6-1 1.5 1.5-1 1.6.5 1.2 1.8.4v2.2l-1.8.4-.5 1.2 1 1.6-1.5 1.5-1.6-1-1.2.5-.4 1.8H6.9l-.4-1.8-1.2-.5-1.6 1-1.5-1.5 1-1.6-.5-1.2-1.8-.4V6.9l1.8-.4.5-1.2-1-1.6 1.5-1.5 1.6 1 1.2-.5z" />
                                <circle cx="8" cy="8" r="2.2" />
                            </svg>
                        </Link>
                        <button className="hd-ib thm" onClick={cycleTheme} title={`Theme: ${theme}. Click for ${THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length]}`} aria-label={`Theme: ${theme}`}>
                            {theme === "dark" && (
                                <svg className="moon on" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                                    <path d="M13.5 9.6A6 6 0 0 1 6.4 2.5a6 6 0 1 0 7.1 7.1Z" />
                                </svg>
                            )}
                            {theme === "light" && (
                                <svg className="sun on" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
                                    <circle cx="8" cy="8" r="3" />
                                    <path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" />
                                </svg>
                            )}
                            {theme === "system" && (
                                <svg className="sys on" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                                    <rect x="1.8" y="2.6" width="12.4" height="8.4" rx="1.6" />
                                    <path d="M5.5 13.6h5M8 11v2.6" />
                                    <path d="M8 4.6v4.4a2.2 2.2 0 0 1 0-4.4z" fill="currentColor" stroke="none" />
                                </svg>
                            )}
                        </button>
                        <Account />
                    </div>
                </div>
                {!compact && (
                    <div className="hd-t2">
                        <Crumb area={meta.label} path={path} tabs={TABS[current]} />
                        <SubTabs tabs={global ? GLOBAL : TABS[current]} later={global ? [] : LATER[current]} color={global ? "var(--acc)" : meta.color} path={path} due={due} />
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
                                    {streaks.data && (
                                        <Link to="/dsa/calendar" className="hd-chip" title={`Perfect streak ${streaks.data.perfect}: days you met the whole plan. Freezes: ${fmt(streaks.data.freezes)}, earned by doing more than planned (an extra new problem is 1, an extra review half).`}>
                                            {streaks.data.perfect > 0 && <>perfect <b>{streaks.data.perfect}</b> · </>}freezes <b className="sk-fz">{fmt(streaks.data.freezes)}</b>
                                        </Link>
                                    )}
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
                    <Link key={a.id} to={a.to} className={!global && a.id === current ? "on" : ""} style={{ "--c": a.color } as React.CSSProperties} aria-current={!global && a.id === current ? "page" : undefined}>
                        <Ring p={ratio(counts[a.id])} color={a.color} />
                        {a.label}
                    </Link>
                ))}
                <Link to="/progress" className={path.startsWith("/progress") ? "on" : ""} style={{ "--c": "var(--acc)" } as React.CSSProperties}>
                    <TrophyIcon />
                    Progress
                </Link>
            </nav>
            {paletteOpen && <Palette onClose={() => setPaletteOpen(false)} />}
        </>
    );
}

const ratio = (c: { done: number | undefined; total: number }) => (c.total > 0 && c.done !== undefined ? c.done / c.total : 0);

function TrophyIcon() {
    return (
        <svg className="hd-tro" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M5 2.5h6v3.5a3 3 0 0 1-6 0zM5 4H2.5v1a2.5 2.5 0 0 0 2.6 2.5M11 4h2.5v1a2.5 2.5 0 0 1-2.6 2.5M8 9v2.5M5.5 13.5h5M6.5 11.5h3v2h-3z" />
        </svg>
    );
}

/** The three areas as one control, and Progress (about all three) after a divider: a solid thumb slides to the current one. */
function AreaSwitcher({ current, counts, progress }: { current: AreaId | null; counts: Record<AreaId, { done: number | undefined; total: number }>; progress: boolean }) {
    const box = useRef<HTMLElement>(null);
    const [thumb, setThumb] = useState<{ x: number; w: number } | null>(null);
    useLayoutEffect(() => {
        const place = () => {
            const on = box.current?.querySelector<HTMLElement>("a.on");
            setThumb(on ? { x: on.offsetLeft, w: on.offsetWidth } : null);
        };
        place();
        // the labels' width changes when the web font arrives and when the counts load
        void document.fonts?.ready.then(place);
        addEventListener("resize", place);
        return () => removeEventListener("resize", place);
    }, [current, progress, counts.dsa.done, counts.rust.done, counts.courses.done, counts.sd.done]);
    const color = current ? AREAS.find((a) => a.id === current)!.color : "var(--acc)";
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
            <span className="hd-asep" aria-hidden="true" />
            <Link to="/progress" className={progress ? "on" : ""} aria-current={progress ? "page" : undefined} title="Progress across DSA, Rust and Courses">
                <TrophyIcon />
                Progress
            </Link>
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

/** The pages of the current area (or the global pages): a tinted pill glides behind the current one. */
function SubTabs({ tabs, later, color, path, due }: { tabs: Tab[]; later: string[]; color: string; path: string; due: number }) {
    const box = useRef<HTMLDivElement>(null);
    const qc = useQueryClient();
    // the Mock page draws from every problem (about 200 KB), so its data starts loading when the tab is pointed at or touched
    const warm = (to: Tab["to"]) => to === "/dsa/mock" && void qc.prefetchQuery({ queryKey: ["dsa-mock"], queryFn: api.mock, staleTime: MOCK_FRESH });
    const [ink, setInk] = useState<{ x: number; w: number } | null>(null);
    useLayoutEffect(() => {
        const place = () => {
            const on = box.current?.querySelector<HTMLElement>("a.on");
            setInk(on ? { x: on.offsetLeft, w: on.offsetWidth } : null);
        };
        place();
        addEventListener("resize", place);
        return () => removeEventListener("resize", place);
    }, [tabs, path, due]);
    return (
        <div className="hd-tabs" ref={box}>
            {ink && <span className="hd-ink" style={{ transform: `translateX(${ink.x}px)`, width: ink.w, "--ink": color } as CSSProperties} />}
            {tabs.map((t) => {
                const on = t.match(path);
                return (
                    <Link key={t.label} to={t.to} className={on ? "on" : ""} aria-current={on ? "page" : undefined} onPointerEnter={() => warm(t.to)} onTouchStart={() => warm(t.to)} onFocus={() => warm(t.to)}>
                        {t.label}
                        {t.badge === "due" && due > 0 && <span className="hd-n hd-hot">{due}</span>}
                    </Link>
                );
            })}
            {later.map((l) => (
                <span key={l} className="hd-later" title="Designed; not built yet">
                    {l}
                </span>
            ))}
        </div>
    );
}

/** The avatar menu: links, and Sign out when login is on. The accent has its own picker in the header. */
function Account() {
    const session = useQuery({ queryKey: ["session"], queryFn: api.session, staleTime: Infinity });
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);
    const menu = useRef<HTMLDivElement>(null);
    useDialog(menu, open, () => setOpen(false));
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
            <button className="av" aria-haspopup="menu" aria-expanded={open} aria-label="Account menu" onClick={() => setOpen(!open)} title="Account">
                gb
            </button>
            {open && (
                <div className="acct-menu" ref={menu} role="menu">
                    <Link role="menuitem" className="acct-link acct-first" to="/settings" onClick={() => setOpen(false)}>
                        All settings
                    </Link>
                    <Link role="menuitem" className="acct-link" to="/log" onClick={() => setOpen(false)}>
                        Quick log (phone)
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


/** The date and the time, kept current. */
function Clock() {
    const [now, setNow] = useState(() => new Date());
    useEffect(() => {
        const id = setInterval(() => setNow(new Date()), 15_000);
        return () => clearInterval(id);
    }, []);
    return (
        <span className="hd-clock" title={now.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", year: "numeric" })}>
            <b>{now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</b>
            <span>{now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}</span>
        </span>
    );
}

/** The focus timer in small: the same timer as the card on the problem page (focusStore), so a session started there shows here. */
function MiniTimer() {
    const { cfg, s } = useFocus();
    const sw = cfg.mode === "sw";
    const p = sw ? ((s.secs % 60) / 60) * 100 : s.total ? ((s.total - s.secs) / s.total) * 100 : 0;
    const brk = cfg.mode === "pom" && s.phase !== "focus";
    const label = sw ? "Stopwatch" : cfg.mode === "timer" ? "Countdown" : brk ? "Break" : "Focus";
    return (
        <span className={`hd-timer${s.running ? " run" : ""}${brk ? " brk" : ""}${s.running || s.secs !== fresh(cfg).secs ? " live" : ""}`}>
            <button className="hd-tring" onClick={toggleTimer} aria-label={`${s.running ? "Pause" : "Start"} the focus timer`} title={`${label}: ${s.running ? "pause" : "start"} (the same timer as the problem page)`}>
                <span className="hd-tarc" style={{ "--p": p } as React.CSSProperties} />
                {s.running ? (
                    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M5 3.5h2v9H5zM9 3.5h2v9H9z" /></svg>
                ) : (
                    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M5 3.5v9l7-4.5z" /></svg>
                )}
            </button>
            <b role="timer" aria-label={`${label} ${mmss(s.secs)}`}>{mmss(s.secs)}</b>
            <button className="hd-treset" onClick={() => setTimer(fresh(cfg))} aria-label="Stop and reset the focus timer" title="Stop and reset">
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 8a5 5 0 1 0 1.5-3.5M3 2.5V5h2.5" /></svg>
            </button>
        </span>
    );
}

/** The accent colour as its own control: deep colours, soft ones, and any colour you like. */
function AccentPicker() {
    const { accent, custom, set, setCustom } = useAppearance();
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLSpanElement>(null);
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
    const group = (g: "deep" | "soft") =>
        ACCENTS.filter((a) => a.group === g).map((a) => (
            <button key={a.id} role="radio" aria-checked={accent === a.id} aria-label={a.label} title={a.label} className={`hd-sw${accent === a.id ? " on" : ""}`} data-swatch={a.id} onClick={() => set(a.id)} />
        ));
    return (
        <span className="hd-accw" ref={box}>
            <button className="hd-ib hd-acc" aria-haspopup="true" aria-expanded={open} aria-label="Accent colour" title="Accent colour" onClick={() => setOpen(!open)}>
                <span className="hd-dot" />
            </button>
            {open && (
                <div className="hd-accp" role="radiogroup" aria-label="Accent colour">
                    <span className="hd-agl">Deep</span>
                    {group("deep")}
                    <span className="hd-asep" />
                    <span className="hd-agl">Soft</span>
                    {group("soft")}
                    <span className="hd-asep" />
                    <label className={`hd-cpick${accent === "custom" ? " on" : ""}`} title="Any colour">
                        <input type="color" value={custom ?? "#c8875b"} onChange={(e) => setCustom(e.target.value)} aria-label="Pick any colour" />
                        <span />
                    </label>
                    <button className="hd-areset" onClick={() => set("copper")} aria-label="Back to the default accent" title="Back to Copper">
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 8a5 5 0 1 0 1.5-3.5M3 2.5V5h2.5" /></svg>
                    </button>
                </div>
            )}
        </span>
    );
}

/** A thin line along the bottom of the header that fills as the page scrolls. */
function ScrollProgress() {
    const bar = useRef<HTMLSpanElement>(null);
    useEffect(() => {
        const set = () => {
            const h = document.documentElement;
            bar.current?.style.setProperty("--sp", String(Math.min(1, h.scrollTop / Math.max(1, h.scrollHeight - innerHeight))));
        };
        set();
        addEventListener("scroll", set, { passive: true });
        addEventListener("resize", set);
        return () => {
            removeEventListener("scroll", set);
            removeEventListener("resize", set);
        };
    }, []);
    return <span className="hd-prog" ref={bar} aria-hidden="true" />;
}
