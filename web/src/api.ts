import { markDown, markUp } from "./connection";
// Types mirror crates/api/src/views.rs and crates/runner/src/result.rs.

export type Band = "easy" | "medium" | "hard";
export type Mode = "write" | "fix" | "stage";
export type Section = "D" | "L" | "S" | "C" | "Y" | "F" | "P";
export type Tier = "core" | "light" | "sde3";
export type ContentStatus = "draft" | "ready";
export type Progress = "not_started" | "started" | "solved" | "assisted";

export interface StageView {
  slug: string;
  name: string;
  band: Band;
  total: number;
  ready: number;
  solved: number;
}

export interface TrackSummary {
  code: string;
  slug: string;
  name: string;
  section: Section;
  tier: Tier;
  order: number;
  summary: string;
  stages: StageView[];
  total: number;
  ready: number;
  solved: number;
  /** 0–100: weighted by level, assisted solves count half. */
  readiness: number;
}

export interface ProblemSummary {
  id: string;
  slug: string;
  title: string;
  mode: Mode;
  level: Band;
  stage: string;
  order: number;
  status: ContentStatus;
  tags: string[];
  /** Companies known to ask it, FAANG first. */
  companies: string[];
  progress: Progress;
}

export interface CompanyGroup {
  name: string;
  companies: string[];
}

export interface TrackDetail extends TrackSummary {
  problems: ProblemSummary[];
  /** Every taggable company, grouped (FAANG, Big tech, Databases, Rust shops) and in display order. */
  company_groups: CompanyGroup[];
}

export interface Hint {
  kind: string;
  text: string;
}

export interface Rules {
  forbid_methods: string[];
  forbid_types: string[];
  forbid_unsafe: boolean;
  max_changed_lines: number | null;
}

export interface SolutionView {
  unlocked: boolean;
  code: string | null;
  notes: { explanation: string; time: string; space: string } | null;
}

export interface AttemptView {
  started: boolean;
  started_at: string | null;
  solved: boolean;
  assisted: boolean;
  hints_revealed: number;
  solution_revealed: boolean;
  /** A scheduled re-solve rather than the first time through. */
  resolve: boolean;
}

export type RunStatus = "passed" | "failed" | "compile_error" | "timeout";
export type Level = "error" | "warning" | "note" | "help";

export interface Span {
  file: string;
  line_start: number;
  line_end: number;
  col_start: number;
  col_end: number;
  primary: boolean;
  label: string | null;
}

export interface Diagnostic {
  level: Level;
  code: string | null;
  message: string;
  rendered: string;
  spans: Span[];
  notes: string[];
}

export interface TestOutcome {
  suite: "visible" | "hidden";
  name: string;
  outcome: "passed" | "failed" | "ignored" | "timed_out";
  duration_ms: number | null;
  check: { input: string; expected: string; got: string } | null;
  panic: string | null;
  stdout: string;
}

export interface RunView {
  id: number;
  kind: "run" | "submit";
  created_at: string;
  code: string;
  status: RunStatus;
  passed: number;
  total: number;
  duration_ms: number;
  diagnostics: Diagnostic[];
  tests: TestOutcome[];
  /** Fix-this rules broken; any one keeps a submit from solving the problem. */
  violations: Violation[];
}

export interface Violation {
  rule: string;
  message: string;
  line: number | null;
}

export type Language = "rust" | "python";

export interface ProblemDetail {
  id: string;
  slug: string;
  title: string;
  mode: Mode;
  /** What the editor and runner speak. */
  language: Language;
  /** Practice problems: the DSA problems that open it, and whether it's also a warm-up for them. */
  unlocked_by: { id: string; slug: string; title: string }[];
  warmup: boolean;
  level: Band;
  status: ContentStatus;
  track: { code: string; slug: string; name: string; section: Section };
  stage: { slug: string; name: string; band: Band; number: number; position: number; count: number };
  position: number;
  count: number;
  prev: string | null;
  next: string | null;
  statement: string;
  tags: string[];
  companies: string[];
  teaches: string[];
  constraints: string[];
  examples: { input: string; output: string }[];
  follow_up: string | null;
  related: string[];
  /** Crates the problem may use, e.g. ["tokio", "serde"]. */
  crates: string[];
  rules: Rules | null;
  starter: string;
  draft: string | null;
  /** The scratch main.rs for Run. */
  scratch: string;
  visible_tests: string;
  /** The hidden tests' source, once the problem has been solved; null before. */
  hidden_tests: string | null;
  hints: { total: number; revealed: Hint[]; locked: string[] };
  solution: SolutionView;
  attempt: AttemptView;
  runs: RunView[];
}

