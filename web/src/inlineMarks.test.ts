import assert from "node:assert/strict";
import { test } from "node:test";
import { marks } from "./components/inlineMarks.ts";

const drawn = (text: string) => marks(text).map((m) => (m.kind === "text" ? m.text : `<${m.kind}>${m.text}</${m.kind}>`)).join("");

test("asterisks and underscores that are part of the text stay as typed", () => {
    for (const text of [
        "An exact count(*) of 2,000,000 rows reads every row; SELECT count(*) FROM (SELECT DISTINCT x) hashes instead",
        "count(*) and count(*)",
        "count(*), sum(x) and max(*)",
        "*.rs",
        "*.rs and *.toml",
        "src/* and tests/*",
        "src/**/*.rs and docs/**",
        "** matches any depth, **/target too",
        "a * b * c",
        "2*n*m and O(n*m) against O(n*k)",
        "2**10 and 3**2",
        "SELECT *",
        "SELECT * FROM t WHERE a * 2 > b *",
        "SELECT *, count(*)",
        "*const T and *mut T",
        "a Vec<*const u8> or (*self).next, then (*other).next",
        "*ptr = *other",
        "/* a comment */",
        "(*) and [*] and {*}",
        "\"*\" or '*' means everything",
        "*, then *.",
        "the *-linux-* targets",
        "Bellman-Ford, A*, xorshift64*, split*, split_at_mut",
        "enable_* settings",
        "TOAST: wide values, jsonb and SELECT *",
        "count(*) OVER () adds count(*) to every page",
        "snake_case_name, __init__, _x_ and (_) => _",
        "x_*_y",
    ]) assert.equal(drawn(text), text, text);
});

test("real emphasis, bold and code are still drawn", () => {
    const cases: [string, string][] = [
        ["Why the *top* bits and the *low* bits", "Why the <em>top</em> bits and the <em>low</em> bits"],
        ["*which column of which input*", "<em>which column of which input</em>"],
        ["it needs a *max-heap of the worst*.", "it needs a <em>max-heap of the worst</em>."],
        ["(a *very* old idea), *really*: yes", "(a <em>very</em> old idea), <em>really</em>: yes"],
        ["\"*not* here\"", "\"<em>not</em> here\""],
        ["the *`v1` column* of it", "the <em>`v1` column</em> of it"],
        ["*optional (and rare)* work", "<em>optional (and rare)</em> work"],
        ["in id order) and **-0.006**: a range", "in id order) and <bold>-0.006</bold>: a range"],
        ["**Note:** read it", "<bold>Note:</bold> read it"],
        ["**bold** and *it*", "<bold>bold</bold> and <em>it</em>"],
        ["**count(*) is exact** here", "<bold>count(*) is exact</bold> here"],
        ["`count(*)` and *then* `a * b`", "<code>count(*)</code> and <em>then</em> <code>a * b</code>"],
        ["count(*) is *exact*, count(*) too", "count(*) is <em>exact</em>, count(*) too"],
    ];
    for (const [text, want] of cases) assert.equal(drawn(text), want, text);
});
