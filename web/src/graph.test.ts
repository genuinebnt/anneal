// Run with: node --test web/src/graph.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import type { DsaOverview, DsaProblem } from "./api.ts";
import { buildGraph, cosine, fastRP, sharedTags, shortestPath } from "./graph.ts";

let n = 0;
const make = (over: Partial<DsaProblem> = {}): DsaProblem =>
  ({
    id: `lc-p${++n}`, slug: `p${n}`, number: n, title: `P${n}`, difficulty: "medium", pattern: "D1", lists: ["neetcode150"], premium: false, tags: [], companies: [],
    video: null, technique: "D1:a", role: "must_learn", practice_of: null, order: n, has_page: false, priority: "must", insight: "", recent: false, list_tag: "neetcode150",
    state: { solved: false, assisted: false, last_grade: null, reps: 0, lapses: 0, last_review: null, due: null, retrievability: null },
    ...over,
  }) as DsaProblem;
const co = (name: string) => ({ name, group: "Big Tech", frequency: 10, recent: true });

const overview = (problems: DsaProblem[]) =>
  ({
    today: "2026-10-31",
    patterns: [{ code: "D1", name: "Arrays" }, { code: "D2", name: "Graphs" }],
    techniques: [{ id: "D1:a", pattern: "Arrays", name: "Seen set" }, { id: "D1:b", pattern: "Arrays", name: "Counting" }, { id: "D2:c", pattern: "Graphs", name: "BFS" }],
    problems,
  }) as unknown as DsaOverview;

test("the graph has patterns, the techniques in scope, problems and the companies that ask most", () => {
  const ps = [
    make({ technique: "D1:a", tags: ["Array", "Hash Table", "Sorting"], companies: [co("Google")] }),
    make({ technique: "D1:b", tags: ["Array", "Hash Table", "Sorting"], companies: [co("Google")] }),
    make({ technique: "D2:c", pattern: "D2", tags: ["Graph"] }),
  ];
  const g = buildGraph(overview(ps), ps);
  const kinds = (k: string) => g.nodes.filter((x) => x.kind === k).length;
  assert.equal(kinds("pat"), 2);
  assert.equal(kinds("tec"), 3);
  assert.equal(kinds("pro"), 3);
  assert.equal(kinds("co"), 1);
  // two problems of different techniques sharing three tags are linked
  assert.equal(g.edges.filter((e) => e[2] === "tag").length, 1);
  // Google asks two Arrays problems, fewer than five: no company link
  assert.equal(g.edges.filter((e) => e[2] === "asks").length, 0);
});

test("the shortest path goes through the links, and there is none between parts that do not touch", () => {
  const ps = [make({ technique: "D1:a" }), make({ technique: "D2:c", pattern: "D2" })];
  const g = buildGraph(overview(ps), ps);
  const a = g.nodes.find((x) => x.key === `pro:${ps[0]!.id}`)!.id;
  const pat = g.nodes.find((x) => x.key === "pat:D1")!.id;
  assert.deepEqual(shortestPath(g, a, pat).length, 3, "problem, technique, pattern");
  const b = g.nodes.find((x) => x.key === `pro:${ps[1]!.id}`)!.id;
  assert.deepEqual(shortestPath(g, a, b), []);
});

test("FastRP gives alike vectors to nodes with alike surroundings", () => {
  const ps = [make({ technique: "D1:a" }), make({ technique: "D1:a" }), make({ technique: "D2:c", pattern: "D2" })];
  const g = buildGraph(overview(ps), ps);
  const v = fastRP(g);
  const id = (p: DsaProblem) => g.nodes.find((x) => x.key === `pro:${p.id}`)!.id;
  const same = cosine(v[id(ps[0]!)]!, v[id(ps[1]!)]!);
  const other = cosine(v[id(ps[0]!)]!, v[id(ps[2]!)]!);
  assert.ok(same > other, `${same} > ${other}`);
  assert.ok(Math.abs(cosine(v[0]!, v[0]!) - 1) < 1e-5, "vectors are unit length");
});

test("shared tags are the tags two problems both carry", () => {
  assert.deepEqual(sharedTags(make({ tags: ["Array", "Graph"] }), make({ tags: ["Graph", "Tree"] })), ["Graph"]);
});
