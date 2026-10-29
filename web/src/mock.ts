// The mock interview's rules: which problems a setup allows, how a round is drawn, and how the clocks read.
// Plain functions with no React, so they can be tested with `node --test web/src/mock.test.ts`.
import type { Band, DsaProblem, Grade, MockConfig, MockItem } from "./api";

export const BANDS: Band[] = ["easy", "medium", "hard"];
export const DEFAULT_MINUTES: [number, number, number] = [12, 25, 45];

export const MOCK_LISTS: [string, string][] = [
  ["neetcode150", "NeetCode 150"],
  ["neetcode250", "NeetCode 250"],
  ["all", "All problems"],
  ["practice", "Practice"],
];

export const MOCK_STATUS: [MockConfig["status"], string, string][] = [
  ["all", "Everything", "Includes problems you have never opened."],
  ["unseen", "Not attempted yet", "Only problems with no log yet, so each one is new to you."],
  ["attempted", "Attempted", "Problems you have logged at least once with any result, including with help and attempted."],
  ["solved", "Solved on my own", "Problems you have logged ✓ on your own. Good for checking you still can."],
  ["due", "Due for review", "Problems the review schedule says you should revisit now. A mock round doubles as an unassisted review."],
];

export const FAVOUR: [MockConfig["favour"], string, string][] = [
  ["random", "Uniformly random", "Every problem that matches has the same chance."],
  ["weak", "Favour my weak ones", "Problems you logged with help or as attempted, and ones overdue for review, are drawn about three times as often."],
  ["asked", "Favour often-asked", "Problems asked by more of the selected companies are drawn more often."],
];

export const defaultConfig = (): MockConfig => ({
  lists: ["neetcode150"],
  status: "all",
  topics: {},
  mix: [0, 2, 0],
  anyDiff: false,
  anyCount: 2,
  groups: [],
  companies: [],
  recent: false,
  format: "total",
  total: 45,
  per: [...DEFAULT_MINUTES],
  strict: true,
  blind: true,
  auto: false,
  warns: true,
  favour: "random",
});

/** The presets fill the problem count and the time; everything else keeps what is set. */
export const PRESETS: { key: string; name: string; detail: string; apply: (c: MockConfig) => MockConfig }[] = [
  { key: "warm", name: "Warm-up", detail: "1 medium · 25 min", apply: (c) => ({ ...c, anyDiff: false, mix: [0, 1, 0], total: 25, format: "total" }) },
  { key: "std", name: "Standard", detail: "2 medium · 45 min", apply: (c) => ({ ...c, anyDiff: false, mix: [0, 2, 0], total: 45, format: "total" }) },
  { key: "loop", name: "Onsite loop", detail: "2 medium, 1 hard · 90 min", apply: (c) => ({ ...c, anyDiff: false, mix: [0, 2, 1], total: 90, format: "total" }) },
];

export const slotsOf = (c: MockConfig): number => (c.anyDiff ? c.anyCount : c.mix[0] + c.mix[1] + c.mix[2]);
/** Minutes the problems would take at the suggested pace. */
export const paceMinutes = (c: MockConfig): number => (c.anyDiff ? c.anyCount * c.per[1] : c.mix.reduce((n, k, i) => n + k * (c.per[i] ?? 0), 0));

/** Where a problem stands for the history filter. */
export function matchesStatus(p: DsaProblem, status: MockConfig["status"], today: string): boolean {
  const s = p.state;
  switch (status) {
    case "all":
      return true;
    case "unseen":
      return !s.last_grade;
    case "attempted":
      return !!s.last_grade;
    case "solved":
      return s.solved && !s.assisted;
    case "due":
      return !!s.due && s.due <= today;
  }
}

/** Whether a problem passes everything in the setup except the lists (the lists are what the topic counts are shown against). */
export function matches(p: DsaProblem, c: MockConfig, today: string): boolean {
  if (p.premium) return false;
  if (!c.lists.some((l) => p.lists.includes(l as never))) return false;
  if (!matchesStatus(p, c.status, today)) return false;
  const included = Object.entries(c.topics).filter(([, v]) => v === 1).map(([k]) => k);
  if (c.topics[p.pattern] === -1) return false;
  if (included.length && !included.includes(p.pattern)) return false;
  if (c.groups.length || c.companies.length) {
    return p.companies.some((co) => (c.groups.includes(co.group) || c.companies.includes(co.name)) && (!c.recent || co.recent));
  }
  return !c.recent || p.companies.some((co) => co.recent);
}

export const poolOf = (problems: DsaProblem[], c: MockConfig, today: string): DsaProblem[] => problems.filter((p) => matches(p, c, today));

export const countBy = (pool: DsaProblem[]): [number, number, number] => {
  const n: [number, number, number] = [0, 0, 0];
  for (const p of pool) n[BANDS.indexOf(p.difficulty)] = (n[BANDS.indexOf(p.difficulty)] ?? 0) + 1;
  return n;
};

/** Why a setup can't start, or null. */
export function shortfall(pool: DsaProblem[], c: MockConfig): string | null {
  const need = slotsOf(c);
  if (need === 0) return "Pick at least one problem.";
  if (c.anyDiff) return pool.length < need ? `Only ${pool.length} problem${pool.length === 1 ? "" : "s"} match. Loosen a filter or ask for fewer.` : null;
  const have = countBy(pool);
  return c.mix.some((k, i) => k > (have[i] ?? 0)) ? "Not enough problems of a difficulty. The red box shows which one. Loosen a filter or ask for fewer." : null;
}

const weakGrades: (Grade | null)[] = ["again", "hard"];

