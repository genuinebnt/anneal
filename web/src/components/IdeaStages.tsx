import type { CSSProperties } from "react";
import type { Frame } from "../ideaDemos";

/** The drawings other than the row of cells that an Idea animation can use: a graph or tree, a table, intervals on a line, a queue.
 *  Their classes are `id-nv` (nodes), `id-tb` (table), `id-rn` (ranges) and `id-qu` (queue) in styles/idea.css. */

const R = 17;

export function GraphView({ g }: { g: NonNullable<Frame["graph"]> }) {
    const W = g.w ?? 380;
    const H = g.h ?? 220;
    const pad = 26;
    const px = (x: number) => pad + x * (W - 2 * pad);
    const py = (y: number) => pad + y * (H - 2 * pad - 8);
    const at = new Map(g.nodes.map((n) => [n.id, n]));
    return (
        <svg className="id-nv" width={W} height={H} viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${g.nodes.length} nodes, ${g.edges.length} edges`}>
            <defs>
                {(["id-nv-ah", "id-nv-ah-on", "id-nv-ah-hit"] as const).map((id) => (
                    <marker key={id} id={id} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto"><path d="M0 0 L8 4 L0 8Z" className={id} /></marker>
                ))}
            </defs>
            {g.edges.map((e) => {
                const a = at.get(e.a);
                const b = at.get(e.b);
                if (!a || !b) return null;
                const x1 = px(a.x), y1 = py(a.y), x2 = px(b.x), y2 = py(b.y);
                const len = Math.hypot(x2 - x1, y2 - y1) || 1;
                const ux = (x2 - x1) / len, uy = (y2 - y1) / len;
                const sx = x1 + ux * R, sy = y1 + uy * R;
                const ex = x2 - ux * (R + (e.directed ? 3 : 0)), ey = y2 - uy * (R + (e.directed ? 3 : 0));
                const marker = e.directed ? `url(#${e.tone === "hit" ? "id-nv-ah-hit" : e.tone ? "id-nv-ah-on" : "id-nv-ah"})` : undefined;
                return (
                    <g key={`${e.a}-${e.b}`} className={`id-nv-e${e.tone ? ` id-nv-${e.tone}` : ""}`}>
                        <path d={`M${sx} ${sy} L${ex} ${ey}`} markerEnd={marker} />
                        {e.w !== undefined && <text x={(x1 + x2) / 2 - uy * 9} y={(y1 + y2) / 2 + ux * 9 + 3} textAnchor="middle">{e.w}</text>}
                    </g>
                );
            })}
            {g.nodes.map((n) => (
                <g key={n.id} className={`id-nv-n${n.tone ? ` id-nv-${n.tone}` : ""}`} transform={`translate(${px(n.x)} ${py(n.y)})`}>
                    <circle r={R} />
                    <text y={5} textAnchor="middle">{n.label}</text>
                    {n.sub !== undefined && <text className="id-nv-sub" y={R + 14} textAnchor="middle">{n.sub}</text>}
                </g>
            ))}
        </svg>
    );
}

export function GridView({ t }: { t: NonNullable<Frame["grid"]> }) {
    const cols = t.cells[0]?.length ?? 0;
    const tone = new Map((t.marks ?? []).map((m) => [`${m.r},${m.c}`, m.tone]));
    return (
        <div className={`id-tb${t.small ? " id-tb-s" : ""}`} style={{ "--cols": cols + (t.rowLabels ? 1 : 0) } as CSSProperties} role="img" aria-label={t.cells.map((r) => r.join(" ")).join("; ")}>
            {t.colLabels && (
                <div className="id-tb-r">
                    {t.rowLabels && <i className="id-tb-h" />}
                    {t.colLabels.map((l, c) => <i key={c} className="id-tb-h">{l}</i>)}
                </div>
            )}
            {t.cells.map((row, r) => (
                <div key={r} className="id-tb-r">
                    {t.rowLabels && <i className="id-tb-h">{t.rowLabels[r]}</i>}
                    {row.map((v, c) => <b key={c} className={`id-tb-c${tone.has(`${r},${c}`) ? ` id-tb-${tone.get(`${r},${c}`)}` : ""}`}>{v}</b>)}
                </div>
            ))}
        </div>
    );
}

export function RangesView({ r }: { r: NonNullable<Frame["ranges"]> }) {
    const W = 400;
    const [lo, hi] = r.axis;
    const span = Math.max(hi - lo, 1);
    const x = (v: number) => ((v - lo) / span) * W;
    return (
        <div className="id-rn" style={{ width: W, height: r.lanes * 30 + 26 }} role="img" aria-label={r.bars.map((b) => `${b.label}: ${b.from} to ${b.to}`).join("; ")}>
            {Array.from({ length: hi - lo + 1 }, (_, i) => <span key={i} className="id-rn-t" style={{ left: x(lo + i) }}><small>{lo + i}</small></span>)}
            {r.bars.map((b) => (
                <div key={`${b.lane}-${b.label}`} className={`id-rn-b${b.tone ? ` id-rn-${b.tone}` : ""}`} style={{ left: x(b.from), width: Math.max(x(b.to) - x(b.from), 8), top: b.lane * 30 }}>{b.label}</div>
            ))}
            {r.marker !== undefined && <u className="id-rn-m" style={{ left: x(r.marker), height: r.lanes * 30 }} />}
        </div>
    );
}

export function QueueView({ q }: { q: NonNullable<Frame["queue"]> }) {
    return (
        <div className="id-quewrap">
            <span className="id-stl">{q.label}</span>
            <div className="id-qu" aria-label={`${q.label}: ${q.items.join(" ") || "empty"}`}>
                <small>front</small>
                {q.items.map((v, n) => <div key={`${n}${v}`} className={`id-cell id-s${q.top === "push" && n === q.items.length - 1 ? " id-enter" : ""}`}><b>{v}</b></div>)}
                {q.items.length === 0 && <em>empty</em>}
            </div>
        </div>
    );
}
