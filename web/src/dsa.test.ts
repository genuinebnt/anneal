import assert from "node:assert/strict";
import test from "node:test";
import type { DsaProblem } from "./api.ts";
import { readinessRows } from "./dsa.ts";

const p = (difficulty: DsaProblem["difficulty"], state: Partial<DsaProblem["state"]> = {}, tags: string[] = ["Array"]) =>
  ({ difficulty, tags, state: { solved: false, assisted: false, retrievability: null, ...state } }) as unknown as DsaProblem;

test("readiness weighs difficulty, halves help and follows recall", () => {
  const ps = [p("easy", { solved: true }), p("medium", { solved: true, assisted: true }), p("hard"), p("medium", { solved: true, retrievability: 0.5 })];
  const [row] = readinessRows(ps, [["Array", () => true]]);
  // weights 1 + 2 + 3 + 2 = 8; earned 1 + 1 + 0 + 1 = 3
  assert.equal(row!.ready, 37.5);
  assert.equal(row!.solved, 3);
  assert.equal(row!.total, 4);
});

test("weakest first, empty groups left out, and a problem can be in two groups", () => {
  const a = p("easy", { solved: true }, ["Array", "String"]);
  const b = p("easy", {}, ["String"]);
  const rows = readinessRows([a, b], [["Array", (x) => x.tags.includes("Array")], ["String", (x) => x.tags.includes("String")], ["Trie", (x) => x.tags.includes("Trie")]]);
  assert.deepEqual(rows.map((r) => [r.name, r.ready]), [["String", 50], ["Array", 100]]);
});
