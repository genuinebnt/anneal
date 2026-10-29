import { useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import { api, type Counted, type ProgressOverview, type ProgressStats, type ReviewsView } from "../api";
import { Header } from "../components/Header";
import { LEVEL_COLOR, pctColor } from "../components/bits";
import { SECTION_NAMES } from "../curriculum";

type Tab = "overview" | "rust" | "reviews";
const TABS: [Tab, string][] = [
  ["overview", "Overview"],
  ["rust", "Rust stats"],
  ["reviews", "Reviews"],
];

const AREA_COLOR = { dsa: "var(--acc)", rust: "var(--vio)" } as const;
const AREA_NAME = { dsa: "DSA", rust: "Rust" } as const;
const SECTION_COLOR: Record<string, string> = { D: "var(--acc)", L: "var(--vio)", S: "var(--vio)", C: "var(--vio)", Y: "var(--vio)" };

const pct = (v: number | null) => (v === null ? "—" : `${Math.round(v)}%`);
const dur = (s: number) => (s < 60 ? `${s}s` : s < 3600 ? `${Math.round(s / 60)}m` : `${Math.floor(s / 3600)}h ${Math.round((s % 3600) / 60)}m`);
const day = (iso: string, opts: Intl.DateTimeFormatOptions) => new Date(`${iso}T00:00`).toLocaleDateString(undefined, opts);

function Kpis({ items }: { items: [string, ReactNode, ReactNode, string?][] }) {
  return (
    <section className="kpis">
      {items.map(([k, v, sub, color]) => (
        <div key={k}>
          <span>{k}</span>
          <b style={color ? { color } : undefined}>{v}</b>
          <small>{sub}</small>
        </div>
      ))}
    </section>
  );
}

function Box({ title, sub, children, chart }: { title: string; sub?: ReactNode; children: ReactNode; chart?: boolean }) {
  return (
    <div className={`pbox${chart ? " chart" : ""}`}>
      <h3>{title}</h3>
      {sub && <p className="sub">{sub}</p>}
      {children}
    </div>
  );
}

// ---------------------------------------------------------------- overview

const RAMP = [0, 30, 50, 72, 100];
const shade = (n: number) => (n === 0 ? "var(--line2)" : `color-mix(in oklab, var(--acc) ${RAMP[n <= 1 ? 1 : n <= 3 ? 2 : n <= 6 ? 3 : 4]}%, var(--panel))`);

function Heatmap({ start, counts }: { start: string; counts: number[] }) {
  const cell = 12, gap = 3, top = 18, left = 28;
  const weeks = Math.ceil(counts.length / 7);
  const w = left + weeks * (cell + gap), h = top + 7 * (cell + gap);
  const first = new Date(`${start}T00:00`);
  const dateOf = (i: number) => new Date(first.getFullYear(), first.getMonth(), first.getDate() + i);
  const months: ReactNode[] = [];
  let lastMonth = -1;
  let lastWeek = -9;
  for (let wk = 0; wk < weeks - 2; wk++) {
    const d = dateOf(wk * 7);
    if (d.getMonth() !== lastMonth) {
      lastMonth = d.getMonth();
      // a label this close to the one before it would print over it
      if (wk - lastWeek < 3) continue;
      lastWeek = wk;
      months.push(
        <text key={`m${wk}`} x={left + wk * (cell + gap)} y="11">
          {d.toLocaleDateString(undefined, { month: "short" })}
        </text>,
      );
    }
  }
  return (
    <div className="heat">
      <svg viewBox={`0 0 ${w} ${h}`} role="img" aria-label="Problems solved per day over the last year">
        {months}
        {["Mon", "Wed", "Fri"].map((t, k) => (
          <text key={t} x="0" y={top + (k * 2) * (cell + gap) + 10}>
            {t}
          </text>
        ))}
        {counts.map((n, i) => (
          <rect key={i} className="d" x={left + Math.floor(i / 7) * (cell + gap)} y={top + (i % 7) * (cell + gap)} width={cell} height={cell} rx="2" fill={shade(n)}>
            <title>{`${dateOf(i).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}: ${n} solved`}</title>
          </rect>
        ))}
      </svg>
    </div>
  );
}

function WeeklyBars({ weeks }: { weeks: ProgressOverview["weekly"] }) {
  const W = 560, H = 190, l = 30, b = 24, t = 10, bw = 26;
  const totals = weeks.map((w) => w.easy + w.medium + w.hard);
  const max = Math.max(4, Math.ceil(Math.max(...totals) / 4) * 4);
  const step = (W - l) / weeks.length;
  const y = (v: number) => t + (H - t - b) * (1 - v / max);
  const ticks = [0, max / 4, max / 2, (3 * max) / 4, max];
  return (
    <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Problems solved per week, by difficulty">
      {ticks.map((v) => (
        <g key={v}>
          <line className="gl" x1={l} x2={W} y1={y(v)} y2={y(v)} />
          <text x="0" y={y(v) + 4}>
            {v}
          </text>
        </g>
      ))}
      {weeks.map((wk, i) => {
        const x = l + i * step + (step - bw) / 2;
        let acc = 0;
        return (
          <g key={wk.week_start}>
            {(["easy", "medium", "hard"] as const).map((lvl) => {
              const v = wk[lvl];
              if (!v) return null;
              const y0 = y(acc + v), hgt = y(acc) - y(acc + v) - 2;
              acc += v;
              return (
                <rect key={lvl} x={x} y={y0} width={bw} height={Math.max(0, hgt)} rx="2" fill={LEVEL_COLOR[lvl]}>
                  <title>{`Week of ${day(wk.week_start, { day: "numeric", month: "short" })}: ${v} ${lvl}`}</title>
                </rect>
              );
            })}
            <text x={x + bw / 2} y={H - 6} textAnchor="middle">
              {i === weeks.length - 1 ? "now" : i % 3 === 0 ? day(wk.week_start, { day: "numeric", month: "short" }) : ""}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

function ReadinessLines({ trend }: { trend: ProgressOverview["readiness_trend"] }) {
  const W = 440, H = 190, l = 30, r = 86, b = 24, t = 10;
  const n = trend[0]?.points.length ?? 12;
  const top = Math.max(20, Math.ceil(Math.max(...trend.flatMap((a) => a.points.map((p) => p ?? 0))) / 20) * 20);
  const x = (i: number) => l + ((W - l - r) * i) / Math.max(1, n - 1);
  const y = (v: number) => t + (H - t - b) * (1 - v / top);
  const ticks = Array.from({ length: top / 20 + 1 }, (_, i) => i * 20);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Readiness by area over 12 weeks">
      {ticks.map((v) => (
        <g key={v}>
          <line className="gl" x1={l} x2={W - r} y1={y(v)} y2={y(v)} />
          <text x="0" y={y(v) + 4}>
            {v}%
          </text>
        </g>
      ))}
      {trend
        .filter((a) => a.points.some((p) => p !== null))
        .map((a) => {
          const pts = a.points.map((p, i) => (p === null ? null : ([x(i), y(p)] as const))).filter((p) => p !== null);
          const last = a.points[a.points.length - 1] ?? 0;
          const color = AREA_COLOR[a.area];
          return (
            <g key={a.area}>
              <polyline fill="none" stroke={color} strokeWidth="2" strokeLinejoin="round" points={pts.map(([px, py]) => `${px},${py}`).join(" ")} />
              <circle cx={x(n - 1)} cy={y(last)} r="4" fill={color} stroke="var(--panel)" strokeWidth="2" />
              <text className="lbl" x={x(n - 1) + 9} y={y(last) + 4} style={{ fill: color }}>
                {AREA_NAME[a.area]} {Math.round(last)}%
              </text>
            </g>
          );
        })}
      <text x={l} y={H - 6}>
        12 weeks ago
      </text>
      <text x={W - r} y={H - 6} textAnchor="end">
        now
      </text>
    </svg>
  );
}

function Overview({ o }: { o: ProgressOverview }) {
  const delta = o.unassisted_30d !== null && o.unassisted_prev_30d !== null ? Math.round(o.unassisted_30d - o.unassisted_prev_30d) : null;
  const lastWeek = o.weekly[o.weekly.length - 1];
  return (
    <>
      <Kpis
        items={[
          ["CURRENT STREAK", `${o.streak} ${o.streak === 1 ? "day" : "days"}`, `longest: ${o.longest_streak} ${o.longest_streak === 1 ? "day" : "days"}`, "var(--acc)"],
          ["SOLVED THIS YEAR", o.solved_year, `on ${o.active_days_year} ${o.active_days_year === 1 ? "day" : "days"}`],
          ["UNASSISTED", pct(o.unassisted_30d), delta === null ? "last 30 days" : `${delta >= 0 ? "+" : ""}${delta} points vs the 30 days before`, "var(--grn)"],
          ["FOCUS TIME", dur(o.focus_week_seconds), `this week · ${dur(o.focus_quarter_seconds)} in 90 days`],
        ]}
      />
      <div className="pbox" style={{ marginTop: 20 }}>
        <h3>Every day you solved something</h3>
        <p className="sub">Darker means more problems solved that day. Hover a day for the count.</p>
        <Heatmap start={o.heat_start} counts={o.heat} />
        <div className="hlegend">
          less
          {[0, 1, 3, 6, 9].map((n) => (
            <i key={n} style={{ background: shade(n) }} />
          ))}
          more
        </div>
      </div>
      <div className="pgrid">
        <Box title="Solved per week" sub={lastWeek ? `Last 12 weeks, by difficulty. This week: ${lastWeek.easy + lastWeek.medium + lastWeek.hard} so far.` : undefined} chart>
          <WeeklyBars weeks={o.weekly} />
          <div className="legend2">
            {(["easy", "medium", "hard"] as const).map((l) => (
              <span key={l}>
                <i style={{ background: LEVEL_COLOR[l] }} />
                {l}
              </span>
            ))}
          </div>
        </Box>
        <Box title="Readiness over time" sub="Weighted by difficulty; assisted solves count half." chart>
          <ReadinessLines trend={o.readiness_trend} />
        </Box>
      </div>
      <div className="pgrid even">
        <Box title="This week" sub={`${day(o.this_week[0]!.date, { weekday: "short", day: "numeric", month: "short" })} – ${day(o.this_week[6]!.date, { weekday: "short", day: "numeric", month: "short" })}`}>
          <table className="ptable">
            <thead>
              <tr>
                <th>DAY</th>
                <th className="num">SOLVED</th>
                <th className="num">UNASSISTED</th>
                <th className="num">FOCUS</th>
              </tr>
            </thead>
            <tbody>
              {o.this_week.map((d) => (
                <tr key={d.date}>
                  <td>{day(d.date, { weekday: "short" })}</td>
                  <td className="num">{d.solved}</td>
                  <td className="num">{d.unassisted}</td>
                  <td className="num">{d.focus_seconds ? dur(d.focus_seconds) : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Box>
        <Box title="DSA solve record" sub="How problems were logged. First tries, later tries and practice are counted apart.">
          <table className="ptable">
            <thead>
              <tr>
                <th />
                <th className="num">ON MY OWN</th>
                <th className="num">WITH HELP</th>
                <th className="num">NOT YET</th>
              </tr>
            </thead>
            <tbody>
              {(
                [
                  ["First try", "a problem's first log", o.record.first],
                  ["Later tries", "scheduled reviews", o.record.later],
                  ["Practice", "extra repeats, +1 each", o.record.practice],
                ] as const
              ).map(([name, note, r]) => (
                <tr key={name}>
                  <td>
                    {name}
                    <small className="rec-note">{note}</small>
                  </td>
                  <td className="num rec-ok">{r.own}</td>
                  <td className="num rec-half">{r.help}</td>
                  <td className="num rec-no">{r.not_yet}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="rec-foot">
            <div>
              <span>PRACTICE COUNT</span>
              <b>{o.record.practice_count}</b>
            </div>
            <div>
              <span>PROBLEMS PRACTISED</span>
              <b>{o.record.problems_practised}</b>
            </div>
            <div>
              <span>PRACTICE THIS WEEK</span>
              <b>{o.record.practice_week}</b>
            </div>
          </div>
        </Box>
        <Box title="By section" sub="Solved of written problems, and readiness.">
          <table className="ptable">
            <thead>
              <tr>
                <th>SECTION</th>
                <th className="num">SOLVED</th>
                <th className="num">READY</th>
              </tr>
            </thead>
            <tbody>
              {o.by_section.map((s) => (
                <tr key={s.section}>
                  <td>
                    <span style={{ color: SECTION_COLOR[s.section] }}>■</span> {SECTION_NAMES[s.section]}
                  </td>
                  <td className="num">
                    {s.solved} / {s.written}
                  </td>
                  <td className="num" style={{ color: s.readiness === null ? "var(--dim)" : pctColor(s.readiness) }}>
                    {pct(s.readiness)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Box>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- Rust stats

function Bars({ rows, color, label, nokey }: { rows: Counted[]; color: string; label: (c: Counted) => string; nokey?: boolean }) {
  if (!rows.length) return <p className="pempty">Nothing in the last 30 days.</p>;
  const max = Math.max(...rows.map((r) => r.count));
  return (
    <div className="hbars">
      {rows.map((r) => (
        <div className={`hbar${nokey ? " nokey" : ""}`} key={r.key}>
          {!nokey && <code style={{ color }}>{r.key}</code>}
          <span className="t">
            <small>
              {label(r)}
              {r.where_most && <span style={{ color: "var(--dim)" }}> · {r.where_most}</span>}
            </small>
            <i>
              <b style={{ width: `${Math.round((100 * r.count) / max)}%`, background: color }} />
            </i>
          </span>
          <em>{r.count}</em>
        </div>
      ))}
    </div>
  );
}

function RustStats({ s }: { s: ProgressStats }) {
  const top = s.errors[0];
  return (
    <>
      <Kpis
        items={[
          ["FIRST-RUN PASS", pct(s.first_run_pass), "of problems passed on the first run"],
          ["RUNS PER SOLVE", s.runs_per_solve ?? "—", s.runs_per_solve_hard === null ? "median" : `median · ${s.runs_per_solve_hard} on hard`],
          ["COMPILE ERRORS", pct(s.compile_error_rate), "of runs in 30 days didn't compile", "var(--bad)"],
          ["WITHIN BUDGET", pct(s.within_budget), "unassisted solves inside the interview clock", "var(--grn)"],
        ]}
      />
      <div className="pgrid even">
        <Box title="Compiler errors you hit most" sub="Last 30 days, from every run, with where each one clusters.">
          <Bars rows={s.errors} color="var(--bad)" label={(c) => c.message} />
          {top && (
            <div className="insight">
              <span>→</span>
              <span>
                <b>{top.key} is your most frequent error</b>
                {top.where_most ? `, mostly in ${top.where_most}` : ""}. Re-solving that stage's problems is the fastest fix.
              </span>
            </div>
          )}
        </Box>
        <Box title="Clippy and rule violations" sub="Lints on your code, and fix-this rules that blocked a solve.">
          <Bars rows={s.lints} color="var(--warn)" label={(c) => c.key} nokey />
          <div style={{ height: 18 }} />
          <Bars rows={s.rules} color="var(--vio)" label={(c) => `rule: ${c.key}`} nokey />
        </Box>
      </div>
      <div className="pbox" style={{ marginTop: 20 }}>
        <h3>Time against the interview clock</h3>
        <p className="sub">Median active editing time to an unassisted solve, by difficulty and mode.</p>
        {s.timing.length ? (
          <table className="ptable">
            <thead>
              <tr>
                <th>LEVEL</th>
                <th>MODE</th>
                <th className="num">BUDGET</th>
                <th className="num">MEDIAN</th>
                <th className="num">WITHIN</th>
                <th className="num">SOLVED</th>
              </tr>
            </thead>
            <tbody>
              {s.timing.map((t) => (
                <tr key={`${t.level}-${t.mode}`}>
                  <td>
                    <span className="pill2" style={{ color: LEVEL_COLOR[t.level], border: `1px solid ${LEVEL_COLOR[t.level]}` }}>
                      {t.level.toUpperCase()}
                    </span>
                  </td>
                  <td>{t.mode === "fix" ? "fix this" : t.mode === "write" ? "write it" : "stage"}</td>
                  <td className="num">{t.budget_minutes}m</td>
                  <td className="num">{t.median_minutes === null ? "—" : `${Math.round(t.median_minutes)}m`}</td>
                  <td className="num" style={{ color: t.within_budget === null ? "var(--dim)" : pctColor(t.within_budget) }}>
                    {pct(t.within_budget)}
                  </td>
                  <td className="num">{t.solved}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="pempty">Solve a few problems and this fills in.</p>
        )}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- reviews

function Forecast({ days }: { days: number[] }) {
  const W = 420, H = 150, l = 24, b = 22, t = 8;
  const max = Math.max(4, Math.ceil(Math.max(...days) / 4) * 4);
  const step = (W - l) / days.length, bw = 16;
  const y = (v: number) => t + (H - t - b) * (1 - v / max);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Re-solves due over the next 14 days">
      {[0, max / 2, max].map((v) => (
        <g key={v}>
          <line className="gl" x1={l} x2={W} y1={y(v)} y2={y(v)} />
          <text x="0" y={y(v) + 4}>
            {v}
          </text>
        </g>
      ))}
      {days.map((v, i) => {
        const x = l + i * step + (step - bw) / 2;
        return (
          <g key={i}>
            <rect x={x} y={y(v)} width={bw} height={y(0) - y(v)} rx="2" fill={i === 0 ? "var(--vio)" : "color-mix(in oklab, var(--vio) 45%, var(--panel))"}>
              <title>{`${i === 0 ? "Today (with overdue)" : `In ${i} day${i > 1 ? "s" : ""}`}: ${v} due`}</title>
            </rect>
            <text x={x + bw / 2} y={H - 6} textAnchor="middle">
              {i === 0 ? "today" : i % 7 === 0 ? `+${i}d` : ""}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

const dueLabel = (d: number) => (d > 1 ? `overdue ${d} days` : d === 1 ? "overdue 1 day" : d === 0 ? "today" : d === -1 ? "tomorrow" : `in ${-d} days`);

function Reviews({ r }: { r: ReviewsView }) {
  const navigate = useNavigate();
  const [busy, setBusy] = useState<string | null>(null);
  const start = async (id: string) => {
    setBusy(id);
    try {
      await api.resolve(id);
      await navigate({ to: "/p/$id", params: { id } });
    } finally {
      setBusy(null);
    }
  };
  const due = r.queue.filter((i) => i.days_overdue >= 0);
  return (
    <>
      <Kpis
        items={[
          ["DUE TODAY", r.due_today, r.due_today ? `~${r.minutes_today} minutes at interview pace` : "nothing due", "var(--vio)"],
          ["OVERDUE", r.overdue, r.overdue ? "costing readiness 10% a week" : "all caught up", r.overdue ? "var(--bad)" : undefined],
          ["RETENTION", pct(r.retention_30d), "re-solves passed unassisted, 30 days", "var(--grn)"],
          ["IN ROTATION", r.in_rotation, `${r.graduated} graduated (60-day step passed)`],
        ]}
      />
      <div className="pgrid">
        <Box title="Review queue" sub="Overdue first, then today's. A re-solve starts from the starter with hints locked again.">
          {r.queue.length ? (
            <table className="ptable">
              <thead>
                <tr>
                  <th>PROBLEM</th>
                  <th>STEP</th>
                  <th>DUE</th>
                  <th>LAST TIME</th>
                </tr>
              </thead>
              <tbody>
                {r.queue.map((i) => (
                  <tr key={i.problem_id} className="go" onClick={() => start(i.problem_id)} title="Start this re-solve">
                    <td>
                      {i.title}
                      <br />
                      <span style={{ font: "500 11px var(--mono)", color: "var(--dim)" }}>
                        {i.track} · <span style={{ color: LEVEL_COLOR[i.level] }}>{i.level}</span>
                      </span>
                    </td>
                    <td>
                      <span className="ladder">
                        {[0, 1, 2, 3].map((k) => (
                          <i key={k} className={k <= i.step ? "on" : ""} />
                        ))}
                      </span>
                      <br />
                      <span style={{ font: "500 11px var(--mono)", color: "var(--dim)" }}>{i.step >= 4 ? "graduated" : `${i.interval_days}-day step`}</span>
                    </td>
                    <td style={{ color: i.days_overdue > 0 ? "var(--bad)" : i.days_overdue === 0 ? "var(--acc)" : "var(--dim)" }}>{busy === i.problem_id ? "opening…" : dueLabel(i.days_overdue)}</td>
                    <td style={{ color: i.last_result === "assisted" ? "var(--acc)" : "var(--mut)" }}>{i.last_result}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <p className="pempty">No reviews yet. Every problem you solve comes back here on a schedule.</p>
          )}
          {due.length > 0 && (
            <div className="btns" style={{ marginTop: 18 }}>
              <button className="btn go sm" onClick={() => start(due[0]!.problem_id)} disabled={busy !== null}>
                Start re-solves · {due.length} →
              </button>
            </div>
          )}
        </Box>
        <Box title="Next 14 days" sub="Re-solves falling due each day." chart>
          <Forecast days={r.forecast} />
          <div className="insight" style={{ background: "var(--raise)", color: "var(--mut)" }}>
            <span>ⓘ</span>
            <span>Reviews: 3 → 7 → 21 → 60 days. An assisted solve goes back to 3.</span>
          </div>
        </Box>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- page

export function ProgressPage() {
  const [tab, setTab] = useState<Tab>(() => {
    const h = location.hash.slice(1);
    return TABS.some(([k]) => k === h) ? (h as Tab) : "overview";
  });
  useEffect(() => {
    history.replaceState(null, "", tab === "overview" ? location.pathname : `#${tab}`);
  }, [tab]);
  const overview = useQuery({ queryKey: ["progress"], queryFn: api.progress, enabled: tab === "overview" });
  const stats = useQuery({ queryKey: ["stats"], queryFn: api.stats, enabled: tab === "rust" });
  const reviews = useQuery({ queryKey: ["reviews"], queryFn: api.reviews });
  const failed = overview.isError || stats.isError || reviews.isError;
  const streak = overview.data?.streak;

  return (
    <>
      <Header />
      <main className="page">
        <div className="wrap" style={{ paddingBlock: "48px 16px" }}>
          <div className="eyebrow">
            <span style={{ color: "var(--acc)" }}>PROGRESS</span>
            {streak !== undefined && (
              <>
                <span>/</span>
                <span>{streak}-DAY STREAK</span>
              </>
            )}
            {reviews.data && (
              <>
                <span>/</span>
                <span>{reviews.data.due_today} DUE TODAY</span>
              </>
            )}
          </div>
          <h1 className="h1 md" style={{ marginTop: 16 }}>
            Steady practice, <span style={{ color: "var(--acc)" }}>measured.</span>
          </h1>
          <div className="ptabs" role="tablist">
            {TABS.map(([k, label]) => (
              <button key={k} role="tab" className={tab === k ? "on" : ""} aria-selected={tab === k} onClick={() => setTab(k)}>
                {label}
                {k === "reviews" && reviews.data?.due_today ? ` · ${reviews.data.due_today}` : ""}
              </button>
            ))}
          </div>
          {failed && <p className="notice bad">Couldn't load your progress. Is the API running?</p>}
          {tab === "overview" && overview.data && <Overview o={overview.data} />}
          {tab === "rust" && stats.data && <RustStats s={stats.data} />}
          {tab === "reviews" && reviews.data && <Reviews r={reviews.data} />}
        </div>
      </main>
    </>
  );
}