export interface RunOutcome {
  run: RunView;
  attempt: AttemptView;
  solution: SolutionView;
}

export interface Activity {
  streak: number;
  week: { date: string; solved: number }[];
  week_solved: number;
  week_unassisted: number;
  recent: {
    problem_id: string;
    title: string;
    track: string;
    outcome: "solved" | "assisted" | "failing" | "started";
    detail: string;
    at: string;
  }[];
  next: NextUp | null;
}

export interface NextUp {
  problem_id: string;
  title: string;
  mode: Mode;
  level: Band;
  track_code: string;
  track_slug: string;
  track_name: string;
  stage_name: string;
  reason: "resume" | "current" | "start";
  /** First paragraph of the statement, markdown. */
  excerpt: string;
  readiness: number;
  gain: number;
}

export interface EditorSettings {
  font_size: number;
  font_family: string;
  vim: boolean;
  /** Programming ligatures in the editor. Off by default. */
  ligatures: boolean;
  /** Workspace status-bar toggles. */
  autocomplete: boolean;
  rust_analyzer: boolean;
  borrow_lanes: boolean;
  /** rust-analyzer checks with clippy, so lints show while editing. */
  live_clippy: boolean;
  /** rustfmt after a 1.5 s pause in typing, not only on ⌘S / ⇧⌥F. */
  format_on_pause: boolean;
}

export type Accent = "copper" | "rose" | "sky" | "teal";

export interface AppearanceSettings {
  accent: Accent;
}

export type Grade = "again" | "hard" | "good" | "easy";
export type Weekday = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

/** Review scheduling and the DSA plan (`settings.srs`). */
export interface SrsSettings {
  retention: number;
  /** Reviews each weekday can take. */
  capacity: Record<Weekday, number>;
  consolidate_on: Weekday | null;
  /** The weekdays a new problem is solved on; the other days are practice or rest. */
  new_days: Weekday[];
  new_per_day: number;
  /** New problems on a particular weekday; 0 (or missing) keeps `new_per_day`. */
  new_by_day: Record<Weekday, number>;
  target_date: string | null;
  /** When more is due than a day takes, the important problems go first. */
  prioritise: boolean;
  /** A higher retention target for the Blind 75 and must-learn problems; null gives everything `retention`. */
  core_retention: number | null;
  goal: { list: "blind75" | "neetcode150" | "neetcode250" | "all"; free_only: boolean; extra: number; custom_left: number | null };
}

export interface Settings {
  editor: EditorSettings;
  appearance: AppearanceSettings;
  srs: SrsSettings;
  accents: Accent[];
  font_families: string[];
  font_sizes: [number, number];
}

export interface ProgressOverview {
  streak: number;
  longest_streak: number;
  heat_start: string;
  heat: number[];
  solved_year: number;
  active_days_year: number;
  unassisted_30d: number | null;
  unassisted_prev_30d: number | null;
  focus_week_seconds: number;
  focus_quarter_seconds: number;
  weekly: { week_start: string; easy: number; medium: number; hard: number }[];
  readiness_trend: { area: "dsa" | "rust"; points: (number | null)[] }[];
  this_week: { date: string; solved: number; unassisted: number; focus_seconds: number }[];
  by_section: { section: Section; solved: number; written: number; readiness: number | null }[];
  /** How the DSA problems were logged: first tries, later (scheduled) tries and practice, each counted apart. */
  record: SolveRecord;
}

export interface Outcomes {
  own: number;
  help: number;
  not_yet: number;
}

export interface SolveRecord {
  first: Outcomes;
  later: Outcomes;
  practice: Outcomes;
  practice_count: number;
  problems_practised: number;
  practice_week: number;
}

/** One logged attempt at a problem, oldest first. */
export interface TryMark {
  kind: "first" | "later" | "practice";
  result: "good" | "hard" | "again";
}

export interface Counted {
  key: string;
  message: string;
  count: number;
  where_most: string | null;
}

export interface ProgressStats {
  first_run_pass: number | null;
  runs_per_solve: number | null;
  runs_per_solve_hard: number | null;
  compile_error_rate: number | null;
  within_budget: number | null;
  errors: Counted[];
  lints: Counted[];
  rules: Counted[];
  timing: { level: Band; mode: Mode; budget_minutes: number; median_minutes: number | null; within_budget: number | null; solved: number }[];
}

