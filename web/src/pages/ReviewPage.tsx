import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api, type Grade, type ReviewItem, type ReviewQueue } from "../api";
import { Header } from "../components/Header";
import { HowFound, Page3, Tile } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { Companies, Md, PyCode } from "../components/dsaBits";
import { RecallCurve } from "../components/RecallCurve";
import { DIFF, inDays, leetcode, niceDate, videoUrl } from "../dsa";
import { mmss } from "../mock";
import { getPref, setPref } from "../prefs";

const GRADES: { grade: Grade; name: string; key: string; hint: string }[] = [
  { grade: "again", name: "Again", key: "1", hint: "couldn't recall it" },
  { grade: "hard", name: "Hard", key: "2", hint: "with real struggle" },
  { grade: "good", name: "Good", key: "3", hint: "on my own" },
  { grade: "easy", name: "Easy", key: "4", hint: "instantly" },
];

interface Result {
  id: string;
  title: string;
  grade: Grade;
  due: string | null;
}

/** Today's reviews, one at a time: recall first, then check the approach and grade how it went (decision 26). */
export function ReviewPage() {
  const q = useQuery({ queryKey: ["dsa-review"], queryFn: api.review, staleTime: 0, gcTime: 0, refetchOnMount: "always" });
  return (
    <>
      <Header area="dsa" />
      <div style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
        {q.data ? (
          <Session queue={q.data} />
        ) : (
          <main className="page">
            <div className="wrap" style={{ paddingBlock: 28 }}>
              {q.isError ? <p className="rempty">Couldn't reach the API. <Link to="/dsa">Back to DSA</Link></p> : <Skeleton lines={5} />}
            </div>
          </main>
        )}
      </div>
    </>
  );
}

type SortKey = "forgotten" | "overdue" | "pattern";
const SORTS: [SortKey, string][] = [
  ["forgotten", "Most forgotten"],
  ["overdue", "Overdue"],
  ["pattern", "Pattern"],
];

/** The reviews that are not due yet, to do early; "weak spots only" narrows them to the ones forgotten twice or likely forgotten. */
function Ahead({ items, today, onPick, open = false }: { items: ReviewItem[]; today: string; onPick: (id: string) => void; open?: boolean }) {
  const [weakOnly, setWeakOnly] = useState(false);
  if (items.length === 0) return null;
  const shown = weakOnly ? items.filter(isWeak) : items;
  return (
    <details className="rv-ahead" open={open || undefined}>
      <summary>Review ahead · {items.length} more ›</summary>
      <label className="rv-weak">
        <input type="checkbox" checked={weakOnly} onChange={(e) => setWeakOnly(e.target.checked)} /> Weak spots only <small>({items.filter(isWeak).length}: lapsed twice or more, or under 70% recall)</small>
      </label>
      <div className="rv-q">
        {shown.map((it) => (
          <button key={it.problem.id} className="rv-qi pick" onClick={() => onPick(it.problem.id)}>
            <span className="n">+</span>
            <span className="t">
              {it.problem.title}
              <small><i className={`dp ${it.problem.difficulty}`} />{DIFF[it.problem.difficulty][0]} · {dueLabel(it.due, today)}</small>
            </span>
            <span className={`rc ${it.recall < 0.7 ? "lo" : it.recall < 0.85 ? "mid" : "hi"}`}>{Math.round(it.recall * 100)}%</span>
          </button>
        ))}
      </div>
    </details>
  );
}

/** The next seven days of reviews as bars, and what follows the reviews today: the new problem, or the next review to come. */
function Coming({ today, items, next }: { today: string; items: ReviewItem[]; next: ReviewQueue }) {
  const base = new Date(`${today}T00:00`);
  const days = Array.from({ length: 7 }, (_, i) => {
    const d = new Date(base.getFullYear(), base.getMonth(), base.getDate() + i);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const n = items.filter((it) => (i === 0 ? it.due <= key : it.due === key)).length;
    return { key, n, label: d.toLocaleDateString("en", { weekday: "short" }).slice(0, 2) };
  });
  const top = Math.max(1, ...days.map((d) => d.n));
  return (
    <div className="rv-coming">
      <span className="rv-lab">NEXT 7 DAYS</span>
      <div className="rv-week" aria-label="Reviews in the next seven days">
        {days.map((d) => (
          <div key={d.key} title={`${d.n} review${d.n === 1 ? "" : "s"}`}>
            <i style={{ height: `${Math.max(d.n ? 8 : 2, (d.n / top) * 100)}%` }} />
            <b>{d.n}</b>
            <span>{d.label}</span>
          </div>
        ))}
      </div>
      {next.solve_day && next.next_new && <Link className="rv-after" to="/d/$slug" params={{ slug: next.next_new.slug }}><small>THEN TODAY'S NEW PROBLEM</small>{next.next_new.title}</Link>}
      {!next.solve_day && next.next_review && <Link className="rv-after" to="/d/$slug" params={{ slug: next.next_review.slug }}><small>NEXT REVIEW {niceDate(next.next_review.due, today).toUpperCase()}</small>{next.next_review.title}</Link>}
    </div>
  );
}

/** How well this one is remembered: the numbers behind it, and the forecast. */
function MemoryCard({ item, today }: { item: ReviewItem; today: string }) {
  const s = item.problem.state;
  return (
    <div className="rv-card" style={{ gap: 12 }}>
      <span className="rv-lab">MEMORY</span>
      <div className="rv-facts">
        <div><b>{Math.round(item.recall * 100)}%</b><span>recall now</span></div>
        <div><b>{item.stability >= 10 ? Math.round(item.stability) : item.stability.toFixed(1)} d</b><span>memory holds</span></div>
        <div><b>{s.reps}</b><span>review{s.reps === 1 ? "" : "s"}</span></div>
        <div><b>{s.lapses}</b><span>lapse{s.lapses === 1 ? "" : "s"}</span></div>
        <div><b>{ago(s.last_review, today) || "–"}</b><span>last reviewed</span></div>
        <div><b>{dueLabel(item.due, today)}</b><span>next</span></div>
      </div>
      <RecallCurve id={item.problem.id} version={s.reps} />
    </div>
  );
}

/** The problem, as LeetCode words it, so a review can start from the statement. It is the question, not the answer. */
function StatementCard({ id, slug }: { id: string; slug: string }) {
  const q = useQuery({ queryKey: ["dsa-statement", id], queryFn: () => api.statement(id), staleTime: Infinity, retry: false });
  const [all, setAll] = useState(false);
  return (
    <div className="rv-card" style={{ gap: 10 }}>
      <span className="rv-lab">THE PROBLEM</span>
      {q.isLoading && <Skeleton lines={4} />}
      {(q.isError || q.data?.locked) && <p className="rv-note">LeetCode does not share this statement here. <a href={leetcode(slug)} target="_blank" rel="noreferrer">Open it on LeetCode ↗</a></p>}
      {q.data?.html && (
        <>
          <div className={`rv-stmt${all ? " all" : ""}`}>
            <div className="d-statement" dangerouslySetInnerHTML={{ __html: q.data.html }} />
          </div>
          <button className="rv-ghost" style={{ alignSelf: "flex-start" }} onClick={() => setAll(!all)}>{all ? "Show less" : "Show all"}</button>
        </>
      )}
    </div>
  );
}

/** After the reveal: the traps, what it is asked with, and what you saved for it. */
function CheckExtras({ item, lesson }: { item: ReviewItem; lesson: Awaited<ReturnType<typeof api.dsaPage>> | null }) {
  const p = item.problem;
  const mine = useQuery({ queryKey: ["dsa-solutions", p.id], queryFn: () => api.solutions(p.id) });
  return (
    <>
      {lesson && lesson.tips.length > 0 && (
        <div className="rv-card" style={{ gap: 8 }}>
          <span className="rv-lab">TIPS AND PITFALLS</span>
          <ul className="rv-tips">
            {lesson.tips.map((t) => (
              <li key={t}><Md inline text={t} /></li>
            ))}
          </ul>
        </div>
      )}
      <div className="rv-card" style={{ gap: 10 }}>
        <span className="rv-lab">TOPICS AND COMPANIES</span>
        <div className="rv-chips">{p.tags.map((t) => <span key={t} className="rv-pill">{t}</span>)}</div>
        <Companies companies={p.companies} limit={8} />
      </div>
      {(mine.data ?? []).length > 0 && (
        <div className="rv-card" style={{ gap: 10 }}>
          <span className="rv-lab">YOUR SOLUTIONS</span>
          {(mine.data ?? []).map((m, i) => (
            <details key={m.id} className="rv-mine">
              <summary>{m.label || `Solution ${i + 1}`}</summary>
              {m.notes && <Md text={m.notes} />}
              <PyCode code={m.code} lineNumbers />
            </details>
          ))}
        </div>
      )}
    </>
  );
}

/** A weak spot: forgotten at least twice, or unlikely to be recalled today. */
const isWeak = (i: ReviewItem) => i.problem.state.lapses >= 2 || i.recall < 0.7;

/** How a review stands against today: "3 days overdue", "due today", "due in 2 days". */
function dueLabel(due: string, today: string): string {
  const days = Math.round((Date.parse(`${today}T00:00`) - Date.parse(`${due}T00:00`)) / 864e5);
  return days > 1 ? `${days} days overdue` : days === 1 ? "1 day overdue" : days === 0 ? "due today" : `due in ${-days} day${days === -1 ? "" : "s"}`;
}

/** The queue is yours to pick from: click any review, or move with J and K. Reviewing one that is not due yet is fine. */
function Session({ queue }: { queue: ReviewQueue }) {
  const qc = useQueryClient();
  const all = useMemo(() => new Map([...queue.items, ...queue.ahead].map((i) => [i.problem.id, i])), [queue.items, queue.ahead]);
  // The session's own list: today's reviews to begin with, and any "review ahead" picked later. Grading takes one out.
  const [pending, setPending] = useState<string[]>(() => {
    const ids = queue.items.map((i) => i.problem.id);
    const slug = new URLSearchParams(location.search).get("p");
    const asked = slug ? queue.ahead.find((i) => i.problem.slug === slug) : undefined;
    return asked && !ids.includes(asked.problem.id) ? [...ids, asked.problem.id] : ids;
  });
  // the review that is open is in the address bar (?p=two-sum): a refresh or a shared link comes back to it
  const [current, setCurrent] = useState<string | undefined>(() => {
    const slug = new URLSearchParams(location.search).get("p");
    const asked = slug ? [...queue.items, ...queue.ahead].find((i) => i.problem.slug === slug) : undefined;
    return asked?.problem.id ?? queue.items[0]?.problem.id;
  });
  const [sort, setSortState] = useState<SortKey>(() => getPref<SortKey>("review.sort", "forgotten"));
  const [results, setResults] = useState<(Result & { seconds: number })[]>([]);
  const [step, setStep] = useState<"recall" | "check">("recall");
  // The timer runs only once it is started, and starts over with each problem.
  const [spent, setSpent] = useState(0);
  const [running, setRunning] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [tab, setTab] = useState(0);
  const sessionStart = useRef(Date.now());

  const sorted = useMemo(() => {
    const it = (id: string) => all.get(id)!;
    const by: Record<SortKey, (a: string, b: string) => number> = {
      forgotten: (a, b) => it(a).recall - it(b).recall,
      overdue: (a, b) => it(a).due.localeCompare(it(b).due) || it(a).recall - it(b).recall,
      pattern: (a, b) => it(a).pattern.localeCompare(it(b).pattern) || it(a).recall - it(b).recall,
    };
    return [...pending].sort(by[sort]);
  }, [pending, sort, all]);
  const setSort = (s: SortKey) => {
    setSortState(s);
    setPref("review.sort", s);
  };

  const item = current ? all.get(current) : undefined;
  useEffect(() => {
    const slug = item?.problem.slug;
    window.history.replaceState(window.history.state, "", slug ? `?p=${slug}` : location.pathname);
  }, [item?.problem.slug]);
  const total = results.length + pending.length;
  const done = results.length;

  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setSpent((n) => n + 1), 1000);
    return () => window.clearInterval(t);
  }, [running]);

  const fresh = () => {
    setStep("recall");
    setTab(0);
    setSpent(0);
    setRunning(false);
    setError(null);
  };
  const refresh = () => {
    for (const key of ["dsa", "practice", "activity", "tracks", "reviews", "progress", "stats", "dsa-mock"]) void qc.invalidateQueries({ queryKey: [key] });
  };
  /** Opens a review: one of today's, or a later one picked from "review ahead", which joins the session. */
  const pick = (id: string) => {
    if (id === current) return;
    if (!pending.includes(id)) setPending((p) => [...p, id]);
    setCurrent(id);
    setNote(null);
    fresh();
  };
  const move = useCallback(
    (delta: number) => {
      if (!sorted.length) return;
      const at = current ? sorted.indexOf(current) : -1;
      const next = sorted[(at + delta + sorted.length) % sorted.length]!;
      if (next !== current) {
        setCurrent(next);
        fresh();
      }
    },
    [sorted, current], // eslint-disable-line react-hooks/exhaustive-deps
  );

  const lesson = useQuery({
    queryKey: ["dsa-page", item?.problem.id],
    queryFn: () => api.dsaPage(item!.problem.id),
    enabled: step === "check" && !!item?.problem.has_page,
  });

  const log = useMutation({
    mutationFn: ({ id, grade }: { id: string; grade: Grade }) => api.logDsa(id, grade),
    onSuccess: (r, { id, grade }) => {
      const it = all.get(id);
      setResults((cur) => [...cur, { id, title: it?.problem.title ?? id, grade, due: r.due, seconds: spent }]);
      const left = sorted.filter((x) => x !== id);
      const at = sorted.indexOf(id);
      setPending(left);
      setCurrent(left.length ? left[Math.min(at, left.length - 1)] : undefined);
      setNote(null);
      fresh();
      refresh();
    },
    onError: (e: Error) => setError(`Couldn't log it: ${e.message}`),
  });
  const undo = useMutation({
    mutationFn: (id: string) => api.undoDsa(id),
    onSuccess: (r, id) => {
      if (!r.undone) return setNote("That grade can't be undone here. Use Reset progress on the problem's page.");
      setResults((cur) => cur.filter((x) => x.id !== id));
      setPending((p) => (p.includes(id) ? p : [...p, id]));
      setCurrent(id);
      setNote(null);
      fresh();
      refresh();
    },
    onError: (e: Error) => setNote(`Couldn't undo it: ${e.message}`),
  });

  const grade = useCallback(
    (g: Grade) => {
      if (item && !log.isPending) log.mutate({ id: item.problem.id, grade: g });
    },
    [item, log],
  );
  const last = results[results.length - 1];

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || (e.target instanceof HTMLElement && e.target.closest("input, textarea, select, [contenteditable]"))) return;
      if (e.key === " " && step === "recall" && item) {
        e.preventDefault();
        setStep("check");
      } else if (step === "check" && "1234".includes(e.key) && e.key) {
        grade(GRADES[Number(e.key) - 1]!.grade);
      } else if (e.key === "j" || e.key === "J" || e.key === "ArrowDown") {
        e.preventDefault();
        move(1);
      } else if (e.key === "k" || e.key === "K" || e.key === "ArrowUp") {
        e.preventDefault();
        move(-1);
      } else if ((e.key === "s" || e.key === "S") && step === "recall") {
        move(1);
      } else if ((e.key === "u" || e.key === "U") && last && !undo.isPending) {
        undo.mutate(last.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [step, item, grade, move, last, undo]);

  if (queue.items.length === 0 && results.length === 0 && pending.length === 0) return <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Review"]} title="Review" sub="Nothing is due. Review ahead below, or go on to today's problem." center={<Empty queue={queue} ahead={queue.ahead} onPick={pick} />} />;
  if (!item) return <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Review"]} title="Review" sub="Today's reviews are done." center={<Finished queue={queue} results={results} seconds={Math.round((now - sessionStart.current) / 1000)} onUndo={last ? () => undo.mutate(last.id) : undefined} />} />;

  const waitingAll = queue.ahead.filter((i) => !pending.includes(i.problem.id) && !results.some((r) => r.id === i.problem.id));
  const timed = results.filter((r) => r.seconds > 0);
  const average = timed.length ? Math.round(timed.reduce((n, r) => n + r.seconds, 0) / timed.length) : null;
  const dueItems = queue.items;
  const overdue = dueItems.filter((i) => i.due < queue.today).length;
  const recallAvg = dueItems.length + queue.ahead.length ? Math.round(([...dueItems, ...queue.ahead].reduce((n, i) => n + i.recall, 0) / (dueItems.length + queue.ahead.length)) * 100) : null;
  const week = (() => {
    const end = new Date(`${queue.today}T00:00`);
    end.setDate(end.getDate() + 6);
    const key = `${end.getFullYear()}-${String(end.getMonth() + 1).padStart(2, "0")}-${String(end.getDate()).padStart(2, "0")}`;
    return [...sorted.map((id) => all.get(id)!).filter(Boolean), ...waitingAll].filter((i) => i.due <= key).length;
  })();
  const gradeN = (g: Grade) => results.filter((r) => r.grade === g).length;

  const tiles = (
    <>
      <Tile em="Due" strong={queue.due} hot={queue.due > 0} color="var(--vio)" small={overdue ? `${overdue} overdue` : "none overdue"} />
      <Tile em="Memory" strong={recallAvg === null ? "–" : <>{recallAvg}<span>%</span></>} small="average recall today" />
      <Tile em="Next 7 days" strong={week} small="reviews coming" />
    </>
  );

  const left = (
    <section className="ly-pn rv-side" aria-label="Reviews">
      <div className="rv-qh">
        <span className="rv-lab">REVIEWS · {pending.length} LEFT</span>
      </div>
      <div className="rv-sort" role="group" aria-label="Sort the reviews">
        {SORTS.map(([k, label]) => (
          <button key={k} className={sort === k ? "on" : ""} aria-pressed={sort === k} onClick={() => setSort(k)}>{label}</button>
        ))}
      </div>
      <div className="rv-q">
        {results.map((r) => (
          <div key={r.id} className="rv-qi done">
            <span className="n">✓</span>
            <span className="t">{r.title}</span>
            <span className={`r ${r.grade}`}>{r.grade}</span>
          </div>
        ))}
        {sorted.map((id, i) => {
          const it = all.get(id)!;
          return (
            <button key={id} className={`rv-qi pick${id === current ? " cur" : ""}`} aria-current={id === current} onClick={() => pick(id)} style={{ "--rc": `${Math.round(it.recall * 100)}%` } as CSSProperties}>
              <span className="n">{done + i + 1}</span>
              <span className="t">
                {it.problem.title}
                <small><i className={`dp ${it.problem.difficulty}`} />{DIFF[it.problem.difficulty][0]} · {it.pattern} · {dueLabel(it.due, queue.today)}</small>
              </span>
              <span className={`rc ${it.recall < 0.7 ? "lo" : it.recall < 0.85 ? "mid" : "hi"}`}>{Math.round(it.recall * 100)}%</span>
            </button>
          );
        })}
      </div>
      <Ahead items={waitingAll} today={queue.today} onPick={pick} open={waitingAll.length > 0 && sorted.length + results.length < 8} />
    </section>
  );

  const center = (
    <>
      <div className="rv-bar">
        <div className="rv-t">
          <span className="l">REVIEW · {done} OF {total} DONE</span>
          <span className="c">{mmss(spent)}</span>
        </div>
        <div className="rv-dots" aria-hidden="true">
          {Array.from({ length: total }, (_, i) => (
            <i key={i} className={i < done ? "done" : i === done ? "cur" : ""} />
          ))}
        </div>
        <span className="rv-cap">{queue.due} due · room for {queue.capacity} today</span>
        <Link className="rv-ghost" to="/dsa">Exit</Link>
      </div>
      <div className="rv-card rv-main" key={item.problem.id}>
        {step === "recall" ? (
          <Recall item={item} spent={spent} running={running} setRunning={setRunning} reveal={() => setStep("check")} skip={() => move(1)} canSkip={sorted.length > 1} today={queue.today} />
        ) : (
          <Check item={item} spent={spent} lesson={lesson.data ?? null} loading={lesson.isLoading} tab={tab} setTab={setTab} grade={grade} busy={log.isPending} error={error} />
        )}
      </div>
      {step === "check" ? <CheckExtras item={item} lesson={lesson.data ?? null} /> : <StatementCard id={item.problem.id} slug={item.problem.slug} />}
    </>
  );

  const right = (
    <div className="rv-ctx" aria-label="About this review">
      <MemoryCard item={item} today={queue.today} />
      <section className="ly-pn rv-sg" aria-label="This session">
        <span className="rv-lab">THIS SESSION</span>
        <div className="rv-sgn"><b>{done}</b> <span>done · {pending.length} left · {average != null ? <>average {mmss(average)}</> : "start the timer to see your average"}</span></div>
        {done > 0 && (
          <div className="rv-stack" aria-hidden="true">
            {GRADES.map((g) => (gradeN(g.grade) ? <i key={g.grade} className={g.grade} style={{ flex: gradeN(g.grade) }} /> : null))}
          </div>
        )}
        <div className="rv-sgl">
          {GRADES.map((g) => <span key={g.grade} className={`rv-pill ${g.grade}`}>{g.name} {gradeN(g.grade)}</span>)}
        </div>
        {last && (
          <button className="rv-ghost" disabled={undo.isPending} onClick={() => undo.mutate(last.id)}>Undo “{last.title}” <kbd>U</kbd></button>
        )}
        {note && <span className="rv-err" role="status">{note}</span>}
      </section>
      <section className="ly-pn">
        <Coming today={queue.today} items={[...sorted.map((id) => all.get(id)!).filter(Boolean), ...waitingAll]} next={queue} />
      </section>
      <HowFound
        title="How the queue is ordered"
        text={sort === "forgotten" ? "By recall: the problem you are most likely to have forgotten is first. Reviewing early still counts." : sort === "overdue" ? "By how long it has been due, then by recall." : "By pattern, then by recall within a pattern."}
        rows={[["Due", `${queue.due}${overdue ? ` (${overdue} overdue)` : ""}`], ["Ranked by", sort === "forgotten" ? "lowest recall" : sort === "overdue" ? "days overdue" : "pattern"], ["Memory model", "FSRS, your grades"], ["Ahead", `${waitingAll.length} more`]]}
      />
      <dl className="ly-pn rv-kv rv-keys">
        <div><dt>Show the approach</dt><dd><kbd>Space</kbd></dd></div>
        <div><dt>Again · Hard · Good · Easy</dt><dd><kbd>1 2 3 4</kbd></dd></div>
        <div><dt>Next · previous</dt><dd><kbd>J</kbd> <kbd>K</kbd></dd></div>
        <div><dt>Undo the last grade</dt><dd><kbd>U</kbd></dd></div>
      </dl>
    </div>
  );

  return (
    <Page3
      crumb={[<Link to="/dsa">DSA</Link>, "Review"]}
      title="Review"
      sub={`${queue.due} due${overdue ? `, ${overdue} overdue` : ""}. Pick any of them; ${sort === "forgotten" ? "the most forgotten is first" : sort === "overdue" ? "the longest overdue is first" : "they are grouped by pattern"}.`}
      tiles={tiles}
      left={left}
      center={center}
      right={right}
      label="Reviews"
    />
  );
}

