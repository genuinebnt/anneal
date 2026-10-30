// Run with: node --test web/src/mock.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import type { DsaProblem, MockConfig } from "./api.ts";
import { clock, countBy, currentIndex, defaultConfig, draw, elapsed, isLogged, limitOf, mmss, newRound, onProblem, poolOf, shortfall, summary, warning } from "./mock.ts";

let n = 0;
const make = (over: Partial<DsaProblem> & { companies?: DsaProblem["companies"] } = {}): DsaProblem => ({
  id: `lc-p${++n}`, slug: `p${n}`, number: n, title: `P${n}`, difficulty: "medium", pattern: "D1", lists: ["neetcode150", "neetcode250", "all"], premium: false, tags: [], companies: [],
  video: null, technique: "t", role: "must_learn", practice_of: null, order: n, has_page: false,
  state: { solved: false, assisted: false, last_grade: null, reps: 0, lapses: 0, last_review: null, due: null, retrievability: null },
  ...over,
});
const cfg = (over: Partial<MockConfig> = {}): MockConfig => ({ ...defaultConfig(), ...over });
const TODAY = "2026-10-07";
const co = (name: string, group: string, recent = false) => ({ name, group, frequency: 10, recent });

test("lists, premium and topics narrow the pool", () => {
  const ps = [make({ pattern: "D1" }), make({ pattern: "D7" }), make({ pattern: "D7", premium: true }), make({ lists: ["practice"], pattern: "D7" }), make({ lists: ["all"] })];
  assert.equal(poolOf(ps, cfg(), TODAY).length, 2, "NeetCode 150 only, no Premium");
  assert.equal(poolOf(ps, cfg({ lists: ["neetcode150", "practice"] }), TODAY).length, 3);
  assert.equal(poolOf(ps, cfg({ topics: { D7: 1 } }), TODAY).length, 1);
  assert.equal(poolOf(ps, cfg({ topics: { D7: -1 } }), TODAY).length, 1);
  assert.equal(poolOf(ps, cfg({ lists: ["all"], topics: { D1: 1, D7: -1 } }), TODAY).length, 2, "include D1; the other 'all' problem is also D1");
});

test("the history filter", () => {
  const solo = make({ state: { solved: true, assisted: false, last_grade: "good", reps: 1, lapses: 0, last_review: TODAY, due: "2026-10-11", retrievability: 1 } });
  const helped = make({ state: { solved: true, assisted: true, last_grade: "hard", reps: 1, lapses: 0, last_review: TODAY, due: TODAY, retrievability: 1 } });
  const failed = make({ state: { solved: false, assisted: false, last_grade: "again", reps: 0, lapses: 1, last_review: TODAY, due: TODAY, retrievability: 1 } });
  const fresh = make();
  const ps = [solo, helped, failed, fresh];
  const ids = (status: MockConfig["status"]) => poolOf(ps, cfg({ status }), TODAY).map((p) => p.id);
  assert.deepEqual(ids("unseen"), [fresh.id]);
  assert.deepEqual(ids("attempted"), [solo.id, helped.id, failed.id]);
  assert.deepEqual(ids("solved"), [solo.id]);
  assert.deepEqual(ids("due"), [helped.id, failed.id]);
  assert.equal(ids("all").length, 4);
});

test("the pool can be only what is done, or that plus unsolved problems in the same patterns", () => {
  const done = { solved: true, assisted: false, last_grade: "good" as const, reps: 1, lapses: 0, last_review: TODAY, due: null, retrievability: 1 };
  const helped = { ...done, assisted: true, last_grade: "hard" as const };
  const a = make({ pattern: "D1", state: done });
  const b = make({ pattern: "D2", state: helped });
  const sameAsA = make({ pattern: "D1" });
  const other = make({ pattern: "D3" });
  const outside = make({ pattern: "D4", lists: ["all"], state: done });
  const premiumSame = make({ pattern: "D1", premium: true });
  const ps = [a, b, sameAsA, other, outside, premiumSame];
  const ids = (reach?: MockConfig["reach"], over: Partial<MockConfig> = {}) => poolOf(ps, cfg({ reach, ...over }), TODAY).map((p) => p.id);
  assert.deepEqual(ids("done"), [a.id, b.id], "solved with or without help; the list still applies");
  assert.deepEqual(ids("similar"), [a.id, b.id, sameAsA.id], "plus the unsolved in a pattern with a solve, no Premium");
  assert.equal(ids("all").length, 4);
  assert.equal(ids(undefined).length, 4, "a setup saved before this means everything");
  assert.deepEqual(ids("similar", { lists: ["neetcode150", "all"] }), [a.id, b.id, sameAsA.id, outside.id], "a solve in another list still counts as a pattern done");
  assert.deepEqual(ids("similar", { topics: { D1: 1 } }), [a.id, sameAsA.id], "other filters still apply");
});

test("companies: a group or a company, and recent only", () => {
  const a = make({ companies: [co("Google", "Big Tech", true), co("Uber", "Top tech")] });
  const b = make({ companies: [co("Stripe", "Top tech")] });
  const c = make({ companies: [] });
  const ps = [a, b, c];
  assert.equal(poolOf(ps, cfg({ groups: ["Big Tech"] }), TODAY).length, 1);
  assert.equal(poolOf(ps, cfg({ groups: ["Top tech"] }), TODAY).length, 2);
  assert.equal(poolOf(ps, cfg({ groups: ["Big Tech"], companies: ["Stripe"] }), TODAY).length, 2);
  assert.equal(poolOf(ps, cfg({ groups: ["Top tech"], recent: true }), TODAY).length, 0, "Uber and Stripe are not recent");
  assert.equal(poolOf(ps, cfg({ companies: ["Google"], recent: true }), TODAY).length, 1);
  assert.equal(poolOf(ps, cfg({ recent: true }), TODAY).length, 1, "recent with no company picked: asked recently by anyone");
});

