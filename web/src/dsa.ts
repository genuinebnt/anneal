// What the DSA screens share: labels, the filters and the arithmetic behind the plan.
import type { Band, DsaListName, DsaOverview, DsaProblem, Grade, SrsSettings, Weekday } from "./api";

export const WEEKDAYS: Weekday[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

export const DIFF: Record<Band, [label: string, color: string]> = {
  easy: ["Easy", "var(--grn)"],
  medium: ["Medium", "var(--warn)"],
  hard: ["Hard", "var(--bad)"],
};
/** Rough minutes per problem on LeetCode, for "to finish". */
export const MINUTES: Record<Band, number> = { easy: 12, medium: 25, hard: 45 };

export type ListKey = DsaListName | "new250";

export const LISTS: [ListKey, string][] = [
  ["blind75", "Blind 75"],
  ["neetcode150", "NeetCode 150"],
  ["neetcode250", "NeetCode 250"],
  ["new250", "New ideas in the 250"],
  ["all", "NeetCode All"],
];

export const inList = (p: DsaProblem, list: ListKey): boolean => {
  if (list === "all") return true;
  if (list === "new250") return p.lists.includes("neetcode250") && !p.lists.includes("neetcode150") && p.role === "must_learn";
  return p.lists.includes(list);
};

/** One line per pattern, from the approved design. Keyed by NeetCode's pattern name. */
export const BLURB: Record<string, string> = {
  "Arrays & Hashing": "Trade memory for time: a hash map or set turns a nested scan into one pass.",
  "Two Pointers": "Two indices walking toward each other, or in step, replace nested loops on sorted or linked data.",
  "Sliding Window": "A window that grows and shrinks keeps a running answer for every subarray.",
  Stack: "Last in, first out fits anything nested, or waiting on a later answer.",
  "Binary Search": "Halve the search space whenever you can answer 'which side?' in O(1).",
  "Linked List": "Pointer surgery: reverse, merge, find the middle, detect the loop.",
  Trees: "Recursion mirrors the shape: solve for the children, combine at the node.",
  Tries: "A prefix tree makes 'starts with' and word search cheap.",
  "Heap / Priority Queue": "Always know the best remaining item without sorting everything.",
  Backtracking: "Build candidates one choice at a time, undo, and prune the dead ends.",
  Graphs: "Model it as nodes and edges, then pick the traversal: DFS, BFS, union-find or topological sort.",
  "Advanced Graphs": "Weighted edges and spanning structures: Dijkstra, Prim and Kruskal, Bellman-Ford.",
  "1-D Dynamic Programming": "Define the state, write the recurrence, remember the sub-answers.",
  "2-D Dynamic Programming": "Two indices as the state: grids, string pairs and knapsacks.",
  Greedy: "Make the locally best choice, and prove it never hurts.",
  Intervals: "Sort by start, then sweep: merge, insert and count overlaps.",
  "Math & Geometry": "Matrices, digits and number tricks with no fancy data structure.",
  "Bit Manipulation": "XOR, shifts and masks for O(1)-space tricks.",
};

export type Status = "new" | "solved" | "due" | "retry";
export const STATUS_LABEL: Record<Status, string> = { new: "Not started", solved: "Solved", due: "Due for review", retry: "Attempted" };

export function statusOf(p: DsaProblem, today: string): Status {
  const s = p.state;
  if (!s.last_grade) return "new";
  if (s.last_grade === "again") return "retry";
  return s.due && s.due <= today ? "due" : "solved";
}

/** The mark on a problem: how the latest log went. */
export const markOf = (p: DsaProblem): "solo" | "help" | "fail" | "" => {
  const g = p.state.last_grade;
  return g === "good" || g === "easy" ? "solo" : g === "hard" ? "help" : g === "again" ? "fail" : "";
};
export const MARK_GLYPH = { solo: "✓", help: "½", fail: "✗", "": "" } as const;

export const GRADES: { grade: Grade; glyph: string; label: string; cls: string }[] = [
  { grade: "good", glyph: "✓", label: "Solved on my own", cls: "solo" },
  { grade: "hard", glyph: "½", label: "Solved with a hint", cls: "" },
  { grade: "again", glyph: "✗", label: "Attempted, not solved", cls: "fail" },
];

/** The log buttons on a problem that is reviewed: the three above plus "instant" (the whole approach recalled at once). */
export const REVIEW_GRADES: typeof GRADES = [...GRADES, { grade: "easy", glyph: "⚡", label: "Instant: recalled the whole approach at once", cls: "solo" }];

export const hours = (m: number) => (m >= 600 ? `~${Math.round(m / 60)}h` : `~${(m / 60).toFixed(1).replace(/\.0$/, "")}h`);

export const leetcode = (slug: string) => `https://leetcode.com/problems/${slug}/`;
/** NeetCode's video id, or the full URL when that's what the data holds. */
export const videoUrl = (v: string) => (v.startsWith("http") ? v : `https://youtu.be/${v}`);

/** "Sun 11 Oct", or "today" and "tomorrow". */
export function niceDate(iso: string, today: string): string {
  const days = Math.round((Date.parse(`${iso}T00:00`) - Date.parse(`${today}T00:00`)) / 864e5);
  if (days === 0) return "today";
  if (days === 1) return "tomorrow";
  return new Date(`${iso}T00:00`).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

export const daysUntil = (iso: string, today: string) => Math.round((Date.parse(`${iso}T00:00`) - Date.parse(`${today}T00:00`)) / 864e5);

// ---- the plan's arithmetic (mirrors crates/api/src/reviews.rs, so the plan screen can preview unsaved edits) ----

const dayName = (d: Date): Weekday => WEEKDAYS[(d.getDay() + 6) % 7] ?? "mon";
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
export const parseDay = (iso: string) => new Date(`${iso}T00:00`);
export const isoDay = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export interface PacePreview {
  solveDaysLeft: number;
  perSolveDay: number | null;
  perWeek: number | null;
  finish: string | null;
  daysVsTarget: number | null;
}

export function pacePreview(remaining: number, today: string, s: Pick<SrsSettings, "new_days" | "new_per_day" | "target_date">): PacePreview {
  const start = parseDay(today);
  const solve = (d: Date) => s.new_days.includes(dayName(d));
  let finish: Date | null = null;
  if (remaining <= 0) finish = start;
  else if (s.new_days.length) {
    let left = remaining;
    for (let i = 0; i < 3660 && !finish; i++) {
      const d = addDays(start, i);
      if (solve(d)) left -= Math.max(1, s.new_per_day);
      if (left <= 0) finish = d;
    }
  }
  const target = s.target_date ? parseDay(s.target_date) : null;
  let solveDaysLeft = 0;
  let perSolveDay: number | null = null;
  let perWeek: number | null = null;
  if (target && target >= start) {
    for (let d = start; d <= target; d = addDays(d, 1)) if (solve(d)) solveDaysLeft++;
    perSolveDay = solveDaysLeft > 0 ? Math.ceil(remaining / solveDaysLeft) : null;
    const weeks = ((target.getTime() - start.getTime()) / 864e5 + 1) / 7;
    perWeek = remaining > 0 && solveDaysLeft > 0 ? remaining / weeks : null;
  }
  return {
    solveDaysLeft,
    perSolveDay,
    perWeek,
    finish: finish ? isoDay(finish) : null,
    daysVsTarget: finish && target ? Math.round((finish.getTime() - target.getTime()) / 864e5) : null,
  };
}

/** Problems left for a goal, counted the way the server does. */
export function goalRemaining(o: DsaOverview, goal: SrsSettings["goal"]): { total: number; done: number; remaining: number } {
  if (goal.custom_left != null) return { total: goal.custom_left, done: 0, remaining: goal.custom_left };
  const inGoal = (p: DsaProblem) => p.lists.includes(goal.list) && !(goal.free_only && p.premium);
  const list = o.problems.filter(inGoal);
  const done = list.filter((p) => p.state.solved).length;
  const beyond = o.problems.filter((p) => !inGoal(p) && p.lists.includes("neetcode250") && p.state.solved).length;
  const extraDone = Math.min(beyond, goal.extra);
  const total = list.length + goal.extra;
  return { total, done: done + extraDone, remaining: Math.max(0, total - done - extraDone) };
}

/** "tomorrow", "in 6 days", "in 5 weeks", "in 3 months". */
export function inDays(days: number): string {
  if (days <= 1) return "tomorrow";
  if (days < 14) return `in ${days} days`;
  if (days < 60) return `in ${Math.round(days / 7)} weeks`;
  return `in ${Math.round(days / 30)} months`;
}