export interface ReviewItem {
  problem_id: string;
  title: string;
  track: string;
  level: Band;
  step: number;
  interval_days: number;
  due_at: string;
  days_overdue: number;
  last_result: "unassisted" | "assisted";
}

export interface ReviewsView {
  due_today: number;
  overdue: number;
  minutes_today: number;
  retention_30d: number | null;
  in_rotation: number;
  graduated: number;
  queue: ReviewItem[];
  forecast: number[];
}

export interface ScratchResult {
  status: "ok" | "exited" | "compile_error" | "timeout";
  diagnostics: Diagnostic[];
  stdout: string;
  stderr: string;
  exit_code: number | null;
  duration_ms: number;
}

export interface DsaCompany {
  name: string;
  group: string;
  frequency: number;
  recent: boolean;
}

export interface DsaStanding {
  /** Solved at least once. */
  solved: boolean;
  assisted: boolean;
  last_grade: Grade | null;
  reps: number;
  lapses: number;
  last_review: string | null;
  due: string | null;
  retrievability: number | null;
}

export type DsaListName = "blind75" | "neetcode150" | "neetcode250" | "all" | "practice";

export interface DsaProblem {
  id: string;
  slug: string;
  number: number;
  title: string;
  difficulty: Band;
  /** The pattern's track code, e.g. D7. */
  pattern: string;
  lists: DsaListName[];
  premium: boolean;
  tags: string[];
  companies: DsaCompany[];
  video: string | null;
  technique: string;
  role: "must_learn" | "practice";
  practice_of: string | null;
  order: number;
  /** A written solution exists for it (a page). */
  has_page: boolean;
  /** `must`, `strong`, `practice` or `warmup`. */
  priority: "must" | "strong" | "practice" | "warmup";
  /** The key insight: one sentence naming the move that solves it. */
  insight: string;
  /** Asked by a company of the site's set in the last six months. */
  recent: boolean;
  /** The narrowest NeetCode list it is in; null for a problem outside them. */
  list_tag: "blind75" | "neetcode150" | "neetcode250" | "all" | null;
  state: DsaStanding;
}

/** The plan and calendar (docs/DSA.md, decision 28). */
export type DayKind = "solve" | "practice" | "break";
export type PlanDiff = "E" | "M" | "H";

export interface PlanRules {
  /** null follows the goal's free-only setting. */
  premium: boolean | null;
  practice: "none" | "targeted" | "all";
  topics: string[] | null;
  companies: string[];
  recent_only: boolean;
  difficulty: PlanDiff[];
  order: "curriculum" | "ramp" | "company" | "important";
  topic_order: string[] | null;
  /** Topic code to date. A target also covers the topics before it, from where you started. */
  targets: Record<string, string>;
  /** null follows the target date in the review settings. */
  finish_by: string | null;
}

export interface PlanItem {
  id: string;
  slug: string;
  title: string;
  topic: string;
  practice: boolean;
  premium: boolean;
  difficulty: Band;
  companies: { name: string; recent: boolean }[];
}

export interface PlanDay {
  date: string;
  kind: DayKind;
  edited: boolean;
  capacity: number;
  /** Today and later. */
  new?: PlanItem[];
  reviews?: { problem: PlanItem; n: number }[];
  flags?: string[];
  /** Today only: what has been done so far. */
  done?: { problems: { problem: PlanItem; grade: Grade }[]; reviews: number };
  /** Before today. */
  past?: { state: "solved" | "break" | "missed" | "reviews" | "rest"; problems?: { problem: PlanItem; grade: Grade }[]; reviews: number };
}

export interface PlanTopicStatus {
  n: number;
  start: string | null;
  end: string | null;
  due: string | null;
  /** Days after its due date; 0 is on time, 9999 means it doesn't fit. */
  late: number;
  slack: number | null;
  targeted: boolean;
}

export interface PlanTopic {
  id: string;
  name: string;
  done_earlier: boolean;
  status: PlanTopicStatus | null;
  target: string | null;
  covered: boolean;
}

export interface PlanSuggestion {
  topic: string;
  name: string;
  late: number;
  due: string | null;
  remedy: { kind: "add_days"; days: string[]; end: string } | { kind: "not_enough"; end: string | null };
}

