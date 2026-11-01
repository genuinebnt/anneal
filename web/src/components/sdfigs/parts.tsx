import type { ReactNode } from "react";

/** The drawing pieces every system design figure shares. Each group's figures live in their own file in this folder (so pages
 *  written in parallel never edit the same map); `SdFigure.tsx` joins them. */
export type Fig = { steps: number; ms: number; draw: (t: number) => ReactNode };

export function Box({ x, y, w = 96, h = 36, label, tone = "line", r = 9, sub }: { x: number; y: number; w?: number; h?: number; label: string; tone?: string; r?: number; sub?: string }) {
    return (
        <g className={`sf-box sf-${tone}`}>
            <rect x={x} y={y} width={w} height={h} rx={r} />
            <text x={x + w / 2} y={y + h / 2 + (sub ? -2 : 4)}>{label}</text>
            {sub && <text className="sf-sub" x={x + w / 2} y={y + h / 2 + 12}>{sub}</text>}
        </g>
    );
}

export function Arrow({ d, on = false, dash = false }: { d: string; on?: boolean; dash?: boolean }) {
    return <path className={`sf-arrow${on ? " on" : ""}`} d={d} strokeDasharray={dash ? "4 4" : undefined} markerEnd="url(#sf-ar)" />;
}

/** A partition: cells with offsets, the tail filling as records arrive. */
export function Log({ x, y, n, filled, reads = [], label }: { x: number; y: number; n: number; filled: number; reads?: { at: number; tone: string; label: string }[]; label: string }) {
    return (
        <g>
            <text className="sf-lab" x={x} y={y - 8}>{label}</text>
            {Array.from({ length: n }, (_, i) => (
                <g key={i}>
                    <rect className={`sf-cell${i < filled ? " full" : ""}${i === filled - 1 ? " new" : ""}`} x={x + i * 30} y={y} width={26} height={26} rx={5} />
                    <text className="sf-off" x={x + i * 30 + 13} y={y + 17}>{i}</text>
                </g>
            ))}
            {reads.map((r, i) => (
                <g key={r.label} className={`sf-${r.tone}`} transform={`translate(${x + r.at * 30 + 13} ${y + 34})`}>
                    <path className="sf-pin" d="M0 0 -6 9 6 9z" />
                    {/* readers at nearby offsets would print over each other: alternate their labels' height */}
                    <text className="sf-pinl" y={22 + i * 13}>{r.label}</text>
                </g>
            ))}
        </g>
    );
}
