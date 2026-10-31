import { Link } from "@tanstack/react-router";
import { GradeGlyph } from "../components/icons";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, type Band, type DsaProblem, type PracticeTechnique } from "../api";
import { Companies, Insight, Mark, ProblemTags, useLogger } from "../components/dsaBits";
import { PatternShell } from "../components/PatternShell";
import { DIFF, GRADES, MINUTES, leetcode } from "../dsa";
import { Panel } from "../components/layout";
import { Segmented } from "../components/ui";

/** A pattern's practice: for each technique, LeetCode problems beyond the NeetCode lists that drill it. */
export function PracticePage({ code }: { code: string }) {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const practice = useQuery({ queryKey: ["practice", code], queryFn: () => api.practice(code) });
  const { log, toast } = useLogger(overview.data?.today);
  const [hidden, setHidden] = useState<"none" | "done">("none");
  const pattern = overview.data?.patterns.find((p) => p.code.toLowerCase() === code.toLowerCase() || p.slug === code);
  const techniques = practice.data?.techniques ?? [];
  const all = techniques.flatMap((t) => t.problems);
  const solved = all.filter((p) => p.state.solved).length;
  const today = overview.data?.today ?? "";
  const [diff, setDiff] = useState<Band[]>([]);
  const left = techniques.length > 0 && (
    <Panel title="Show">
      <div className="ps-fsum"><b>{solved}</b> of {all.length} solved<span className="ps-fbar"><i style={{ width: `${all.length ? (solved / all.length) * 100 : 0}%` }} /></span></div>
      <p>Not in a NeetCode list: tracked, but not counted in your goal, reviews or calendar.</p>
      <span className="ly-lab">Status</span>
      <Segmented className="ps-fseg" label="Status" value={hidden} options={[["none", "All"], ["done", "Not solved"]] as const} onChange={setHidden} />
      <span className="ly-lab">Difficulty</span>
      <div className="ps-fchips">
        {(["easy", "medium", "hard"] as Band[]).map((b) => (
          <button key={b} className={`fchip${diff.includes(b) ? " on" : ""}`} aria-pressed={diff.includes(b)} onClick={() => setDiff((d) => (d.includes(b) ? d.filter((x) => x !== b) : [...d, b]))}>{DIFF[b][0]}</button>
        ))}
      </div>
    </Panel>
  );
  return (
    <PatternShell code={code} tab="practice" left={left || undefined}>
          {practice.isError && <p className="notice bad">Couldn't load the practice list: {(practice.error as Error).message}</p>}
          {practice.isSuccess && techniques.length === 0 && (
            <div className="d-note">No practice list for {pattern?.name ?? "this pattern"} yet.</div>
          )}
          {techniques.map((t) => (
            <Technique key={t.id} t={t} today={today} hideSolved={hidden === "done"} diff={diff} log={log} />
          ))}
      {toast}
    </PatternShell>
  );
}

function Technique({ t, today, hideSolved, diff, log }: { t: PracticeTechnique; today: string; hideSolved: boolean; diff: Band[]; log: ReturnType<typeof useLogger>["log"] }) {
  const rows = t.problems.filter((p) => (!hideSolved || !p.state.solved) && (diff.length === 0 || diff.includes(p.difficulty)));
  return (
    <section className="d-tech">
      <div className="d-th">
        <h2>{t.name}</h2>
        <span className="d-tcount">
          {t.solved} / {t.problems.length}
        </span>
      </div>
      <div className="d-tfrom">
        learn it from{" "}
        <Link to="/d/$slug" params={{ slug: t.must_learn.slug }}>
          #{t.must_learn.number} {t.must_learn.title}
        </Link>
        {t.must_learn.solved ? <span className="d-done"> ✓ done</span> : null}
      </div>
      <div className="d-plist">
        {rows.map((p) => (
          <PracticeRow key={p.id} p={p} today={today} log={log} />
        ))}
        {rows.length === 0 && <p className="rempty">All solved here.</p>}
      </div>
    </section>
  );
}

export function PracticeRow({ p, today, log }: { p: DsaProblem; today: string; log: ReturnType<typeof useLogger>["log"] }) {
  return (
    <article className="d-pcard d-prow">
      <Mark p={p} today={today} />
      <div className="d-pbody">
        <div className="d-ph">
          <span className="d-pnum">#{p.number}</span>
          <Link className="d-ptitle" to="/d/$slug" params={{ slug: p.slug }}>
            {p.title}
          </Link>
          <a className="d-pext" href={leetcode(p.slug)} target="_blank" rel="noreferrer" title="Solve it on LeetCode">
            LeetCode ↗
          </a>
        </div>
        <Insight text={p.insight} />
        <div className="d-pmeta">
          <ProblemTags p={p} />
          <span>~{MINUTES[p.difficulty]}m</span>
          <span>{p.tags.slice(0, 3).join(" · ")}</span>
        </div>
        <Companies companies={p.companies} limit={4} />
      </div>
      <div className="d-pside">
        <div className="d-acts">
          {GRADES.map((g) => (
            <button key={g.grade} className={g.cls} title={g.label} onClick={() => log(p, g.grade)}>
              <GradeGlyph grade={g.grade} />
            </button>
          ))}
        </div>
      </div>
    </article>
  );
}