export interface PlanView {
  today: string;
  active: string;
  rules: PlanRules;
  overrides: Record<string, DayKind>;
  /** Weekdays that are problem days in the routine, Monday = 0. */
  solve_days: number[];
  topics: PlanTopic[];
  days: PlanDay[];
  summary: {
    finish: string | null;
    baseline_finish: string | null;
    finish_by: string | null;
    left: number;
    main: number;
    practice: number;
    left_out_of_horizon: number;
    per_week: number;
    solve_days_28: number;
    premium: boolean;
    late: number;
  };
  plans: { id: string; name: string; problems: number; finish: string | null; late: number; rules: PlanRules }[];
  suggestion: PlanSuggestion | null;
  history: {
    weeks: { start: string; problems: number; reviews: number }[];
    streak: number;
    problems: number;
    kept_percent: number | null;
    clean_percent: number | null;
    mix: Record<string, number>;
  };
  companies: string[];
  free_only: boolean;
}

export interface PlanPreview {
  diff: { moved: number; reviews_moved: number; finish_days: number; added: number; dropped: number };
  finish: string | null;
  late: number;
  left: number;
}

/** One of the owner's own solutions to a problem. */
export interface MySolution {
  id: number;
  problem_id: string;
  label: string;
  code: string;
  notes: string;
  created_at: string;
  updated_at: string;
}

export interface DsaApproach {
  name: string;
  label: string;
  idea: string;
  code: string;
  time: string;
  space: string;
  /** Why the time and space are what they are. Optional. */
  time_why?: string;
  space_why?: string;
  note: string;
}

/** The written lesson for a problem. Text is markdown. */
export interface DsaPage {
  intuition: string;
  tips: string[];
  approaches: DsaApproach[];
}

export interface DsaPattern {
  code: string;
  slug: string;
  name: string;
  total: number;
  in_150: number;
  solved: number;
  /** LeetCode practice problems for the pattern: how many, and how many are solved. */
  practice_total: number;
  practice_solved: number;
  /** The pattern has an Idea tab. */
  has_idea: boolean;
}

/** A pattern's Idea tab (content/dsa/ideas, docs/DSA_IDEA_TAB.md). Text may use `**bold**`, `*italic*` and backticks. */
export interface PatternIdea {
  pattern: string;
  /** The idea in one sentence. */
  tagline: string;
  what: string[];
  /** Names of the animations, from `ideaDemos.ts`. */
  demos: string[];
  applies: string[];
  yes: string[];
  no: string[];
  which: { says: string; kind: string; move: string; glyph: string; technique: string }[];
  cost: { name: string; time: string; space: string; why: string }[];
  with: { pattern: string; code: string | null; has_idea: boolean; how: string; example: { slug: string; title: string | null; number: number | null } }[];
  apply: string[];
}

/** One technique's practice: the NeetCode problem that teaches it, and LeetCode problems that drill it. */
export interface PracticeTechnique {
  id: string;
  name: string;
  must_learn: { id: string; slug: string; number: number; title: string; solved: boolean };
  solved: number;
  problems: DsaProblem[];
}

export interface PracticeList {
  pattern: string;
  code: string;
  techniques: PracticeTechnique[];
}

/** One more tab of a template: the same idea written another way. */
export interface LessonVariant {
  name: string;
  note: string;
  template: string;
}

/** What a pattern lesson says about one technique. */
export interface TechniqueLesson {
  id: string;
  signals: string[];
  template: string;
  pitfalls: string[];
  /** The group of the page it sits under (patterns with grouped pages only). */
  group?: string;
  /** The name of the first tab when there are variants. */
  template_name?: string;
  variant?: LessonVariant[];
}

/** A lesson for a technique with no must-learn problem in the lists, taught with example problems. */
export interface PatternExtra {
  id: string;
  name: string;
  group: string;
  lesson: TechniqueLesson & { name: string; examples: string[] };
  examples: DsaProblem[];
}

/** A technique that belongs on the page's map and has no lesson yet. */
export interface PatternListed {
  name: string;
  group: string;
  note: string;
}

export interface PatternTechnique {
  id: string;
  name: string;
  /** None until the lesson is written. */
  lesson: TechniqueLesson | null;
  solved: number;
  /** In the NeetCode lists, must-learn first. */
  problems: DsaProblem[];
  practice_total: number;
  practice_solved: number;
}

export interface PatternLessons {
  code: string;
  pattern: string;
  intro: string | null;
  total: number;
  idea: PatternIdea | null;
  /** The groups of the page in order; null for a pattern whose page is not grouped. */
  groups: string[] | null;
  techniques: PatternTechnique[];
  extras: PatternExtra[];
  listed: PatternListed[] | null;
}

