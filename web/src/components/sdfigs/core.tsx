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

/** Twenty clients fail at once and retry against a server that answers four calls per time slot (the rest fail and
 *  retry): calls arriving in each slot, waiting 2, 4, 8… slots each time, or a random 1 to that many (full jitter). */
function retryWaves(jitter: boolean): number[] {
    let seed = 11;
    const rnd = (n: number) => {
        seed = (seed * 1103515245 + 12345) % 2147483648;
        return Math.floor((seed / 2147483648) * n);
    };
    const loads: number[] = Array(24).fill(0);
    const clients = Array.from({ length: 20 }, () => ({ at: 0, tries: 0 }));
    for (let slot = 0; slot < loads.length; slot++) {
        const here = clients.filter((c) => c.at === slot);
        loads[slot] = here.length;
        here.forEach((c, i) => {
            if (i < 4) {
                c.at = -1; // served
                return;
            }
            const w = 2 ** (c.tries + 1);
            c.tries += 1;
            c.at = slot + (jitter ? 1 + rnd(w) : w);
        });
    }
    return loads;
}
const WAVES = { plain: retryWaves(false), jitter: retryWaves(true) };

/** One step of a two-session race: what each session runs and shows, the row's state, and the narration. */
type RaceStep = { a: [string, string, string]; b: [string, string, string]; row: [string, string, string]; say: string; on: "a" | "b" | "ab" | "" };

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
    // pattern pages
    "lock-vs-version": {
        steps: 10,
        ms: 1300,
        draw: (t) => {
            // [statement, tone, state] for A and B, [label, tone, state] for the row
            const lock: RaceStep[] = [
                { a: ["SELECT … FOR UPDATE", "acc", "reads 10"], b: ["", "line", ""], row: ["stock 10", "warn", "locked by A"], say: "A locks the row and reads stock 10", on: "a" },
                { a: ["", "acc", "working"], b: ["SELECT … FOR UPDATE", "warn", "waiting"], row: ["stock 10", "warn", "locked by A"], say: "B asks for the same lock and waits", on: "b" },
                { a: ["UPDATE stock = 9 · COMMIT", "grn", "done"], b: ["", "warn", "waiting"], row: ["stock 9", "line", "lock released"], say: "A commits, and the lock passes to B", on: "a" },
                { a: ["", "grn", "done"], b: ["", "acc", "reads 9"], row: ["stock 9", "warn", "locked by B"], say: "B reads the committed 9, not the old 10", on: "b" },
                { a: ["", "grn", "done"], b: ["UPDATE stock = 8 · COMMIT", "grn", "done"], row: ["stock 8", "grn", "both sales counted"], say: "correct, and B paid for it by waiting", on: "b" },
            ];
            const version: RaceStep[] = [
                { a: ["SELECT stock, version", "acc", "10 at v1"], b: ["SELECT stock, version", "acc", "10 at v1"], row: ["stock 10", "line", "version 1"], say: "no locks: both read version 1", on: "ab" },
                { a: ["UPDATE … WHERE version = 1", "grn", "1 row"], b: ["", "acc", "10 at v1"], row: ["stock 9", "grn", "version 2"], say: "A writes first: one row changed, version 2", on: "a" },
                { a: ["", "grn", "done"], b: ["UPDATE … WHERE version = 1", "bad", "0 rows"], row: ["stock 9", "line", "version 2"], say: "B's version no longer matches: zero rows", on: "b" },
                { a: ["", "grn", "done"], b: ["SELECT stock, version", "acc", "9 at v2"], row: ["stock 9", "line", "version 2"], say: "B rereads and decides again", on: "b" },
                { a: ["", "grn", "done"], b: ["UPDATE … WHERE version = 2", "grn", "1 row"], row: ["stock 8", "grn", "version 3"], say: "nobody waited, but B did its work twice", on: "b" },
            ];
            const s = (t < 5 ? lock : version)[t % 5]!;
            return (
                <>
                    <text className="sf-lab" x={14} y={20}>{t < 5 ? "pessimistic: lock, then decide" : "optimistic: check the version on write"}</text>
                    <Box x={14} y={44} w={110} label="session A" tone={s.a[1]} sub={s.a[2]} />
                    <Box x={14} y={140} w={110} label="session B" tone={s.b[1]} sub={s.b[2]} />
                    <Box x={450} y={88} w={170} h={44} label={s.row[0]} tone={s.row[1]} sub={s.row[2]} />
                    <Arrow d="M124 62 L448 104" on={s.on.includes("a")} />
                    <Arrow d="M124 158 L448 118" on={s.on.includes("b")} dash={s.b[2] === "waiting"} />
                    <text className="sf-lab" x={140} y={52}>{s.a[0]}</text>
                    <text className="sf-lab" x={140} y={180}>{s.b[0]}</text>
                    <text className="sf-lab" x={14} y={212}>{s.say}</text>
                </>
            );
        },
    },
    "read-layers": {
        steps: 8,
        ms: 1300,
        draw: (t) => {
            // steps 0-4 follow a warm wave of reads layer by layer; 5-7 flush the cache
            const cold = t >= 5;
            const reached = cold ? 4 : t;
            const layers: { name: string; load: number; sub: string; tone: string }[] = [
                { name: "users", load: 100000, sub: "100,000 views/s", tone: "acc" },
                { name: "CDN", load: 100000, sub: "answers 80,000", tone: "grn" },
                { name: "Redis", load: 20000, sub: cold ? "flushed" : "answers 19,000", tone: cold ? "bad" : "grn" },
                { name: "replica", load: cold && t >= 6 ? 20000 : 1000, sub: cold && t >= 6 ? "20,000/s" : "serves 1,000", tone: cold && t >= 6 ? "bad" : "grn" },
                { name: "primary", load: 50, sub: "50 edits/s", tone: "warn" },
            ];
            const say = [
                "100,000 profile views a second leave the users",
                "the CDN answers 80% of them at the edge",
                "Redis answers 95% of what is left",
                "the replica sees 1,000 reads a second",
                "edits go to the primary, which streams them to the replica",
                "the cache is flushed: every miss goes straight through",
                "the replica now gets twenty times its warm load",
                "size for this, or warm the cache while the CDN serves stale pages",
            ][t]!;
            return (
                <>
                    {layers.map((l, i) => {
                        const on = i <= reached || i === 4;
                        const h = 4 + 96 * Math.sqrt(l.load / 100000);
                        return (
                            <g key={l.name}>
                                <Box x={14 + i * 126} y={40} w={108} h={44} label={l.name} tone={on ? l.tone : "line"} sub={on ? l.sub : ""} />
                                {on && <rect className={`sf-bar${l.tone === "bad" ? " hot" : ""}`} x={48 + i * 126} y={190 - h} width={40} height={h} rx={5} />}
                            </g>
                        );
                    })}
                    <Arrow d="M122 62 H138" on={reached >= 1} />
                    <Arrow d="M248 62 H264" on={reached >= 2} />
                    <Arrow d="M374 62 H390" on={reached >= 3} />
                    <Arrow d="M518 62 H502" on={t === 4} dash />
                    <text className="sf-lab" x={14} y={22}>reads arriving at each layer (bar: square-root scale)</text>
                    <text className="sf-lab" x={14} y={212}>{say}</text>
                </>
            );
        },
    },
    "visibility-timeout": {
        steps: 9,
        ms: 1200,
        draw: (t) => {
            // m7 is received by A (t1), A crashes (t3), the timeout ends (t5), B receives it (t6) and acks (t8)
            const holder = t >= 1 && t < 5 ? "A" : t >= 6 && t < 8 ? "B" : "";
            const gone = t >= 8;
            const timer = t >= 1 && t < 5 ? (t - 1) * 10 : t >= 6 && t < 8 ? (t - 6) * 10 : 0;
            const say = [
                "m7 waits in the queue",
                "worker A receives m7: it is hidden, not deleted",
                "A charges the card…",
                "…and crashes before it acks",
                "nobody acks: the timeout keeps running",
                "30 s without an ack: m7 is visible again",
                "worker B receives m7, its second delivery",
                "B must not charge again: the handler is idempotent",
                "B acks: m7 is deleted for good",
            ][t]!;
            return (
                <>
                    <text className="sf-lab" x={14} y={22}>queue</text>
                    {["m7", "m8", "m9", "m10"].map((m, i) => (
                        <g key={m}>
                            {!(i === 0 && gone) && <rect className={`sf-cell${i > 0 || !holder ? " full" : ""}`} x={14 + i * 52} y={34} width={44} height={36} rx={6} strokeDasharray={i === 0 && holder ? "4 3" : undefined} />}
                            {!(i === 0 && gone) && <text className="sf-off" x={36 + i * 52} y={56}>{m}</text>}
                        </g>
                    ))}
                    <text className="sf-lab" x={14} y={96}>{holder ? `m7 in flight with ${holder}, hidden from others` : gone ? "m7 acked and deleted" : "m7 visible: any worker may take it"}</text>
                    <text className="sf-lab" x={14} y={130}>visibility timeout 30 s</text>
                    <rect className="sf-cell" x={14} y={140} width={200} height={14} rx={7} />
                    {timer > 0 && <rect className="sf-bar" x={14} y={140} width={(timer / 30) * 200} height={14} rx={7} />}
                    <text className="sf-off" x={234} y={151}>{holder ? `${timer} s` : ""}</text>
                    <Box x={440} y={30} w={150} label="worker A" tone={t >= 3 && t < 6 ? "bad" : holder === "A" ? "acc" : "line"} sub={t >= 3 ? "crashed" : holder === "A" ? "charging m7" : "idle"} />
                    <Box x={440} y={130} w={150} label="worker B" tone={gone ? "grn" : holder === "B" ? "acc" : "line"} sub={gone ? "acked m7" : holder === "B" ? "m7, delivery 2" : "idle"} />
                    <Arrow d="M232 52 L438 48" on={t === 1} />
                    {t === 8 ? <Arrow d="M438 146 L232 62" on dash /> : <Arrow d="M232 58 L438 146" on={t === 6} />}
                    <text className="sf-lab" x={14} y={212}>{say}</text>
                </>
            );
        },
    },
    "realtime-ways": {
        steps: 10,
        ms: 1000,
        draw: (t) => {
            const x = (u: number) => 150 + u * 46; // time 0 to 10 across the lane
            const rows: { name: string; y: number; got: number[]; polls?: number[]; held?: [number, number][]; open?: boolean; up?: number }[] = [
                { name: "polling", y: 74, got: [4, 8], polls: [0, 4, 8] },
                { name: "long polling", y: 108, got: [3, 7], held: [[0, 3], [3.2, 7], [7.2, 10]] },
                { name: "SSE", y: 142, got: [3, 7], open: true },
                { name: "WebSocket", y: 176, got: [3, 7], open: true, up: 5 },
            ];
            const say = [
                "every client connects, or asks, at time 0",
                "polling asks on a timer; the others wait on an open request or connection",
                "nothing has happened yet: polling's first answer came back empty",
                "an event: long polling, SSE and the WebSocket deliver it at once",
                "polling learns of it only at its next request",
                "a WebSocket client can also send on the same connection",
                "long polling asked again straight after its answer",
                "a second event: pushed at once wherever a connection is open",
                "polling catches up only at its next request, up to an interval late",
                "push costs an open connection per client; polling costs requests, mostly empty",
            ][t]!;
            return (
                <>
                    <text className="sf-lab" x={14} y={36}>events</text>
                    {[3, 7].map((e) => <circle key={e} className={t >= e ? "sf-node" : "sf-ring"} cx={x(e)} cy={32} r={7} />)}
                    {rows.map((r) => (
                        <g key={r.name}>
                            <text className="sf-lab" x={14} y={r.y + 4}>{r.name}</text>
                            <path className="sf-arrow" d={`M${x(0)} ${r.y} H${x(10)}`} strokeDasharray="2 5" />
                            {r.open && <rect className="sf-bar" x={x(0)} y={r.y - 5} width={x(Math.min(t, 9.6)) - x(0)} height={10} rx={5} />}
                            {r.held?.filter(([a]) => t >= a).map(([a, b]) => <rect key={a} className="sf-bar" x={x(a)} y={r.y - 5} width={x(Math.min(t, b)) - x(a)} height={10} rx={5} />)}
                            {r.polls?.filter((p) => t >= p).map((p) => <rect key={p} className="sf-cell full" x={x(p) - 5} y={r.y - 9} width={10} height={18} rx={3} />)}
                            {r.got.filter((g) => t >= g).map((g) => <circle key={g} className="sf-key" cx={x(g)} cy={r.y} r={6} />)}
                            {r.up !== undefined && t >= r.up && <circle className="sf-node" cx={x(r.up)} cy={r.y} r={6} />}
                        </g>
                    ))}
                    <path className="sf-arrow on" d={`M${x(t)} 46 V190`} />
                    <text className="sf-lab" x={14} y={212}>{say}</text>
                </>
            );
        },
    },
    "retry-jitter": {
        steps: 12,
        ms: 800,
        draw: (t) => {
            const upto = t * 2 + 1; // slots shown so far
            const lanes = [
                { label: "no jitter", loads: WAVES.plain, base: 104 },
                { label: "full jitter", loads: WAVES.jitter, base: 186 },
            ];
            return (
                <>
                    <text className="sf-lab" x={14} y={20}>calls arriving per time slot · dashed: the 4 the server can take</text>
                    {lanes.map((l) => (
                        <g key={l.label}>
                            <text className="sf-lab" x={14} y={l.base - 24}>exponential</text>
                            <text className="sf-lab" x={14} y={l.base - 10}>{l.label}</text>
                            <path className="sf-arrow" d={`M150 ${l.base - 12} H620`} strokeDasharray="3 4" />
                            {l.loads.slice(0, upto).map((n, s) => (n > 0 ? <rect key={s} className={`sf-bar${n > 4 ? " hot" : ""}`} x={152 + s * 19.5} y={l.base - n * 3} width={14} height={n * 3} rx={3} /> : null))}
                        </g>
                    ))}
                    <text className="sf-lab" x={14} y={212}>{t < 4 ? "20 clients fail together; without jitter they come back together" : t < 8 ? "each synchronised wave overloads the server again" : "with jitter the same retries spread out and fit under the limit"}</text>
                </>
            );
        },
    },
    "multipart-upload": {
        steps: 10,
        ms: 1200,
        draw: (t) => {
            // parts 1-4 upload in parallel from t3; part 3 fails at t4 and is retried at t5
            const part = (i: number) => (t < 3 ? "line" : i === 2 && t === 4 ? "bad" : (i === 2 ? t >= 6 : t >= 4) ? "grn" : "acc");
            const say = [
                "the client asks the API to upload a 2 GB video",
                "the API checks the user, creates the multipart upload and a row: uploading",
                "it returns one presigned URL per part; no bytes pass through the API",
                "the client PUTs parts straight to storage, several at once",
                "part 3 fails on a flaky network…",
                "…and only part 3 is sent again",
                "every part is stored, each with its ETag",
                "the client asks storage to complete the upload with the list of ETags",
                "storage emits an object-created event",
                "the API checks size and checksum, marks the row uploaded, queues processing",
            ][t]!;
            return (
                <>
                    <Box x={14} y={84} w={100} label="Client" tone="acc" />
                    <Box x={250} y={14} w={140} label="API" tone={t === 1 || t === 2 || t === 9 ? "warn" : "line"} sub={t >= 9 ? "row: uploaded" : t >= 1 ? "row: uploading" : ""} />
                    <Box x={470} y={84} w={150} h={44} label="Object storage" tone={t >= 7 ? "grn" : "line"} sub={t >= 7 ? "video.mp4 complete" : t >= 3 ? "receiving parts" : ""} />
                    {[0, 1, 2, 3].map((i) => <Box key={i} x={160 + i * 72} y={156} w={64} h={30} r={6} label={part(i) === "bad" ? "failed" : `part ${i + 1}`} tone={part(i)} />)}
                    <Arrow d="M114 92 L248 36" on={t === 0} />
                    <Arrow d="M248 46 L114 100" on={t === 2} dash />
                    <Arrow d="M114 112 L468 112" on={t >= 3 && t <= 7} />
                    <Arrow d="M545 84 L392 40" on={t >= 8} dash={t === 8} />
                    <text className="sf-lab" x={14} y={212}>{say}</text>
                </>
            );
        },
    },
};
