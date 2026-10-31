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

test("breadth-first search gives each node its fewest-edges distance and queues a node only once", () => {
    const frames = run("graph-bfs");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "4 steps");
    assert.equal(frames.length, 1 + 6 + 1);
    assert.equal(last.graph!.nodes.find((n) => n.id === 3)!.sub, "d=2", "3 is two edges from 0, by either route");
    assert.equal(last.graph!.nodes.find((n) => n.id === 5)!.sub, "d=4");
    assert.equal(last.work.bars[1]!.value, 12, "every edge is looked at from both ends");
    assert.equal(Math.max(...frames.map((f) => f.queue!.items.length)), 2);
});

test("breadth-first search says which nodes it cannot reach, and reads its input strictly", () => {
    const frames = DEMOS["graph-bfs"]!.simulate({ items: ["0-1", "2-3"], param: 0 });
    assert.match(frames[frames.length - 1]!.say, /2, 3 cannot be reached/);
    const d = DEMOS["graph-bfs"]!;
    assert.equal(d.parse("0-1", "0"), null, "needs two edges");
    assert.equal(d.parse("0-1, 1-1", "0"), null, "no edge from a node to itself");
    assert.equal(d.parse("0-1, 1-2", "5"), null, "the start must be a node");
    assert.deepEqual(d.parse("0-1, 1-2", "2")!.param, 2);
});

test("flood fill counts each island once and floods every land cell once", () => {
    const frames = run("grid-islands");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3");
    assert.equal(frames.length, 1 + (1 + 4) + (1 + 1) + (1 + 2) + 1);
    assert.equal(last.work.bars[0]!.value, 7);
    assert.equal(value(last, "LAND SEEN"), "7/7");
    assert.equal(last.grid!.marks!.filter((m) => m.tone === "done").length, 7, "all land is finished");
    assert.equal(DEMOS["grid-islands"]!.parse("101,11", ""), null, "rows must be the same length");
});

test("Dijkstra settles the closest node first, skips a stale heap entry, and ends with the shortest distances", () => {
    const frames = run("graph-dijkstra");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "max 7");
    assert.equal(value(last, "SHORTEST SO FAR"), "0=0 1=3 2=1 3=4 4=7");
    assert.ok(frames.some((f) => /stale/.test(f.say)), "node 1 was first reached at 4, then at 3 through node 2");
    assert.equal(last.work.bars[0]!.value, 5);
    assert.deepEqual(last.graph!.nodes.map((n) => n.tone), ["done", "done", "done", "done", "done"]);
});

test("Kruskal keeps an edge only when it joins two groups, and stops at n minus one edges", () => {
    const frames = run("graph-kruskal");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "weight 7");
    assert.equal(last.work.bars[1]!.value, 4);
    assert.equal(last.graph!.edges.filter((e) => e.tone === "hit").length, 4);
    assert.equal(last.graph!.edges.filter((e) => e.tone === "cut").length, 2, "the two heavier edges that would close a cycle");
    assert.equal(value(last, "GROUPS"), 1);
});
