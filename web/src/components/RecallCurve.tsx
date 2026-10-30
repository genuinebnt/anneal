import { useQuery } from "@tanstack/react-query";
import { api } from "../api";

/** How likely you are to remember this problem on each of the next 60 days if you leave it, with the day it is due and the retention the schedule aims for. */
export function RecallCurve({ id, version }: { id: string; version: number }) {
    const q = useQuery({ queryKey: ["dsa-forecast", id, version], queryFn: () => api.forecast(id) });
    const pts = q.data?.points;
    if (!pts || pts.length < 2) return null;
    const W = 300, H = 96, L = 4, R = 4, T = 8, B = 18;
    const x = (d: number) => L + (d / (pts.length - 1)) * (W - L - R);
    const y = (r: number) => T + (1 - r) * (H - T - B);
    const line = pts.map((r, d) => `${d ? "L" : "M"}${x(d).toFixed(1)} ${y(r).toFixed(1)}`).join(" ");
    const due = q.data?.due_in;
    const target = q.data?.retention ?? 0.85;
    const label = `Recall chance over the next 60 days: ${Math.round(pts[0]! * 100)}% now, ${Math.round(pts[pts.length - 1]! * 100)}% in 60 days${due != null ? `. Review due ${due <= 0 ? "now" : `in ${due} day${due === 1 ? "" : "s"}`}` : ""}.`;
    return (
        <div className="pp-card rc-card">
            <h3><span className="hinted" tabIndex={0} data-tip="How likely you are to solve this problem cold. It falls as days pass and resets when you review it.">RECALL</span><span>next 60 days</span></h3>
            <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={label}>
                <path className="rc-area" d={`${line} L${x(pts.length - 1)} ${y(0)} L${x(0)} ${y(0)}Z`} />
                <line className="rc-target" x1={L} x2={W - R} y1={y(target)} y2={y(target)} />
                {due != null && due >= 0 && due < pts.length && <line className="rc-due" x1={x(due)} x2={x(due)} y1={T} y2={H - B} />}
                <path className="rc-line" d={line} />
                <text x={L} y={H - 4}>today {Math.round(pts[0]! * 100)}%</text>
                <text x={W - R} y={H - 4} textAnchor="end">60 days {Math.round(pts[pts.length - 1]! * 100)}%</text>
                {due != null && due >= 0 && due < pts.length && <text x={Math.min(x(due) + 4, W - 40)} y={T + 8}>due</text>}
            </svg>
        </div>
    );
}
