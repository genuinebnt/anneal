import type { CSSProperties, ReactNode } from "react";

/** The small looping drawing of a pattern's idea (the cards of genuinebasil.dev, in anneal's colours). Always moving; still under Reduce
 *  motion (styles/loops.css). Every class is `lp-` prefixed. Add a drawing here, and its pattern to `LOOP_OF`. */
export type LoopKind = "buffer-pool" | "latch-crabbing" | "query-pipeline" | "recovery" | "ownership" | "skip-list" | "sliding-window" | "two-pointers" | "binary-search" | "stack" | "linked-list" | "trees" | "graphs" | "dp-1d" | "backtracking" | "hashing" | "heap" | "tries" | "advanced-graphs" | "dp-2d" | "greedy" | "intervals" | "math" | "bits";

export const LOOP_OF: Record<string, LoopKind> = {
    "Arrays & Hashing": "hashing",
    "Two Pointers": "two-pointers",
    "Sliding Window": "sliding-window",
    Stack: "stack",
    "Binary Search": "binary-search",
    "Linked List": "linked-list",
    Trees: "trees",
    Graphs: "graphs",
    "1-D Dynamic Programming": "dp-1d",
    Backtracking: "backtracking",
    "Heap / Priority Queue": "heap",
    Tries: "tries",
    "Advanced Graphs": "advanced-graphs",
    "2-D Dynamic Programming": "dp-2d",
    Greedy: "greedy",
    Intervals: "intervals",
    "Math & Geometry": "math",
    "Bit Manipulation": "bits",
};

const Cells = ({ n = 8, x0 = 14, w = 22, g = 4, y = 32 }: { n?: number; x0?: number; w?: number; g?: number; y?: number }) => (
    <>{Array.from({ length: n }, (_, i) => <rect key={i} className="lp-cl" x={x0 + i * (w + g)} y={y} width={w} height={w} rx={4} />)}</>
);

/** Bar heights of the greedy drawing: each is taken from the tallest down. */
const GREEDY = [22, 38, 14, 46, 30, 54];

const delay = (s: number): CSSProperties => ({ animationDelay: `${s}s` });

