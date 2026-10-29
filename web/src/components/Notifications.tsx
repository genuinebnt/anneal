import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { counted, niceDate } from "../dsa";
import { getPref, setPref } from "../prefs";
import { LATEST, WHATS_NEW } from "../whatsnew";
import { clearNotes, useNotes } from "./toasts";
import { useDialog } from "./useDialog";

type Need = { id: string; text: string; to: string; slug?: string };

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
    const notes = useNotes();
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const activity = useQuery({ queryKey: ["activity", "header"], queryFn: () => api.activity(["D"]) });
    const progress = useQuery({ queryKey: ["progress"], queryFn: api.progress });
    const plan = useQuery({ queryKey: ["plan", "bell"], queryFn: () => api.plan(today(), today()), enabled: open });
    const [dismissed, setDismissed] = useState<string[]>(() => {
        const d = getPref<{ day: string; ids: string[] }>("notif.dismissed", { day: "", ids: [] });
        return d.day === today() ? d.ids : [];
    });
    const [seenNew, setSeenNew] = useState(() => getPref("whatsnew.seen", "") === LATEST);

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
        if (p.due > 0) needs.push({ id: "due", text: `${p.due} review${p.due === 1 ? "" : "s"} due${p.overdue ? `, ${p.overdue} overdue` : ""}`, to: "/dsa/review" });
        if (p.solve_day && next && counted(o, next) && doneToday < o.settings.new_per_day) needs.push({ id: "today", text: `Today's problem: ${next.title}`, to: "/d/$slug", slug: next.slug });
        const streak = progress.data?.streak ?? 0;
        if (p.solve_day && streak > 0 && doneToday === 0 && new Date().getHours() >= 17) needs.push({ id: "streak", text: `Your ${streak}-day streak needs a solve today`, to: next ? "/d/$slug" : "/dsa", slug: next?.slug });
        if (plan.data && plan.data.summary.late > 0) needs.push({ id: "late", text: `Your plan is ${plan.data.summary.late} problem${plan.data.summary.late === 1 ? "" : "s"} late${plan.data.summary.finish ? `, finishing ${niceDate(plan.data.summary.finish, plan.data.today)}` : ""}`, to: "/dsa/calendar" });
    }
    const live = needs.filter((n) => !dismissed.includes(n.id));
    const showNew = !seenNew;
    const count = live.filter((n) => n.id !== "late").length + (showNew ? 1 : 0);

    const dismiss = (id: string) => {
        const ids = [...dismissed, id];
        setDismissed(ids);
        setPref("notif.dismissed", { day: today(), ids });
    };
    const markNewSeen = () => {
        setSeenNew(true);
        setPref("whatsnew.seen", LATEST);
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
                    <span className="nt-lab">NEEDS YOU</span>
                    {live.length === 0 && <p className="nt-empty">Nothing needs you right now.</p>}
                    {live.map((n) => (
                        <div key={n.id} className="nt-item">
                            {n.slug ? <Link to="/d/$slug" params={{ slug: n.slug }} onClick={() => setOpen(false)}>{n.text}</Link> : <Link to={n.to as "/dsa/review"} onClick={() => setOpen(false)}>{n.text}</Link>}
                            <button className="nt-x" aria-label={`Dismiss: ${n.text}`} title="Dismiss for today" onClick={() => dismiss(n.id)}>×</button>
                        </div>
                    ))}
                    {showNew && (
                        <>
                            <span className="nt-lab">NEW</span>
                            <div className="nt-item">
                                <Link to="/settings" hash="new" onClick={() => { markNewSeen(); setOpen(false); }}>anneal was updated: {WHATS_NEW[0]!.title}</Link>
                                <button className="nt-x" aria-label="Dismiss what's new" title="Mark as seen" onClick={markNewSeen}>×</button>
                            </div>
                        </>
                    )}
                    <span className="nt-lab">RECENT <button className="nt-clear" onClick={clearNotes} disabled={notes.length === 0}>clear</button></span>
                    {notes.length === 0 && <p className="nt-empty">Nothing yet. What you log, undo and change shows up here.</p>}
                    {notes.slice(0, 8).map((n) => (
                        <div key={n.id} className={`nt-note ${n.kind}`}>
                            <span>{n.title}{n.text ? <small> {n.text}</small> : null}</span>
                            <time>{ago(n.at)}</time>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
