import assert from "node:assert/strict";
import test from "node:test";
import { DEMOS, type Frame } from "./ideaDemos.ts";

const run = (name: string) => DEMOS[name]!.simulate(DEMOS[name]!.initial);
const value = (f: Frame, label: string) => f.read.find((r) => r.label === label)!.value;

test("the fixed window slides one step at a time and finds the best sum", () => {
    const frames = run("window-fixed");
    // 8 numbers, k = 4: the first window, 4 slides, the end
    assert.equal(frames.length, 6);
    assert.deepEqual(frames[0]!.read.map((r) => r.value), [9, 9]);
    assert.deepEqual(frames[1]!.read.map((r) => r.value), [10, 10]);
    assert.equal(frames[5]!.result, "12");
    assert.equal(frames[2]!.enter, 5, "the second slide takes in index 5");
    assert.equal(frames[2]!.leave, 5 - 4, "and drops the item k behind it");
    const [brute, slide] = frames[5]!.work.bars;
    assert.equal(slide!.value, 4 + 2 * 4);
    assert.equal(brute!.value, 4 * 5, "recomputing adds k numbers for each of the 5 windows");
});

test("the variable window shrinks only while it is broken, and each letter enters once", () => {
    const frames = run("window-variable");
    assert.equal(frames[frames.length - 1]!.result, "3");
    assert.equal(frames[frames.length - 1]!.work.bars[0]!.value, 8, "every letter came in exactly once");
    // a window is never wider than the string, and the left end never moves back
    for (const f of frames) assert.ok(f.win![0] <= f.win![1] && f.win![1] <= 8);
    for (let i = 1; i < frames.length; i++) assert.ok(frames[i]!.win![0] >= frames[i - 1]!.win![0]);
    const bad = frames.filter((f) => f.dup !== undefined);
    assert.ok(bad.length > 0 && bad.every((f) => f.leave === f.win![0]), "a broken window drops the letter at the left end");
});

test("opposite pointers close in, rule out one item a step, and find the pair", () => {
    const frames = run("pointers-opposite");
    // 1 3 4 6 8 11 15, target 14: 1+15 too big, 1+11 too small, 3+11 found
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3 + 11");
    assert.deepEqual(last.hit, [1, 5]);
    assert.equal(frames.length, 1 + 3, "the start and three looks");
    const looked = last.work.bars[1]!.value;
    assert.equal(looked, 3);
    assert.ok(looked <= 7 - 1, "never more than n - 1 pairs");
    // the ruled-out cells only grow
    for (let i = 2; i < frames.length; i++) assert.ok((frames[i]!.dim ?? []).length >= (frames[i - 1]!.dim ?? []).length);
    assert.equal(value(frames[1]!, "SUM"), 16);
    // no pair: it ends with none
    const none = DEMOS["pointers-opposite"]!.simulate({ items: ["1", "2", "3"], param: 100 });
    assert.equal(none[none.length - 1]!.result, "none");
});

test("same-direction pointers write only what is new, in one pass", () => {
    const frames = run("pointers-same");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "4");
    assert.deepEqual(last.cells!.slice(0, 4), ["1", "2", "3", "4"]);
    const [reads, writes] = last.work.bars;
    assert.equal(reads!.value, 8);
    assert.equal(writes!.value, 3, "three new values after the first");
    assert.ok(frames.filter((f) => f.dup !== undefined).every((f) => f.cells![f.dup!] === f.cells![f.win![1] - 1]), "a skipped item equals the last one kept");
});

test("typed input is read strictly", () => {
    const f = DEMOS["window-fixed"]!;
    assert.deepEqual(f.parse("1, 2,3", "2"), { items: ["1", "2", "3"], param: 2 });
    assert.equal(f.parse("1,2,3", "4"), null, "k larger than the row");
    assert.equal(f.parse("1,x,3", "2"), null);
    assert.equal(f.parse("1,2,3,4,5,6,7,8,9,10,11", "2"), null, "more cells than fit");
    const v = DEMOS["window-variable"]!;
    assert.deepEqual(v.parse("Ab c", ""), { items: ["a", "b", "c"] });
    assert.equal(v.parse("a", ""), null);
    const o = DEMOS["pointers-opposite"]!;
    assert.deepEqual(o.parse("9, 1, 5", "10"), { items: ["1", "5", "9"], param: 10 }, "a pair row is sorted for you");
    assert.equal(o.parse("1,2,3", ""), null, "a target is needed");
});
