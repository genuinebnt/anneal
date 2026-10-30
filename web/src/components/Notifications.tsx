import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { counted, newOn, niceDate, WEEKDAYS } from "../dsa";
import { getPref, setPref } from "../prefs";
import { clearNotes, useNotes } from "./toasts";
import { useDialog } from "./useDialog";

type Need = { id: string; label: string; text: ReactNode; sub: string; tone: "due" | "next" | "warn" | "late"; to: string; slug?: string };

const today = () => new Date().toLocaleDateString("en-CA");
const ago = (t: number) => {
    const m = Math.round((Date.now() - t) / 60000);
    return m < 1 ? "just now" : m < 60 ? `${m} min ago` : m < 1440 ? `${Math.round(m / 60)} h ago` : `${Math.round(m / 1440)} d ago`;
};

/** A number that counts up to its value when the panel opens (at once under reduced motion). */
function useCount(target: number, run: boolean) {
    const [v, setV] = useState(target);
    useEffect(() => {
        const calm = document.documentElement.dataset.motion === "reduce" || window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
        if (!run || calm || target <= 0) {
            setV(target);
            return;
        }
        let raf = 0;
        const t0 = performance.now();
        const step = (t: number) => {
            const k = Math.min(1, (t - t0) / 650);
            setV(Math.round(target * (1 - (1 - k) ** 3)));
            if (k < 1) raf = requestAnimationFrame(step);
        };
        raf = requestAnimationFrame(step);
        return () => cancelAnimationFrame(raf);
    }, [target, run]);
    return v;
}

const ICON: Record<Need["tone"], ReactNode> = {
    due: <path d="M8 3.2v4.9l3 1.8M14 8A6 6 0 1 1 2 8a6 6 0 0 1 12 0z" />,
    next: <path d="M5.5 3.5l7 4.5-7 4.5z" />,
    warn: <path d="M8.6 1.8L4.2 8.6h3.1L6.9 14.2l4.9-7H8.5z" />,
    late: <path d="M3 13V3M3 3.5h8.5l-1.7 2.6 1.7 2.6H3" />,
};

/** The bell: what needs you today, what is new, and what was said lately. Everything on it is worked out from data the app already loads; an
 *  item can be dismissed for the day. */