export interface DsaTechnique {
  id: string;
  pattern: string;
  name: string;
  must_learn: string;
  problems: number;
}

export interface DsaPace {
  remaining: number;
  solve_days_left: number;
  per_solve_day: number | null;
  per_week: number | null;
  finish_at_current: string | null;
  days_vs_target: number | null;
}

export interface DsaPlan {
  goal_total: number;
  goal_done: number;
  pace: DsaPace;
  /** The next problems in order, from `start`. */
  next_up: string[];
  start: string | null;
  solve_day: boolean;
  capacity: number;
  /** Today's reviews, most forgotten first. */
  review_ids: string[];
  due: number;
  overdue: number;
}

export interface DsaOverview {
  today: string;
  settings: SrsSettings;
  patterns: DsaPattern[];
  techniques: DsaTechnique[];
  /** Lessons per pattern name for techniques with no must-learn problem. */
  lesson_extras: Record<string, number>;
  company_groups: { name: string; companies: string[] }[];
  problems: DsaProblem[];
  plan: DsaPlan;
}

/** LeetCode's statement and hints for a problem. The HTML is cleaned by the server. */
export interface DsaStatement {
  html: string | null;
  hints: string[];
  /** A Premium problem: LeetCode doesn't share its statement. */
  locked: boolean;
  fetched_at: string;
  /** An older copy, because LeetCode couldn't be reached. */
  stale: boolean;
}

/** When a grade would bring a problem back. */
export interface ReviewPreview {
  due: string;
  /** Days from today, after moving to a day with room. */
  days: number;
}

export interface ReviewItem {
  problem: DsaProblem;
  pattern: string;
  technique: string | null;
  /** The chance of recalling it today, 0 to 1. */
  recall: number;
  /** Days until recall drops to 90%. */
  stability: number;
  previews: Record<Grade, ReviewPreview>;
  /** When it is due (a local date). */
  due: string;
}

/** Today's review session (docs/DSA.md, decision 26). */
export interface ReviewQueue {
  today: string;
  capacity: number;
  due: number;
  solve_day: boolean;
  items: ReviewItem[];
  /** The other reviews, soonest first: more due today than the day takes, then the ones coming up. */
  ahead: ReviewItem[];
  next_new: { id: string; slug: string; title: string } | null;
  next_review: { id: string; slug: string; title: string; due: string } | null;
}

/** What a mock interview round is drawn from and how it is timed (docs/DSA.md, decision 25). */
export interface MockConfig {
  /** `neetcode150`, `neetcode250`, `all` or `practice`. */
  lists: string[];
  /** Which problems may be drawn: any in the lists, only ones solved (with or without help), or those plus unsolved ones in a pattern with a solve.
   *  Missing in setups saved before this existed, which means `all`. */
  reach?: "all" | "done" | "similar";
  status: "all" | "unseen" | "attempted" | "solved" | "due";
  /** Pattern code to 1 (include) or -1 (exclude). */
  topics: Record<string, 1 | -1>;
  /** How many easy, medium and hard problems. */
  mix: [number, number, number];
  anyDiff: boolean;
  anyCount: number;
  /** Company group names and company names; a problem needs one of them. */
  groups: string[];
  companies: string[];
  recent: boolean;
  format: "total" | "per" | "up";
  /** Round length in minutes (`total`). */
  total: number;
  /** Suggested minutes for an easy, medium and hard problem (`per` and the pace shown for the others). */
  per: [number, number, number];
  strict: boolean;
  blind: boolean;
  auto: boolean;
  warns: boolean;
  favour: "random" | "weak" | "asked";
}

export interface MockPreset {
  name: string;
  config: MockConfig;
}

export interface MockSaved {
  last?: MockConfig;
  presets?: MockPreset[];
}

export interface MockItem {
  id: string;
  /** None when the problem was never logged. */
  grade: Grade | null;
  seconds: number;
}

export interface MockRound {
  id: number;
  finished_at: string;
  config: MockConfig;
  items: MockItem[];
  seconds: number;
}