const ago = (iso: string | null, today: string) => {
  if (!iso) return "";
  const days = Math.round((Date.parse(`${today}T00:00`) - Date.parse(`${iso}T00:00`)) / 864e5);
  return days <= 0 ? "today" : days === 1 ? "yesterday" : `${days} days ago`;
};
const TRY_GLYPH = { good: "✓", hard: "½", again: "!" } as const;
const TRY_NAME = { good: "on my own", hard: "with help", again: "attempted" } as const;
const ORD = (n: number) => (n % 100 >= 11 && n % 100 <= 13 ? "th" : ["th", "st", "nd", "rd"][n % 10] ?? "th");
const LAST: Record<string, string> = { good: "✓ on my own", easy: "⚡ instantly", hard: "½ with help", again: "! attempted" };

function Recall({ item, spent, running, setRunning, reveal, skip, canSkip, today }: { item: ReviewItem; spent: number; running: boolean; setRunning: (r: boolean) => void; reveal: () => void; skip: () => void; canSkip: boolean; today: string }) {
  const p = item.problem;
  const s = p.state;
  const tries = useQuery({ queryKey: ["dsa-preview", p.id], queryFn: () => api.preview(p.id) });
  const history = tries.data?.history ?? [];
  return (
    <>
      <span className="rv-lab">RECALL · SUGGESTED 6 MIN</span>
      <h2 className="rv-title">{p.title}</h2>
      <div className="rv-meta">
        <span className="rv-pill" style={{ color: DIFF[p.difficulty][1] }}>{DIFF[p.difficulty][0]}</span>
      </div>
      <div className="rv-prior">
        <div><b>{s.last_grade ? LAST[s.last_grade] : "—"}</b><span>{ago(s.last_review, today)}</span></div>
        <div><b>{Math.round(item.recall * 100)}%</b><span>chance you'd recall it today</span></div>
        <div><b>{item.stability >= 10 ? Math.round(item.stability) : item.stability.toFixed(1)} days</b><span>how long the memory holds</span></div>
      </div>
      <div className="rv-timer">
        <div className="tk"><i style={{ width: `${Math.min(100, (spent / 360) * 100)}%` }} /></div>
        <div className="lb">
          <span>{mmss(spent)} so far</span>
          <button onClick={() => setRunning(!running)}>{running ? "Pause timer" : spent ? "Resume timer" : "Start timer"}</button>
          <span>06:00 suggested</span>
        </div>
      </div>
      <div className="rv-row">
        <button className="rv-go" onClick={reveal}>
          Show the approach <kbd>Space</kbd>
        </button>
        <a className="rv-ghost" href={leetcode(p.slug)} target="_blank" rel="noreferrer">Open on LeetCode ↗</a>
        <span className="sp" />
        {canSkip && (
          <button className="rv-ghost" onClick={skip}>
            Skip for now <kbd>S</kbd>
          </button>
        )}
      </div>
      {history.length > 0 && (
        <section className="rv-sec" aria-label="Your tries">
          <span className="rv-lab">YOUR TRIES</span>
          <div className="rv-tries">
            {history.map((h, i) => (
              <span key={i} className={`rv-try ${h.result}`}>
                <i>{TRY_GLYPH[h.result]}</i>
                {h.kind === "practice" ? "Practice" : `${history.slice(0, i + 1).filter((x) => x.kind !== "practice").length}${ORD(history.slice(0, i + 1).filter((x) => x.kind !== "practice").length)} try`}
                <small>{TRY_NAME[h.result]}</small>
              </span>
            ))}
          </div>
        </section>
      )}
      {p.companies.length > 0 && (
        <section className="rv-sec" aria-label="Asked by">
          <span className="rv-lab">ASKED BY</span>
          <Companies companies={p.companies} limit={10} />
        </section>
      )}
    </>
  );
}

