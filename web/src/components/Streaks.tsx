import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api, type StreakDay, type StreakMarkKind, type StreakRepair, type Streaks } from "../api";
import { toast } from "./toasts";
import { useDialog } from "./useDialog";

export const useStreaks = () => useQuery({ queryKey: ["streaks"], queryFn: api.streaks });

/** Freezes are worked out in halves: "2", "2.5". */
export const fmt = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(1));

export const niceDay = (date: string) => new Date(`${date}T12:00:00`).toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });

export const addDays = (date: string, n: number) => {
    const d = new Date(`${date}T12:00:00`);
    d.setDate(d.getDate() + n);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

/** The broken days worth offering a repair for: the last week, and only ones that cut a streak (something kept before them). */
export const offered = (s: Streaks | undefined): StreakRepair[] => (s ? s.repairs.filter((r) => r.date >= addDays(s.today, -7) && r.before > 0) : []);

/** Marks a day, or takes a mark back. Every number that depends on the streak is refreshed. */
export function useMarkDay() {
    const qc = useQueryClient();
    const done = (s: Streaks) => {
        qc.setQueryData(["streaks"], s);
        for (const key of ["progress", "activity", "plan", "dsa"]) void qc.invalidateQueries({ queryKey: [key] });
    };
    const fail = (e: unknown) => toast("er", "Couldn't do that", e instanceof Error ? e.message : String(e));
    const mark = useMutation({ mutationFn: (v: { day: string; kind: StreakMarkKind }) => api.markDay(v.day, v.kind), onSuccess: done, onError: fail });
    const undo = useMutation({ mutationFn: (id: number) => api.unmarkDay(id), onSuccess: done, onError: fail });
    return { mark, undo, busy: mark.isPending || undo.isPending };
}

const NAME: Record<StreakMarkKind, string> = { freeze: "frozen", holiday: "a holiday", sick: "a sick day" };

/** What can be done with one day: spend a freeze on it, call it a holiday or a sick day, or take a mark back. Shown in the repair dialog and in
 *  the Calendar's day panel. A perfect streak has no shortcut: the choices never make a day perfect. */
export function MarkChoices({ s, date, repair, after }: { s: Streaks; date: string; repair?: StreakRepair; after?: () => void }) {
    const { mark, undo, busy } = useMarkDay();
    const day: StreakDay | undefined = s.days.find((d) => d.date === date);
    if (!day) return null;
    const recent = date === s.today || date === addDays(s.today, -1);
    const run = (kind: StreakMarkKind) => mark.mutate({ day: date, kind }, { onSuccess: () => { toast("ok", `${niceDay(date)} is ${NAME[kind]}`); after?.(); } });
    if (day.mark_id !== null) {
        return (
            <div className="sk-choices">
                <p className="sk-note">
                    {day.state === "frozen" ? "A freeze was used here. Undoing it gives the freeze back, and the day is missed again." : "This day is neutral: it neither breaks the streak nor adds to it."}
                </p>
                <button className="sk-btn" disabled={busy} onClick={() => undo.mutate(day.mark_id!, { onSuccess: () => { toast("in", "Mark undone", niceDay(date)); after?.(); } })}>Undo this mark</button>
            </div>
        );
    }
    const missed = day.state === "missed";
    const unsolved = day.state === "missed" || day.state === "open" || day.state === "free";
    if (!missed && !(recent && unsolved)) return null;
    const canFreeze = s.freezes >= 1;
    return (
        <div className="sk-choices">
            {missed && repair && (
                <div className="sk-join" aria-label="What the streak becomes">
                    <i>{repair.before} before</i>
                    <span>+</span>
                    <i className="new">this day</i>
                    <span>+</span>
                    <i>{repair.after} after</i>
                    <span>=</span>
                    <i className="sum">{repair.before + 1 + repair.after} days</i>
                </div>
            )}
            {missed && (
                <div className="sk-row">
                    <span><b>Use a freeze</b><small>{canFreeze ? `Costs 1. You have ${fmt(s.freezes)}.` : `You have ${fmt(s.freezes)}. Do an extra problem to earn one, or extra reviews for half.`}</small></span>
                    <button className="sk-btn pri" disabled={busy || !canFreeze} onClick={() => run("freeze")}>Repair</button>
                </div>
            )}
            {recent && (
                <>
                    <div className="sk-row">
                        <span><b>{missed ? "It was a holiday" : "Mark a holiday"}</b><small>Free. Keeps the streak, adds nothing.</small></span>
                        <button className="sk-btn" disabled={busy} onClick={() => run("holiday")}>Mark</button>
                    </div>
                    <div className="sk-row">
                        <span><b>{missed ? "I was sick" : "Mark a sick day"}</b><small>Free. The same as a holiday.</small></span>
                        <button className="sk-btn" disabled={busy} onClick={() => run("sick")}>Mark</button>
                    </div>
                </>
            )}
            {missed && !recent && <p className="sk-note">A holiday or a sick day can only be marked for today or yesterday. This day can still be repaired with a freeze.</p>}
        </div>
    );
}

/** The dialog that offers to repair the newest broken day. Not now closes it; the chance stays in the bell and the Calendar. */
export function RepairDialog({ s, repair, onClose }: { s: Streaks; repair: StreakRepair; onClose: () => void }) {
    const box = useRef<HTMLDivElement>(null);
    useDialog(box, true, onClose);
    return createPortal(
        <div className="sk-scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="sk-dlg" ref={box} role="dialog" aria-modal="true" aria-label={`${niceDay(repair.date)} was missed`}>
                <h3>{niceDay(repair.date)} was missed</h3>
                <p className="sk-note">Your {repair.before}-day streak ended there. You can still join it up.</p>
                <MarkChoices s={s} date={repair.date} repair={repair} after={onClose} />
                <div className="sk-foot">
                    <span>{fmt(s.freezes)} {s.freezes === 1 ? "freeze" : "freezes"}</span>
                    <button className="sk-btn" onClick={onClose}>Not now</button>
                </div>
            </div>
        </div>,
        document.body,
    );
}

const GLYPH: Record<string, string> = { perfect: "★", kept: "✓", frozen: "F", holiday: "H", sick: "S", missed: "×", open: "·", free: "–" };
const LABEL: Record<string, string> = { perfect: "perfect", kept: "kept", frozen: "frozen", holiday: "holiday", sick: "sick", missed: "missed", open: "today", free: "nothing planned" };

/** The last four weeks as a strip of days, each with its state. */
export function DayStrip({ s, count = 28, onPick }: { s: Streaks; count?: number; onPick?: (date: string) => void }) {
    const days = s.days.slice(-count);
    return (
        <div className="sk-strip" role="list">
            {days.map((d) => (
                <button
                    key={d.date}
                    role="listitem"
                    className={`sk-day ${d.state}${d.date === s.today ? " today" : ""}`}
                    title={`${niceDay(d.date)}: ${LABEL[d.state]}${d.earned > 0 ? `, +${fmt(d.earned)} freeze` : ""}`}
                    aria-label={`${niceDay(d.date)}: ${LABEL[d.state]}`}
                    onClick={() => onPick?.(d.date)}
                    disabled={!onPick}
                >
                    <span className="g">{GLYPH[d.state]}</span>
                </button>
            ))}
        </div>
    );
}

export const STREAK_LEGEND: [string, string][] = [["perfect", "plan met in full"], ["kept", "a solve"], ["frozen", "freeze used"], ["holiday", "holiday"], ["sick", "sick"], ["missed", "missed"]];
export const GLYPH_OF = GLYPH;

/** The Streaks box on Progress: the four numbers, the freeze bar, the last four weeks, and the rules in one place. */
export function StreaksBox() {
    const q = useStreaks();
    const [repairing, setRepairing] = useState(false);
    const s = q.data;
    if (!s) return null;
    const open = offered(s)[0];
    const toNext = s.freezes % 1;
    return (
        <div className="pbox sk-box" style={{ marginTop: 20 }}>
            <h3>Streaks and freezes</h3>
            <p className="sub">A perfect day is the day's plan met in full. A freeze mends a missed day, but never makes it perfect.</p>
            <div className="sk-stats">
                <div className="sk-stat sk-streak"><span>STREAK</span><b>{s.streak}</b><small>planned days kept</small></div>
                <div className="sk-stat sk-perfect"><span>PERFECT STREAK</span><b>{s.perfect}</b><small>plan met in full</small></div>
                <div className="sk-stat sk-freeze">
                    <span>FREEZES</span><b>{fmt(s.freezes)}</b><small>earned {fmt(s.earned)}, spent {s.spent}</small>
                    <div className="sk-bar" aria-hidden="true"><i style={{ width: `${(toNext || (s.freezes > 0 ? 1 : 0)) * 100}%` }} /></div>
                </div>
                <div className="sk-stat"><span>LONGEST</span><b>{s.longest}</b><small>days kept in a row</small></div>
            </div>
            <DayStrip s={s} />
            <div className="sk-legend">
                {STREAK_LEGEND.map(([k, label]) => (
                    <span key={k}><i className={`sk-day ${k}`}>{GLYPH[k]}</i>{label}</span>
                ))}
            </div>
            {open && (
                <div className="sk-row">
                    <span><b>{niceDay(open.date)} was missed</b><small>Joins {open.before} before and {open.after} after into {open.before + 1 + open.after} days.</small></span>
                    <button className="sk-btn pri" onClick={() => setRepairing(true)}>Repair</button>
                </div>
            )}
            {repairing && open && <RepairDialog s={s} repair={open} onClose={() => setRepairing(false)} />}
            <details className="sk-rules">
                <summary>How streaks work</summary>
                <ul>
                    <li><b>Streak</b>: every planned problem day with a solve adds one; a planned day with none breaks it. Rest, practice and break days neither add nor break.</li>
                    <li><b>Perfect streak</b>: a day counts only when that day's planned problems and planned reviews were all done. A freeze or a miss ends it.</li>
                    <li><b>Freezes</b> have no limit. An extra new problem beyond the plan earns 1. A review done ahead of its due date earns half, but only once in a row per problem, so repeating a review earns nothing.</li>
                    <li>A freeze goes on a missed planned day, on any later day. A holiday or a sick day can be marked for today or yesterday; both are free and neutral.</li>
                </ul>
            </details>
        </div>
    );
}
