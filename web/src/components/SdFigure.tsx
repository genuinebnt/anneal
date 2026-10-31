import { useEffect, useState, type ReactNode } from "react";

/** A step counter that ticks while the figure is on screen, unless motion is reduced (then it holds a still frame). */
function useTick(ms: number, steps: number) {
    const [t, setT] = useState(0);
    const [paused, setPaused] = useState(false);
    useEffect(() => {
        const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches || document.documentElement.dataset.motion === "reduce";
        if (reduce || paused) return;
        const id = window.setInterval(() => setT((x) => (x + 1) % steps), ms);
        return () => window.clearInterval(id);
    }, [ms, steps, paused]);
    return { t, paused, toggle: () => setPaused((p) => !p) };
}

const W = 640;
const H = 220;

function Box({ x, y, w = 96, h = 36, label, tone = "line", r = 9, sub }: { x: number; y: number; w?: number; h?: number; label: string; tone?: string; r?: number; sub?: string }) {
    return (
        <g className={`sf-box sf-${tone}`}>
            <rect x={x} y={y} width={w} height={h} rx={r} />
            <text x={x + w / 2} y={y + h / 2 + (sub ? -2 : 4)}>{label}</text>
            {sub && <text className="sf-sub" x={x + w / 2} y={y + h / 2 + 12}>{sub}</text>}
        </g>
    );
}

function Arrow({ d, on = false, dash = false }: { d: string; on?: boolean; dash?: boolean }) {
    return <path className={`sf-arrow${on ? " on" : ""}`} d={d} strokeDasharray={dash ? "4 4" : undefined} markerEnd="url(#sf-ar)" />;
}

