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
    "cache-stampede": {
        steps: 10,
        ms: 800,
        draw: (t) => {
            const lease = t >= 5;
            const k = t % 5;
            return (
                <>
                    <text className="sf-lab" x={14} y={20}>{lease ? "with a lease" : "no protection"}</text>
                    {[0, 1, 2, 3, 4].map((i) => <Box key={i} x={14} y={30 + i * 34} w={86} h={26} label={`request ${i + 1}`} tone={lease && i === 0 ? "acc" : "line"} r={6} />)}
                    <Box x={250} y={84} w={120} label="Cache" tone={k >= 3 ? "grn" : "warn"} sub={k >= 3 ? "refilled" : "key expired"} />
                    <Box x={480} y={84} w={130} label="Database" tone={!lease && k >= 1 && k < 3 ? "bad" : "line"} sub={lease ? (k >= 1 && k < 3 ? "1 query" : "") : k >= 1 && k < 3 ? "5 identical queries" : ""} />
                    {[0, 1, 2, 3, 4].map((i) => <Arrow key={i} d={`M100 ${43 + i * 34} L248 ${90 + i * 6}`} on={k === 0} />)}
                    {k >= 1 && k < 3 && (lease ? [2] : [0, 1, 2, 3, 4]).map((i) => <Arrow key={i} d={`M370 ${90 + i * 6} L478 ${90 + i * 6}`} on />)}
                    {lease && k >= 1 && k < 3 && <text className="sf-lab" x={120} y={204}>requests 2-5 wait a moment, then read the cache</text>}
                    {!lease && k >= 1 && k < 3 && <text className="sf-lab" x={120} y={204}>every miss rebuilds the same value</text>}
                    {k >= 3 && <text className="sf-lab" x={120} y={204}>{lease ? "the database saw one query" : "the database saw five; at scale, thousands"}</text>}
                </>
            );
        },
    },
    "lsm-tree": {
        steps: 10,
        ms: 800,
        draw: (t) => {
            const mem = t < 4 ? t + 1 : t === 4 ? 0 : t < 8 ? t - 4 : 3;
            const l0 = t < 4 ? 1 : t < 9 ? 2 : 0;
            const l1 = t < 9 ? 1 : 2;
            const merging = t === 8;
            return (
                <>
                    <Box x={14} y={84} w={84} label="Writes" tone="acc" />
                    <Box x={150} y={22} w={150} h={32} label="WAL · append" r={6} />
                    <Box x={150} y={96} w={150} label="memtable" tone={t === 4 ? "line" : "warn"} sub={`${mem} of 4 keys · sorted`} />
                    <Arrow d="M98 96 L148 40" on={t % 2 === 0} />
                    <Arrow d="M98 104 L148 112" on={t % 2 === 0} />
                    <Arrow d="M300 114 L328 76" on={t === 4} dash />
                    <text className="sf-lab" x={330} y={30}>L0 · flushed files</text>
                    {Array.from({ length: l0 }, (_, i) => <Box key={i} x={330 + i * 92} y={40} w={84} h={32} label={`SSTable ${i + 1}`} tone={merging ? "vio" : t === 4 && i === 1 ? "grn" : "line"} r={6} />)}
                    <text className="sf-lab" x={330} y={122}>L1 · compacted</text>
                    {Array.from({ length: l1 }, (_, i) => <Box key={i} x={330 + i * 112} y={132} w={104} h={32} label={i === 0 ? "a…m" : "n…z"} tone={t === 9 && i === 1 ? "grn" : "line"} r={6} />)}
                    {merging && <Arrow d="M420 74 L420 128" on />}
                    <text className="sf-lab" x={14} y={208}>{t === 4 ? "memtable full: written out as an immutable sorted file" : merging ? "compaction merges files, keeps the newest version of each key" : t === 9 ? "fewer files to check on each read" : "a write is a log append plus an in-memory insert"}</text>
                </>
            );
        },
    },
    saga: {
        steps: 9,
        ms: 850,
        draw: (t) => {
            const steps = ["Reserve stock", "Charge card", "Book courier"];
            const comp = ["Release stock", "Refund"];
            // the courier step fails at t = 3; the charge is refunded at 4 and the stock released at 5
            const undone = (i: number) => (i === 1 && t >= 4) || (i === 0 && t >= 5);
            const state = (i: number) => (i === 2 ? (t >= 3 ? "bad" : t === 2 ? "acc" : "line") : undone(i) ? "vio" : t > i ? "grn" : t === i ? "acc" : "line");
            return (
                <>
                    <Box x={14} y={84} w={100} label="Orchestrator" tone="warn" />
                    {steps.map((s, i) => <Box key={s} x={170 + i * 156} y={40} w={130} label={s} tone={state(i)} sub={i === 2 && t >= 3 ? "failed" : t > i && i < 2 ? "done" : ""} />)}
                    {comp.map((c, i) => <Box key={c} x={170 + i * 156} y={140} w={130} h={32} label={c} tone={(i === 1 && t >= 4) || (i === 0 && t >= 5) ? "vio" : "line"} r={6} />)}
                    <Arrow d="M300 58 H324" on={t === 1} />
                    <Arrow d="M456 58 H480" on={t === 2} />
                    {t >= 4 && <Arrow d="M391 76 L391 136" on={t === 4} dash />}
                    {t >= 5 && <Arrow d="M235 76 L235 136" on={t === 5} dash />}
                    <text className="sf-lab" x={14} y={208}>{t < 3 ? "each step commits on its own" : t === 3 ? "a step fails: undo what already happened" : t < 6 ? "compensations run in reverse order" : "the order ends cancelled, nothing half done"}</text>
                </>
            );
        },
    },
    "circuit-breaker": {
        steps: 12,
        ms: 750,
        draw: (t) => {
            const st = t < 4 ? "closed" : t < 8 ? "open" : t < 10 ? "half-open" : "closed";
            const fails = t < 4 ? t + 1 : 0;
            const passes = st === "closed" || (st === "half-open" && t === 9);
            return (
                <>
                    <Box x={14} y={84} w={90} label="Caller" tone="acc" />
                    <Box x={210} y={84} w={120} label="Breaker" tone={st === "open" ? "bad" : st === "half-open" ? "warn" : "grn"} sub={st} />
                    <Box x={450} y={84} w={120} label="Payments" tone={t < 4 ? "bad" : t >= 9 ? "grn" : "line"} sub={t < 4 ? "timing out" : t >= 9 ? "recovered" : "resting"} />
                    <Arrow d="M104 102 H208" on />
                    {passes && <Arrow d="M330 102 H448" on={t % 2 === 0} />}
                    {st === "open" && <text className="sf-lab" x={210} y={150}>fails fast · fallback</text>}
                    {["closed", "open", "half-open"].map((s, i) => <Box key={s} x={170 + i * 110} y={10} w={100} h={26} label={s} tone={s === st ? (s === "open" ? "bad" : s === "half-open" ? "warn" : "grn") : "line"} r={13} />)}
                    <text className="sf-lab" x={14} y={208}>{st === "closed" && t < 4 ? `calls pass · failures ${fails} of 4` : st === "open" ? "too many failures: stop calling for a while" : st === "half-open" ? "a trial call goes through" : "it worked: closed again"}</text>
                </>
            );
        },
    },
    quorum: {
        steps: 8,
        ms: 900,
        draw: (t) => {
            const write = t < 4;
            const v = (i: number) => (write ? (i < 2 && t >= 1 ? "v2" : "v1") : i === 2 && t < 7 ? "v1" : "v2");
            return (
                <>
                    <Box x={14} y={84} w={84} label="Client" tone="acc" />
                    <Box x={170} y={84} w={110} label="Coordinator" tone="warn" sub={write ? (t >= 2 ? "2 acks: done" : "write v2") : t >= 5 ? "newest: v2" : "read"} />
                    {[0, 1, 2].map((i) => (
                        <g key={i}>
                            <Box x={430} y={18 + i * 66} w={130} label={`replica ${i + 1}`} tone={write ? (i < 2 && t >= 1 ? "grn" : "line") : i > 0 ? (i === 2 && t >= 5 && t < 7 ? "vio" : "grn") : "line"} sub={v(i)} />
                            {(write || i > 0) && <Arrow d={`M280 102 L428 ${36 + i * 66}`} on={write ? i < 2 && t % 2 === 0 : t % 2 === 1} dash={write && i === 2} />}
                        </g>
                    ))}
                    <Arrow d="M98 102 H168" on />
                    {!write && t >= 6 && <Arrow d="M495 106 L495 148" on dash />}
                    <text className="sf-lab" x={14} y={208}>{write ? "N = 3, W = 2: acknowledged once two replicas have it" : t < 6 ? "R = 2: ask two, the newest version wins" : "read repair sends v2 to the stale replica"}</text>
                </>
            );
        },
    },
    "bloom-filter": {
        steps: 8,
        ms: 1000,
        draw: (t) => {
            const ops: { word: string; bits: number[]; add: boolean }[] = [
                { word: "add a", bits: [2, 7, 11], add: true },
                { word: "add b", bits: [4, 7, 13], add: true },
                { word: "check c", bits: [2, 9, 13], add: false },
                { word: "check d", bits: [4, 11, 13], add: false },
            ];
            const step = Math.min(3, Math.floor(t / 2));
            const op = ops[step]!;
            const set = new Set(ops.slice(0, Math.min(2, step + 1)).filter((o) => o.add).flatMap((o) => o.bits));
            const verdict = !op.add ? (op.bits.every((b) => set.has(b)) ? "all set: probably seen (here a false positive)" : "a bit is 0: definitely never added") : `set bits ${op.bits.join(", ")}`;
            return (
                <>
                    <Box x={14} y={30} w={110} label={op.word} tone={op.add ? "acc" : "vio"} />
                    {Array.from({ length: 16 }, (_, i) => (
                        <g key={i}>
                            <rect className={`sf-cell${set.has(i) ? " full" : ""}${op.bits.includes(i) ? " new" : ""}`} x={40 + i * 36} y={110} width={30} height={30} rx={5} />
                            <text className="sf-off" x={55 + i * 36} y={160}>{i}</text>
                        </g>
                    ))}
                    {op.bits.map((b) => <Arrow key={b} d={`M124 52 L${55 + b * 36} 106`} on />)}
                    <text className="sf-lab" x={14} y={200}>{verdict}</text>
                </>
            );
        },
    },
    "inverted-index": {
        steps: 6,
        ms: 1000,
        draw: (t) => {
            const docs = ["1 · rust ownership", "4 · rust async tokio", "7 · async python"];
            const terms: [string, string][] = [["async", "4, 7"], ["ownership", "1"], ["python", "7"], ["rust", "1, 4"], ["tokio", "4"]];
            const q = t >= 2;
            return (
                <>
                    <text className="sf-lab" x={14} y={20}>documents</text>
                    {docs.map((d, i) => <Box key={d} x={14} y={30 + i * 50} w={170} h={34} label={d} tone={q && t >= 4 && i === 1 ? "grn" : "line"} r={6} />)}
                    <text className="sf-lab" x={250} y={20}>term → postings</text>
                    {terms.map(([w, p], i) => <Box key={w} x={250} y={30 + i * 36} w={180} h={28} label={`${w}  →  ${p}`} tone={q && (w === "rust" || w === "async") ? "warn" : "line"} r={6} />)}
                    {t < 2 && <Arrow d="M184 96 L248 50" on />}
                    <Box x={480} y={60} w={140} label={q ? "rust async" : "analyse"} tone="acc" sub={q ? "query" : "lowercase, split"} />
                    {q && <Arrow d="M480 80 L432 48" on={t % 2 === 0} />}
                    {q && <Arrow d="M480 86 L432 152" on={t % 2 === 1} />}
                    <text className="sf-lab" x={14} y={208}>{t < 2 ? "indexing splits text into terms and records where each appears" : t < 4 ? "a query looks up each term's list" : "intersect {1, 4} and {4, 7}: document 4"}</text>
                </>
            );
        },
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