export interface MockData {
  today: string;
  patterns: { code: string; name: string }[];
  company_groups: { name: string; companies: string[] }[];
  /** Every DSA problem, the practice ones too. */
  problems: DsaProblem[];
  saved: MockSaved | null;
  rounds: MockRound[];
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

export type StreakState = "perfect" | "kept" | "frozen" | "holiday" | "sick" | "missed" | "open" | "free";
export type StreakMarkKind = "freeze" | "holiday" | "sick";
export type StreakDay = { date: string; state: StreakState; planned_new: number; planned_reviews: number; done_new: number; done_reviews: number; earned: number; mark_id: number | null };
/** `before` and `after` are the streak days either side of a missed day: repairing it makes `before + 1 + after`. */
export type StreakRepair = { date: string; before: number; after: number };
export type Streaks = { today: string; streak: number; perfect: number; longest: number; freezes: number; earned: number; spent: number; days: StreakDay[]; repairs: StreakRepair[] };

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`/api${path}`, {
      method,
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (e) {
    // no answer at all (the server is down or the network dropped): the connection bar says so at once, not after the retries
    markDown();
    throw e;
  }
  markUp();
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: "unknown", message: res.statusText }));
    // Signed out (or the session expired): go to the login page and come back afterwards.
    if (res.status === 401 && err.error === "unauthorized" && location.pathname !== "/login") {
      location.assign(`/login?next=${encodeURIComponent(location.pathname + location.search)}`);
    }
    throw new ApiError(res.status, err.error ?? "unknown", err.message ?? res.statusText);
  }
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}


// ---- courses (crates/api/src/course.rs) ----

export type StageState = "todo" | "solved" | "assisted";
export type StageKind = "learn" | "build" | "boss" | "challenge";
export type StageDifficulty = "very-easy" | "easy" | "medium" | "hard";

export interface CourseStageRow {
  id: string;
  title: string;
  kind: StageKind;
  difficulty: StageDifficulty;
  rank: number;
  state: StageState;
  runs?: number;
  /** Not part of BusTub: a challenge, or a stage the course adds. */
  beyond?: boolean;
}

export interface CourseModuleRow {
  code: string;
  /** Not (re)written yet: the stages are the old ones and will change. */
  planned?: boolean;
  /** Not on the main path: the next stage is never picked from it. */
  optional?: boolean;
  /** Not part of BusTub: the module ports none of BusTub's own files. */
  beyond?: boolean;
  title: string;
  summary: string;
  stages: CourseStageRow[];
}

export interface CourseOverview {
  id: string;
  title: string;
  total: number;
  done: number;
  /** Extra-practice stages (kind challenge): their own tally, not part of total and done. */
  challenges?: number;
  challenges_done?: number;
  /** The first stage not passed yet. */
  current: string | null;
  /** The newest run the CLI reported for this course, from any laptop. */
  last_run?: { stage_id: string; ok: boolean; passed: number; total: number; at: string } | null;
  projects: { number: number; title: string; planned: boolean;
  /** Not on the main path: the next stage is never picked from it. */
  optional?: boolean; modules: CourseModuleRow[] }[];
}

export interface CourseSection {
  id: string;
  title: string;
  md: string;
}

export interface CourseLecture {
  id: string;
  title: string;
  term: string;
  slides: string;
  notes: string | null;
  video: string | null;
}

export interface CourseRunTest {
  name: string;
  ok: boolean;
  detail: string;
}

export interface CourseRun {
  id: number;
  ok: boolean;
  passed: number;
  total: number;
  tests: CourseRunTest[];
  problem: string | null;
  commit_sha: string | null;
  duration_ms: number;
  at: string;
}

export interface SolutionFile {
  path: string;
  /** Diff lines: a leading ' ', '+' or '-'. */
  lines: string[];
}

export interface CourseConceptRef {
  id: string;
  title: string;
  summary: string;
  minutes: number;
  /** Needed for the stage; the others are further reading. */
  required: boolean;
  read: boolean;
}

export interface CourseConceptPage {
  course: { id: string; title: string };
  concept: { id: string; title: string; summary: string; minutes: number; sections: CourseSection[] };
  used_in: { id: string; title: string; rank: number; module: string }[];
}

export interface CourseStagePage {
  course: { id: string; title: string; total: number };
  stage: { id: string; title: string; learn: string[]; kind: StageKind; difficulty: StageDifficulty; tests: string[]; rank: number; intro: string; sections: CourseSection[]; test_sources?: Record<string, string> };
  module: {
    code: string;
    title: string;
    summary: string;
    project: number;
    stages: CourseStageRow[];
    lectures: CourseLecture[];
    bustub: string[];
    resources: { kind: string; title: string; url: string }[];
  };
  concepts: CourseConceptRef[];
  prev: { id: string; title: string; rank: number } | null;
  next: { id: string; title: string; rank: number } | null;
  state: StageState;
  hints: { total: number; revealed: { title: string; md: string }[]; titles: string[] };
  solution: { available: boolean; open: boolean; files: SolutionFile[] | null };
  /** When the CLI started a run it has not reported yet (the page shows "Running…"); null when nothing is running. */
  running?: string | null;
  last_run: CourseRun | null;
  /** The latest runs of this stage, newest first (at most ten). */
  runs: CourseRun[];
}

