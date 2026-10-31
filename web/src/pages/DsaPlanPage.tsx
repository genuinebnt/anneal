import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { api, type DsaOverview, type SrsSettings, type Weekday } from "../api";
import { Link } from "@tanstack/react-router";
import { Header } from "../components/Header";
import { HowFound, Page3, Tile, PageLoading, LoadFailed } from "../components/layout";
import { PlanSwitcher } from "../components/PlanSwitcher";
import { DateField, Segmented, Select, Stepper } from "../components/ui";
import { WEEKDAYS, goalRemaining, newOn, niceDate, pacePreview } from "../dsa";

type Mode = "solve" | "practice" | "rest";

const modeOf = (s: SrsSettings, d: Weekday): Mode => (s.new_days.includes(d) ? "solve" : s.capacity[d] > 0 ? "practice" : "rest");

/** The plan: a target date, which weekdays are for new problems, practice or rest, and the pace that follows. */
export function DsaPlanPage() {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const o = overview.data;
  return (
    <>
      <Header area="dsa" />
      <div className="p-3" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
        {o ? <Editor key={o.today} o={o} /> : (
          <PageLoading crumb={[<Link to="/dsa">DSA</Link>, "Plan"]} title={TITLE} error={overview.isError ? <LoadFailed what="your plan" error={overview.error} retry={() => void overview.refetch()} /> : undefined} />
        )}
      </div>
    </>
  );
}

