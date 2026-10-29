import { Link, useNavigate } from "@tanstack/react-router";
import { useQueries, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent } from "react";
import { api, type Activity, type Band, type DsaOverview, type DsaProblem, type TrackSummary } from "../api";
import { Companies, Insight, Mark, ProblemTags, useLogger } from "../components/dsaBits";
import { Header } from "../components/Header";
import { MockRoot, SlidingSeg, rise, useReady } from "../components/mock";
import { CountUp } from "../components/kit";
import { PracticeRow } from "./PracticePage";
import { pad2, pctColor } from "../components/bits";
import { BLURB, DIFF, REVIEW_GRADES, LISTS, MINUTES, STATUS_LABEL, daysUntil, hours, inList, leetcode, niceDate, statusOf, videoUrl, type ListKey, type Status } from "../dsa";

type Role = "must_learn" | "practice";
type View = "patterns" | "problems" | "practice";
type RailTab = "activity" | "filters";

interface Filters {
  list: ListKey;
  rail: RailTab;
  status: Set<Status>;
  role: Set<Role>;
  diff: Set<Band>;
  patterns: Set<string>;
  groups: Set<string>;
  companies: Set<string>;
  recent: boolean;
  tags: Set<string>;
  hidePremium: boolean;
  q: string;
}

type SetKey = "status" | "role" | "diff" | "patterns" | "groups" | "companies" | "tags";

const fresh = (list: ListKey = "neetcode150"): Filters => ({
  list,
  rail: "activity",
  status: new Set(),
  role: new Set(),
  diff: new Set(),
  patterns: new Set(),
  groups: new Set(),
  companies: new Set(),
  recent: false,
  tags: new Set(),
  hidePremium: false,
  q: "",
});

function useStored<T extends string>(key: string, initial: T): [T, (v: T) => void] {
  const [v, set] = useState<T>(() => {
    try {
      return (localStorage.getItem(key) as T | null) ?? initial;
    } catch {
      return initial;
    }
  });
  return [
    v,
    (next) => {
      set(next);
      try {
        localStorage.setItem(key, next);
      } catch {
        // Storage can be unavailable; the choice still applies for this visit.
      }
    },
  ];
}

const activeCount = (f: Filters) =>
  f.status.size + f.role.size + f.diff.size + f.patterns.size + f.groups.size + f.companies.size + (f.recent ? 1 : 0) + f.tags.size + (f.hidePremium ? 1 : 0) + (f.q ? 1 : 0);