export const api = {
  tracks: () => request<TrackSummary[]>("GET", "/tracks"),
  activity: (sections: readonly Section[]) => request<Activity>("GET", `/activity?sections=${sections.join(",")}`),
  track: (slug: string) => request<TrackDetail>("GET", `/tracks/${slug}`),
  resetTrack: (slug: string) => request<{ problems: number; attempts_removed: number; drafts_removed: number }>("POST", `/tracks/${slug}/reset`, {}),
  problem: (id: string) => request<ProblemDetail>("GET", `/problems/${id}`),
  saveDraft: (id: string, code: string) => request<void>("PUT", `/problems/${id}/draft`, { code }),
  reset: (id: string) => request<ProblemDetail>("DELETE", `/problems/${id}/draft`),
  run: (id: string, code: string) => request<RunOutcome>("POST", `/problems/${id}/run`, { code }),
  saveScratch: (id: string, code: string) => request<void>("PUT", `/problems/${id}/scratch`, { code }),
  runScratch: (id: string, lib: string, main: string) => request<ScratchResult>("POST", `/problems/${id}/scratch/run`, { lib, main }),
  submit: (id: string, code: string) => request<RunOutcome>("POST", `/problems/${id}/submit`, { code }),
  revealHint: (id: string) => request<ProblemDetail>("POST", `/problems/${id}/hints`),
  revealSolution: (id: string) => request<ProblemDetail>("POST", `/problems/${id}/solution`),
  session: () => request<{ required: boolean; authenticated: boolean }>("GET", "/auth/session"),
  login: (passphrase: string) => request<void>("POST", "/auth/login", { passphrase }),
  logout: () => request<void>("POST", "/auth/logout"),
  settings: () => request<Settings>("GET", "/settings"),
  saveEditor: (editor: EditorSettings) => request<EditorSettings>("PUT", "/settings/editor", editor),
  format: (code: string) => request<{ code: string }>("POST", "/format", { code }).then((r) => r.code),
  saveAppearance: (appearance: AppearanceSettings) => request<AppearanceSettings>("PUT", "/settings/appearance", appearance),
  progress: () => request<ProgressOverview>("GET", "/progress"),
  streaks: () => request<Streaks>("GET", "/streaks"),
  markDay: (day: string, kind: StreakMarkKind) => request<Streaks>("POST", "/streaks/marks", { day, kind }),
  unmarkDay: (id: number) => request<Streaks>("DELETE", `/streaks/marks/${id}`),
  stats: () => request<ProgressStats>("GET", "/stats"),
  reviews: () => request<ReviewsView>("GET", "/reviews"),
  resolve: (id: string) => request<ProblemDetail>("POST", `/problems/${id}/resolve`),
  dsa: () => request<DsaOverview>("GET", "/dsa"),
  preview: (id: string) => request<{ previews: Record<Grade, ReviewPreview> | null; history: TryMark[] }>("GET", `/dsa/problems/${id}/preview`),
  statement: (id: string) => request<DsaStatement>("GET", `/dsa/problems/${id}/statement`),
  plan: (from: string, to: string) => request<PlanView>("GET", `/plan?from=${from}&to=${to}`),
  planPreview: (body: { rules?: PlanRules; overrides?: Record<string, DayKind | null> }) => request<PlanPreview>("POST", "/plan/preview", body),
  savePlanState: (body: { rules: PlanRules; overrides: Record<string, DayKind> }) => request<{ saved: boolean }>("PUT", "/plan/state", body),
  createPlan: (name: string) => request<{ id: string }>("POST", "/plans", { name }),
  setActivePlan: (id: string) => request<{ active: string }>("PUT", "/plans/active", { id }),
  deletePlan: (id: string) => request<{ deleted: string }>("DELETE", `/plans/${id}`),
  solutions: (id: string) => request<MySolution[]>("GET", `/dsa/problems/${id}/solutions`),
  addSolution: (id: string, body: { label: string; code: string; notes: string }) => request<MySolution>("POST", `/dsa/problems/${id}/solutions`, body),
  saveSolution: (sid: number, body: { label: string; code: string; notes: string }) => request<MySolution>("PUT", `/dsa/solutions/${sid}`, body),
  deleteSolution: (sid: number) => request<{ deleted: number }>("DELETE", `/dsa/solutions/${sid}`),
  dsaPage: (id: string) => request<DsaPage | null>("GET", `/dsa/problems/${id}/page`),
  /** One problem as a row, in a list or not (the problem page of a practice problem). */
  dsaRow: (id: string) => request<DsaProblem>("GET", `/dsa/problems/${id}/row`),
  patternLessons: (pattern: string) => request<PatternLessons>("GET", `/dsa/patterns/${pattern}`),
  practice: (pattern: string) => request<PracticeList>("GET", `/dsa/practice/${pattern}`),
  resetDsa: (id: string) => request<{ id: string; archived: boolean }>("DELETE", `/dsa/problems/${id}/log`),
  restoreDsa: (id: string) => request<{ id: string; restored: boolean }>("POST", `/dsa/problems/${id}/restore`),
  weekSummary: () => request<{ from: string; to: string; solved: number; unassisted: number; reviews: number; active_days: number; attempted: number; weakest: { name: string; misses: number } | null }>("GET", "/week-summary"),
  forecast: (id: string) => request<{ points: number[] | null; due_in?: number; retention?: number }>("GET", `/dsa/problems/${id}/forecast`),
  feed: () => request<{ path: string | null }>("GET", "/feed"),
  makeFeed: () => request<{ path: string }>("POST", "/feed"),
  companyPlan: (body: { company: string; interview?: string | null; recent_only?: boolean }) => request<{ id: string; created: boolean }>("POST", "/plans/company", body),
  setWhy: (id: string, why: string) => request<{ id: string; why: string }>("POST", `/dsa/problems/${id}/why`, { why }),
  day: (date: string) => request<{ date: string; solves: { problem_id: string; title: string; slug: string | null; kind: string; assisted: boolean; at: string }[] }>("GET", `/day/${date}`),
  undoDsa: (id: string) => request<{ id: string; undone: boolean }>("POST", `/dsa/problems/${id}/undo`),
  logDsa: (id: string, grade: Grade, practice = false, hints = 0) => request<{ id: string; due: string | null; ideal_days: number | null; undoable: boolean }>("POST", `/dsa/problems/${id}/log`, { grade, practice, hints }),
  review: () => request<ReviewQueue>("GET", "/dsa/review"),
  mock: () => request<MockData>("GET", "/dsa/mock"),
  saveMock: (saved: MockSaved) => request<MockSaved>("PUT", "/dsa/mock/config", saved),
  finishMock: (round: { config: MockConfig; items: MockItem[]; seconds: number }) => request<{ id: number }>("POST", "/dsa/mock/rounds", round),
  startDsa: (from: string) => request<{ start: string }>("POST", "/dsa/start", { from }),
  prefs: () => request<Record<string, unknown>>("GET", "/prefs"),
  savePref: (key: string, value: unknown) => request<{ saved: string }>("POST", "/prefs", { key, value }),
  importBackup: (file: unknown) => request<{ added: Record<string, number> }>("POST", "/import", file),
  saveSrs: (srs: SrsSettings) => request<SrsSettings>("PUT", "/settings/srs", srs),
  focus: (id: string, seconds: number) => request<void>("POST", `/problems/${id}/focus`, { seconds }),
  course: (id: string) => request<CourseOverview>("GET", `/courses/${id}`),
  courseStage: (course: string, id: string) => request<CourseStagePage>("GET", `/courses/${course}/stages/${id}`),
  resetCourse: (course: string, scope: { all: true } | { project: number } | { module: string }) =>
    request<{ stages: number; stage_states_removed: number; runs_removed: number }>("POST", `/courses/${course}/reset`, scope),
  setConceptRead: (course: string, id: string, read: boolean) => request<{ read: boolean }>("PUT", `/courses/${course}/concepts/${id}/read`, { read }),
  courseConcept: (course: string, id: string) => request<CourseConceptPage>("GET", `/courses/${course}/concepts/${id}`),
  revealCourseHint: (course: string, id: string) => request<CourseStagePage>("POST", `/courses/${course}/stages/${id}/hints`),
  revealCourseSolution: (course: string, id: string) => request<CourseStagePage>("POST", `/courses/${course}/stages/${id}/solution`),
};

/** The stages that count towards a module's or the course's progress: challenges are extra practice with a tally of their own. */
export const coreStages = <T extends { kind: string }>(stages: T[]): T[] => stages.filter((x) => x.kind !== "challenge");