test("a draw gives the asked mix, easiest first, without repeats", () => {
  const ps = [...Array(6)].map(() => make({ difficulty: "easy" })).concat([...Array(6)].map(() => make({ difficulty: "medium" })), [...Array(3)].map(() => make({ difficulty: "hard" })));
  const c = cfg({ mix: [1, 2, 1] });
  for (let i = 0; i < 50; i++) {
    const d = draw(poolOf(ps, c, TODAY), c, TODAY)!;
    assert.deepEqual(d.map((p) => p.difficulty), ["easy", "medium", "medium", "hard"]);
    assert.equal(new Set(d.map((p) => p.id)).size, 4);
  }
  const any = cfg({ anyDiff: true, anyCount: 5 });
  assert.equal(draw(poolOf(ps, any, TODAY), any, TODAY)!.length, 5);
});

test("a setup the pool can't fill says so and draws nothing", () => {
  const ps = [make({ difficulty: "medium" }), make({ difficulty: "easy" })];
  assert.match(shortfall(poolOf(ps, cfg({ mix: [0, 2, 0] }), TODAY), cfg({ mix: [0, 2, 0] }))!, /Not enough/);
  assert.equal(draw(poolOf(ps, cfg({ mix: [0, 2, 0] }), TODAY), cfg({ mix: [0, 2, 0] }), TODAY), null);
  assert.match(shortfall([], cfg({ mix: [0, 0, 0] }))!, /at least one/);
  assert.match(shortfall(ps, cfg({ anyDiff: true, anyCount: 3 }))!, /Only 2/);
  assert.deepEqual(countBy(ps), [1, 1, 0]);
});

test("weak problems are drawn about three times as often", () => {
  const weak = make({ state: { solved: false, assisted: false, last_grade: "again", reps: 0, lapses: 1, last_review: TODAY, due: "2026-10-20", retrievability: 1 } });
  const others = [...Array(5)].map(() => make());
  const c = cfg({ favour: "weak", mix: [0, 1, 0] });
  let hits = 0;
  let seed = 1;
  const rand = () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
  for (let i = 0; i < 4000; i++) if (draw([weak, ...others], c, TODAY, rand)![0].id === weak.id) hits++;
  // 3 of 8 total weight is 37.5%; uniform would be 16.7%.
  assert.ok(hits / 4000 > 0.33 && hits / 4000 < 0.42, `weak drawn ${hits / 4000}`);
});

test("the clocks: pauses don't count, per-problem time restarts when the next problem opens", () => {
  const c = cfg({ total: 45 });
  const r = newRound(c, ["a", "b", "c"], 1_000_000);
  assert.equal(elapsed(r, 1_000_000 + 90_000), 90);
  r.pausedAt = 1_000_000 + 60_000;
  assert.equal(elapsed(r, 1_000_000 + 600_000), 60, "frozen while paused");
  r.pausedMs = 540_000;
  r.pausedAt = null;
  assert.equal(elapsed(r, 1_000_000 + 600_000), 60, "the pause left out afterwards");
  assert.deepEqual(clock(r, "medium", 1_000_000 + 600_000), { value: 45 * 60 - 60, counting: "down" });
  // Problem a is logged at 40 s but "next" is clicked at 50 s: b's clock starts then.
  r.marks.push({ grade: "good", to: 40, due: "2026-10-11" });
  assert.equal(isLogged(r), true);
  assert.equal(onProblem(r, 1_000_000 + 600_000), 40, "a logged problem's time stops at the log");
  r.starts.push(50);
  assert.equal(isLogged(r), false);
  assert.equal(currentIndex(r), 1);
  assert.equal(onProblem(r, 1_000_000 + 600_000), 10);
  const per = { ...r, config: cfg({ format: "per" }) };
  assert.equal(clock(per, "medium", 1_000_000 + 600_000).value, 25 * 60 - 10);
  const up = { ...r, config: cfg({ format: "up" }) };
  assert.deepEqual(clock(up, null, 1_000_000 + 600_000), { value: 60, counting: "up" });
  assert.equal(limitOf(per.config, "hard"), 45 * 60);
  assert.equal(limitOf(c, "hard"), 45 * 60);
  r.ended = 75;
  assert.equal(elapsed(r, 9e12), 75);
  // b was opened at 50 and never logged: it keeps its 25 s; c was never opened.
  assert.deepEqual(summary(r, 0), { items: [{ id: "a", grade: "good", seconds: 40 }, { id: "b", grade: null, seconds: 25 }, { id: "c", grade: null, seconds: 0 }], seconds: 75 });
});

test("warnings fire once, as the clock crosses 10, 5 and 1 minutes", () => {
  assert.equal(warning(601, 600, 2700), 600);
  assert.equal(warning(700, 400, 2700), 600);
  assert.equal(warning(600, 599, 2700), null, "already past 600");
  assert.equal(warning(301, 300, 2700), 300);
  assert.equal(warning(61, 60, 2700), 60);
  assert.equal(warning(300, 299, 300), null, "a 5 minute round doesn't warn about 5 minutes");
  assert.equal(mmss(125), "02:05");
  assert.equal(mmss(-65), "+01:05");
  assert.equal(mmss(3725), "1:02:05");
});
