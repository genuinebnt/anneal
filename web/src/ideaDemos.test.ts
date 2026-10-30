import assert from "node:assert/strict";
import test from "node:test";
import { DEMOS } from "./ideaDemos.ts";

test("the fixed window slides one step at a time and finds the best sum", () => {
    const d = DEMOS["window-fixed"]!;
    const frames = d.simulate(d.initial);
    // 8 numbers, k = 4: the first window, 4 slides, the end
    assert.equal(frames.length, 6);
    assert.deepEqual(frames[0]!.read.map((r) => r.value), [9, 9]);
    assert.deepEqual(frames[1]!.read.map((r) => r.value), [10, 10]);
    assert.equal(frames[5]!.result, "12");
    assert.equal(frames[2]!.enter, 5, "the second slide takes in index 5");
    assert.equal(frames[2]!.leave, 5 - 4, "and drops the item k behind it");
    const end = frames[5]!.work;
    assert.equal(end.kind, "fixed");
    if (end.kind === "fixed") {
        assert.equal(end.slide, 4 + 2 * 4);
        assert.equal(end.brute, 4 * 5, "recomputing adds k numbers for each of the 5 windows");
    }
});

test("the variable window shrinks only while it is broken, and each letter enters once", () => {
    const d = DEMOS["window-variable"]!;
    const frames = d.simulate(d.initial);
    assert.equal(frames[frames.length - 1]!.result, "3");
    const end = frames[frames.length - 1]!.work;
    assert.equal(end.kind, "variable");
    if (end.kind === "variable") assert.equal(end.enters, 8, "every letter came in exactly once");
    // a window is never wider than the string and its ends never cross
    for (const f of frames) assert.ok(f.l <= f.r && f.r <= 8);
    // the left end never moves back
    for (let i = 1; i < frames.length; i++) assert.ok(frames[i]!.l >= frames[i - 1]!.l);
    const bad = frames.filter((f) => f.dup !== undefined);
    assert.ok(bad.length > 0 && bad.every((f) => f.leave === f.l), "a broken window drops the letter at the left end");
});

test("typed input is read strictly", () => {
    const f = DEMOS["window-fixed"]!;
    assert.deepEqual(f.parse("1, 2,3", "2"), { type: "numbers", items: ["1", "2", "3"], k: 2 });
    assert.equal(f.parse("1,2,3", "4"), null, "k larger than the row");
    assert.equal(f.parse("1,x,3", "2"), null);
    assert.equal(f.parse("1,2,3,4,5,6,7,8,9,10,11", "2"), null, "more cells than fit");
    const v = DEMOS["window-variable"]!;
    assert.deepEqual(v.parse("Ab c", ""), { type: "letters", items: ["a", "b", "c"] });
    assert.equal(v.parse("a", ""), null);
});
