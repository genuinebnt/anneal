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
    /** delivery-framework: a 45-minute round as a bar of phases, each with what it should leave on the board. */
    "delivery-timeline": {
        steps: 7,
        ms: 1600,
        draw: (t) => {
            const phases: { name: string; short: string; min: number; out: string }[] = [
                { name: "Requirements", short: "Reqs", min: 5, out: "3-5 functional lines, measurable non-functional ones" },
                { name: "Entities", short: "E", min: 2, out: "User, Link, Click: a list, no fields yet" },
                { name: "API", short: "API", min: 5, out: "POST /links, GET /{code}: one endpoint per requirement" },
                { name: "High-level design", short: "High-level design", min: 13, out: "boxes and arrows that work end to end, tables beside the DB" },
                { name: "Deep dives", short: "Deep dives", min: 15, out: "the hot path, failures, the hard edge cases" },
                { name: "Wrap-up", short: "Wrap-up", min: 5, out: "bottlenecks left, what you would do next; their questions" },
            ];
            const k = Math.min(t, phases.length - 1);
            const scale = 600 / 45;
            let x = 20;
            return (
                <>
                    <text className="sf-lab" x={20} y={22}>a 45-minute round</text>
                    {phases.map((p, i) => {
                        const w = p.min * scale;
                        const box = <Box key={p.name} x={x} y={34} w={w - 4} h={44} r={6} label={p.short} sub={p.min >= 5 ? `${p.min} min` : undefined} tone={i === k && t < phases.length ? "acc" : i < k || t >= phases.length ? "grn" : "line"} />;
                        x += w;
                        return box;
                    })}
                    {t < phases.length ? (
                        <>
                            <text className="sf-lab" x={20} y={120}>{`now: ${phases[k]!.name} (${phases[k]!.min} min)`}</text>
                            <text className="sf-lab" x={20} y={142}>{`on the board: ${phases[k]!.out}`}</text>
                        </>
                    ) : (
                        <text className="sf-lab" x={20} y={120}>a design that works first, then the parts that make it hard</text>
                    )}
                    <text className="sf-lab" x={20} y={200}>functional requirements shape the design; non-functional ones, the deep dives</text>
                </>
            );
        },
    },
    /** unique-ids: a Snowflake id is time, machine and sequence side by side; the sequence counts within a millisecond. */
    "snowflake-bits": {
        steps: 8,
        ms: 900,
        draw: (t) => {
            // three ids in millisecond 1000, two in 1001, one in 1002; then the clock steps back and the generator refuses
            const ISSUED: [number, number][] = [[1000, 0], [1000, 1], [1000, 2], [1001, 0], [1001, 1], [1002, 0]];
            const refused = t >= 6;
            const ms = refused ? [999, 1001][t - 6]! : ISSUED[t]![0];
            const seq = refused ? 0 : ISSUED[t]![1];
            const issued = ISSUED.slice(0, Math.min(t + 1, ISSUED.length));
            const fresh = !refused && seq === 0 && t > 0;
            return (
                <>
                    <Box x={14} y={20} w={26} h={40} r={4} label="0" />
                    <Box x={44} y={20} w={300} h={40} r={4} label={refused ? `clock reads ${ms} ms: behind 1002` : `${ms} ms since the epoch`} tone={refused ? "bad" : fresh ? "warn" : "acc"} />
                    <Box x={348} y={20} w={110} h={40} r={4} label="machine 7" tone="vio" />
                    <Box x={462} y={20} w={150} h={40} r={4} label={refused ? "–" : `sequence ${seq}`} tone={refused ? "line" : "grn"} />
                    <text className="sf-lab" x={14} y={78}>1</text>
                    <text className="sf-lab" x={44} y={78}>41 bits: time (69 years)</text>
                    <text className="sf-lab" x={348} y={78}>10 bits: 1,024</text>
                    <text className="sf-lab" x={462} y={78}>12 bits: 4,096 a ms</text>
                    <text className="sf-lab" x={14} y={110}>ids issued (time · machine · sequence), each larger than the last</text>
                    {issued.map(([m, q], k) => <Box key={k} x={14 + k * 102} y={120} w={96} h={30} r={6} label={`${m}·7·${q}`} tone={k === t ? "grn" : "line"} />)}
                    <text className="sf-lab" x={14} y={190}>{refused ? "the clock is behind the last id's time (1002): refuse, or wait until it passes" : fresh ? "a new millisecond: the sequence restarts at 0" : "same millisecond: the sequence counts up"}</text>
                    <text className="sf-lab" x={14} y={208}>no coordination: another machine has its own id, so its ids never collide with these</text>
                </>
            );
        },
    },
};