/** A partition: cells with offsets, the tail filling as records arrive. */
function Log({ x, y, n, filled, reads = [], label }: { x: number; y: number; n: number; filled: number; reads?: { at: number; tone: string; label: string }[]; label: string }) {
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

const FIGS: Record<string, { steps: number; ms: number; draw: (t: number) => ReactNode }> = {
    "kafka-log": {
        steps: 12,
        ms: 700,
        draw: (t) => (
            <>
                <Box x={14} y={40} label="Producer" tone="acc" />
                <Arrow d="M110 58 H168" on={t % 2 === 0} />
                <Log x={180} y={44} n={12} filled={Math.min(12, 4 + t)} label="partition 0 · append only" reads={[{ at: Math.min(11, 2 + Math.floor(t / 2)), tone: "vio", label: "billing" }, { at: Math.min(11, t), tone: "grn", label: "search" }]} />
                <Log x={180} y={150} n={12} filled={Math.min(12, 2 + Math.floor(t * 0.8))} label="partition 1" />
            </>
        ),
    },
    "consumer-groups": {
        steps: 6,
        ms: 900,
        draw: (t) => (
            <>
                {[0, 1, 2, 3].map((p) => <Box key={p} x={30} y={14 + p * 50} w={110} label={`partition ${p}`} tone={t % 4 === p ? "acc" : "line"} />)}
                <text className="sf-lab" x={250} y={14}>group: billing</text>
                <Box x={250} y={30} w={110} label="consumer A" tone="vio" sub="p0, p1" />
                <Box x={250} y={110} w={110} label="consumer B" tone="vio" sub="p2, p3" />
                <text className="sf-lab" x={470} y={14}>group: search</text>
                <Box x={470} y={70} w={130} label="consumer C" tone="grn" sub="all four" />
                <Arrow d="M140 32 Q200 40 248 46" on={t % 4 === 0} />
                <Arrow d="M140 82 Q200 70 248 52" on={t % 4 === 1} />
                <Arrow d="M140 132 Q200 130 248 126" on={t % 4 === 2} />
                <Arrow d="M140 182 Q200 160 248 132" on={t % 4 === 3} />
                <Arrow d={`M140 ${32 + (t % 4) * 50} Q320 200 468 92`} on dash />
            </>
        ),
    },
    "queue-vs-stream": {
        steps: 8,
        ms: 800,
        draw: (t) => (
            <>
                <text className="sf-lab" x={20} y={20}>queue: each message to one worker, then gone</text>
                {[0, 1, 2, 3, 4].map((i) => (i >= t % 6 ? <rect key={i} className="sf-cell full" x={20 + (i - (t % 6)) * 30 + 60} y={34} width={26} height={26} rx={5} /> : null))}
                <Box x={250} y={30} w={90} label="worker 1" tone={t % 2 ? "acc" : "line"} />
                <Box x={350} y={30} w={90} label="worker 2" tone={t % 2 ? "line" : "acc"} />
                <text className="sf-lab" x={20} y={118}>stream: the log stays, each reader keeps its place</text>
                <Log x={20} y={132} n={10} filled={10} label="" reads={[{ at: Math.min(9, t), tone: "vio", label: "billing" }, { at: Math.min(9, Math.floor(t / 2)), tone: "grn", label: "analytics" }]} />
            </>
        ),
    },
    shards: {
        steps: 8,
        ms: 700,
        draw: (t) => {
            const load = [3, 2, 3, 2 + t];
            return (
                <>
                    <Box x={20} y={92} w={110} label="hash(key) % 4" tone="acc" />
                    {[0, 1, 2, 3].map((s) => (
                        <g key={s}>
                            <Arrow d={`M130 110 L${238} ${36 + s * 48}`} on={(t + s) % 4 === 0} />
                            <Box x={240} y={18 + s * 48} w={110} label={`shard ${s}`} tone={s === 3 && t > 4 ? "bad" : "line"} />
                            <rect className={`sf-bar${s === 3 && t > 4 ? " hot" : ""}`} x={370} y={26 + s * 48} width={Math.min(240, load[s]! * 22)} height={20} rx={5} />
                        </g>
                    ))}
                    <text className="sf-lab" x={370} y={212}>{t > 4 ? "a celebrity's key: shard 3 runs hot" : "keys spread evenly"}</text>
                </>
            );
        },
    },
    "hash-ring": {
        steps: 10,
        ms: 800,
        draw: (t) => {
            const cx = 200, cy = 110, r = 80;
            const at = (a: number) => [cx + r * Math.cos((a - 90) * (Math.PI / 180)), cy + r * Math.sin((a - 90) * (Math.PI / 180))] as const;
            const servers = [20, 130, 250, ...(t >= 5 ? [190] : [])];
            const keys = [60, 100, 160, 200, 300, 340];
            const owner = (k: number) => [...servers].sort((a, b) => a - b).find((s) => s >= k) ?? Math.min(...servers);
            return (
                <>
                    <circle className="sf-ring" cx={cx} cy={cy} r={r} />
                    {servers.map((s) => { const [x, y] = at(s); return <g key={s} className={s === 190 ? "sf-new" : ""}><circle className="sf-node" cx={x} cy={y} r={11} /><text className="sf-nl" x={x} y={y + 4}>{["A", "B", "C", "D"][servers.indexOf(s)]}</text></g>; })}
                    {keys.map((k) => { const [x, y] = at(k); const moved = t >= 5 && owner(k) === 190; return <circle key={k} className={`sf-key${moved ? " moved" : ""}`} cx={x} cy={y} r={5} />; })}
                    <text className="sf-lab" x={330} y={80}>{t >= 5 ? "D joins: only the keys between C and D move" : "each key belongs to the next server clockwise"}</text>
                    <text className="sf-lab" x={330} y={104}>{t >= 5 ? "every other key stays where it was" : "4 servers, 6 keys"}</text>
                </>
            );
        },
    },
    "cache-aside": {
        steps: 8,
        ms: 800,
        draw: (t) => {
            const hit = t < 4;
            return (
                <>
                    <Box x={20} y={92} label="App" tone="acc" />
                    <Box x={260} y={30} w={110} label="Cache" tone={hit ? "grn" : "line"} sub={hit ? "hit · 0.5 ms" : "miss"} />
                    <Box x={260} y={150} w={110} label="Database" tone={hit ? "line" : "warn"} sub={hit ? "" : "5 ms"} />
                    <Arrow d="M116 104 L258 52" on />
                    {!hit && <Arrow d="M116 116 L258 166" on={t % 2 === 0} />}
                    {!hit && <Arrow d="M315 148 L315 72" on={t % 2 === 1} dash />}
                    <text className="sf-lab" x={420} y={100}>{hit ? "read the cache first" : "on a miss, read the database"}</text>
                    <text className="sf-lab" x={420} y={124}>{hit ? "most reads end here" : "and fill the cache with a TTL"}</text>
                </>
            );
        },
    },
    replication: {
        steps: 8,
        ms: 800,
        draw: (t) => (
            <>
                <Box x={20} y={92} label="Writes" tone="acc" />
                <Box x={200} y={92} w={110} label="Leader" tone="warn" sub={`v${4 + t}`} />
                <Box x={420} y={30} w={130} label="Follower 1" tone="grn" sub={`v${4 + t}`} />
                <Box x={420} y={150} w={130} label="Follower 2" tone="vio" sub={`v${4 + Math.max(0, t - 2)} · ${t >= 2 ? "lagging" : "caught up"}`} />
                <Arrow d="M116 110 H198" on={t % 2 === 0} />
                <Arrow d="M310 104 L418 52" on />
                <Arrow d="M310 118 L418 166" on={t % 3 === 0} dash />
            </>
        ),
    },
    "btree-index": {
        steps: 6,
        ms: 900,
        draw: (t) => (
            <>
                <Box x={250} y={14} w={140} label="[ 40 | 80 ]" tone={t >= 1 ? "acc" : "line"} />
                <Box x={70} y={90} w={140} label="[ 10 | 25 ]" />
                <Box x={250} y={90} w={140} label="[ 50 | 65 ]" tone={t >= 2 ? "acc" : "line"} />
                <Box x={430} y={90} w={140} label="[ 90 | 99 ]" />
                {[0, 1, 2, 3].map((i) => <Box key={i} x={180 + i * 72} y={170} w={64} h={30} label={["50", "57", "61", "65"][i]!} tone={t >= 3 && i === 2 ? "grn" : "line"} />)}
                <Arrow d="M300 50 L160 88" />
                <Arrow d="M320 50 L320 88" on={t >= 1} />
                <Arrow d="M340 50 L480 88" />
                <Arrow d="M320 126 L360 168" on={t >= 2} />
                <text className="sf-lab" x={20} y={212}>{t >= 3 ? "find 61: three page reads instead of a scan" : "looking up 61…"}</text>
            </>
        ),
    },
    "redis-structures": {
        steps: 8,
        ms: 700,
        draw: (t) => {
            const cmds = ["INCR views:42", "ZINCRBY board 5 ana", "LPUSH jobs j9", "SADD likes:7 u3", "GET user:1", "ZREVRANGE board 0 2", "BRPOP jobs", "SET lock:x NX EX 10"];
            return (
                <>
                    <text className="sf-lab" x={20} y={20}>commands arrive from many clients</text>
                    {cmds.slice(0, 5).map((c, i) => <Box key={c} x={20} y={30 + i * 36} w={170} h={28} label={cmds[(t + i) % cmds.length]!} tone={i === 0 ? "acc" : "line"} r={6} />)}
                    <Box x={240} y={92} w={120} label="one thread" tone="warn" sub="one at a time" />
                    <Arrow d="M190 44 L238 104" on />
                    {["string", "hash", "list", "set", "sorted set"].map((s, i) => <Box key={s} x={420} y={14 + i * 40} w={150} h={32} label={s} tone={i === t % 5 ? "grn" : "line"} r={6} />)}
                    <Arrow d={`M360 110 L418 ${30 + (t % 5) * 40}`} on />
                </>
            );
        },
    },
    "write-fanout": {
        steps: 8,
        ms: 600,
        draw: (t) => (
            <>
                {[0, 1, 2, 3].map((i) => <Box key={i} x={14} y={14 + i * 48} w={88} h={30} label={`client ${i + 1}`} tone={(t + i) % 3 === 0 ? "acc" : "line"} />)}
                <Log x={150} y={70} n={8} filled={Math.min(8, 3 + t)} label="log (Kafka)" />
                <Box x={420} y={66} w={90} label="batcher" tone="vio" sub="per second" />
                <Box x={540} y={66} w={86} label="DB" tone="grn" sub={`+${(t + 1) * 120}`} />
                <Arrow d="M102 60 L148 82" on={t % 2 === 0} />
                <Arrow d="M392 84 H418" on />
                <Arrow d="M510 84 H538" on={t % 4 === 3} />
            </>
        ),
    },
};

/** An animated drawing for a system design page, by name (the names are listed in `crates/content/src/sd.rs`). */
export function SdFigure({ name, caption }: { name: string; caption: string }) {
    const fig = FIGS[name];
    const { t, paused, toggle } = useTick(fig?.ms ?? 1000, fig?.steps ?? 1);
    if (!fig) return null;
    return (
        <figure className="sd-fig">
            <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={caption}>
                <defs>
                    <marker id="sf-ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                        <path d="M0 0 10 5 0 10z" fill="currentColor" />
                    </marker>
                </defs>
                {fig.draw(t)}
            </svg>
            <figcaption>
                <span>{caption}</span>
                <button onClick={toggle}>{paused ? "Play" : "Pause"}</button>
            </figcaption>
        </figure>
    );
}
