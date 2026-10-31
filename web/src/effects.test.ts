import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

/** Every source file under src. */
function files(dir: string): string[] {
    return readdirSync(dir).flatMap((name) => {
        const path = join(dir, name);
        return statSync(path).isDirectory() ? files(path) : /\.tsx?$/.test(name) && !name.endsWith(".test.ts") ? [path] : [];
    });
}

// An effect written as one expression returns that expression's value, and React calls a returned value as the cleanup.
// `scrollTo` returning a promise in current browsers crashed the problem page this way, so effects use a block or return
// their cleanup explicitly (`() => () => …`).
test("effects never return the value of an expression", () => {
    const bad = files(dirname(fileURLToPath(import.meta.url)))
        .flatMap((f) => readFileSync(f, "utf8").split("\n").map((line, i) => [f, i + 1, line] as const))
        .filter(([, , line]) => /use(Layout)?Effect\(\(\) => (?!\{|\(\) =>)/.test(line))
        .map(([f, n, line]) => `${f}:${n}: ${line.trim()}`);
    assert.deepEqual(bad, []);
});
