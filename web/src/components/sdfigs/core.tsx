import { Arrow, Box, type Fig } from "./parts";

/** One step of a two-session race: what each session runs and shows, the row's state, and the narration. */
type RaceStep = { a: [string, string, string]; b: [string, string, string]; row: [string, string, string]; say: string; on: "a" | "b" | "ab" | "" };

/** Figures for the core, fundamentals, patterns, low-level design and Rust pages. Add each name to `SD_FIGURES_CORE` in `crates/content/src/sd.rs`. */
export const FIGS_CORE: Record<string, Fig> = {
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
};
