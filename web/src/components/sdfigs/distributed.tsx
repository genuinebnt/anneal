import { Arrow, Box, type Fig } from "./parts";

/** Figures for the distributed and advanced pages. Add each name to `SD_FIGURES_DISTRIBUTED` in `crates/content/src/sd.rs`. */
export const FIGS_DISTRIBUTED: Record<string, Fig> = {
    "trie-topk": {
        steps: 5,
        ms: 1300,
        draw: (t) => {
            // queries and how often they were searched: cat 90, dog 80, car 70, cart 40, cab 30, cot 10
            const nodes: { id: string; x: number; y: number; parent?: string; top: string }[] = [
                { id: "", x: 40, y: 110, top: "cat 90 · dog 80 · car 70" },
                { id: "c", x: 150, y: 70, parent: "", top: "cat 90 · car 70 · cart 40" },
                { id: "d", x: 150, y: 165, parent: "", top: "dog 80" },
                { id: "ca", x: 260, y: 50, parent: "c", top: "cat 90 · car 70 · cart 40" },
                { id: "co", x: 260, y: 105, parent: "c", top: "cot 10" },
                { id: "do", x: 260, y: 165, parent: "d", top: "dog 80" },
                { id: "cat", x: 370, y: 20, parent: "ca", top: "cat 90" },
                { id: "car", x: 370, y: 60, parent: "ca", top: "car 70 · cart 40" },
                { id: "cab", x: 370, y: 100, parent: "ca", top: "cab 30" },
                { id: "cot", x: 370, y: 140, parent: "co", top: "cot 10" },
                { id: "dog", x: 370, y: 180, parent: "do", top: "dog 80" },
                { id: "cart", x: 470, y: 60, parent: "car", top: "cart 40" },
            ];
            const typed = ["", "c", "ca", "car", "cart"][t]!;
            const at = (id: string) => nodes.find((n) => n.id === id)!;
            const onPath = (id: string) => typed.startsWith(id);
            const cur = at(typed);
            return (
                <>
                    {nodes.filter((n) => n.parent !== undefined).map((n) => {
                        const p = at(n.parent!);
                        return <path key={`e${n.id}`} className={`sf-arrow${onPath(n.id) ? " on" : ""}`} d={`M${p.x + 11} ${p.y} L${n.x - 11} ${n.y}`} />;
                    })}
                    {nodes.map((n) => (
                        <g key={n.id || "root"} className={n.id === typed ? "sf-new" : ""}>
                            <circle className="sf-node" cx={n.x} cy={n.y} r={11} />
                            <text className="sf-nl" x={n.x} y={n.y + 4}>{n.id ? n.id.slice(-1) : "·"}</text>
                        </g>
                    ))}
                    <Box x={420} y={110} w={210} h={50} label={`typed "${typed}": ${typed.length} step${typed.length === 1 ? "" : "s"} down`} tone="grn" sub={cur.top} />
                    <text className="sf-lab" x={14} y={212}>each node stores its best completions, so a request is a walk of the prefix and one read</text>
                </>
            );
        },
    },
    watermark: {
        steps: 12,
        ms: 900,
        draw: (t) => {
            // event times in minutes, in the order the job receives them; the watermark trails the newest by half a minute
            const arrivals = [0.2, 0.5, 0.9, 1.1, 0.7, 1.6, 2.2, 1.4, 2.7, 0.8, 3.3];
            const x = (m: number) => 60 + m * 135;
            const seen = arrivals.slice(0, t);
            let max = 0;
            const fired = new Set<number>();
            const late = new Set<number>();
            seen.forEach((e, i) => {
                const w = Math.floor(e);
                if (fired.has(w)) {
                    late.add(i);
                    return;
                }
                max = Math.max(max, e);
                for (let k = 0; k < 4; k++) if (k + 1 <= max - 0.5) fired.add(k);
            });
            const wm = Math.max(0, max - 0.5);
            const counts = [0, 1, 2, 3].map((k) => seen.filter((e, i) => Math.floor(e) === k && !late.has(i)).length);
            return (
                <>
                    {[0, 1, 2, 3].map((k) => (
                        <g key={k}>
                            <rect className={`sf-cell${fired.has(k) ? " full" : ""}`} x={x(k) + 2} y={60} width={131} height={60} rx={6} />
                            <text className="sf-lab" x={x(k) + 8} y={52}>{`${k}:00-${k + 1}:00`}</text>
                            {fired.has(k) && <text className="sf-lab" x={x(k) + 8} y={138}>{`fired: ${counts[k]} clicks`}</text>}
                        </g>
                    ))}
                    {seen.map((e, i) =>
                        late.has(i) ? (
                            <g key={i}>
                                <circle className="sf-node" cx={x(e)} cy={170} r={6} />
                                <text className="sf-lab" x={x(e) + 10} y={174}>late: its window already fired</text>
                            </g>
                        ) : (
                            <circle key={i} className={`sf-key${i === t - 1 ? " moved" : ""}`} cx={x(e)} cy={90} r={5} />
                        ),
                    )}
                    {t > 0 && <path className="sf-arrow on" d={`M${x(wm)} 20 V150`} strokeDasharray="4 4" />}
                    {t > 0 && <text className="sf-lab" x={x(wm) + 6} y={24}>{`watermark ${wm.toFixed(1)}`}</text>}
                    <text className="sf-lab" x={14} y={212}>{t === 0 ? "one-minute windows on event time; the watermark trails the newest event by 0.5 min" : late.has(t - 1) ? "an event behind the watermark: drop it, side-output it, or correct the result" : "a window fires when the watermark passes its end"}</text>
                </>
            );
        },
    },
    "geohash-cells": {
        steps: 6,
        ms: 1200,
        draw: (t) => {
            const x0 = 330, y0 = 10, s = 50;
            const q = { x: 427, y: 104 }; // near the corner of its cell
            const r = 44;
            const pts = [[352, 40], [410, 30], [470, 52], [520, 70], [390, 84], [445, 88], [470, 120], [416, 126], [440, 140], [378, 150], [500, 160], [430, 190], [356, 118], [512, 112], [462, 74], [404, 62], [540, 30], [345, 195]] as const;
            const cellOf = (x: number, y: number) => [Math.floor((x - x0) / s), Math.floor((y - y0) / s)] as const;
            const [qc, qr] = cellOf(q.x, q.y);
            const inOwn = (x: number, y: number) => { const [c, rr] = cellOf(x, y); return c === qc && rr === qr; };
            const inNine = (x: number, y: number) => { const [c, rr] = cellOf(x, y); return Math.abs(c - qc) <= 1 && Math.abs(rr - qr) <= 1; };
            const near = (x: number, y: number) => Math.hypot(x - q.x, y - q.y) <= r;
            const lit = (cx: number, cy: number) => (t === 2 ? cx === qc && cy === qr : t >= 3 ? Math.abs(cx - qc) <= 1 && Math.abs(cy - qr) <= 1 : false);
            const found = pts.filter(([x, y]) => near(x, y)).length;
            const own = pts.filter(([x, y]) => inOwn(x, y) && near(x, y)).length;
            const cand = pts.filter(([x, y]) => inNine(x, y)).length;
            const say = [
                "points filed by the cell they fall in",
                "find everything within the circle",
                `the own cell alone finds ${own} of ${found}: the point sits near an edge`,
                `the cell and its 8 neighbours: ${cand} candidates`,
                `filter by exact distance: all ${found} found`,
                "cells at least as big as the radius always cover the circle",
            ];
            return (
                <>
                    {Array.from({ length: 16 }, (_, i) => {
                        const cx = i % 4, cy = Math.floor(i / 4);
                        return <rect key={i} className={`sf-cell${lit(cx, cy) ? " full" : ""}`} x={x0 + cx * s} y={y0 + cy * s} width={s - 2} height={s - 2} rx={4} />;
                    })}
                    {t >= 1 && <circle className="sf-ring" cx={q.x} cy={q.y} r={r} />}
                    {pts.map(([x, y], i) => {
                        const on = (t === 2 && inOwn(x, y) && near(x, y)) || (t === 3 && inNine(x, y)) || (t >= 4 && near(x, y));
                        return <circle key={i} className={`sf-key${on ? " moved" : ""}`} cx={x} cy={y} r={4} />;
                    })}
                    {t >= 1 && <circle className="sf-node" cx={q.x} cy={q.y} r={5} />}
                    <Box x={14} y={40} w={250} h={44} label={t < 2 ? "query: within the circle" : t === 2 ? "own cell only" : t === 3 ? "own cell + 8 neighbours" : "then exact distance"} tone={t === 2 ? "bad" : t >= 4 ? "grn" : "acc"} />
                    <text className="sf-lab" x={14} y={212}>{say[t]}</text>
                </>
            );
        },
    },
    "fencing-token": {
        steps: 8,
        ms: 1100,
        draw: (t) => {
            const holder = t <= 2 ? 1 : t >= 4 ? 2 : 0;
            const token = t >= 4 ? 34 : 33;
            const highest = t >= 5 ? 34 : t >= 1 ? 33 : 0;
            const c1 = t === 6 || t === 7 ? "bad" : t >= 2 && t <= 5 ? "warn" : "acc";
            const c1sub = t === 0 ? "lease, token 33" : t === 1 ? "writes with 33" : t >= 2 && t <= 5 ? "paused (GC)" : "wakes, writes with 33";
            const say = [
                "client 1 takes the lease and gets token 33",
                "its write carries 33; storage remembers 33",
                "client 1 stops for a long garbage-collection pause",
                "the lease expires while it sleeps",
                "client 2 takes the lease: token 34",
                "client 2 writes with 34; storage now expects at least 34",
                "client 1 wakes, still sure it holds the lock, and writes with 33",
                "storage rejects 33 < 34: the stale holder cannot do damage",
            ];
            return (
                <>
                    <Box x={14} y={30} w={150} label="client 1" tone={c1} sub={c1sub} />
                    <Box x={14} y={130} w={150} label="client 2" tone={t >= 4 ? "acc" : "line"} sub={t >= 4 ? "lease, token 34" : "waiting"} />
                    <Box x={250} y={80} w={140} label="lock service" tone={t === 3 ? "warn" : "line"} sub={holder ? `lease: client ${holder}, ${token}` : "lease expired"} />
                    <Box x={470} y={80} w={150} label="storage" tone={t === 7 ? "grn" : "line"} sub={`highest token ${highest || "–"}`} />
                    {t === 0 && <Arrow d="M248 92 L166 52" on />}
                    {t === 4 && <Arrow d="M248 110 L166 146" on />}
                    {(t === 1 || t === 6) && <Arrow d="M164 44 Q320 20 468 88" on dash={t === 6} />}
                    {t === 5 && <Arrow d="M164 150 Q320 190 468 112" on />}
                    <text className="sf-lab" x={14} y={212}>{say[t]}</text>
                </>
            );
        },
    },
    "raft-log": {
        steps: 10,
        ms: 900,
        draw: (t) => {
            // the leader appends one entry a step (4 to 7); followers trail it by 1, 2 and 3 steps; S5 is down
            const leaderLen = (s: number) => 3 + Math.max(0, Math.min(4, s));
            const lens = [leaderLen(t), leaderLen(t - 1), leaderLen(t - 2), leaderLen(t - 3), 3];
            const commit = [...lens].sort((a, b) => b - a)[2]!;
            const names = ["S1 leader", "S2", "S3", "S4", "S5"];
            return (
                <>
                    {lens.map((len, r) => (
                        <g key={r}>
                            <text className="sf-lab" x={14} y={r * 40 + 27}>{names[r]}</text>
                            {Array.from({ length: 8 }, (_, i) => (
                                <rect key={i} className={`sf-cell${i < Math.min(len, commit) ? " full" : ""}${i >= commit && i < len ? " new" : ""}`} x={110 + i * 30} y={r * 40 + 10} width={26} height={26} rx={5} />
                            ))}
                            {r === 4 && <text className="sf-lab" x={360} y={r * 40 + 27}>down</text>}
                        </g>
                    ))}
                    <Box x={420} y={60} w={190} label={`commit index ${commit}`} tone="grn" sub="held by 3 of 5" />
                    <text className="sf-lab" x={420} y={130}>{t >= 1 && t <= 4 ? "client command appended" : t > 4 && commit < 7 ? "followers catch up" : "all caught up"}</text>
                    <text className="sf-lab" x={14} y={214}>an entry commits once a majority of logs hold it; S5 being down changes nothing</text>
                </>
            );
        },
    },
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
