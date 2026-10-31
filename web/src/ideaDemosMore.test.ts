import assert from "node:assert/strict";
import test from "node:test";
import { DEMOS, type Frame } from "./ideaDemos.ts";

const run = (name: string) => DEMOS[name]!.simulate(DEMOS[name]!.initial);
const value = (f: Frame, label: string) => f.read.find((r) => r.label === label)!.value;

test("a depth-first walk enters every node once, keeps only the path on the stack, and visits a node before its children", () => {
    const frames = run("tree-dfs");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "4 2 1 3 6 5 7");
    // the start, an enter and an exit for each of 7 nodes, and the end
    assert.equal(frames.length, 1 + 14 + 1);
    assert.equal(Math.max(...frames.map((f) => f.stack!.items.length)), 3, "the stack never holds more than the height");
    assert.equal(frames[frames.length - 1]!.stack!.items.length, 0);
    assert.equal(last.work.bars[0]!.value, 7);
    assert.deepEqual(frames[1]!.stack!.items, ["4"]);
    assert.equal(frames[1]!.stack!.top, "push");
});

test("a breadth-first walk goes level by level and its queue can hold a whole level", () => {
    const frames = run("tree-bfs");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "4 2 6 1 3 5 7");
    assert.equal(frames.length, 1 + 7 + 1);
    assert.equal(last.work.bars[1]!.value, 4, "the leaves' level of four is the most waiting at once");
    assert.equal(value(frames[4]!, "LEVEL"), 2);
});

test("a tree with empty places skips them, and an empty place hides what is under it", () => {
    const frames = DEMOS["tree-dfs"]!.simulate({ items: ["1", "_", "3", "9", "_", "4"] });
    assert.equal(frames[frames.length - 1]!.result, "1 3 4", "node 9 hangs under an empty place, so it is not in the tree");
    assert.equal(DEMOS["tree-dfs"]!.parse("_,1", ""), null);
    assert.deepEqual(DEMOS["tree-bfs"]!.parse("1, null, 3", "")!.items, ["1", "_", "3"]);
    assert.equal(DEMOS["tree-bfs"]!.parse("1,2,3,4,5,6,7,8", ""), null, "at most seven places");
});