export function DsaPage() {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const activity = useQuery({ queryKey: ["activity", "dsa"], queryFn: () => api.activity(["D"]) });
  const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
  const o = overview.data;
  const { log, toast } = useLogger(o?.today);
  const [f, setF] = useState<Filters>(() => fresh());
  // `view` is the saved choice, changed only by the toggle. Shortcuts that open the list (a pattern's problems, the due
  // reviews) borrow it for this visit through `drill`, so they never overwrite what was chosen.
  const [view, setView] = useStored<View>("anneal-dsa-view", "patterns");
  const [drill, setDrill] = useState<View | null>(null);
  const shown = drill ?? view;
  const [openGroups, setOpenGroups] = useState<Set<string>>(new Set());
  const search = useRef<HTMLInputElement>(null);
  const update = (patch: Partial<Filters>) => setF((cur) => ({ ...cur, ...patch }));
  const flip = <T,>(key: SetKey, v: T) =>
    setF((cur) => {
      const next = new Set(cur[key] as Set<T>);
      if (next.has(v)) next.delete(v);
      else next.add(v);
      return { ...cur, [key]: next };
    });
  const clearAll = () => setF((cur) => ({ ...fresh(cur.list), rail: cur.rail }));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, [contenteditable]");
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "/") {
        e.preventDefault();
        search.current?.focus();
      }
      if (e.key === "f") setF((cur) => ({ ...cur, rail: cur.rail === "filters" ? "activity" : "filters" }));
      if (e.key === "1" || e.key === "2" || e.key === "3") {
        setView((["patterns", "problems", "practice"] as const)[Number(e.key) - 1]!);
        setDrill(null);
      }
      if (e.key === "Escape") setF((cur) => (cur.rail === "filters" ? { ...cur, rail: "activity" } : cur));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const model = useMemo(() => (o ? build(o, f) : null), [o, f]);

  if (!o || !model) {
    return (
      <>
        <Header area="dsa" />
        <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
          <div className="wrap">{overview.isError ? <p className="notice bad">Couldn't reach the API. Is `cargo run -p anneal-api` running?</p> : <p className="rempty">Loading…</p>}</div>
        </main>
      </>
    );
  }

  return (
    <>
      <Header area="dsa" />
      <MockRoot prefix="s-" className="s-dsa" style={{ "--s-ac": "var(--acc)", "--s-acb": "var(--acc-bg)", "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
        <main className="s-wrap">
          <section className="s-hero">
            <div>
              <div className="s-eye s-rv" style={rise(0).style}>
                <b>DSA</b> / NEETCODE
              </div>
              <h1 className="s-rv" style={rise(1).style}>
                Every NeetCode problem, <em>tracked.</em>
              </h1>
              <p className="s-lead s-rv" style={rise(2).style}>
                Solve on LeetCode, log it here. Anneal keeps the schedule: reviews, streak and readiness. Learn each pattern once, then practise it.
              </p>
            </div>
            <div className="s-stats s-rv" style={rise(3).style}>
              <div>
                <b>
                  <CountUp value={o.patterns.length} />
                </b>
                <span>PATTERNS</span>
              </div>
              <div>
                <b>
                  <CountUp value={o.problems.length} />
                </b>
                <span>PROBLEMS</span>
              </div>
              <div>
                <b>
                  <CountUp value={model.minutesLeft150} format={hours} />
                </b>
                <span>TO FINISH THE 150</span>
              </div>
            </div>
          </section>

          <Goals o={o} f={f} setList={(list) => update({ list })} />

          <div className="s-main">
            <div className="cat-main">
              <NextUp o={o} streak={activity.data?.streak ?? 0} />
              <div className="s-tools s-rv" style={rise(5).style}>
                <label className="s-search">
                  <span>⌕</span>
                  <input ref={search} type="search" placeholder="search title, number or tag…" value={f.q} onChange={(e) => update({ q: e.target.value })} aria-label="Search problems" autoComplete="off" spellCheck={false} />
                  <kbd>/</kbd>
                </label>
                <SlidingSeg
                  label="View"
                  value={shown}
                  onChange={(v) => {
                    setView(v);
                    setDrill(null);
                  }}
                  options={(["patterns", "problems", "practice"] as const).map((v) => ({
                    key: v,
                    label: (
                      <>
                        {v === "patterns" ? "▦ patterns" : v === "problems" ? "≡ problems" : "✎ practice"}
                        <small className="vn">{v === "patterns" ? o.patterns.length : v === "problems" ? model.items.length : o.patterns.reduce((n, p) => n + p.practice_total, 0)}</small>
                      </>
                    ),
                  }))}
                />
                <button className={`d-fbtn${f.rail === "filters" ? " on" : ""}`} aria-pressed={f.rail === "filters"} onClick={() => update({ rail: f.rail === "filters" ? "activity" : "filters" })}>
                  ⚙ Filters {model.active > 0 && <i>{model.active}</i>}
                  <kbd>f</kbd>
                </button>
              </div>
              {shown !== "practice" && (
                <>
                <div className="s-flab s-rv" style={rise(6).style}>LIST</div>
                <div className="s-chips s-rv" style={rise(6).style}>
                  {LISTS.map(([k, label]) => (
                    <button key={k} className={`s-chip${f.list === k ? " s-on" : ""}`} aria-pressed={f.list === k} onClick={() => update({ list: k })}>
                      {label} <small>{model.listCounts[k]}</small>
                    </button>
                  ))}
                </div>
                <ActiveChips f={f} o={o} flip={flip} update={update} clearAll={clearAll} />
                </>
              )}
              <div id="content" style={{ display: "flex", flexDirection: "column", gap: 28 }}>
                {shown === "patterns" ? (
                  <PatternGrid o={o} f={f} model={model} />
                ) : shown === "practice" ? (
                  <PracticeGrid o={o} log={log} />
                ) : (
                  <ProblemGroups o={o} model={model} pickCompany={(name) => flip("companies", name)} picked={f.companies} uncapped={f.patterns.size > 0} openGroups={openGroups} openGroup={(code) => setOpenGroups(new Set([...openGroups, code]))} log={log} clearAll={clearAll} />
                )}
              </div>
            </div>
            {f.rail === "filters" && <button className="d-backdrop" aria-label="Close filters" tabIndex={-1} onClick={() => update({ rail: "activity" })} />}
            <aside className={`cat-rail s-rail${f.rail === "filters" ? " drawer" : ""}`} aria-label="Activity and filters">
              <div className="d-drawer-head">
                <b>Filters</b>
                <button onClick={() => update({ rail: "activity" })}>Show {model.items.length} problems</button>
              </div>
              <div className="d-railtabs" role="tablist">
                <button className={f.rail === "activity" ? "on" : ""} role="tab" aria-selected={f.rail === "activity"} onClick={() => update({ rail: "activity" })}>
                  Activity
                </button>
                <button className={f.rail === "filters" ? "on" : ""} role="tab" aria-selected={f.rail === "filters"} onClick={() => update({ rail: "filters" })}>
                  Filters {model.active > 0 && <i>{model.active}</i>}
                </button>
              </div>
              {f.rail === "activity" ? (
                <ActivityRail
                  o={o}
                  activity={activity.data}
                  tracks={tracks.data ?? []}
                  showDue={() => {
                    setDrill("problems");
                    update({ status: new Set<Status>(["due"]), rail: "filters" });
                  }}
                />
              ) : (
                <FilterRail o={o} f={f} model={model} flip={flip} update={update} clearAll={clearAll} />
              )}
            </aside>
          </div>
        </main>
      </MockRoot>
      {toast}
    </>
  );
}

// ---------------------------------------------------------------- the model: what passes the filters

interface Model {
  tests: Record<string, (p: DsaProblem) => boolean>;
  passing: (except: string) => DsaProblem[];
  items: DsaProblem[];
  active: number;
  listCounts: Record<ListKey, number>;
  minutesLeft150: number;
  byId: Map<string, DsaProblem>;
  patternName: Map<string, string>;
}

function build(o: DsaOverview, f: Filters): Model {
  const today = o.today;
  const needle = f.q.trim().toLowerCase();
  const tests: Model["tests"] = {
    list: (p) => inList(p, f.list),
    status: (p) => !f.status.size || f.status.has(statusOf(p, today)),
    role: (p) => !f.role.size || f.role.has(p.role),
    diff: (p) => !f.diff.size || f.diff.has(p.difficulty),
    patterns: (p) => !f.patterns.size || f.patterns.has(p.pattern),
    company: (p) => (!f.groups.size && !f.companies.size && !f.recent) || p.companies.some((c) => (f.companies.size ? f.companies.has(c.name) : f.groups.size ? f.groups.has(c.group) : true) && (!f.recent || c.recent)),
    tags: (p) => !f.tags.size || [...f.tags].every((t) => p.tags.includes(t)),
    premium: (p) => !f.hidePremium || !p.premium,
    q: (p) => !needle || `${p.title} ${p.number} ${p.tags.join(" ")}`.toLowerCase().includes(needle),
  };
  const passing = (except: string) => o.problems.filter((p) => Object.entries(tests).every(([k, t]) => k === except || t(p)));
  const unsolved150 = o.problems.filter((p) => inList(p, "neetcode150") && !p.state.solved);
  const listCounts = Object.fromEntries(LISTS.map(([k]) => [k, o.problems.filter((p) => inList(p, k) && Object.entries(tests).every(([t, test]) => t === "list" || test(p))).length])) as Record<ListKey, number>;
  return {
    tests,
    passing,
    items: passing(""),
    active: activeCount(f),
    listCounts,
    minutesLeft150: unsolved150.reduce((n, p) => n + MINUTES[p.difficulty], 0),
    byId: new Map(o.problems.map((p) => [p.id, p])),
    patternName: new Map(o.patterns.map((p) => [p.code, p.name])),
  };
}

// ---------------------------------------------------------------- goals, next up

function Goals({ o, f, setList }: { o: DsaOverview; f: Filters; setList: (l: ListKey) => void }) {
  const defs: [ListKey, string, string][] = [
    ["neetcode150", "GOAL · NEETCODE 150", "var(--acc)"],
    ["new250", "NEW IDEAS IN THE 250", "var(--vio)"],
    ["all", "NEETCODE ALL", "var(--grn)"],
  ];
  return (
    <section className="d-goals" aria-label="Goals">
      {defs.map(([k, label, color]) => {
        const items = o.problems.filter((p) => inList(p, k));
        const done = items.filter((p) => p.state.solved).length;
        const left = items.filter((p) => p.role === "must_learn" && !p.state.solved).length;
        const note =
          k === "all" ? (
            <>
              <b>{o.problems.filter((p) => p.premium).length}</b> need LeetCode Premium
            </>
          ) : k === "new250" ? (
            <>must-learn problems the 150 doesn't cover</>
          ) : (
            <>
              <b>{left}</b> must-learn left
            </>
          );
        return (
          <button key={k} className={`d-goal${f.list === k ? " on" : ""}`} style={{ "--gc": color } as CSSProperties} aria-pressed={f.list === k} onClick={() => setList(k)}>
            <span>{label}</span>
            <b>
              {done}
              <small>/ {items.length}</small>
            </b>
            <div className="bar">
              <i style={{ width: `${items.length ? (done / items.length) * 100 : 0}%`, background: color }} />
            </div>
            <em>{note}</em>
          </button>
        );
      })}
    </section>
  );
}

function paceLine(o: DsaOverview): string {
  const { pace, goal_done, goal_total } = o.plan;
  if (pace.remaining === 0) return "the goal is done";
  const perDay = pace.per_solve_day != null ? `${pace.per_solve_day} a solve day` : "no solve days set";
  let end = "";
  if (pace.finish_at_current && pace.days_vs_target != null) {
    const d = Math.abs(pace.days_vs_target);
    end = ` · finishing ${niceDate(pace.finish_at_current, o.today)} (${d === 0 ? "right on the target" : `${d} day${d === 1 ? "" : "s"} ${pace.days_vs_target < 0 ? "early" : "late"}`})`;
  }
  return `${goal_done} of ${goal_total} · needs ${perDay}${end}`;
}

function NextUp({ o, streak }: { o: DsaOverview; streak: number }) {
  const ready = useReady();
  const navigate = useNavigate();
  const byId = new Map(o.problems.map((p) => [p.id, p]));
  const next = o.plan.next_up[0] ? byId.get(o.plan.next_up[0]) : undefined;
  const after = o.plan.start ? byId.get(o.plan.start) : undefined;
  const patternName = (code: string) => o.patterns.find((p) => p.code === code)?.name ?? "";
  const { goal_done, goal_total } = o.plan;
  const pct = goal_total ? (100 * goal_done) / goal_total : 0;
  const gain = goal_total ? 100 / goal_total : 0;
  const technique = next ? o.techniques.find((t) => t.id === next.technique) : undefined;
  const reviews = o.plan.review_ids.length;
  if (!next) {
    return (
      <article className="s-card s-next s-rv" style={rise(4).style} aria-label="Next up">
        <h3>Everything on these lists is done.</h3>
        <p>Reviews keep it fresh. {o.plan.due} due now.</p>
      </article>
    );
  }
  // The whole card opens the problem page; the links and buttons inside keep their own action.
  const open = (e: MouseEvent<HTMLElement>) => {
    if (!(e.target as HTMLElement).closest("a, button")) void navigate({ to: "/d/$slug", params: { slug: next.slug } });
  };
  return (
    <article className="s-card s-next s-rv s-click" style={rise(4).style} aria-label="Next up" onClick={open}>
      <div className="s-nrow">
        <div className="s-ring">
          <svg viewBox="0 0 64 64" aria-hidden="true">
            <circle className="s-t" cx="32" cy="32" r="28" />
            <circle className="s-v" cx="32" cy="32" r="28" style={{ strokeDashoffset: ready ? 176 * (1 - Math.min(1, streak / 7)) : 176 }} />
          </svg>
          <span>{streak}</span>
        </div>
        <div>
          <div className="s-kick">
            <span className="s-pulse" />
            NEXT UP · {patternName(next.pattern).toUpperCase()}
            {technique ? ` · ${technique.name.toUpperCase()}` : ""}
          </div>
          <div style={{ marginTop: 4, color: "var(--mut)", fontSize: 14 }}>
            {streak > 0 ? (
              <>
                <b style={{ color: "var(--fg)" }}>{streak}-day streak</b>, keep it going.
              </>
            ) : (
              <>
                <b style={{ color: "var(--fg)" }}>No streak yet.</b> Log one today to start it.
              </>
            )}{" "}
            {after ? <>It follows {after.title} in order. </> : <>It's the first one on the list. </>}
            {o.plan.solve_day ? "Today is a solve day" : "Today is a practice day"}
            {reviews ? (
              <>
                , with <b style={{ color: "var(--fg)" }}>{reviews}</b> review{reviews === 1 ? "" : "s"} to do.
              </>
            ) : (
              <>, no reviews due.</>
            )}
          </div>
        </div>
      </div>
      <h3>{next.title}</h3>
      <p>{technique ? `The idea: ${technique.name.toLowerCase()}.` : BLURB[patternName(next.pattern)]}</p>
      <div className="s-pills">
        <span className={`s-pill${next.difficulty === "easy" ? " s-g" : ""}`} style={next.difficulty === "easy" ? undefined : { color: DIFF[next.difficulty][1] }}>
          {next.difficulty}
        </span>
        <span className="s-pill" style={{ color: next.role === "must_learn" ? "var(--s-ac)" : undefined }}>
          {next.role === "must_learn" ? "must learn" : "practice"}
        </span>
        <span className="s-pill">{patternName(next.pattern)}</span>
        <span className="s-pill">~{MINUTES[next.difficulty]}m</span>
        {next.state.last_grade === "again" && (
          <span className="s-pill" style={{ color: "var(--bad)" }}>
            attempted{next.state.due ? `, retry ${niceDate(next.state.due, o.today)}` : ""}
          </span>
        )}
        {next.premium && (
          <span className="s-pill" style={{ color: "var(--warn)" }}>
            premium
          </span>
        )}
      </div>
      <div className="s-ml">
        <span>GOAL · {Math.round(pct)}% DONE</span>
        <b>+{gain.toFixed(1)}% IF UNASSISTED</b>
      </div>
      <div className="s-meter">
        <i style={{ width: ready ? `${pct}%` : 0 }} />
      </div>
      <a className="s-cta" href={leetcode(next.slug)} target="_blank" rel="noreferrer">
        Solve on LeetCode <span className="s-ar">↗</span>
      </a>
      <div className="s-links">
        <Link to="/d/$slug" params={{ slug: next.slug }}>
          the problem page
        </Link>
        {next.video && (
          <a href={videoUrl(next.video)} target="_blank" rel="noreferrer">
            ▶ NeetCode's video
          </a>
        )}
        {reviews > 0 && (
          <Link className="d-plan-link" to="/dsa/review" style={{ color: "var(--vio)" }}>
            Review {reviews} due ›
          </Link>
        )}
        <Link className="d-plan-link" to="/dsa/calendar">
          Plan: {paceLine(o)} ›
        </Link>
      </div>
    </article>
  );
}

function ActiveChips({ f, o, flip, update, clearAll }: { f: Filters; o: DsaOverview; flip: <T>(k: SetKey, v: T) => void; update: (p: Partial<Filters>) => void; clearAll: () => void }) {
  const chips: [label: string, off: () => void][] = [];
  f.status.forEach((v) => chips.push([STATUS_LABEL[v], () => flip("status", v)]));
  f.role.forEach((v) => chips.push([v === "must_learn" ? "Must learn" : "Practice", () => flip("role", v)]));
  f.diff.forEach((v) => chips.push([DIFF[v][0], () => flip("diff", v)]));
  f.patterns.forEach((v) => chips.push([o.patterns.find((p) => p.code === v)?.name ?? v, () => flip("patterns", v)]));
  f.groups.forEach((v) => chips.push([v, () => flip("groups", v)]));
  f.companies.forEach((v) => chips.push([v, () => flip("companies", v)]));
  if (f.recent) chips.push(["Asked in the last 6 months", () => update({ recent: false })]);
  f.tags.forEach((v) => chips.push([`#${v}`, () => flip("tags", v)]));
  if (f.hidePremium) chips.push(["No Premium", () => update({ hidePremium: false })]);
  if (f.q) chips.push([`“${f.q}”`, () => update({ q: "" })]);
  if (!chips.length) return null;
  return (
    <div className="d-active">
      {chips.map(([label, off]) => (
        <span className="d-chip" key={label}>
          {label}
          <button aria-label={`Remove ${label}`} onClick={off}>
            ×
          </button>
        </span>
      ))}
      <button className="d-clear" onClick={clearAll}>
        clear all
      </button>
    </div>
  );
}

// ---------------------------------------------------------------- pattern cards and problem cards

/** Practice as a grid of its own: for each pattern, the next LeetCode problems that drill its techniques. */
function PracticeGrid({ o, log }: { o: DsaOverview; log: ReturnType<typeof useLogger>["log"] }) {
  const withPractice = o.patterns.filter((p) => p.practice_total > 0);
  const lists = useQueries({ queries: withPractice.map((p) => ({ queryKey: ["practice", p.code], queryFn: () => api.practice(p.code), staleTime: 60_000 })) });
  return (
    <>
      <div className="s-flab">PRACTICE · THE NEXT PROBLEMS FOR EACH PATTERN</div>
      <p className="pg-hint">Not in a NeetCode list: no goal, no reviews.</p>
      {withPractice.map((pat, i) => {
        const techniques = lists[i]?.data?.techniques ?? [];
        const next = techniques.flatMap((t) => t.problems.filter((p) => !p.state.solved).slice(0, 2).map((p) => ({ p, from: t.must_learn }))).slice(0, 3);
        return (
          <section key={pat.code} className="pg-group">
            <h4>
              <span>{pat.name.toUpperCase()} · {pat.practice_solved}/{pat.practice_total}</span>
              <Link to="/dsa/practice/$code" params={{ code: pat.code }}>open the pattern ›</Link>
            </h4>
            {lists[i]?.isLoading && <p className="rempty">Loading…</p>}
            {next.map(({ p, from }) => (
              <div key={p.id} className="pg-row">
                <PracticeRow p={p} today={o.today} log={log} />
                <small>practises <Link to="/d/$slug" params={{ slug: from.slug }}>{from.title}</Link></small>
              </div>
            ))}
            {lists[i]?.isSuccess && next.length === 0 && <p className="rempty">All solved here.</p>}
          </section>
        );
      })}
    </>
  );
}

function PatternGrid({ o, f, model }: { o: DsaOverview; f: Filters; model: Model }) {
  const navigate = useNavigate();
  const listName = LISTS.find(([k]) => k === f.list)?.[1] ?? "";
  const filtered = model.active > 0;
  return (
    <>
      <div className="s-flab">{filtered ? `PATTERNS · ${model.items.length} PROBLEMS MATCH` : "ALL PATTERNS · IN RECOMMENDED ORDER"}</div>
      <div className="s-grid" onPointerMove={(e) => {
        const card = (e.target as HTMLElement).closest<HTMLElement>(".s-trk");
        if (!card) return;
        const r = card.getBoundingClientRect();
        card.style.setProperty("--mx", `${e.clientX - r.left}px`);
        card.style.setProperty("--my", `${e.clientY - r.top}px`);
      }}>
        {o.patterns.map((pat, i) => {
          const scope = o.problems.filter((p) => p.pattern === pat.code && model.tests.list?.(p));
          if (!scope.length) return null;
          const match = scope.filter((p) => model.items.includes(p));
          const done = scope.filter((p) => p.state.solved).length;
          const left = scope.filter((p) => !p.state.solved).reduce((n, p) => n + MINUTES[p.difficulty], 0);
          const next = scope.filter((p) => !p.state.solved).sort((a, b) => (a.role === "must_learn" ? 0 : 1) - (b.role === "must_learn" ? 0 : 1) || a.order - b.order)[0];
          const techs = o.techniques.filter((t) => t.pattern === pat.name).slice(0, 4);
          const isNow = o.plan.next_up[0] ? model.byId.get(o.plan.next_up[0])?.pattern === pat.code : false;
          const state = done === scope.length ? "done" : isNow ? "cur" : "";
          const filled = Math.round((done / scope.length) * 5);
          return (
            <div key={pat.code} className={`s-trk s-rv${state === "cur" ? " s-cur" : ""}${filtered && !match.length ? " s-gone" : ""}`} style={rise(Math.min(7 + i, 16)).style} tabIndex={0} role="button" aria-label={`${pat.name}: open the lesson`} onClick={() => void navigate({ to: "/dsa/patterns/$code", params: { code: pat.code } })} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), void navigate({ to: "/dsa/patterns/$code", params: { code: pat.code } }))}>
              <div className="s-th">
                <span className="s-no">{pad2(i + 1)}</span>
                <span className="s-tk">PATTERN · {listName.toUpperCase()}</span>
                {state === "cur" && <span className="s-tag s-now">NEXT</span>}
                {state === "done" && <span className="s-tag" style={{ color: "var(--grn)", background: "var(--grn-bg)", borderColor: "transparent" }}>DONE</span>}
              </div>
              <h4>{pat.name}</h4>
              <p>{BLURB[pat.name] ?? ""}</p>
              <div className="s-meta s-x">
                <b>{scope.length}</b> problems · <b>{scope.filter((p) => p.role === "must_learn").length}</b> must learn · {hours(left)} left
                {filtered && <> · <span className="tc-hit">{match.length} match</span></>}
              </div>
              <div className="s-segs" title={`${done} of ${scope.length} solved`}>
                {Array.from({ length: 5 }, (_, k) => (
                  <i key={k} className={k < filled ? "s-f" : ""} style={{ "--k": k } as CSSProperties} />
                ))}
              </div>
              <div className="s-foot">
                <span>{next ? `next · ${next.title}` : "all solved · reviews keep it fresh"}</span>
              </div>
              {techs.length > 0 && <div className="s-pills" style={{ margin: 0 }}>{techs.map((t) => <span className="s-pill" key={t.id}>{t.name}</span>)}</div>}
              <div className="tc-doors" onClick={(e) => e.stopPropagation()}>
                <Link to="/dsa/patterns/$code" params={{ code: pat.code }} title="When to use each technique, with a template">
                  <b>Learn<i>›</i></b>
                  <small>{techs.length ? `${o.techniques.filter((t) => t.pattern === pat.name).length} techniques` : "lesson"}</small>
                </Link>
                <Link to="/dsa/patterns/$code/problems" params={{ code: pat.code }} title="This pattern's NeetCode problems">
                  <b>Problems<i>›</i></b>
                  <small>{done} / {scope.length} solved</small>
                </Link>
                {pat.practice_total > 0 ? (
                  <Link to="/dsa/practice/$code" params={{ code: pat.code }} title="More LeetCode problems for this pattern">
                    <b>Practice<i>›</i></b>
                    <small>{pat.practice_solved} / {pat.practice_total}</small>
                  </Link>
                ) : (
                  <span className="off"><b>Practice</b><small>none yet</small></span>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </>
  );
}

function ProblemGroups({ o, model, pickCompany, picked, uncapped, openGroups, openGroup, log, clearAll }: { o: DsaOverview; pickCompany: (name: string) => void; picked: ReadonlySet<string>; uncapped: boolean; model: Model; openGroups: Set<string>; openGroup: (code: string) => void; log: ReturnType<typeof useLogger>["log"]; clearAll: () => void }) {
  const { items } = model;
  if (!items.length)
    return (
      <div className="cat-empty">
        No problems match.{" "}
        <button style={{ color: "var(--ca)" }} onClick={clearAll}>
          Clear filters
        </button>
      </div>
    );
  return (
    <>
      {o.patterns.map((pat) => {
        const inPat = items.filter((p) => p.pattern === pat.code);
        if (!inPat.length) return null;
        const roots = [...inPat].sort((a, b) => a.order - b.order);
        const cap = openGroups.has(pat.code) || uncapped ? roots.length : 15;
        return (
          <div className="d-group" key={pat.code}>
            <div className="s-flab d-flabel">
              {pat.name.toUpperCase()} · {inPat.length}
            </div>
            <div className="d-plist" style={{ marginTop: 14 }}>
              {roots.slice(0, cap).map((p) => (
                <ProblemCard key={p.id} p={p} o={o} model={model} log={log} pickCompany={pickCompany} picked={picked} />
              ))}
              {roots.length > cap && (
                <button className="d-more" onClick={() => openGroup(pat.code)}>
                  show {roots.length - cap} more
                </button>
              )}
            </div>
          </div>
        );
      })}
    </>
  );
}

function ProblemCard({ p, o, model, log, pickCompany, picked }: { p: DsaProblem; o: DsaOverview; model: Model; log: ReturnType<typeof useLogger>["log"]; pickCompany: (name: string) => void; picked: ReadonlySet<string> }) {
  const status = statusOf(p, o.today);
  const teacher = p.practice_of ? model.byId.get(p.practice_of) : undefined;
  const when =
    status === "due" ? (
      <span className="d-when due">review due {p.state.due && p.state.due < o.today ? "now" : "today"}</span>
    ) : p.state.last_grade === "again" ? (
      <span className="d-when">retry {p.state.due ? niceDate(p.state.due, o.today) : "soon"}</span>
    ) : p.state.due ? (
      <span className="d-when">review {niceDate(p.state.due, o.today).replace(/^(\w)/, (c) => c.toLowerCase())}{daysUntil(p.state.due, o.today) > 1 ? ` · in ${daysUntil(p.state.due, o.today)}d` : ""}</span>
    ) : null;
  return (
    <article className={`d-pcard${status === "due" ? " due" : ""}`}>
      <Mark p={p} today={o.today} />
      <div className="d-pbody">
        <div className="d-ph">
          <span className="d-pnum">#{p.number}</span>
          <Link className="d-ptitle" to="/d/$slug" params={{ slug: p.slug }}>
            {p.title}
          </Link>
          <a className="d-lc" href={leetcode(p.slug)} target="_blank" rel="noreferrer">
            LeetCode ↗
          </a>
        </div>
        <Insight text={p.insight} />
        <div className="d-pmeta">
          <ProblemTags p={p} />
          <span>{p.tags.slice(0, 3).join(" · ")}</span>
          {teacher && (
            <span className="d-of">
              · practice of{" "}
              <Link to="/d/$slug" params={{ slug: teacher.slug }} style={{ color: "var(--mut)" }}>
                {teacher.title}
              </Link>
            </span>
          )}
        </div>
        <Companies companies={p.companies} onPick={pickCompany} picked={picked} />
      </div>
      <div className="d-pside">
        {when}
        <div className="d-acts">
          {p.video && (
            <a href={videoUrl(p.video)} target="_blank" rel="noreferrer" title="NeetCode's video">
              ▶
            </a>
          )}
          {REVIEW_GRADES.map((g) => (
            <button key={g.grade} className={g.cls} title={g.label} onClick={() => log(p, g.grade)}>
              {g.glyph}
            </button>
          ))}
        </div>
      </div>
    </article>
  );
}

// ---------------------------------------------------------------- the rail

const OUTCOME_COLOR = { solved: "var(--grn)", assisted: "var(--acc)", failing: "var(--bad)", started: "var(--dim)" } as const;

function ActivityRail({ o, activity, tracks, showDue }: { o: DsaOverview; activity?: Activity; tracks: TrackSummary[]; showDue: () => void }) {
  const ready = useReady();
  const progress = useQuery({ queryKey: ["progress"], queryFn: api.progress });
  const rec = progress.data?.record;
  const shade = (n: number) => (n === 0 ? "var(--raise)" : `color-mix(in oklab, var(--grn) ${Math.min(100, 25 + n * 20)}%, var(--raise))`);
  const byId = new Map(o.problems.map((p) => [p.id, p]));
  const due = o.plan.review_ids.map((id) => byId.get(id)).filter((p): p is DsaProblem => !!p);
  const dsa = tracks.filter((t) => t.section === "D" && t.total > 0);
  const meters = [...dsa].sort((a, b) => b.readiness - a.readiness).filter((t) => t.readiness > 0).slice(0, 6);
  const shownMeters = meters.length ? meters : dsa.slice(0, 6);
  return (
    <div className="s-side">
      <div className="s-card s-rv" style={rise(5).style}>
        <h5>THIS WEEK</h5>
        <div className="s-week">
          {(activity?.week ?? []).map((d) => (
            <i key={d.date} style={d.solved ? { background: shade(d.solved) } : undefined} title={`${d.date}: ${d.solved} solved`} />
          ))}
        </div>
        <div style={{ marginTop: 12, display: "flex", justifyContent: "space-between", font: "500 12px var(--mono)", color: "var(--dim)" }}>
          <span>unassisted</span>
          <b style={{ color: "var(--fg)" }}>
            {activity?.week_unassisted ?? 0} of {activity?.week_solved ?? 0}
          </b>
        </div>
      </div>
      <div className="s-card s-rv" style={rise(6).style}>
        <h5>
          PRACTICE
          <Link to="/progress" style={{ color: "var(--dim)", font: "inherit", float: "right" }}>
            solve record ›
          </Link>
        </h5>
        <div className="s-prac">
          <div>
            <b>{rec?.practice_count ?? 0}</b>
            <span>practice solves</span>
          </div>
          <div>
            <b>{rec?.practice_week ?? 0}</b>
            <span>this week</span>
          </div>
          <div>
            <b>{rec?.problems_practised ?? 0}</b>
            <span>problems</span>
          </div>
        </div>
        <p className="s-none" style={{ marginTop: 10 }}>
          {rec ? `${rec.practice.own} on my own · ${rec.practice.help} with help. Counts toward the streak, schedules nothing.` : "Repeat a logged problem from its page."}
        </p>
      </div>
      <div className="s-card s-rv" style={rise(6).style}>
        <h5>RECENT</h5>
        {activity?.recent.length ? (
          <div className="s-list">
            {activity.recent.map((r) => (
              <Link key={r.problem_id + r.at} className="s-item" to="/d/$slug" params={{ slug: r.problem_id.replace(/^lc-/, "") }}>
                <i style={{ background: OUTCOME_COLOR[r.outcome] }} />
                <span>{r.title}</span>
                <small>
                  {r.track} · {r.detail}
                </small>
              </Link>
            ))}
          </div>
        ) : (
          <p className="s-none">Nothing yet. What you log shows up here.</p>
        )}
      </div>
      <div className="s-card s-rv" style={rise(6).style}>
        <h5>
          REVIEWS TODAY
          <button style={{ color: "var(--vio)", font: "inherit", float: "right" }} onClick={showDue}>
            {o.plan.due} due →
          </button>
        </h5>
        {due.length ? (
          <div className="s-list">
            {due.slice(0, 5).map((p) => (
              <Link key={p.id} className="s-item" to="/d/$slug" params={{ slug: p.slug }}>
                <i style={{ background: "var(--vio)" }} />
                <span>{p.title}</span>
                <small>{p.state.due && p.state.due < o.today ? `${-daysUntil(p.state.due, o.today)}d late` : "today"}</small>
              </Link>
            ))}
          </div>
        ) : (
          <p className="s-none">{o.plan.capacity === 0 ? "A rest day." : "Nothing due. Nice."}</p>
        )}
        {o.plan.overdue > 0 && <p className="s-none" style={{ marginTop: 8 }}>{o.plan.overdue} overdue, in line for the next days with room.</p>}
        <div className="s-links" style={{ marginTop: 10 }}>
          {due.length > 0 && (
            <Link to="/dsa/review" style={{ color: "var(--vio)" }}>
              start the review session ›
            </Link>
          )}
          <Link to="/dsa/calendar">open the calendar ›</Link>
        </div>
      </div>
      <div className="s-card s-rv" style={rise(7).style}>
        <h5>READINESS</h5>
        <div className="s-rd">
          {shownMeters.map((t) => (
            <div key={t.code}>
              <div className="s-l">
                <span>{t.name}</span>
                <b style={{ color: t.readiness > 0 ? pctColor(t.readiness) : "var(--dim)" }}>{Math.round(t.readiness)}%</b>
              </div>
              <div className="s-meter" style={{ margin: 0 }}>
                <i style={{ width: ready ? `${t.readiness}%` : 0, background: pctColor(t.readiness) }} />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Opt({ label, n, on, onClick, sw, hint }: { label: string; n: number; on: boolean; onClick: () => void; sw?: string; hint?: string }) {
  return (
    <button className={`d-opt${on ? " on" : ""}${n === 0 && !on ? " zero" : ""}`} onClick={onClick} aria-pressed={on}>
      {sw ? <span className="sw" style={{ background: sw }} /> : <span className="bx" />}
      <span className="d">
        {label}
        {hint && <small>{hint}</small>}
      </span>
      <span className="c">{n}</span>
    </button>
  );
}

function Tog({ label, hint, on, onClick }: { label: string; hint?: string; on: boolean; onClick: () => void }) {
  return (
    <div className="d-tog">
      <span>
        {label}
        {hint && <small>{hint}</small>}
      </span>
      <button className={`d-sw${on ? " on" : ""}`} aria-pressed={on} aria-label={label} onClick={onClick} />
    </div>
  );
}

function count<T>(items: DsaProblem[], key: (p: DsaProblem) => T | T[]): Map<T, number> {
  const m = new Map<T, number>();
  for (const p of items) for (const v of ([] as T[]).concat(key(p))) m.set(v, (m.get(v) ?? 0) + 1);
  return m;
}

function FilterRail({ o, f, model, flip, update, clearAll }: { o: DsaOverview; f: Filters; model: Model; flip: <T>(k: SetKey, v: T) => void; update: (p: Partial<Filters>) => void; clearAll: () => void }) {
  const [moreTags, setMoreTags] = useState(false);
  const [morePatterns, setMorePatterns] = useState(false);
  const [tagQ, setTagQ] = useState("");
  const shown = model.items.length;
  const inListN = o.problems.filter((p) => inList(p, f.list)).length;
  const listName = LISTS.find(([k]) => k === f.list)?.[1] ?? "";
  const sc = count(model.passing("status"), (p) => statusOf(p, o.today));
  const rc = count(model.passing("role"), (p) => p.role);
  const dc = count(model.passing("diff"), (p) => p.difficulty);
  const pc = count(model.passing("patterns"), (p) => p.pattern);
  const pats = o.patterns.filter((p) => pc.get(p.code) || f.patterns.has(p.code));
  const base = model.passing("company");
  const tc = count(model.passing("tags"), (p) => p.tags);
  const allTags = [...tc.entries()].sort((a, b) => b[1] - a[1]).map(([t]) => t);
  const tagList = tagQ ? allTags.filter((t) => t.toLowerCase().includes(tagQ.toLowerCase())) : [...new Set([...f.tags, ...(moreTags ? allTags : allTags.slice(0, 10))])];
  const n = model.active;
  return (
    <div className="d-rail-in">
      <div className="rbox d-sum">
        <div className="big">
          <b>{shown}</b>
          <span>
            of {inListN} in {listName}
          </span>
        </div>
        <div className="bar">
          <i style={{ width: `${inListN ? (shown / inListN) * 100 : 0}%` }} />
        </div>
        {n ? <button onClick={clearAll}>reset all filters</button> : <span style={{ font: "500 12px var(--mono)", color: "var(--dim)" }}>no filters on</span>}
      </div>
      <div className="rbox">
        <h4>
          <span>STATUS</span>
        </h4>
        <div className="d-opts">
          {([["new", "var(--line)"], ["solved", "var(--grn)"], ["due", "var(--vio)"], ["retry", "var(--bad)"]] as [Status, string][]).map(([k, c]) => (
            <Opt key={k} label={STATUS_LABEL[k]} n={sc.get(k) ?? 0} on={f.status.has(k)} sw={c} onClick={() => flip("status", k)} />
          ))}
        </div>
      </div>
      <div className="rbox">
        <h4>
          <span>IDEA</span>
        </h4>
        <div className="d-opts">
          <Opt label="Must learn" hint="teaches a new idea" n={rc.get("must_learn") ?? 0} on={f.role.has("must_learn")} onClick={() => flip("role", "must_learn")} />
          <Opt label="Practice" hint="reuses an idea you learned" n={rc.get("practice") ?? 0} on={f.role.has("practice")} onClick={() => flip("role", "practice")} />
        </div>
      </div>
      <div className="rbox">
        <h4>
          <span>DIFFICULTY</span>
        </h4>
        <div className="d-opts">
          {(["easy", "medium", "hard"] as const).map((k) => (
            <Opt key={k} label={DIFF[k][0]} n={dc.get(k) ?? 0} on={f.diff.has(k)} sw={DIFF[k][1]} onClick={() => flip("diff", k)} />
          ))}
        </div>
      </div>
      <div className="rbox">
        <h4>
          <span>PATTERN</span>
          <small>{pats.length}</small>
        </h4>
        <div className="d-opts">
          {(morePatterns ? pats : pats.slice(0, 7)).map((p) => (
            <Opt key={p.code} label={p.name} n={pc.get(p.code) ?? 0} on={f.patterns.has(p.code)} onClick={() => flip("patterns", p.code)} />
          ))}
        </div>
        {pats.length > 7 && (
          <button className="d-linkmore" onClick={() => setMorePatterns(!morePatterns)}>
            {morePatterns ? "show fewer" : `+${pats.length - 7} more`}
          </button>
        )}
      </div>
      <div className="rbox">
        <h4>
          <span>COMPANIES</span>
          <small>pick a group, then narrow</small>
        </h4>
        <div className="d-opts">
          {o.company_groups.map((g) => {
            const inG = base.filter((p) => p.companies.some((c) => c.group === g.name));
            const open = f.groups.has(g.name);
            const names = count(inG, (p) => p.companies.filter((c) => c.group === g.name).map((c) => c.name));
            return (
              <div key={g.name} style={{ display: "contents" }}>
                <Opt label={g.name} n={inG.length} on={open} onClick={() => flip("groups", g.name)} />
                {open && (
                  <div className="d-sub">
                    {[...names.entries()]
                      .sort((a, b) => b[1] - a[1])
                      .slice(0, 9)
                      .map(([name, k]) => (
                        <Opt key={name} label={name} n={k} on={f.companies.has(name)} onClick={() => flip("companies", name)} />
                      ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <Tog label="Asked in the last 6 months" hint="for the company filters above" on={f.recent} onClick={() => update({ recent: !f.recent })} />
      </div>
      <div className="rbox">
        <h4>
          <span>LEETCODE TAGS</span>
          <small>all of</small>
        </h4>
        <input className="d-tsearch" placeholder="find a tag…" value={tagQ} onChange={(e) => setTagQ(e.target.value)} />
        <div className="d-opts">
          {tagList.map((t) => (
            <Opt key={t} label={t} n={tc.get(t) ?? 0} on={f.tags.has(t)} onClick={() => flip("tags", t)} />
          ))}
        </div>
        {!tagQ && allTags.length > 10 && (
          <button className="d-linkmore" onClick={() => setMoreTags(!moreTags)}>
            {moreTags ? "show fewer" : `+${allTags.length - 10} more`}
          </button>
        )}
      </div>
      <div className="rbox">
        <h4>
          <span>OPTIONS</span>
        </h4>
        <Tog label="Hide LeetCode Premium" hint={`${o.problems.filter((p) => p.premium).length} problems need it`} on={f.hidePremium} onClick={() => update({ hidePremium: !f.hidePremium })} />
      </div>
    </div>
  );
}
