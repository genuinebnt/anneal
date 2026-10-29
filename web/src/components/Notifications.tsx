import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactNode } from "react";
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

/** The bell: what needs you today, what is new, and what was said lately. Everything on it is worked out from data the app already loads; an
 *  item can be dismissed for the day. */
export function NotificationBell() {
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);
    const pop = useRef<HTMLDivElement>(null);
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

    const dismiss = (id: string) => {
        const ids = [...dismissed, id];
        setDismissed(ids);
        setPref("notif.dismissed", { day: today(), ids });
    };

    return (
        <div className="nt" ref={box}>
            <button className="hd-ib bell" aria-haspopup="dialog" aria-expanded={open} aria-label={count ? `Notifications, ${count} need you` : "Notifications"} title="Notifications" onClick={() => setOpen(!open)}>
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <path d="M3.5 11.5h9l-1.2-1.6V7a3.3 3.3 0 0 0-6.6 0v2.9z" />
                    <path d="M6.6 13.4a1.5 1.5 0 0 0 2.8 0" />
                </svg>
                {count > 0 && <span className="nt-n" aria-hidden="true">{count}</span>}
            </button>
            {open && (
                <div className="nt-pop" ref={pop} role="dialog" aria-label="Notifications">
                    {o && (
                        <div className="nt-sum" aria-label="Today at a glance">
                            <div><b>{progress.data?.streak ?? 0}</b><span>streak</span></div>
                            <div><b>{o.plan.due}</b><span>due</span></div>
                            <div><b>{activity.data?.week[activity.data.week.length - 1]?.solved ?? 0}</b><span>today</span></div>
                            <div><b>{o.plan.goal_done}<small>/{o.plan.goal_total}</small></b><span>goal</span></div>
                        </div>
                    )}
                    <span className="nt-lab">NEEDS YOU</span>
                    {live.length === 0 && <p className="nt-empty"><i aria-hidden="true">✓</i> You're caught up.</p>}
                    {live.map((n) => {
                        const inner = (
                            <>
                                <i className={`nt-dot ${n.tone}`} aria-hidden="true" />
                                <span className="nt-tx"><span>{n.text}</span><small>{n.sub}</small></span>
                                <span className="nt-go" aria-hidden="true">›</span>
                            </>
                        );
                        return (
                            <div key={n.id} className="nt-item">
                                {n.slug ? <Link to="/d/$slug" params={{ slug: n.slug }} onClick={() => setOpen(false)}>{inner}</Link> : <Link to={n.to as "/dsa/review"} onClick={() => setOpen(false)}>{inner}</Link>}
                                <button className="nt-x" aria-label={`Dismiss: ${n.label}`} title="Dismiss for today" onClick={() => dismiss(n.id)}>×</button>
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
                    {recent.length === 0 && <p className="nt-empty">Nothing yet. What you log, undo and change shows up here.</p>}
                    {recent.slice(0, 8).map((n) => (
                        <div key={n.id} className={`nt-note ${n.kind}`}>
                            <i className="nt-ico" aria-hidden="true">{n.kind === "ok" ? "✓" : n.kind === "er" ? "×" : "i"}</i>
                            <span className="nt-tx"><span>{n.title}</span>{n.text ? <small>{n.text}</small> : null}</span>
                            <time>{ago(n.at)}</time>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