const DRAW: Record<LoopKind, ReactNode> = {
    "buffer-pool": (
        <>
            <text className="lp-tx" x={14} y={16}>LRU-K</text>
            {Array.from({ length: 5 }, (_, i) => <rect key={i} className={`lp-fr${i === 1 ? " lp-ev" : ""}`} style={delay(i * 0.5)} x={44 + i * 32} y={8} width={26} height={14} rx={3} />)}
            <text className="lp-tx" x={14} y={48}>ARC</text>
            {Array.from({ length: 5 }, (_, i) => <rect key={i} className="lp-fr lp-g" style={delay(0.3 + i * 0.5)} x={44 + i * 32} y={40} width={26} height={14} rx={3} />)}
            <text className="lp-tc" x={206} y={18} style={{ fill: "color-mix(in oklab,var(--bad) 65%,var(--panel))" }}>evict</text>
            <text className="lp-tc" x={206} y={50} style={{ fill: "var(--lp-c)" }}>hit</text>
        </>
    ),
    "latch-crabbing": (
        <>
            <path className="lp-ed" d="M112 16 L68 42 M112 16 L156 42 M68 42 L40 68 M68 42 L96 68" />
            <rect className="lp-cl" x={92} y={6} width={40} height={18} rx={4} />
            <rect className="lp-cl" x={48} y={33} width={40} height={18} rx={4} />
            <rect className="lp-cl" x={20} y={59} width={40} height={18} rx={4} />
            <rect className="lp-cl" x={136} y={33} width={40} height={18} rx={4} />
            <rect className="lp-ac lp-la0" x={90} y={4} width={44} height={22} rx={5} />
            <rect className="lp-ac lp-la1" x={46} y={31} width={44} height={22} rx={5} />
            <rect className="lp-ac lp-la2" x={18} y={57} width={44} height={22} rx={5} />
        </>
    ),
    "query-pipeline": (
        <>
            {["scan", "filter", "join", "agg"].map((n, i) => (
                <g key={n}>
                    <rect className="lp-cl" x={10 + i * 54} y={30} width={42} height={24} rx={5} />
                    <text className="lp-tx" x={31 + i * 54} y={45} textAnchor="middle">{n}</text>
                    {i < 3 && <path className="lp-ed" d={`M${52 + i * 54} 42 L${64 + i * 54} 42`} />}
                </g>
            ))}
            {[0, 1.1, 2.2].map((d) => <circle key={d} className="lp-acf lp-tup" style={delay(d)} cx={14} cy={36} r={3} />)}
        </>
    ),
    recovery: (
        <>
            <rect className="lp-acf lp-pa" x={14} y={24} width={60} height={7} rx={2} style={{ fill: "var(--lp-cell)" }} />
            <rect className="lp-acf lp-pr2" x={74} y={38} width={64} height={7} rx={2} />
            <rect className="lp-acf lp-pu" x={138} y={52} width={60} height={7} rx={2} style={{ fill: "color-mix(in oklab,var(--lp-c) 45%,var(--panel))" }} />
            <path className="lp-cr" d="M74 14 L74 70" stroke="color-mix(in oklab,var(--bad) 65%,var(--panel))" strokeWidth={1.4} strokeDasharray="3 3" fill="none" />
            <text className="lp-tx" x={14} y={20}>analysis</text>
            <text className="lp-tx" x={78} y={35}>redo</text>
            <text className="lp-tx" x={142} y={49}>undo</text>
            <text className="lp-tc lp-cr" x={78} y={80} style={{ fill: "color-mix(in oklab,var(--bad) 65%,var(--panel))" }}>crash</text>
        </>
    ),
    ownership: (
        <>
            <rect className="lp-cl" x={20} y={26} width={60} height={34} rx={6} />
            <rect className="lp-cl" x={144} y={26} width={60} height={34} rx={6} />
            <text className="lp-tx" x={30} y={20}>a</text>
            <text className="lp-tx" x={154} y={20}>b</text>
            <rect className="lp-cf lp-mv" x={34} y={34} width={32} height={18} rx={4} />
            <path className="lp-ed" d="M84 43 L140 43 M134 38 L140 43 L134 48" />
            <text className="lp-tc lp-moved" x={50} y={76} textAnchor="middle">moved</text>
        </>
    ),
    "skip-list": (
        <>
            {[[10, [0, 3, 6]], [38, [0, 1, 3, 5, 6]], [66, [0, 1, 2, 3, 4, 5, 6]]].map(([y, xs]) => (xs as number[]).map((i) => <rect key={`${y}${i}`} className="lp-cl" x={20 + i * 28} y={y as number} width={20} height={14} rx={3} />))}
            {([[20, 10], [104, 10], [104, 38], [132, 38], [132, 66], [160, 66]] as const).map(([x, y], k) => <rect key={k} className="lp-sk" style={delay(k * 0.9)} x={x} y={y} width={20} height={14} rx={3} />)}
        </>
    ),
    "sliding-window": (
        <>
            <Cells />
            <rect className="lp-ac lp-sw" x={11} y={29} width={76} height={28} rx={7} />
            <text className="lp-tc" x={14} y={20}>L</text>
            <text className="lp-tc" x={86} y={20}>R</text>
        </>
    ),
    "two-pointers": (
        <>
            <Cells />
            <path className="lp-acf lp-pl" d="M25 68 L21 76 L29 76Z" />
            <path className="lp-acf lp-pr" d="M199 68 L195 76 L203 76Z" />
            <text className="lp-tx" x={14} y={18}>lo</text>
            <text className="lp-tx" x={196} y={18}>hi</text>
        </>
    ),
    "binary-search": (
        <>
            <Cells />
            <rect className="lp-ac lp-bs" x={11} y={29} width={208} height={28} rx={7} />
        </>
    ),
    stack: (
        <>
            <rect className="lp-cl" x={88} y={12} width={48} height={62} rx={4} style={{ strokeDasharray: "3 3" }} />
            <rect className="lp-cf lp-s1" x={94} y={52} width={36} height={16} rx={3} />
            <rect className="lp-cf lp-s2" x={94} y={33} width={36} height={16} rx={3} />
            <rect className="lp-cf lp-s3" x={94} y={14} width={36} height={16} rx={3} />
        </>
    ),
    "linked-list": (
        <>
            {Array.from({ length: 5 }, (_, i) => (
                <g key={i}>
                    <rect className="lp-cl" x={14 + i * 44} y={30} width={30} height={24} rx={5} />
                    {i < 4 && <path className="lp-ed" d={`M${44 + i * 44} 42 L${58 + i * 44} 42`} />}
                </g>
            ))}
            <circle className="lp-acf lp-sl" cx={29} cy={66} r={4.5} />
            <circle className="lp-ac lp-fa" cx={29} cy={20} r={4.5} style={{ fill: "var(--panel)" }} />
            <text className="lp-tx" x={14} y={12}>fast</text>
            <text className="lp-tx" x={14} y={82}>slow</text>
        </>
    ),
    trees: (
        <>
            <path className="lp-ed" d="M112 18 L70 44 M112 18 L154 44 M70 44 L46 68 M70 44 L94 68 M154 44 L130 68 M154 44 L178 68" />
            {([[112, 18], [70, 44], [46, 68], [94, 68], [154, 44], [130, 68], [178, 68]] as const).map(([x, y], k) => <circle key={k} className="lp-nd" style={delay(k * 0.9)} cx={x} cy={y} r={9} />)}
        </>
    ),
    graphs: (
        <>
            <path className="lp-ed" d="M40 42 L84 20 M40 42 L84 64 M84 20 L140 28 M84 64 L140 60 M140 28 L190 42 M140 60 L190 42" />
            {([[40, 42, 0], [84, 20, 1], [84, 64, 1], [140, 28, 2], [140, 60, 2], [190, 42, 3]] as const).map(([x, y, k], i) => <circle key={i} className="lp-gn" style={delay(k * 1.1)} cx={x} cy={y} r={9} />)}
        </>
    ),
    "dp-1d": (
        <>
            {Array.from({ length: 8 }, (_, i) => <rect key={i} className="lp-dp" style={delay(i * 0.55)} x={14 + i * 26} y={32} width={22} height={22} rx={4} />)}
            <path className="lp-ed" d="M85 24 Q98 12 111 24" />
            <text className="lp-tx" x={14} y={76}>dp[i] = dp[i-1] + dp[i-2]</text>
        </>
    ),
    backtracking: (
        <>
            <path className="lp-bt" d="M112 10 L60 38" />
            <path className="lp-bt" style={delay(0.4)} d="M112 10 L164 38" />
            <path className="lp-bt" style={delay(0.9)} d="M60 38 L34 68" />
            <path className="lp-bt lp-cut" style={delay(1.1)} d="M60 38 L86 68" />
            <path className="lp-bt" style={delay(1.4)} d="M164 38 L138 68" />
            <path className="lp-bt" style={delay(1.6)} d="M164 38 L190 68" />
        </>
    ),
    hashing: (
        <>
            {Array.from({ length: 6 }, (_, i) => <rect key={i} className="lp-cl" x={14 + i * 34} y={40} width={28} height={26} rx={5} />)}
            {([[1, 0], [3, 1.4], [4, 2.8]] as const).map(([i, d]) => <rect key={i} className="lp-cf lp-ah" style={delay(d)} x={20 + i * 34} y={44} width={16} height={18} rx={3} />)}
            <path className="lp-ed" d="M30 12 L48 12 M44 8 L48 12 L44 16" />
            <text className="lp-tx" x={54} y={15}>hash(key) % 6</text>
        </>
    ),
    heap: (
        <>
            <path className="lp-ed" d="M112 16 L70 42 M112 16 L154 42 M70 42 L46 68 M70 42 L94 68 M154 42 L130 68 M154 42 L178 68" />
            {([[112, 16, 0], [70, 42, 1], [154, 42, 1], [46, 68, 2], [94, 68, 2], [130, 68, 2], [178, 68, 2]] as const).map(([x, y, k], i) => <circle key={i} className="lp-gn" style={delay(k * 1.3)} cx={x} cy={y} r={9} />)}
            <text className="lp-tc" x={128} y={14}>min</text>
        </>
    ),
    tries: (
        <>
            {([["M112 14 L64 40", 0], ["M112 14 L160 40", 0.3], ["M64 40 L38 68", 0.9], ["M64 40 L90 68", 1.2], ["M160 40 L160 68", 1.5], ["M160 40 L186 68", 1.8]] as const).map(([d, t], i) => <path key={i} className="lp-bt" style={delay(t)} d={d} />)}
            {([[112, 14, ""], [64, 40, "c"], [160, 40, "d"], [38, 68, "a"], [90, 68, "o"], [160, 68, "o"], [186, 68, "e"]] as const).map(([x, y, c], i) => (
                <g key={i}>
                    <circle className="lp-cl" cx={x} cy={y} r={8} />
                    {c && <text className="lp-tc" x={x} y={y + 2.6} textAnchor="middle">{c}</text>}
                </g>
            ))}
        </>
    ),
    "advanced-graphs": (
        <>
            <path className="lp-ed" d="M40 42 L84 20 M40 42 L84 64 M84 20 L140 28 M84 64 L140 60 M140 28 L190 42 M140 60 L190 42" />
            {([[62, 28, "2"], [62, 56, "5"], [112, 21, "4"], [112, 64, "1"], [166, 32, "3"], [166, 58, "2"]] as const).map(([x, y, w], i) => <text key={i} className="lp-tx" x={x} y={y} textAnchor="middle">{w}</text>)}
            {([[40, 42, 0], [84, 20, 1], [84, 64, 2], [140, 28, 3], [140, 60, 3.6], [190, 42, 4.4]] as const).map(([x, y, k], i) => <circle key={i} className="lp-gn" style={delay(k * 0.8)} cx={x} cy={y} r={9} />)}
        </>
    ),
    "dp-2d": (
        <>
            {Array.from({ length: 15 }, (_, i) => <rect key={i} className="lp-dp" style={delay(((i % 5) + Math.floor(i / 5)) * 0.5)} x={46 + (i % 5) * 28} y={14 + Math.floor(i / 5) * 24} width={24} height={20} rx={4} />)}
            <path className="lp-ed" d="M58 52 L84 40" />
            <text className="lp-tx" x={14} y={80}>dp[i][j] from the cells above and to the left</text>
        </>
    ),
    greedy: (
        <>
            {GREEDY.map((h, i) => <rect key={i} className="lp-cl" x={20 + i * 32} y={74 - h} width={24} height={h} rx={4} />)}
            {GREEDY.map((h, i) => <rect key={i} className="lp-sk" style={delay([...GREEDY].sort((x, y) => y - x).indexOf(h) * 1.0)} x={20 + i * 32} y={74 - h} width={24} height={h} rx={4} />)}
        </>
    ),
    intervals: (
        <>
            {([[14, 10, 84], [58, 32, 80], [150, 54, 60]] as const).map(([x, y, w], i) => <rect key={i} className="lp-sk" style={delay(i * 1.1)} x={x} y={y} width={w} height={14} rx={4} />)}
            <rect className="lp-acf lp-pa" x={14} y={76} width={124} height={4} rx={2} />
            <text className="lp-tx" x={14} y={72}>merged</text>
        </>
    ),
    math: (
        <>
            {([[0, 0], [1, 0], [2, 0], [2, 1], [2, 2], [1, 2], [0, 2], [0, 1], [1, 1]] as const).map(([c, r], k) => <rect key={k} className="lp-dp" style={delay(k * 0.5)} x={82 + c * 30} y={8 + r * 26} width={26} height={22} rx={4} />)}
        </>
    ),
    bits: (
        <>
            {Array.from({ length: 8 }, (_, i) => <rect key={i} className={[1, 3, 4, 6].includes(i) ? "lp-dp" : "lp-cl"} style={[1, 3, 4, 6].includes(i) ? delay([0, 0.8, 1.6, 2.4][[1, 3, 4, 6].indexOf(i)]!) : undefined} x={14 + i * 26} y={30} width={22} height={26} rx={4} />)}
            {Array.from({ length: 8 }, (_, i) => <text key={i} className="lp-tx" x={25 + i * 26} y={47} textAnchor="middle">{[1, 3, 4, 6].includes(i) ? "1" : "0"}</text>)}
            <text className="lp-tc" x={14} y={20}>x ^ y</text>
        </>
    ),
};

/** A drawing by kind. `tone` picks the accent (copper for DSA, violet for Rust, green for the courses). */
export function Loop({ kind, tone = "acc", className = "" }: { kind: LoopKind; tone?: "acc" | "vio" | "grn"; className?: string }) {
    return (
        <svg className={`lp lp-${tone}${className ? ` ${className}` : ""}`} viewBox="0 0 224 84" aria-hidden="true" focusable="false">
            {DRAW[kind]}
        </svg>
    );
}

/** The drawing for a pattern (by name), or nothing when it has none yet. */
export function PatternLoop({ pattern, tone = "acc", className = "" }: { pattern: string; tone?: "acc" | "vio" | "grn"; className?: string }) {
    const kind = LOOP_OF[pattern];
    return kind ? <Loop kind={kind} tone={tone} className={className} /> : null;
}