/** How likely a problem is to be drawn, relative to the others. */
export function weight(p: DsaProblem, c: MockConfig, today: string): number {
  if (c.favour === "weak") {
    const s = p.state;
    return weakGrades.includes(s.last_grade) || (!!s.due && s.due <= today) ? 3 : 1;
  }
  if (c.favour === "asked") {
    const picked = c.groups.length || c.companies.length;
    const n = p.companies.filter((co) => !picked || c.groups.includes(co.group) || c.companies.includes(co.name)).length;
    return 1 + n;
  }
  return 1;
}

/** `k` problems chosen without repeats, each more likely the heavier it is. */
function sample(pool: DsaProblem[], k: number, c: MockConfig, today: string, rand: () => number): DsaProblem[] {
  const left = pool.map((p) => ({ p, w: weight(p, c, today) }));
  const out: DsaProblem[] = [];
  while (out.length < k && left.length) {
    let r = rand() * left.reduce((n, x) => n + x.w, 0);
    let i = 0;
    while (i < left.length - 1 && r >= (left[i]?.w ?? 0)) r -= left[i++]?.w ?? 0;
    const [picked] = left.splice(i, 1);
    if (picked) out.push(picked.p);
  }
  return out;
}

/** The round's problems, easiest first. Returns null when the pool can't fill the setup. */
export function draw(pool: DsaProblem[], c: MockConfig, today: string, rand: () => number = Math.random): DsaProblem[] | null {
  if (shortfall(pool, c)) return null;
  if (c.anyDiff) return sample(pool, c.anyCount, c, today, rand).sort((a, b) => BANDS.indexOf(a.difficulty) - BANDS.indexOf(b.difficulty));
  return BANDS.flatMap((band, i) =>
    sample(
      pool.filter((p) => p.difficulty === band),
      c.mix[i] ?? 0,
      c,
      today,
      rand,
    ),
  );
}

// ---------------------------------------------------------------- the clock

/** One problem's outcome in a round: how it was logged (none when skipped), when, and what happens to it next. */
export interface Mark {
  grade: Grade | null;
  /** Seconds since the round began. */
  to: number;
  /** When it comes back for review; none for practice problems and skips. */
  due: string | null;
}

/** A round in progress. Times are seconds since the round began, with paused time left out. */
export interface Round {
  config: MockConfig;
  ids: string[];
  startedAt: number;
  pausedMs: number;
  pausedAt: number | null;
  /** When each problem so far was opened; the first is 0. */
  starts: number[];
  /** One per problem logged or skipped, in order. The current problem has none until it is. */
  marks: Mark[];
  /** Set when the round is over: the seconds it ran. */
  ended: number | null;
  saved: boolean;
}

export const newRound = (config: MockConfig, ids: string[], now: number): Round => ({ config, ids, startedAt: now, pausedMs: 0, pausedAt: null, starts: [0], marks: [], ended: null, saved: false });

export function elapsed(r: Round, now: number): number {
  if (r.ended != null) return r.ended;
  return Math.max(0, Math.floor(((r.pausedAt ?? now) - r.startedAt - r.pausedMs) / 1000));
}

/** The index of the problem on screen: the one being worked on, or the one just logged and waiting for "next". */
export const currentIndex = (r: Round): number => r.starts.length - 1;
/** The current problem has been logged or skipped. */
export const isLogged = (r: Round): boolean => r.marks.length === r.starts.length;
/** Seconds the current problem took, or has taken so far. */
export function onProblem(r: Round, now: number): number {
  const start = r.starts.at(-1) ?? 0;
  const end = isLogged(r) ? (r.marks.at(-1)?.to ?? start) : elapsed(r, now);
  return end - start;
}

/** Suggested seconds for a problem. */
export const suggested = (c: MockConfig, band: Band): number => (c.per[BANDS.indexOf(band)] ?? 0) * 60;

/**
 * What the big clock shows: seconds left (negative once over) for a countdown, or seconds counted up.
 * For a per-problem countdown it belongs to the current problem.
 */
export function clock(r: Round, band: Band | null, now: number): { value: number; counting: "down" | "up" } {
  const c = r.config;
  if (c.format === "up") return { value: elapsed(r, now), counting: "up" };
  if (c.format === "per" && band) return { value: suggested(c, band) - onProblem(r, now), counting: "down" };
  return { value: c.total * 60 - elapsed(r, now), counting: "down" };
}

/** Seconds that count as "the limit" for warnings: the round length, or the current problem's suggested time. */
export function limitOf(c: MockConfig, band: Band | null): number {
  if (c.format === "per" && band) return suggested(c, band);
  return c.total * 60;
}

export const WARN_AT = [600, 300, 60];

/** The warning that applies when the clock goes from `before` to `after` seconds left, or null. */
export function warning(before: number, after: number, limit: number): number | null {
  return WARN_AT.find((w) => limit > w && before > w && after <= w) ?? null;
}

export const mmss = (s: number): string => {
  const a = Math.abs(Math.round(s));
  const h = Math.floor(a / 3600);
  const m = Math.floor((a % 3600) / 60);
  const sec = a % 60;
  return `${s < 0 ? "+" : ""}${h ? `${h}:${String(m).padStart(2, "0")}` : String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
};

/** What the round sends to the server. A problem opened but never logged keeps the time spent on it. */
export function summary(r: Round, now: number): { items: MockItem[]; seconds: number } {
  const total = elapsed(r, now);
  const items: MockItem[] = r.ids.map((id, i) => {
    const start = r.starts[i];
    if (start == null) return { id, grade: null, seconds: 0 };
    const mark = r.marks[i];
    return { id, grade: mark?.grade ?? null, seconds: Math.max(0, (mark?.to ?? total) - start) };
  });
  return { items, seconds: total };
}
