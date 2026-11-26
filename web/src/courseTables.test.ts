// A `|` inside a table cell splits the cell, in backticks too, unless it is written `\|`. Marked then drops what is over,
// so the page shows half a line of code. This reads every course page and finds such rows.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

const root = new URL("../../courses/", import.meta.url);

const cells = (row: string) => row.trim().replace(/^\|/, "").replace(/(?<!\\)\|$/, "").split(/(?<!\\)\|/);

/** `line: text` for each row with more cells than its header, or with a code span cut in two. */
function broken(text: string): string[] {
    const lines = text.split("\n").map((l) => l.replace(/^(>\s?)+/, ""));
    const out: string[] = [];
    let fence = false;
    for (let i = 0; i < lines.length; i++) {
        if (lines[i]!.trim().startsWith("```")) fence = !fence;
        if (fence || !lines[i]!.trim().startsWith("|") || !/^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(lines[i + 1] ?? "")) continue;
        const width = cells(lines[i]!).length;
        for (i += 2; (lines[i] ?? "").trim().startsWith("|"); i++) {
            const row = cells(lines[i]!);
            if (row.length > width || row.some((c) => (c.split("`").length - 1) % 2)) out.push(`${i + 1}: ${lines[i]!.trim()}`);
        }
        i--;
    }
    return out;
}

test("no table row in a course page is cut at an unescaped pipe", () => {
    const pages = readdirSync(root, { recursive: true, encoding: "utf8" }).filter((f) => f.endsWith(".md"));
    assert.ok(pages.length > 300, `only ${pages.length} pages found`);
    assert.deepEqual(pages.flatMap((f) => broken(readFileSync(new URL(f, root), "utf8")).map((r) => `${f}:${r}`)), []);
});