function Check({ item, spent, lesson, loading, tab, setTab, grade, busy, error }: { item: ReviewItem; spent: number; lesson: Awaited<ReturnType<typeof api.dsaPage>> | null; loading: boolean; tab: number; setTab: (n: number) => void; grade: (g: Grade) => void; busy: boolean; error: string | null }) {
  const p = item.problem;
  const a = lesson?.approaches[tab];
  return (
    <>
      <span className="rv-lab">CHECK · {mmss(spent)} SPENT</span>
      <h2 className="rv-title">{p.title}</h2>
      <div className="rv-meta">
        <span className="rv-pill" style={{ color: DIFF[p.difficulty][1] }}>{DIFF[p.difficulty][0]}</span>
        <span className="rv-pill">{item.pattern}</span>
        {item.technique && <span className="rv-pill" style={{ color: "var(--vio)" }}>{item.technique}</span>}
      </div>
      {loading && <p className="rv-note">Loading the approach…</p>}
      {lesson ? (
        <>
          <div className="rv-reveal">
            <span className="rv-lab" style={{ color: "var(--vio)" }}>THE IDEA</span>
            <Md text={lesson.intuition} />
          </div>
          {lesson.approaches.length > 1 && (
            <div className="rv-tabs" role="tablist">
              {lesson.approaches.map((x, i) => (
                <button key={x.name} role="tab" aria-selected={i === tab} className={i === tab ? "on" : ""} onClick={() => setTab(i)}>{x.label}</button>
              ))}
            </div>
          )}
          {a && (
            <div className="rv-ap">
              <PyCode code={a.code} />
              <p className="rv-note">time {a.time} · space {a.space}. <Link to="/d/$slug" params={{ slug: p.slug }}>Full page</Link></p>
            </div>
          )}
        </>
      ) : (
        !loading && (
          <div className="rv-reveal">
            <p style={{ margin: 0 }}>No written page yet. Check your own notes against the problem.</p>
            <div className="rv-row">
              <Link className="rv-ghost" to="/d/$slug" params={{ slug: p.slug }}>The problem page</Link>
              <a className="rv-ghost" href={leetcode(p.slug)} target="_blank" rel="noreferrer">LeetCode ↗</a>
              {p.video && <a className="rv-ghost" href={videoUrl(p.video)} target="_blank" rel="noreferrer">▶ NeetCode's video</a>}
            </div>
          </div>
        )
      )}
      <div>
        <span className="rv-lab">HOW DID IT GO?</span>
        <div className="rv-grades">
          {GRADES.map((g) => (
            <button key={g.grade} className={`rv-g ${g.grade}`} disabled={busy} onClick={() => grade(g.grade)}>
              <b>{g.name}</b>
              <span>{inDays(item.previews[g.grade].days)}</span>
              <small>{g.hint}</small>
              <kbd>{g.key}</kbd>
            </button>
          ))}
        </div>
        {error && <p className="rv-err">{error}</p>}
      </div>
    </>
  );
}

function Empty({ queue, ahead, onPick }: { queue: ReviewQueue; ahead: ReviewItem[]; onPick: (id: string) => void }) {
  const next = queue.next_review;
  return (
    <div className="rv-wide rv-stage rv-empty">
      <div className="rv-card">
        <span className="rv-lab">{queue.capacity === 0 ? "A REST DAY" : "NOTHING DUE"}</span>
        <h2 className="rv-title">{queue.capacity === 0 ? "No reviews today." : "You're caught up."}</h2>
        <p className="rv-note" style={{ fontSize: 14, color: "var(--mut)" }}>
          {next ? (
            <>
              The next review is <b style={{ color: "var(--fg)" }}>{next.title}</b>, {niceDate(next.due, queue.today)}.
            </>
          ) : (
            "Nothing is scheduled yet. Log a problem and it comes back here."
          )}
          {queue.solve_day && queue.next_new ? " Today is a solve day, so your next new problem is ready." : ""}
        </p>
        <Next queue={queue} />
      </div>
      <div className="rv-card">
        <Coming today={queue.today} items={ahead} next={queue} />
        <Ahead items={ahead} today={queue.today} onPick={onPick} open />
        {ahead.length === 0 && <p className="rv-note">Nothing is waiting. Log a problem and its reviews are planned here.</p>}
      </div>
    </div>
  );
}

function Next({ queue }: { queue: ReviewQueue }) {
  return (
    <div className="rv-row">
      {queue.next_new && (
        <Link className="rv-go" to="/d/$slug" params={{ slug: queue.next_new.slug }}>
          Next new problem: {queue.next_new.title} →
        </Link>
      )}
      <Link className="rv-ghost" to="/dsa">Back to DSA</Link>
    </div>
  );
}

function Finished({ queue, results, seconds, onUndo }: { queue: ReviewQueue; results: Result[]; seconds: number; onUndo?: () => void }) {
  const count = (g: Grade) => results.filter((r) => r.grade === g).length;
  const tomorrow = results.filter((r) => r.due && r.due <= tomorrowOf(queue.today)).length;
  return (
    <div className="wrap rv-stage" style={{ gridTemplateColumns: "minmax(0, 1fr)" }}>
      <div className="rv-card">
        <span className="rv-lab">SESSION DONE · {niceDate(queue.today, queue.today).toUpperCase()}</span>
        <div className="rv-row" style={{ alignItems: "flex-end", gap: 18 }}>
          <span className="rv-big">{results.length}</span>
          <span style={{ color: "var(--mut)", paddingBottom: 4 }}>
            review{results.length === 1 ? "" : "s"} in <b style={{ color: "var(--fg)" }}>{Math.max(1, Math.round(seconds / 60))} min</b>
          </span>
        </div>
        <div className="rv-dist">
          {GRADES.map((g) => (
            <span key={g.grade} className={`rv-pill ${g.grade}`}>{g.name} {count(g.grade)}</span>
          ))}
        </div>
        <span className="rv-lab" style={{ marginTop: 6 }}>COMES BACK</span>
        <div className="rv-q">
          {results.map((r) => (
            <div key={r.id} className="rv-next">
              <span>{r.title}</span>
              <span className={`rv-pill ${r.grade}`}>{GRADES.find((g) => g.grade === r.grade)?.name}</span>
              <span className="w">{r.due ? niceDate(r.due, queue.today) : "no review"}</span>
            </div>
          ))}
        </div>
        <Next queue={queue} />
        {onUndo && <button className="rv-ghost" style={{ alignSelf: "flex-start" }} onClick={onUndo}>Undo the last grade <kbd>U</kbd></button>}
        <p className="rv-note">{tomorrow ? `Tomorrow: ${tomorrow} review${tomorrow === 1 ? " is" : "s are"} due.` : "Nothing is due tomorrow."}</p>
      </div>
    </div>
  );
}

function tomorrowOf(iso: string): string {
  const d = new Date(`${iso}T00:00`);
  d.setDate(d.getDate() + 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
