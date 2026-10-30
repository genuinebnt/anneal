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

test("binary search halves the row each look and finds the target, or says it is not there", () => {
    const frames = run("search-find");
    const last = frames[frames.length - 1]!;
    // 2 5 8 12 16 23 38 56 72 91, target 23: mid 4 (16) too small, mid 7 (56) too big, mid 5 (23) found
    assert.equal(last.result, "index 5");
    assert.equal(frames.length, 1 + 3);
    assert.ok(last.work.bars[1]!.value <= Math.floor(Math.log2(10)) + 1, "never more looks than log2 n + 1");
    assert.deepEqual(last.hit, [5]);
    // the ruled-out part only grows
    for (let i = 2; i < frames.length; i++) assert.ok((frames[i]!.dim ?? []).length >= (frames[i - 1]!.dim ?? []).length);
    const none = DEMOS["search-find"]!.simulate({ items: ["1", "3", "5"], param: 4 });
    assert.equal(none[none.length - 1]!.result, "not found");
    assert.ok(none.length <= 1 + 2 + 1);
});

test("first-true search asks about few cells, keeps mid when it is True and drops it when False", () => {
    const frames = run("search-first-true");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "index 6");
    assert.deepEqual(last.hit, [6]);
    const asked = last.work.bars[1]!.value;
    assert.ok(asked <= Math.ceil(Math.log2(10)), "about log2 n questions");
    // most cells were never asked: they still show a question mark
    assert.ok(last.cells!.filter((c) => c === "?").length >= 5);
    // an answer shown is the truth
    for (const f of frames) for (const i of f.ok ?? []) assert.equal(f.cells![i], "T");
    for (const f of frames) for (const i of f.nope ?? []) assert.equal(f.cells![i], "F");
    const f = DEMOS["search-first-true"]!;
    assert.deepEqual(f.parse("ffftt", ""), { items: [..."FFFTT"] });
    assert.equal(f.parse("FTFT", ""), null, "it must flip once");
    assert.equal(f.parse("FFF", ""), null, "and have a True");
});

test("brackets: openers wait on the stack, closers must meet the top, and a crossing or a leftover is invalid", () => {
    const frames = run("stack-brackets");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "valid");
    assert.equal(frames.length, 1 + 6 + 1);
    assert.deepEqual(last.stack!.items, []);
    // pushed and popped are each at most once per character
    const [pushed, popped] = last.work.bars;
    assert.equal(pushed!.value, 3);
    assert.equal(popped!.value, 3);
    // the stack never holds more than the openers seen
    for (const f of frames) assert.ok(f.stack!.items.length <= 3);
    const sim = (s: string) => DEMOS["stack-brackets"]!.simulate({ items: [...s] });
    const crossing = sim("(]");
    assert.equal(crossing[crossing.length - 1]!.result, "invalid");
    assert.deepEqual(crossing[crossing.length - 1]!.nope, [1, 0], "the closer and the opener it met");
    const empty = sim("))");
    assert.equal(empty[empty.length - 1]!.result, "invalid");
    assert.deepEqual(empty[empty.length - 1]!.nope, [0], "nothing was open");
    const open = sim("((");
    assert.equal(open[open.length - 1]!.result, "invalid");
    assert.deepEqual(open[open.length - 1]!.nope, [0, 1], "two still open");
    const p = DEMOS["stack-brackets"]!;
    assert.deepEqual(p.parse("([ a ]{})", ""), { items: [..."([]{})"] }, "other characters are ignored");
    assert.equal(p.parse("(", ""), null);
});

test("monotonic stack: each index is pushed once and popped at most once, and the answers are the next greater numbers", () => {
    const frames = run("stack-mono");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "5 5 6 6 -1 -1");
    const [pushed, popped] = last.work.bars;
    assert.equal(pushed!.value, 6);
    assert.ok(popped!.value <= 6);
    // the stack stays in decreasing order of value
    for (const f of frames) {
        const v = f.stack!.items.map(Number);
        for (let i = 1; i < v.length; i++) assert.ok(v[i - 1]! >= v[i]!, `decreasing: ${v}`);
    }
    // a pop step resolves exactly the cell it popped
    const resolved = frames.map((f) => (f.ok ?? []).length);
    for (let i = 1; i < resolved.length; i++) assert.ok(resolved[i]! >= resolved[i - 1]!);
    const decreasing = DEMOS["stack-mono"]!.simulate({ items: ["5", "4", "3"] });
    assert.equal(decreasing[decreasing.length - 1]!.result, "-1 -1 -1", "nothing is greater: they all stay on the stack");
});

test("reversing in place flips each link once, makes no node, and ends with the order turned round", () => {
    const frames = run("list-reverse");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "5 4 3 2 1");
    assert.equal(frames.length, 1 + 5 + 1);
    const [flips, made] = last.work.bars;
    assert.equal(flips!.value, 5);
    assert.equal(made!.value, 0);
    // at the end every link points the other way and the first node points at nothing
    assert.deepEqual(last.links!.map((l) => l.to), [-1, 0, 1, 2, 3]);
    // exactly the link just flipped is hot
    for (const f of frames.slice(1, -1)) assert.equal(f.links!.filter((l) => l.hot).length, 1);
    // prev is always the node before cur in the original order
    for (const f of frames.slice(1, -1)) {
        const prev = f.pointers.find((p) => p.label === "prev");
        const cur = f.pointers.find((p) => p.label === "cur")!;
        if (prev) assert.equal(prev.at, cur.at - 1);
    }
});

test("slow and fast meet inside a loop and never meet without one", () => {
    const looped = run("list-cycle");
    const last = looped[looped.length - 1]!;
    assert.equal(last.result, "cycle");
    assert.equal(last.hit!.length, 1);
    assert.ok(last.hit![0]! >= 2, "they meet inside the loop, from index 2 on");
    assert.ok(looped.length <= 1 + 8, "a few steps");
    assert.equal(last.work.bars[1]!.value, 2, "two pointers, whatever the length");
    const none = DEMOS["list-cycle"]!.simulate({ items: ["1", "2", "3", "4", "5"], param: -1 });
    assert.equal(none[none.length - 1]!.result, "no cycle");
    for (const f of none) assert.ok(!f.hit, "no meeting");
    const d = DEMOS["list-cycle"]!;
    assert.deepEqual(d.parse("1,2,3", "1"), { items: ["1", "2", "3"], param: 1 });
    assert.equal(d.parse("1,2,3", "5"), null, "the loop must point at a node of the list");
    assert.deepEqual(d.parse("1,2,3", "-1"), { items: ["1", "2", "3"], param: -1 });
});