export function NotificationBell() {
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);
    const pop = useRef<HTMLDivElement>(null);
    const [at, setAt] = useState<{ top: number; right: number; width: number; max: number } | null>(null);
    const [leaving, setLeaving] = useState<string[]>([]);
    useDialog(pop, open, () => setOpen(false));
    // an entry from the removed "What's new" list can still be in this browser's history
    const recent = useNotes().filter((n) => !/^anneal was updated/i.test(n.title));
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const activity = useQuery({ queryKey: ["activity", "header"], queryFn: () => api.activity(["D"]) });
    const progress = useQuery({ queryKey: ["progress"], queryFn: api.progress });
    const plan = useQuery({ queryKey: ["plan", "bell"], queryFn: () => api.plan(today(), today()), enabled: open });
    const [dismissed, setDismissed] = useState<string[]>(() => {
        const d = getPref<{ day: string; ids: string[] }>("notif.dismissed", { day: "", ids: [] });
        return d.day === today() ? d.ids : [];
    });

    // The panel hangs off the page, not off the header, so nothing in the header can clip it; it is placed under the bell and kept inside the window.
    useLayoutEffect(() => {
        if (!open) return;
        const place = () => {
            const r = box.current?.getBoundingClientRect();
            if (!r) return;
            const width = Math.min(440, window.innerWidth - 24);
            const right = Math.max(12, Math.min(window.innerWidth - r.right, window.innerWidth - 12 - width));
            const top = r.bottom + 10;
            setAt({ top, right, width, max: Math.max(240, window.innerHeight - top - 16) });
        };
        place();
        window.addEventListener("resize", place);
        return () => window.removeEventListener("resize", place);
    }, [open]);

    useEffect(() => {
        if (!open) return;
        const close = (e: MouseEvent) => !box.current?.contains(e.target as Node) && !pop.current?.contains(e.target as Node) && setOpen(false);
        const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
        document.addEventListener("mousedown", close);
        document.addEventListener("keydown", esc);
        return () => {
            document.removeEventListener("mousedown", close);
            document.removeEventListener("keydown", esc);
        };
    }, [open]);

    const needs: Need[] = [];
    const o = dsa.data;
    if (o) {
        const p = o.plan;
        const doneToday = activity.data?.week[activity.data.week.length - 1]?.solved ?? 0;
        const next = o.problems.find((x) => x.id === p.next_up[0]);
        const due = p.due === 1 ? "review" : "reviews";
        if (p.due > 0) needs.push({ id: "due", label: `${p.due} ${due} due`, text: <><b className="nt-num">{p.due}</b> {due} due{p.overdue ? <>, <b className="nt-num">{p.overdue}</b> overdue</> : null}</>, sub: "Open today's queue", tone: "due", to: "/dsa/review" });
        if (p.solve_day && next && counted(o, next) && doneToday < newOn(o.settings, WEEKDAYS[(new Date().getDay() + 6) % 7] ?? "mon")) needs.push({ id: "today", label: `Today's problem: ${next.title}`, text: <>Today's problem: <b>{next.title}</b></>, sub: `${next.difficulty} · new`, tone: "next", to: "/d/$slug", slug: next.slug });
        const streak = progress.data?.streak ?? 0;
        if (p.solve_day && streak > 0 && doneToday === 0 && new Date().getHours() >= 17) needs.push({ id: "streak", label: `Your ${streak}-day streak needs a solve today`, text: <>Your <b className="nt-num">{streak}</b>-day streak needs a solve today</>, sub: "Before midnight", tone: "warn", to: next ? "/d/$slug" : "/dsa", slug: next?.slug });
        if (plan.data && plan.data.summary.late > 0) {
            const late = plan.data.summary.late;
            needs.push({ id: "late", label: `Your plan is ${late} problems late`, text: <>Your plan is <b className="nt-num">{late}</b> problem{late === 1 ? "" : "s"} late</>, sub: plan.data.summary.finish ? `Finishing ${niceDate(plan.data.summary.finish, plan.data.today)}` : "Open the calendar", tone: "late", to: "/dsa/calendar" });
        }
    }
    const live = needs.filter((n) => !dismissed.includes(n.id));
    const count = live.filter((n) => n.id !== "late").length;

    const save = (ids: string[]) => {
        setDismissed(ids);
        setPref("notif.dismissed", { day: today(), ids });
    };
    // the card folds away first, then it is dismissed for the day
    const dismiss = (id: string) => {
        setLeaving((l) => [...l, id]);
        window.setTimeout(() => {
            save([...dismissed, id]);
            setLeaving((l) => l.filter((x) => x !== id));
        }, 240);
    };
    const hidden = needs.filter((n) => dismissed.includes(n.id)).length;
    const streakN = useCount(progress.data?.streak ?? 0, open);
    const dueN = useCount(o?.plan.due ?? 0, open);
    const todayN = useCount(activity.data?.week[activity.data.week.length - 1]?.solved ?? 0, open);
    const goalDone = useCount(o?.plan.goal_done ?? 0, open);
    const goalPct = o && o.plan.goal_total ? Math.round((o.plan.goal_done / o.plan.goal_total) * 100) : 0;

    return (
        <div className="nt" ref={box}>
            <button className="hd-ib bell" aria-haspopup="dialog" aria-expanded={open} aria-label={count ? `Notifications, ${count} need you` : "Notifications"} title="Notifications" onClick={() => setOpen(!open)}>
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <path d="M3.5 11.5h9l-1.2-1.6V7a3.3 3.3 0 0 0-6.6 0v2.9z" />
                    <path d="M6.6 13.4a1.5 1.5 0 0 0 2.8 0" />
                </svg>
                {count > 0 && <span className="nt-n" aria-hidden="true">{count}</span>}
            </button>
            {open && at && createPortal(
                <div className="nt-pop" ref={pop} role="dialog" aria-label="Notifications" style={{ top: at.top, right: at.right, width: at.width, maxHeight: at.max }}>
                    <header className="nt-head">
                        <b>Today</b>
                        <time>{new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}</time>
                        <button className="nt-close" aria-label="Close notifications" title="Close (Esc)" onClick={() => setOpen(false)}>
                            <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" /></svg>
                        </button>
                    </header>
                    {o && (
                        <div className="nt-sum" aria-label="Today at a glance">
                            <div style={{ "--i": 0 } as CSSProperties}><b>{streakN}</b><span>streak</span></div>
                            <div style={{ "--i": 1 } as CSSProperties}><b>{dueN}</b><span>due</span></div>
                            <div style={{ "--i": 2 } as CSSProperties}><b>{todayN}</b><span>today</span></div>
                            <div className="goal" style={{ "--i": 3, "--p": `${goalPct}%` } as CSSProperties}><b>{goalDone}<small>/{o.plan.goal_total}</small></b><span>goal</span><i className="nt-bar" aria-hidden="true"><u /></i></div>
                        </div>
                    )}
                    <span className="nt-lab">NEEDS YOU{hidden > 0 && <button className="nt-clear" onClick={() => save([])}>show {hidden} dismissed</button>}</span>
                    {live.length === 0 && (
                        <p className="nt-empty">
                            <svg className="nt-ok" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10" /><path d="M7.5 12.5l3 3 6-7" /></svg>
                            <span><b>You're caught up.</b><small>Nothing needs you right now.</small></span>
                        </p>
                    )}
                    {live.map((n, k) => {
                        const inner = (
                            <>
                                <i className={`nt-ico2 ${n.tone}`} aria-hidden="true"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">{ICON[n.tone]}</svg></i>
                                <span className="nt-tx"><span>{n.text}</span><small>{n.sub}</small></span>
                                <svg className="nt-go" viewBox="0 0 8 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M1.5 1.5L6 6l-4.5 4.5" /></svg>
                            </>
                        );
                        return (
                            <div key={n.id} className={`nt-item ${n.tone}${leaving.includes(n.id) ? " out" : ""}`} style={{ "--i": k + 4 } as CSSProperties}>
                                <div className="nt-in">
                                    {n.slug ? <Link to="/d/$slug" params={{ slug: n.slug }} onClick={() => setOpen(false)}>{inner}</Link> : <Link to={n.to as "/dsa/review"} onClick={() => setOpen(false)}>{inner}</Link>}
                                    <button className="nt-x" aria-label={`Dismiss: ${n.label}`} title="Dismiss for today" onClick={() => dismiss(n.id)}>
                                        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
                                    </button>
                                </div>
                            </div>
                        );
                    })}
                    <div className="nt-links">
                        <Link to="/dsa/plan" onClick={() => setOpen(false)}>Plan</Link>
                        <Link to="/dsa/calendar" onClick={() => setOpen(false)}>Calendar</Link>
                        <Link to="/dsa/review" onClick={() => setOpen(false)}>Review</Link>
                        <Link to="/settings" onClick={() => setOpen(false)}>Settings</Link>
                    </div>
                    <span className="nt-lab">RECENT <button className="nt-clear" onClick={clearNotes} disabled={recent.length === 0}>clear</button></span>
                    {recent.length === 0 && <p className="nt-none">Nothing yet. What you log, undo and change shows up here.</p>}
                    <ol className="nt-line">
                        {recent.slice(0, 8).map((n) => (
                            <li key={n.id} className={`nt-note ${n.kind}`}>
                                <i className="nt-ico" aria-hidden="true">{n.kind === "ok" ? "✓" : n.kind === "er" ? "×" : "i"}</i>
                                <span className="nt-tx"><span>{n.title}</span>{n.text ? <small>{n.text}</small> : null}</span>
                                <time>{ago(n.at)}</time>
                            </li>
                        ))}
                    </ol>
                </div>,
                document.body,
            )}
        </div>
    );
}
