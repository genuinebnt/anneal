import { Arrow, Box, type Fig } from "./parts";

/** Figures for the distributed and advanced pages. Add each name to `SD_FIGURES_DISTRIBUTED` in `crates/content/src/sd.rs`. */
export const FIGS_DISTRIBUTED: Record<string, Fig> = {
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