const TITLE = <>Your plan, <span style={{ color: "var(--acc)" }}>on pace.</span></>;

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

  const restDays = WEEKDAYS.filter((d) => modeOf(s, d) === "rest").length;
  const practiceDays = 7 - solveDays - restDays;
  const done = goal.total - goal.remaining;

  const modeOptions = [["solve", "Solve"], ["practice", "Practice"], ["rest", "Rest"]] as const;
  const coreOptions: (readonly [string, string])[] = [["", `The same as the rest (${Math.round(s.retention * 100)}%)`], ...[0.9, 0.92, 0.95].filter((r) => r > s.retention).map((r) => [String(r), `${Math.round(r * 100)}%`] as const)];
  const firstOptions: (readonly [string, string])[] = [["", "Any day with room"], ...WEEKDAYS.filter((d) => s.capacity[d] > 0).map((d) => [d, d.charAt(0).toUpperCase() + d.slice(1)] as const)];

  const tiles = (
    <>
      <Tile em="Left to solve" hint="Problems in your goal you have not solved yet." strong={goal.remaining} small={goalKind === "150" ? "NeetCode 150" : goalKind === "plus" ? `NeetCode 150 + ${s.goal.extra}` : "your own number"} />
      <Tile em="New a week" hint="New problems you will start in a week, counting every solve day." strong={newPerWeek} small={`on ${solveDays} solve day${solveDays === 1 ? "" : "s"}`} />
      <Tile em="Finish at this pace" hint="The day the last problem is solved if you keep to the week below." strong={<span className="p-tfin">{pace.finish ? niceDate(pace.finish, o.today) : "never"}</span>} hot={pace.daysVsTarget != null && pace.daysVsTarget > 0} color="var(--warn)" small={pace.daysVsTarget == null ? "set a target date" : pace.daysVsTarget <= 0 ? `${Math.abs(pace.daysVsTarget)} days before the target` : `${pace.daysVsTarget} days after the target`} />
    </>
  );
  const center = (
    <>
        <section className="p-card">
          <h2>TARGET</h2>
          <div className="f-grid">
            <div className="f-fld"><span>Finish by</span><DateField label="Finish by" value={s.target_date ?? ""} onChange={(v) => setS({ ...s, target_date: v || null })} /></div>
            <div className="f-fld">
              <span>Goal</span>
              <Segmented label="Goal" value={goalKind} onChange={pickGoal} options={[["150", "NeetCode 150"], ["plus", "+ from the 250"], ["custom", "Custom"]]} />
            </div>
            {goalKind === "plus" && <div className="f-fld"><span>Extra from the 250</span><Stepper label="extra problems" value={s.goal.extra} min={1} max={100} onChange={(extra) => setS({ ...s, goal: { ...s.goal, extra } })} /></div>}
            {goalKind === "custom" && <div className="f-fld"><span>Problems left</span><Stepper label="problems left" value={s.goal.custom_left ?? 40} min={1} max={943} step={5} onChange={(custom_left) => setS({ ...s, goal: { ...s.goal, custom_left } })} /></div>}
            <div className="f-fld">
              <span>Premium problems</span>
              <Segmented label="Premium problems" value={s.goal.free_only ? "out" : "in"} onChange={(v) => setS({ ...s, goal: { ...s.goal, free_only: v === "out" } })} options={[["out", "Left out"], ["in", "Included"]]} />
            </div>
          </div>
          <div className="f-prog" role="img" aria-label={`${done} of ${goal.total} done`}>
            <div className="bar"><i style={{ width: `${goal.total ? (done / goal.total) * 100 : 0}%` }} /></div>
            <span><b>{done}</b> of {goal.total} done · <b>{goal.remaining}</b> left</span>
          </div>
        </section>

        <section className="p-card">
          <h2>
            WEEK
            <span className="p-tally"><b>{newPerWeek}</b> new · <b>{room}</b> practice · <b>{restDays}</b> rest</span>
          </h2>
          <div className="p-rows">
            <div className="p-r head" aria-hidden="true"><span>Day</span><span>Mode</span><span>Solve</span><span>Practice</span><span>Load</span></div>
            {WEEKDAYS.map((d, i) => {
              const m = modeOf(s, d);
              const n = newByDay[i] ?? 0;
              const r = m === "rest" ? 0 : s.capacity[d];
              return (
                <div key={d} className={`p-r ${m}`}>
                  <b className="dn">{d.toUpperCase()}</b>
                  <Segmented label={`${d} mode`} className="p-mode" value={m} onChange={(v) => setMode(d, v)} options={modeOptions} />
                  <span className="cell" data-l="Solve"><Stepper compact label={`${d} solve`} disabled={m !== "solve"} value={m === "solve" ? newOn(s, d) : 0} min={1} max={20} onChange={(v) => setS({ ...s, new_by_day: { ...s.new_by_day, [d]: v } })} /></span>
                  <span className="cell" data-l="Practice"><Stepper compact label={`${d} practice`} disabled={m === "rest"} value={r} min={m === "solve" ? 0 : 1} max={30} onChange={(v) => setS({ ...s, capacity: { ...s.capacity, [d]: v } })} /></span>
                  <span className="ld" title={`${n} new, ${r} practice`}>
                    <span className="split"><i className="nw" style={{ width: `${(n / busiest) * 100}%` }} /><i className="rv" style={{ width: `${(r / busiest) * 100}%` }} /></span>
                    <b>{n + r}</b>
                  </span>
                </div>
              );
            })}
          </div>
        </section>

        <section className="p-card">
          <h2>RULES</h2>
          <div className="f-grid">
            <div className="f-fld"><span>Usual new problems on a solve day</span><Stepper label="usual new problems" value={s.new_per_day} min={1} max={5} onChange={(new_per_day) => setS({ ...s, new_per_day })} /></div>
            <div className="f-fld"><span>First reviews go to</span><Select label="First reviews go to" value={s.consolidate_on ?? ""} options={firstOptions} onChange={(v) => setS({ ...s, consolidate_on: (v || null) as Weekday | null })} /></div>
            <div className="f-fld">
              <span>When a day is full</span>
              <Segmented label="Review order" value={s.prioritise ? "important" : "forgotten"} onChange={(v) => setS({ ...s, prioritise: v === "important" })} options={[["important", "Important first"], ["forgotten", "Most forgotten first"]]} />
            </div>
            <div className="f-fld"><span>Remember core problems to</span><Select label="Remember core problems to" value={s.core_retention == null ? "" : String(s.core_retention)} options={coreOptions} onChange={(v) => setS({ ...s, core_retention: v ? Number(v) : null })} /></div>
          </div>
        </section>
    </>
  );
  const right = (
    <>
        <section className="p-card p-res">
          <h2>YOUR PACE</h2>
          <div className="p-big">{pace.perSolveDay ?? "—"}<small>{pace.perSolveDay == null ? "no solve days" : "a solve day"}</small></div>
          {pace.finish == null ? <span className="p-pill none">No solve days chosen</span> : pace.daysVsTarget == null ? null : pace.daysVsTarget <= 0 ? <span className="p-pill ok">{Math.abs(pace.daysVsTarget)} days before the target</span> : <span className="p-pill late">{pace.daysVsTarget} days after the target</span>}
          <dl className="p-kv">
            <div><dt>Solve days left</dt><dd>{pace.solveDaysLeft}</dd></div>
            <div><dt>Needed per week</dt><dd>{pace.perWeek != null ? pace.perWeek.toFixed(1) : "—"}</dd></div>
            <div><dt>Finish at your pace</dt><dd>{pace.finish ? niceDate(pace.finish, o.today) + (pace.finish > o.today ? `, ${pace.finish.slice(0, 4)}` : "") : "never"}</dd></div>
            <div><dt>New per week</dt><dd>{newPerWeek}</dd></div>
            <div><dt>Practice per week</dt><dd>{room}</dd></div>
            <div><dt>Busiest day</dt><dd>{busiest}</dd></div>
          </dl>
          {/* the week as columns: each day's new problems and practice, the busiest one standing out */}
          <div className="p-wkc" role="img" aria-label={`${solveDays} solve days, ${practiceDays} practice days, ${restDays} rest days; busiest day ${busiest}`}>
            {WEEKDAYS.map((d, i) => {
              const n = newByDay[i] ?? 0;
              const r = modeOf(s, d) === "rest" ? 0 : s.capacity[d];
              return (
                <span key={d} className={`${modeOf(s, d)}${n + r === busiest ? " top" : ""}`} title={`${d}: ${n} new, ${r} practice`}>
                  <i className="rv" style={{ height: `${(r / busiest) * 100}%` }} />
                  <i className="nw" style={{ height: `${(n / busiest) * 100}%` }} />
                  <em>{d.slice(0, 2)}</em>
                </span>
              );
            })}
          </div>
          {room < 14 && <p className="p-warn" role="status">Under 14 practice problems a week, recall starts to slip.</p>}
          <div className="p-save">
            <button className="nu-go" disabled={!dirty || save.isPending} onClick={() => save.mutate()}>{save.isPending ? "Saving…" : "Save plan"}</button>
            {dirty && <button className="d-clear" onClick={() => { setS(o.settings); }}>discard</button>}
            {save.isSuccess && !dirty && <span className="p-hint">Saved.</span>}
            {save.isError && <span className="p-hint" style={{ color: "var(--bad)" }}>{(save.error as Error).message}</span>}
          </div>
        </section>
        <HowFound
          title="How the pace is worked out"
          text="The problems left, divided by the solve days before your target. Rest days are skipped; practice days add practice, not new problems."
          rows={[["Left", goal.remaining], ["Solve days", pace.solveDaysLeft], ["Per solve day", pace.perSolveDay ?? "—"], [pace.daysVsTarget != null && pace.daysVsTarget > 0 ? "Late by" : "Spare", pace.daysVsTarget == null ? "—" : `${Math.abs(pace.daysVsTarget)} days`]]}
        />
    </>
  );

  return <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Plan"]} title={TITLE} sub="Your routine: which days bring new problems, how many, and the pace that follows." tiles={tiles} left={<PlanSwitcher link />} center={center} right={right} label="Plans" />;
}
