import { Box, type Fig } from "./parts";

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
};
