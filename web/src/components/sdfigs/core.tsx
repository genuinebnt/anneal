import { Arrow, Box, type Fig } from "./parts";

/** load-balancing: the loads of six servers after `t` requests placed by two random choices, one request finishing per step. */
const P2C_PAIRS: [number, number][] = [[0, 1], [2, 5], [3, 4], [1, 5], [0, 3], [2, 4], [1, 2], [0, 5]];
function p2cState(t: number) {
    const load = [3, 1, 4, 2, 5, 2];
    let pick = 0;
    for (let s = 0; s <= t; s++) {
        const [a, b] = P2C_PAIRS[s % P2C_PAIRS.length]!;
        pick = load[b]! < load[a]! ? b : a;
        if (s < t) {
            load[pick]! += 1;
            const done = (s * 5 + 4) % 6; // some server finishes a request
            if (load[done]! > 0) load[done]! -= 1;
        }
    }
    return { load, pick, pair: P2C_PAIRS[t % P2C_PAIRS.length]! };
}

/** Figures for the core, fundamentals, patterns, low-level design and Rust pages. Add each name to `SD_FIGURES_CORE` in `crates/content/src/sd.rs`. */
export const FIGS_CORE: Record<string, Fig> = {
    /** api-styles: two new posts arrive between page 1 and page 2; the offset page repeats two posts, the cursor page does not. */
    "offset-vs-cursor": {
        steps: 7,
        ms: 1000,
        draw: (t) => {
            const arrived = t >= 2;
            const feed = arrived ? [12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1] : [10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
            const page2 = t >= 3;
            const seen = new Set([10, 9, 8, 7]);
            const row = (y: number, cursor: boolean) =>
                feed.map((id, i) => {
                    const onPage = page2 ? (cursor ? id <= 6 && id >= 3 : i >= 4 && i < 8) : seen.has(id);
                    const tone = !page2 ? (onPage ? "acc" : arrived && id > 10 ? "warn" : "line") : onPage ? (seen.has(id) ? "bad" : "grn") : arrived && id > 10 ? "warn" : "line";
                    return <Box key={`${y}-${id}`} x={20 + i * 50} y={y} w={42} h={30} r={6} label={String(id)} tone={tone} />;
                });
            return (
                <>
                    <text className="sf-lab" x={20} y={22}>LIMIT 4 OFFSET 4</text>
                    {row(32, false)}
                    <text className="sf-lab" x={20} y={84}>{page2 ? "page 2 is positions 4 to 7: posts 8 and 7 again" : t === 2 ? "posts 12 and 11 arrive: every position shifts by two" : "page 1: posts 10, 9, 8, 7"}</text>
                    <text className="sf-lab" x={20} y={122}>WHERE (created_at, id) &lt; last seen LIMIT 4</text>
                    {row(132, true)}
                    <text className="sf-lab" x={20} y={184}>{page2 ? "page 2 starts after post 7: 6, 5, 4, 3" : t === 2 ? "the cursor still names post 7" : "page 1 ends at post 7: that is the cursor"}</text>
                    <text className="sf-lab" x={20} y={210}>newest on the left</text>
                </>
            );
        },
    },
    /** load-balancing: power of two choices. Each request samples two servers at random and goes to the less loaded one. */
    "two-choices": {
        steps: 8,
        ms: 1100,
        draw: (t) => {
            const { load, pick, pair } = p2cState(t);
            const other = pair[0] === pick ? pair[1] : pair[0];
            return (
                <>
                    <Box x={14} y={92} w={110} label={`request ${t + 1}`} tone="acc" />
                    {load.map((l, i) => (
                        <g key={i}>
                            <Box x={300} y={10 + i * 34} w={100} h={28} r={6} label={`server ${i}`} tone={i === pick ? "grn" : i === other ? "warn" : "line"} />
                            <rect className="sf-bar" x={416} y={16 + i * 34} width={Math.max(4, l * 26)} height={16} rx={4} />
                            <text className="sf-lab" x={422 + Math.max(4, l * 26)} y={28 + i * 34}>{l}</text>
                        </g>
                    ))}
                    <Arrow d={`M124 104 L298 ${24 + other * 34}`} dash />
                    <Arrow d={`M124 112 L298 ${24 + pick * 34}`} on />
                    <text className="sf-lab" x={14} y={180}>{`samples servers ${pair[0]} and ${pair[1]}`}</text>
                    <text className="sf-lab" x={14} y={198}>{`server ${pick} has ${load[pick]} queued, server ${other} has ${load[other]}`}</text>
                    <text className="sf-lab" x={14} y={216}>{`→ server ${pick}; no global view needed`}</text>
                </>
            );
        },
    },
};
