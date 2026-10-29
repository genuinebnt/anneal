import { Link } from "@tanstack/react-router";
import { Skeleton } from "../components/Skeleton";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { api, type DsaOverview, type SrsSettings, type Weekday } from "../api";
import { Header } from "../components/Header";
import { WEEKDAYS, goalRemaining, newOn, niceDate, pacePreview } from "../dsa";

type Mode = "solve" | "practice" | "rest";
const NEXT: Record<Mode, Mode> = { solve: "practice", practice: "rest", rest: "solve" };

const modeOf = (s: SrsSettings, d: Weekday): Mode => (s.new_days.includes(d) ? "solve" : s.capacity[d] > 0 ? "practice" : "rest");

/** The plan: a target date, which weekdays are for new problems, practice or rest, and the pace that follows. */
export function DsaPlanPage() {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const o = overview.data;
  return (
    <>
      <Header area="dsa" />
      <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
        <div className="wrap" style={{ paddingBlock: 28 }}>
          <div className="eyebrow">
            <Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link>
            <span>/</span>
            <span>PLAN</span>
          </div>
          <h1 className="h1 md">Your plan</h1>
          {o ? <Editor key={o.today} o={o} /> : overview.isError ? <p className="rempty">Couldn't reach the API.</p> : <Skeleton lines={6} />}
        </div>
      </main>
    </>
  );
}

function Stepper({ value, onChange, min, max, step = 1 }: { value: number; onChange: (n: number) => void; min: number; max: number; step?: number }) {
  return (
    <span className="p-step">
      <button aria-label="less" onClick={() => onChange(Math.max(min, value - step))}>−</button>
      <b>{value}</b>
      <button aria-label="more" onClick={() => onChange(Math.min(max, value + step))}>+</button>
    </span>
  );
}

function Editor({ o }: { o: DsaOverview }) {
  const qc = useQueryClient();
  const [s, setS] = useState<SrsSettings>(o.settings);
  const [goalKind, setGoalKind] = useState<"150" | "plus" | "custom">(o.settings.goal.custom_left != null ? "custom" : o.settings.goal.extra > 0 ? "plus" : "150");
  const dirty = JSON.stringify(s) !== JSON.stringify(o.settings);
  const save = useMutation({
    mutationFn: () => api.saveSrs(s),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["dsa"] }),
  });
  useEffect(() => {
    if (save.isSuccess && !dirty) {
      const t = window.setTimeout(() => save.reset(), 2500);
      return () => window.clearTimeout(t);
    }
  }, [save, dirty]);

  const goal = useMemo(() => goalRemaining(o, s.goal), [o, s.goal]);
  const pace = useMemo(() => pacePreview(goal.remaining, o.today, s), [goal.remaining, o.today, s]);
  const room = WEEKDAYS.reduce((n, d) => n + (modeOf(s, d) === "rest" ? 0 : s.capacity[d]), 0);
  const solveDays = s.new_days.length;
  const newByDay = WEEKDAYS.map((d) => (s.new_days.includes(d) ? newOn(s, d) : 0));
  const newPerWeek = newByDay.reduce((a, b) => a + b, 0);
  const busiest = Math.max(1, ...WEEKDAYS.map((d, i) => (modeOf(s, d) === "rest" ? 0 : s.capacity[d]) + (newByDay[i] ?? 0)));

  const setMode = (d: Weekday, m: Mode) =>
    setS((cur) => {
      const new_days = m === "solve" ? WEEKDAYS.filter((x) => x === d || cur.new_days.includes(x)) : cur.new_days.filter((x) => x !== d);
      const capacity = { ...cur.capacity };
      if (m === "rest") capacity[d] = 0;
      else if (capacity[d] === 0) capacity[d] = m === "solve" ? 1 : 3;
      const consolidate_on = cur.consolidate_on === d && m === "rest" ? (WEEKDAYS.find((x) => x !== d && capacity[x] > 0) ?? null) : cur.consolidate_on;
      return { ...cur, new_days, capacity, consolidate_on };
    });
  const pickGoal = (kind: typeof goalKind) => {
    setGoalKind(kind);
    setS((cur) => ({ ...cur, goal: { ...cur.goal, list: "neetcode150", free_only: true, extra: kind === "plus" ? 5 : 0, custom_left: kind === "custom" ? (cur.goal.custom_left ?? 40) : null } }));
  };

  const status =
    pace.finish == null ? (
      <span className="p-pill none">no solve days chosen</span>
    ) : pace.daysVsTarget == null ? null : pace.daysVsTarget <= 0 ? (
      <span className="p-pill ok">{Math.abs(pace.daysVsTarget)} days before the target</span>
    ) : (
      <span className="p-pill late">{pace.daysVsTarget} days after the target</span>
    );
  const note =
    room < 14
      ? "Under about 14 reviews a week, recall of what you learned starts to slip (86% in the simulation). Add room on a practice day."
      : solveDays === 0
        ? "With no solve days, anneal only schedules reviews."
        : `A week: ${newPerWeek} new, up to ${room} reviews, ${WEEKDAYS.filter((d) => modeOf(s, d) === "rest").length} rest.`;

  return (
    <div className="p-grid">
      <div className="p-col">
        <section className="p-card">
          <h2>TARGET <em>a soft goal, not a deadline</em></h2>
          <div className="p-row">
            <label className="p-fld">
              <span>Finish by</span>
              <input type="date" value={s.target_date ?? ""} onChange={(e) => setS({ ...s, target_date: e.target.value || null })} />
            </label>
            <div className="p-fld">
              <span>Goal</span>
              <div className="seg" role="group" aria-label="Goal">
                <button className={goalKind === "150" ? "on" : ""} onClick={() => pickGoal("150")}>NeetCode 150 · free</button>
                <button className={goalKind === "plus" ? "on" : ""} onClick={() => pickGoal("plus")}>+ from the 250</button>
                <button className={goalKind === "custom" ? "on" : ""} onClick={() => pickGoal("custom")}>Custom</button>
              </div>
            </div>
            {goalKind === "plus" && (
              <div className="p-fld"><span>Extra from the 250</span><Stepper value={s.goal.extra} min={1} max={100} onChange={(extra) => setS({ ...s, goal: { ...s.goal, extra } })} /></div>
            )}
            {goalKind === "custom" && (
              <div className="p-fld"><span>Problems left</span><Stepper value={s.goal.custom_left ?? 40} min={1} max={943} step={5} onChange={(custom_left) => setS({ ...s, goal: { ...s.goal, custom_left } })} /></div>
            )}
          </div>
          <div className="p-row" style={{ alignItems: "center" }}>
            <div className="p-fld">
              <span>Premium problems</span>
              <div className="seg" role="group" aria-label="Premium problems">
                <button className={s.goal.free_only ? "on" : ""} onClick={() => setS({ ...s, goal: { ...s.goal, free_only: true } })}>Left out</button>
                <button className={s.goal.free_only ? "" : "on"} onClick={() => setS({ ...s, goal: { ...s.goal, free_only: false } })}>Included</button>
              </div>
            </div>
          </div>
          <p className="p-hint">{goal.remaining} left of {goal.total}. Solved problems come off on their own. Premium problems need a LeetCode subscription, so the plan and the calendar leave them out unless you include them.</p>
        </section>

        <section className="p-card">
          <h2>WEEK <em>click a day to cycle solve → practice → rest</em></h2>
          <div className="p-week">
            {WEEKDAYS.map((d) => {
              const m = modeOf(s, d);
              return (
                <div key={d} className={`p-day ${m}`}>
                  <span className="n">{d.toUpperCase()}</span>
                  <button className="mode" onClick={() => setMode(d, NEXT[m])}>{m}</button>
                  <div className={`p-cap${m === "rest" ? " off" : ""}`}>
                    <span>practice</span>
                    <Stepper value={m === "rest" ? 0 : s.capacity[d]} min={m === "solve" ? 0 : 1} max={30} onChange={(n) => setS({ ...s, capacity: { ...s.capacity, [d]: n } })} />
                  </div>
                  <div className={`p-cap${m === "solve" ? "" : " off"}`}>
                    <span>solve</span>
                    <Stepper value={m === "solve" ? newOn(s, d) : 0} min={1} max={20} onChange={(n) => setS({ ...s, new_by_day: { ...s.new_by_day, [d]: n } })} />
                  </div>
                </div>
              );
            })}
          </div>
          <div className="p-load" aria-label="Problems per day">
            {WEEKDAYS.map((d, i) => {
              const n = newByDay[i] ?? 0;
              const r = modeOf(s, d) === "rest" ? 0 : s.capacity[d];
              return (
                <div key={d} className="p-bar" title={`${d}: ${n} new, ${r} practice`}>
                  <div className="stack">
                    <i className="rv" style={{ height: `${(r / busiest) * 100}%` }} />
                    <i className="nw" style={{ height: `${(n / busiest) * 100}%` }} />
                  </div>
                  <b>{n + r}</b>
                </div>
              );
            })}
          </div>
          <div className="p-key"><span><i style={{ background: "var(--acc)" }} />solve: new problems, plus practice</span><span><i style={{ background: "var(--vio)" }} />practice: reviews only</span><span><i style={{ background: "var(--line)" }} />rest: nothing</span></div>
          <div className="p-row" style={{ alignItems: "center" }}>
            <div className="p-fld"><span>Usual new problems on a solve day</span><Stepper value={s.new_per_day} min={1} max={5} onChange={(new_per_day) => setS({ ...s, new_per_day })} /></div>
            <label className="p-fld">
              <span>First reviews go to</span>
              <select value={s.consolidate_on ?? ""} onChange={(e) => setS({ ...s, consolidate_on: (e.target.value || null) as Weekday | null })}>
                <option value="">any day with room</option>
                {WEEKDAYS.filter((d) => s.capacity[d] > 0).map((d) => <option key={d} value={d}>{d}</option>)}
              </select>
            </label>
          </div>
          <div className="p-row" style={{ alignItems: "center" }}>
            <div className="p-fld">
              <span>When a day is full</span>
              <div className="seg" role="group" aria-label="Review order">
                <button className={s.prioritise ? "on" : ""} onClick={() => setS({ ...s, prioritise: true })}>Important first</button>
                <button className={s.prioritise ? "" : "on"} onClick={() => setS({ ...s, prioritise: false })}>Most forgotten first</button>
              </div>
            </div>
            <label className="p-fld">
              <span>Remember core problems to</span>
              <select value={s.core_retention ?? ""} onChange={(e) => setS({ ...s, core_retention: e.target.value ? Number(e.target.value) : null })}>
                <option value="">the same as the rest ({Math.round(s.retention * 100)}%)</option>
                {[0.9, 0.92, 0.95].filter((r) => r > s.retention).map((r) => <option key={r} value={r}>{Math.round(r * 100)}%, a few more reviews</option>)}
              </select>
            </label>
          </div>
        </section>
      </div>

      <aside className="p-col">
        <section className="p-card p-res">
          <h2>YOUR PACE</h2>
          <div className="p-big">{pace.perSolveDay ?? "—"}<small>{pace.perSolveDay == null ? "no solve days" : "a solve day"}</small></div>
          {status}
          <dl className="p-kv">
            <div><dt>Solve days left</dt><dd>{pace.solveDaysLeft}</dd></div>
            <div><dt>Needed per week</dt><dd>{pace.perWeek != null ? pace.perWeek.toFixed(1) : "—"}</dd></div>
            <div><dt>Finish at your pace</dt><dd>{pace.finish ? niceDate(pace.finish, o.today) + (pace.finish > o.today ? `, ${pace.finish.slice(0, 4)}` : "") : "never"}</dd></div>
            <div><dt>New problems per week</dt><dd>{newPerWeek}</dd></div>
            <div><dt>Practice room per week</dt><dd>{room}</dd></div>
            <div><dt>Problems in a busy day</dt><dd>{busiest}</dd></div>
          </dl>
          <p className="p-hint">{note}</p>
          <div className="p-save">
            <button className="nu-go" disabled={!dirty || save.isPending} onClick={() => save.mutate()}>{save.isPending ? "Saving…" : "Save plan"}</button>
            {dirty && <button className="d-clear" onClick={() => { setS(o.settings); }}>discard</button>}
            {save.isSuccess && !dirty && <span className="p-hint">Saved.</span>}
            {save.isError && <span className="p-hint" style={{ color: "var(--bad)" }}>{(save.error as Error).message}</span>}
          </div>
        </section>
      </aside>
    </div>
  );
}
