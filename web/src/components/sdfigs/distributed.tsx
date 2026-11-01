import { Arrow, Box, type Fig } from "./parts";

/** Figures for the distributed and advanced pages. Add each name to `SD_FIGURES_DISTRIBUTED` in `crates/content/src/sd.rs`. */
export const FIGS_DISTRIBUTED: Record<string, Fig> = {
    "vector-clocks": {
        steps: 9,
        ms: 1100,
        draw: (t) => {
            const lane: Record<string, number> = { A: 50, B: 115, C: 180 };
            // events in the order they are revealed; `from` is the event whose message this one receives
            const ev: { n: string; x: number; v: string; from?: number }[] = [
                { n: "A", x: 120, v: "[1,0,0]" },
                { n: "B", x: 210, v: "[1,1,0]", from: 0 },
                { n: "C", x: 170, v: "[0,0,1]" },
                { n: "B", x: 310, v: "[1,2,0]" },
                { n: "C", x: 400, v: "[1,2,2]", from: 3 },
                { n: "A", x: 450, v: "[2,0,0]" },
            ];
            const shown = Math.min(t, ev.length);
            const pair = t === 7 ? [0, 4] : t === 8 ? [5, 4] : [];
            return (
                <>
                    {Object.entries(lane).map(([n, y]) => (
                        <g key={n}>
                            <text className="sf-nl" x={30} y={y + 4}>{n}</text>
                            <path className="sf-ring" d={`M50 ${y} H600`} />
                        </g>
                    ))}
                    {ev.slice(0, shown).map((e, i) => {
                        const y = lane[e.n]!;
                        const src = e.from !== undefined ? ev[e.from]! : null;
                        return (
                            <g key={i}>
                                {src && <Arrow d={`M${src.x} ${lane[src.n]!} L${e.x - 8} ${y - 4}`} on={i === t - 1} />}
                                <circle className={pair.includes(i) ? "sf-node" : `sf-key${i === t - 1 ? " moved" : ""}`} cx={e.x} cy={y} r={pair.includes(i) ? 9 : 6} />
                                <text className="sf-lab" x={e.x - 22} y={y - 12}>{e.v}</text>
                            </g>
                        );
                    })}
                    <text className="sf-lab" x={14} y={212}>
                        {t === 0
                            ? "each node keeps one counter per node"
                            : t <= ev.length
                              ? ev[t - 1]!.from !== undefined
                                  ? "a receive: entry-wise max with the message, then tick your own"
                                  : "a local event or send: tick your own entry"
                              : t === 7
                                ? "[1,0,0] ≤ [1,2,2] in every entry: the first happened before"
                                : "[2,0,0] and [1,2,2]: each is ahead somewhere, so they are concurrent"}
                    </text>
                </>
            );
        },
    },
    "count-min": {
        steps: 9,
        ms: 1000,
        draw: (t) => {
            // three rows of ten counters; each word lands in one counter per row (cat and dog share a counter in row 1)
            const at: Record<string, number[]> = { cat: [2, 7, 4], dog: [5, 7, 1], emu: [2, 3, 8] };
            const stream = ["cat", "dog", "cat", "emu", "dog", "cat"];
            const added = stream.slice(0, Math.min(t, stream.length));
            const grid = [0, 1, 2].map((r) => Array.from({ length: 10 }, (_, c) => added.filter((w) => at[w]![r] === c).length));
            const word = t >= 1 && t <= 6 ? stream[t - 1]! : t === 7 ? "cat" : t === 8 ? "dog" : "";
            const query = t >= 7;
            const vals = word ? at[word]!.map((c, r) => grid[r]![c]!) : [];
            const truth = added.filter((w) => w === word).length;
            return (
                <>
                    <Box x={14} y={84} w={110} label={word ? `${query ? "count" : "add"} ${word}` : "empty sketch"} tone={query ? "vio" : "acc"} />
                    {grid.map((row, r) => (
                        <g key={r}>
                            <text className="sf-lab" x={170} y={52 + r * 50}>{`h${r + 1}`}</text>
                            {row.map((v, c) => {
                                const hit = word !== "" && at[word]![r] === c;
                                return (
                                    <g key={c}>
                                        <rect className={`sf-cell${v > 0 ? " full" : ""}${hit ? " new" : ""}`} x={200 + c * 40} y={30 + r * 50} width={34} height={34} rx={5} />
                                        <text className="sf-off" x={217 + c * 40} y={51 + r * 50}>{v}</text>
                                    </g>
                                );
                            })}
                            {word && <Arrow d={`M124 102 L${198 + at[word]![r]! * 40} ${47 + r * 50}`} on />}
                        </g>
                    ))}
                    <text className="sf-lab" x={14} y={208}>
                        {t === 0
                            ? "d rows of w counters, one hash per row"
                            : !query
                              ? `+1 in one counter per row (${added.length} of ${stream.length} added)`
                              : `rows say ${vals.join(", ")}: the minimum, ${Math.min(...vals)}, is the estimate (true ${truth})`}
                    </text>
                </>
            );
        },
    },
};
