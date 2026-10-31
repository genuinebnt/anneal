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

test("a heap pushes at the end and lets a smaller item rise, then a pop sinks the replacement", () => {
    const frames = run("heap-sift");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "1 2 3", "three pops give the three smallest in order");
    // after pushing 5 3 8 1 9 2 7 the array is a valid min-heap
    const afterPush = frames.find((f) => f.say.startsWith("Pop:"))!;
    const h = afterPush.cells!.map(Number);
    h.push(0);
    const before = frames[frames.indexOf(afterPush) - 1]!.cells!.map(Number);
    for (let i = 1; i < before.length; i++) assert.ok(before[(i - 1) >> 1]! <= before[i]!, "a parent is never larger than its child");
    assert.equal(before[0], 1);
    assert.equal(last.cells!.length, 4, "7 pushed, 3 popped");
    for (let i = 1; i < last.cells!.length; i++) assert.ok(Number(last.cells![(i - 1) >> 1]) <= Number(last.cells![i]));
    assert.ok(last.work.bars[0]!.value < last.work.bars[1]!.value + 20);
    assert.equal(DEMOS["heap-sift"]!.parse("1,2", "3"), null, "cannot pop more than was pushed");
});

test("the size-k heap keeps only the k largest and drops anything that does not beat its root", () => {
    const frames = run("heap-topk");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "9 8 7");
    assert.ok(Math.max(...frames.map((f) => f.graph!.nodes.length)) <= 3, "the heap never holds more than k");
    assert.ok(frames.some((f) => /does not beat/.test(f.say)), "small numbers are thrown away at once");
    assert.equal(DEMOS["heap-topk"]!.parse("1,2,3", "3"), null, "k must be smaller than the stream");
});

test("a trie reuses shared prefixes, and a search follows one edge per letter", () => {
    const ins = run("trie-insert");
    const end = ins[ins.length - 1]!;
    assert.equal(end.result, "9 nodes", "car card care cat dog is 17 letters but only 9 nodes");
    assert.equal(ins.filter((f) => /already in the trie/.test(f.say)).length, 8, "card and care reuse c a r (3 each); cat reuses c a");
    const found = run("trie-search");
    assert.equal(found[found.length - 1]!.result, "found");
    const prefix = DEMOS["trie-search"]!.simulate({ items: ["car", "card", "ca"] });
    assert.equal(prefix[prefix.length - 1]!.result, "prefix only");
    const none = DEMOS["trie-search"]!.simulate({ items: ["car", "cow"] });
    assert.equal(none[none.length - 1]!.result, "not found");
});

test("the subsets tree finds every subset once, records each at a leaf, and undoes each choice it made", () => {
    const frames = run("bt-subsets");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "8 subsets");
    assert.equal(last.work.bars[1]!.value, 7, "one path.pop for each of the 7 take-branches");
    assert.equal(Math.max(...frames.map((f) => f.stack!.items.length)), 3, "the path is never longer than the number of items");
    assert.equal(frames.filter((f) => /complete subset/.test(f.say)).length, 8);
    assert.equal(last.graph!.nodes.length, 15, "the whole decision tree has been drawn by the end");
    assert.equal(DEMOS["bt-subsets"]!.parse("1,2,3,4", ""), null, "at most three items");
});

test("N-Queens prunes attacked squares, backs up when a row is full, and ends with a safe board", () => {
    const frames = run("bt-queens");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "columns 2 4 1 3");
    assert.equal(last.grid!.cells.flat().filter((c) => c === "♛").length, 4);
    assert.ok(frames.some((f) => /Prune/.test(f.say)));
    assert.ok(frames.some((f) => /undo/.test(f.say)), "the first column fails and has to be undone");
    assert.ok((last.work.bars[0]!.value as number) < 4 ** 4, "far fewer squares than full boards");
    assert.equal(DEMOS["bt-queens"]!.simulate({ items: [], param: 5 }).at(-1)!.result, "columns 1 3 5 2 4");
});

test("coin change fills one cell per amount from smaller cells, and ends with the fewest coins", () => {
    const frames = run("dp-coins");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3 coins");
    assert.equal(frames.length, 1 + 11 + 1);
    assert.deepEqual(last.grid!.cells[0], ["0", "1", "1", "2", "2", "1", "2", "2", "3", "3", "2", "3"]);
    assert.ok((last.work.bars[0]!.value as number) > 100, "a plain recursion would make hundreds of calls");
    assert.equal(DEMOS["dp-coins"]!.simulate({ items: ["5"], param: 3 }).at(-1)!.result, "impossible");
    assert.equal(frames[5]!.grid!.marks!.filter((m) => m.tone === "read").length, 3, "amount 5 reads from 4, 3 and 0");
});

test("take or skip keeps the better of leaving a house and robbing it, each prefix once", () => {
    const frames = run("dp-rob");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "19");
    assert.deepEqual(last.grid!.cells[1], ["0", "2", "7", "11", "11", "12", "19"]);
    assert.equal(frames.length, 1 + 5 + 1);
});

test("the LCS table has a cell per pair of prefixes, and a match extends the diagonal", () => {
    const frames = run("dp2-lcs");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3");
    assert.equal(frames.length, 1 + 15 + 1);
    assert.equal(last.grid!.cells.length, 6);
    assert.equal(last.grid!.cells[0]!.length, 4);
    assert.equal(last.work.bars[0]!.value, 15);
    assert.ok(frames.some((f) => /a match/.test(f.say)));
    assert.equal(DEMOS["dp2-lcs"]!.parse("abc", ""), null, "needs two words");
});

