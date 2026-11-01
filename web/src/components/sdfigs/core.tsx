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
};