test("the cheapest grid path adds the cost of a cell to the cheaper of above and left, and traces back", () => {
    const frames = run("dp2-path");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "7");
    assert.equal(frames.length, 1 + 9 + 1);
    assert.equal(last.grid!.marks!.length, 5, "a route through a 3x3 grid touches five cells");
    assert.equal(last.grid!.cells[2]![2], "7");
});

test("the jump game keeps one number, stops when it can reach the end, and says when it is stranded", () => {
    const frames = run("greedy-jump");
    assert.equal(frames[frames.length - 1]!.result, "reachable");
    assert.equal(frames.length, 1 + 2, "reach 4 covers the last index at the second item");
    const stuck = DEMOS["greedy-jump"]!.simulate({ items: ["3", "2", "1", "0", "4"] });
    assert.equal(stuck[stuck.length - 1]!.result, "stranded");
    assert.equal(value(stuck[stuck.length - 1]!, "REACH"), 3);
});

test("the best run extends while it helps and restarts when the old run drags it down", () => {
    const frames = run("greedy-kadane");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "6");
    assert.deepEqual(last.win, [3, 7]);
    assert.ok(frames.some((f) => /Start a new run/.test(f.say)));
    assert.equal(frames.length, 1 + 8 + 1);
});

test("merging sorts by start, stretches an overlapping interval and starts a new one after a gap", () => {
    const frames = run("intervals-merge");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3 intervals");
    assert.deepEqual(last.ranges!.bars.filter((b) => b.tone === "merged").map((b) => b.label), ["1-6", "8-10", "15-18"]);
    assert.equal(frames.length, 1 + 4 + 1);
    assert.ok(frames.some((f) => /Stretch/.test(f.say)));
    const unsorted = DEMOS["intervals-merge"]!.simulate({ items: ["8-10", "1-3", "2-6"] });
    assert.equal(unsorted[unsorted.length - 1]!.result, "2 intervals", "the input order does not matter");
    assert.equal(DEMOS["intervals-merge"]!.parse("5-2, 1-3", ""), null, "an interval must end after it starts");
});

test("the sweep counts meetings in progress at each start and end and reports the peak", () => {
    const frames = run("intervals-sweep");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3 rooms");
    assert.equal(frames.length, 1 + 10 + 1);
    assert.equal(Math.max(...frames.map((f) => Number(value(f, "ROOMS IN USE")))), 3);
    assert.ok(frames.every((f) => Number(value(f, "ROOMS IN USE")) >= 0));
    assert.equal(value(frames[frames.length - 2]!, "ROOMS IN USE"), 0, "the last event ends the last meeting");
    const touching = DEMOS["intervals-sweep"]!.simulate({ items: ["1-3", "3-5"] });
    assert.equal(touching[touching.length - 1]!.result, "1 room", "one ends exactly when the next starts, so it can reuse the room");
});

test("rotating transposes then flips each row, ending with the matrix turned clockwise", () => {
    const frames = run("math-rotate");
    const last = frames[frames.length - 1]!;
    assert.deepEqual(last.grid!.cells, [["7", "4", "1"], ["8", "5", "2"], ["9", "6", "3"]]);
    assert.equal(frames.length, 1 + 3 + 1 + 3 + 1);
    const four = DEMOS["math-rotate"]!.simulate({ items: [], param: 4 });
    assert.deepEqual(four[four.length - 1]!.grid!.cells[0], ["13", "9", "5", "1"]);
});

test("fast power multiplies by x for each set bit of the exponent and squares x between bits", () => {
    const frames = run("math-pow");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, String(3 ** 13));
    assert.deepEqual(last.cells, ["1", "1", "0", "1"]);
    assert.equal(last.work.bars[1]!.value, 3 + 3, "three squarings fewer than the bits, plus one multiply per set bit");
    assert.ok((last.work.bars[0]!.value as number) > (last.work.bars[1]!.value as number));
    const big = DEMOS["math-pow"]!.simulate({ items: ["9"], param: 30 });
    assert.equal(big[big.length - 1]!.result, String(9n ** 30n), "exact even beyond 2^53");
});

test("XOR cancels what appears twice and flips only the bits that differ", () => {
    const frames = run("bits-xor");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "4");
    assert.equal(frames.length, 1 + 5 + 1);
    assert.deepEqual(last.grid!.cells[1], ["0", "1", "0", "0"]);
    assert.equal(value(frames[3]!, "RESULT"), 7, "4 ^ 1 ^ 2 = 7 before the second 1 arrives");
    assert.equal(DEMOS["bits-xor"]!.parse("1,2", ""), null, "at least three numbers");
    assert.equal(DEMOS["bits-xor"]!.parse("1,2,16", ""), null, "four bits only");
});

test("counting set bits clears the lowest one each step, so it runs once per 1 bit", () => {
    const frames = run("bits-count");
    const last = frames[frames.length - 1]!;
    assert.equal(last.result, "3", "148 = 10010100 has three set bits");
    assert.equal(frames.length, 1 + 3 + 1);
    assert.deepEqual(frames[0]!.cells, ["1", "0", "0", "1", "0", "1", "0", "0"]);
    assert.deepEqual(frames[1]!.cells, ["1", "0", "0", "1", "0", "0", "0", "0"], "the lowest set bit is gone");
    assert.equal(value(frames[3]!, "n"), 0);
});
