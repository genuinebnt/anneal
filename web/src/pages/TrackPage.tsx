import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Skeleton } from "../components/Skeleton";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQueries, useQuery } from "@tanstack/react-query";
import { marked } from "marked";
import { api, ApiError, type Band, type ProblemSummary, type TrackSummary } from "../api";
import { Header } from "../components/Header";
import { ResetTrack } from "../components/ResetTrack";
import { Level, ModeTag, StatusIcon, pad2, pctColor, progressLabel } from "../components/bits";
import { Opt } from "../components/filterBits";
import { Page3, Panel, ReadingPath, RingTile, Tile, type PathStep } from "../components/layout";
import { MockRoot } from "../components/mock";
import { FollowUps, Fold, PhaseBar, PhaseTag, Teaches, TestResults } from "../components/trackBits";
import { BLURB, PLANNED, SECTION_NAMES, sectionOf } from "../curriculum";
import "../styles/cookbook.css";

const TIER = { core: "CORE", light: "LIGHT", sde3: "SDE-3" } as const;
const MINUTES: Record<Band, number> = { easy: 12, medium: 25, hard: 45 };
const BAND_RANK: Record<Band, number> = { easy: 0, medium: 1, hard: 2 };
const PROGRESS_RANK = { not_started: 0, started: 1, solved: 2, assisted: 2 } as const;
const BANDS: Band[] = ["easy", "medium", "hard"];

type Tab = "learn" | "problems";
type SortKey = "n" | "t" | "m" | "l" | "s";
const SORT_LABELS: Record<SortKey, string> = { n: "#", t: "Problem", m: "Mode", l: "Level", s: "Status" };
type SetKey = "stages" | "levels" | "modes" | "status" | "tags" | "companies";
type Filters = Record<SetKey, Set<string>>;
const fresh = (): Filters => ({ stages: new Set(), levels: new Set(), modes: new Set(), status: new Set(), tags: new Set(), companies: new Set() });

/** The matched part of `text` in a <mark>, for the filter box. */
function Hl({ text, find }: { text: string; find: string }) {
  const i = find ? text.toLowerCase().indexOf(find.toLowerCase()) : -1;
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <mark>{text.slice(i, i + find.length)}</mark>
      {text.slice(i + find.length)}
    </>
  );
}

const statusKey = (p: ProblemSummary) => (p.status === "draft" ? "draft" : p.progress === "assisted" ? "solved" : p.progress);
const STATUS_NAME: Record<string, string> = { not_started: "not started", started: "in progress", solved: "solved", draft: "not written" };

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="rbox">
      <h4>{title}</h4>
      <div className="d-opts">{children}</div>
    </div>
  );
}

function count<T>(items: ProblemSummary[], key: (p: ProblemSummary) => T | T[]): Map<T, number> {
  const m = new Map<T, number>();
  for (const p of items) for (const v of ([] as T[]).concat(key(p))) m.set(v, (m.get(v) ?? 0) + 1);
  return m;
}

/** A page title with its last word in the accent gradient. The text itself is unchanged. */
function Accented({ text }: { text: string }) {
  const at = text.lastIndexOf(" ");
  return at < 0 ? <em className="bk-rgb">{text}</em> : <>{text.slice(0, at + 1)}<em className="bk-rgb">{text.slice(at + 1)}</em></>;
}

/** The first sentence of a statement, for a card. */
function firstSentence(statement: string): string {
  const para = (statement.split(/\n\s*\n/)[0] ?? "").replace(/\s+/g, " ").trim();
  const first = /^.{20,}?[.!?](?=\s|$)/.exec(para)?.[0] ?? para;
  return first.length > 220 ? `${first.slice(0, 220).replace(/\s+\S*$/, "")}…` : first;
}

const AREA_COLOR: Record<string, string> = { L: "var(--vio)", S: "var(--acc)", C: "var(--fn)", Y: "var(--grn)", F: "var(--warn)" };

/** Every written track as a card, grouped by section; the open one lists its stages. */
function TrackRail({ tracks, slug, here, onStage }: { tracks: TrackSummary[]; slug: string; here: { slug: string; name: string; solved: number; total: number }[]; onStage: (stage: string) => void }) {
  const [q, setQ] = useState("");
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const list = box.current;
    const on = list?.querySelector<HTMLElement>(".sr-card.on");
    if (list && on) list.scrollTop += on.getBoundingClientRect().top - list.getBoundingClientRect().top - 40;
  }, [slug, tracks.length]);
  const found = tracks.filter((c) => `${c.code} ${c.name}`.toLowerCase().includes(q.trim().toLowerCase()));
  const sections = [...new Set(found.map((c) => c.section))];
  return (
    <nav className="sr-rail" aria-label="All tracks">
      <input type="search" className="sr-filter" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter tracks…" aria-label="Filter tracks" />
      <div className="sr-list" ref={box}>
        {found.length === 0 && <p className="cb-dim">No track matches.</p>}
        {sections.map((section) => (
          <div key={section} className="sr-group">
            <h6><span>{SECTION_NAMES[section]}</span><small>{found.filter((c) => c.section === section).length}</small></h6>
            {found.filter((c) => c.section === section).map((c) => {
              const on = c.slug === slug;
              return (
                <div key={c.code} className={`sr-card${on ? " on" : ""}`} style={{ "--c": AREA_COLOR[c.section] ?? "var(--vio)" } as CSSProperties}>
                  <Link to="/t/$track" params={{ track: c.slug }} aria-current={on ? "page" : undefined}>
                    <span className="sr-card-h"><strong>{c.name}</strong><small>{c.solved}/{c.total}</small></span>
                    <span className="sr-bar" aria-hidden="true"><i style={{ width: `${c.total ? (c.solved / c.total) * 100 : 0}%` }} /></span>
                    <span className="sr-card-f"><em>{c.code}</em><small>{c.stages.length} stage{c.stages.length === 1 ? "" : "s"}</small></span>
                  </Link>
                  {on && (
                    <div className="sr-stages">
                      {here.map((x) => (
                        <button key={x.slug} type="button" onClick={() => onStage(x.slug)}><span>{x.name}</span><small>{x.solved}/{x.total}</small></button>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </nav>
  );
}

export function TrackPage({ slug }: { slug: string }) {
  const [resetOpen, setResetOpen] = useState(false);
  const q = useQuery({ queryKey: ["track", slug], queryFn: () => api.track(slug), retry: (n, e) => n < 2 && !(e instanceof ApiError && e.status === 404) });
  const all = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
  const reviews = useQuery({ queryKey: ["reviews"], queryFn: api.reviews });
  const [tab, setTabState] = useState<Tab>(() => (new URLSearchParams(location.search).get("tab") === "problems" ? "problems" : "learn"));
  const setTab = (t: Tab) => {
    setTabState(t);
    // keeps the router's own history state, so Back still works
    window.history.replaceState(window.history.state, "", t === "learn" ? location.pathname : `?tab=${t}`);
  };
  const [f, setF] = useState<Filters>(fresh);
  const [find, setFind] = useState("");
  const [sort, setSort] = useState<{ k: SortKey; dir: 1 | -1 } | null>(null);
  const [cur, setCur] = useState(-1);
  const navigate = useNavigate();
  const findBox = useRef<HTMLInputElement>(null);
  const strip = useRef<HTMLDivElement>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  // the rows on screen and the neighbouring tracks, for the keyboard handler (set on each render once the track has loaded)
  const shownRef = useRef<{ id: string; ready: boolean }[]>([]);
  const nearRef = useRef<{ prev?: TrackSummary; next?: TrackSummary }>({});
  const tabRef = useRef<Tab>(tab);
  tabRef.current = tab;
  const curRef = useRef(-1);
  curRef.current = cur;
  const t = q.data;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null; // the element the key went to, not the focus now: an input's own Escape blurs it first
      const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
      const rows = shownRef.current;
      if (e.key === "1") setTab("learn");
      else if (e.key === "2") setTab("problems");
      else if (e.key === "[" && nearRef.current.prev) void navigate({ to: "/t/$track", params: { track: nearRef.current.prev.slug } });
      else if (e.key === "]" && nearRef.current.next) void navigate({ to: "/t/$track", params: { track: nearRef.current.next.slug } });
      else if (e.key === "Escape") void navigate({ to: "/rust" });
      else if (tabRef.current !== "problems") return;
      else if (e.key === "/") {
        e.preventDefault();
        findBox.current?.focus();
        findBox.current?.select();
      } else if (e.key === "j" || e.key === "k") {
        if (rows.length === 0) return;
        const next = Math.max(0, Math.min(rows.length - 1, curRef.current + (e.key === "j" ? 1 : -1)));
        setCur(next);
        tableRef.current?.querySelector<HTMLElement>(`[data-row="${next}"]`)?.scrollIntoView({ block: "nearest" });
      } else if (e.key === "Enter") {
        const r = rows[curRef.current];
        if (r?.ready) void navigate({ to: "/p/$id", params: { id: r.id } });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);

  // The track's problems, in full, for the Learn tab: what each teaches and what an interviewer asks next.
  const readyIds = (t?.problems ?? []).filter((p) => p.status === "ready").map((p) => p.id);
  const details = useQueries({
    queries: readyIds.map((id) => ({ queryKey: ["problem", id, "brief"], queryFn: () => api.problem(id), staleTime: 5 * 60_000, enabled: tab === "learn" })),
  });
  const brief = useMemo(() => new Map(details.flatMap((d) => (d.data ? [[d.data.id, d.data] as const] : []))), [details.map((d) => d.dataUpdatedAt).join(",")]);

  // The tracks of this area that are written, in the recommended order: the strip across the top.
  const written = useMemo(() => {
    const sec = t?.section;
    const area = sec ? (sec === "D" ? ["D"] : ["L", "S", "C", "Y", "F"]) : [];
    const order = (c: string) => PLANNED.findIndex((x) => x.code === c);
    return (all.data ?? []).filter((x) => area.includes(x.section) && x.ready > 0).sort((a, b) => order(a.code) - order(b.code));
  }, [all.data, t?.section]);
  const at = written.findIndex((x) => x.slug === slug);
  const prevTrack = written.length > 1 && at >= 0 ? written[(at + written.length - 1) % written.length] : undefined;
  const nextTrack = written.length > 1 && at >= 0 ? written[(at + 1) % written.length] : undefined;
  nearRef.current = { prev: prevTrack, next: nextTrack };
  useEffect(() => {
    const el = strip.current?.querySelector<HTMLElement>(".pchip.on");
    if (el && strip.current) strip.current.scrollLeft = el.offsetLeft - strip.current.clientWidth / 2 + el.offsetWidth / 2;
  }, [at]);

  if (!t) {
    return (
      <>
        <Header />
        <main className="page">
          <div className="wrap">
            {q.isError ? (
              <p className="notice bad">{`Couldn't load track ${slug}.`}</p>
            ) : (
              <Skeleton lines={6} />
            )}
          </div>
        </main>
      </>
    );
  }

  // The stage you're on: the first with something written and unsolved; otherwise the first unfinished one.
  const workable = t.stages.findIndex((s) => s.ready > 0 && s.solved < s.total);
  const current = workable >= 0 ? workable : t.stages.findIndex((s) => s.solved < s.total);
  const nextUp = t.problems.find((p) => p.status === "ready" && p.progress !== "solved" && p.progress !== "assisted");
  const faang = new Set(t.company_groups.find((g) => g.name === "FAANG")?.companies ?? []);
  const needle = find.trim().toLowerCase();
  const matches = (p: ProblemSummary) => !needle || `${p.title} ${p.tags.join(" ")} ${p.companies.join(" ")}`.toLowerCase().includes(needle);
  const tests: Record<SetKey, (p: ProblemSummary) => boolean> = {
    stages: (p) => !f.stages.size || f.stages.has(p.stage),
    levels: (p) => !f.levels.size || f.levels.has(p.level),
    modes: (p) => !f.modes.size || f.modes.has(p.mode),
    status: (p) => !f.status.size || f.status.has(statusKey(p)),
    tags: (p) => !f.tags.size || p.tags.some((x) => f.tags.has(x)),
    companies: (p) => !f.companies.size || p.companies.some((x) => f.companies.has(x)),
  };
  /** The problems that pass every filter except one, so each group's counts say what choosing it would leave. */
  const passing = (except: SetKey | "") => t.problems.filter((p) => matches(p) && (Object.keys(tests) as SetKey[]).every((k) => k === except || tests[k](p)));
  const filtered = passing("");
  const rows = sort
    ? [...filtered].sort((a, b) => {
        const key = { n: (p: ProblemSummary) => p.order, t: (p: ProblemSummary) => p.title.toLowerCase(), m: (p: ProblemSummary) => p.mode as string, l: (p: ProblemSummary) => BAND_RANK[p.level], s: (p: ProblemSummary) => (p.status === "draft" ? 9 : PROGRESS_RANK[p.progress]) }[sort.k];
        const x = key(a);
        const y = key(b);
        return (x < y ? -1 : x > y ? 1 : a.order - b.order) * sort.dir;
      })
    : filtered;
  shownRef.current = rows.map((p) => ({ id: p.id, ready: p.status === "ready" }));
  const active = (Object.keys(f) as SetKey[]).reduce((n, k) => n + f[k].size, 0);
  const flip = (k: SetKey, v: string) => {
    setF((cur0) => {
      const next = new Set(cur0[k]);
      if (next.has(v)) next.delete(v);
      else next.add(v);
      return { ...cur0, [k]: next };
    });
    setCur(-1);
  };
  const clearAll = () => {
    setF(fresh());
    setFind("");
    setCur(-1);
  };
  const toProblems = (stage?: string) => {
    setF({ ...fresh(), stages: new Set(stage ? [stage] : []) });
    setTab("problems");
  };

  const sc = count(passing("stages"), (p) => p.stage);
  const lc = count(passing("levels"), (p) => p.level);
  const mc = count(passing("modes"), (p) => p.mode);
  const stc = count(passing("status"), statusKey);
  const tc = count(passing("tags"), (p) => p.tags);
  const cc = count(passing("companies"), (p) => p.companies);
  const tagList = [...tc.entries()].sort((a, b) => b[1] - a[1]).slice(0, 14).map(([x]) => x);
  const coList = [...cc.entries()].sort((a, b) => (faang.has(b[0]) ? 1 : 0) - (faang.has(a[0]) ? 1 : 0) || b[1] - a[1]).slice(0, 12).map(([x]) => x);

  const minutesLeft = t.problems.filter((p) => p.progress !== "solved" && p.progress !== "assisted").reduce((n, p) => n + MINUTES[p.level], 0);
  const hours = minutesLeft >= 600 ? `~${Math.round(minutesLeft / 60)}h` : `~${(minutesLeft / 60).toFixed(1).replace(/\.0$/, "")}h`;
  const due = (reviews.data?.queue ?? []).filter((r) => r.track === t.code && r.days_overdue >= 0);
  const area = sectionOf(t.code) === "D" ? "dsa" : "rust";

  const tiles = (
    <>
      <RingTile p={t.total ? (t.solved / t.total) * 100 : 0} center={`${t.solved}/${t.total}`} em="Solved" strong={t.total > 0 && t.solved === t.total ? "Track done" : `${t.total - t.solved} to go`} small={`${t.ready} written`} title="Problems solved in this track" />
      <Tile em="Stages" strong={t.stages.length} small={current >= 0 ? `now on ${t.stages[current]?.name}` : "every stage solved"} />
      <Tile em="To finish" strong={hours} small="at your pace" hint="An estimate of the time left: 12 minutes for each unsolved easy problem, 25 for a medium, 45 for a hard." />
      <Tile em="Re-solves due" strong={due.length} hot={due.length > 0} color="var(--vio)" to={{ to: "/progress", hash: "reviews" }} small={due.length > 0 ? <><span className="ly-dotp" aria-hidden="true" />Re-solve now →</> : "nothing due here"} />
    </>
  );

  const left =
    tab === "problems" ? (
      <>
        <Panel title="Filters" aside={active > 0 ? <button className="d-clearall" onClick={clearAll}>clear {active}</button> : undefined} id="d-filters">
          <div className="d-rail-in">
            <div className="rbox d-sum">
              <div className="big">
                <b>{rows.length}</b>
                <span>of {t.problems.length} in {t.name}</span>
              </div>
              <div className="bar">
                <i style={{ width: `${t.problems.length ? (rows.length / t.problems.length) * 100 : 0}%` }} />
              </div>
            </div>
            <Group title="Stage">
              {t.stages.map((s) => (
                <Opt key={s.slug} label={s.name} n={sc.get(s.slug) ?? 0} on={f.stages.has(s.slug)} onClick={() => flip("stages", s.slug)} />
              ))}
            </Group>
            <Group title="Status">
              {(["not_started", "started", "solved", "draft"] as const).map((k) => (stc.get(k) || f.status.has(k) ? <Opt key={k} label={STATUS_NAME[k]!} n={stc.get(k) ?? 0} on={f.status.has(k)} onClick={() => flip("status", k)} /> : null))}
            </Group>
            <Group title="Level">
              {BANDS.map((b) => (lc.get(b) || f.levels.has(b) ? <Opt key={b} label={b} n={lc.get(b) ?? 0} on={f.levels.has(b)} onClick={() => flip("levels", b)} /> : null))}
            </Group>
            <Group title="Mode">
              {(["write", "fix"] as const).map((m) => (mc.get(m) || f.modes.has(m) ? <Opt key={m} label={m === "fix" ? "fix this" : "write it"} n={mc.get(m) ?? 0} on={f.modes.has(m)} onClick={() => flip("modes", m)} /> : null))}
            </Group>
            {tagList.length > 0 && (
              <Group title="Topics">
                {[...new Set([...f.tags, ...tagList])].map((x) => (
                  <Opt key={x} label={x} n={tc.get(x) ?? 0} on={f.tags.has(x)} onClick={() => flip("tags", x)} />
                ))}
              </Group>
            )}
            {coList.length > 0 && (
              <Group title="Asked at">
                {[...new Set([...f.companies, ...coList])].map((x) => (
                  <Opt key={x} label={x} n={cc.get(x) ?? 0} on={f.companies.has(x)} onClick={() => flip("companies", x)} />
                ))}
                <p className="s-none">Company tags are approximate, from reported questions.</p>
              </Group>
            )}
          </div>
        </Panel>
      </>
    ) : (
      <Panel title="Stages">
        <div className="rt-stages">
          {t.stages.map((s, i) => (
            <button key={s.slug} className="rt-st" onClick={() => toProblems(s.slug)}>
              <span>{pad2(i + 1)}</span>
              <b>{s.name}</b>
              <Level level={s.band} />
              <small>{s.solved}/{s.total}</small>
            </button>
          ))}
        </div>
      </Panel>
    );

  const steps: PathStep[] = t.stages.map((s, i) => ({
    key: s.slug,
    title: (
      <>
        {s.name} <Level level={s.band} />
      </>
    ),
    text: s.total === 0 ? "nothing here yet" : `${s.solved} of ${s.total} solved · ${s.ready} written`,
    state: s.total > 0 && s.solved === s.total ? "done" : i === current ? "cur" : s.solved > 0 ? "half" : "",
    onClick: () => toProblems(s.slug),
  }));

  const learn = (
    <div className="rt-learn">
      <section className="s-card">
        <h5>THE PATH</h5>
        <p className="rt-lead" dangerouslySetInnerHTML={{ __html: marked.parseInline(t.summary || BLURB[t.code] || "", { async: false }) as string }} />
        <PhaseBar stages={t.stages} />
        <ReadingPath steps={steps} />
      </section>
      {t.stages.map((s, si) => {
        const ps = t.problems.filter((p) => p.stage === s.slug);
        if (!ps.length) return null;
        return (
          <section key={s.slug} className="rt-stage">
            <div className="rt-sh">
              <span className="rt-sn">{pad2(si + 1)}</span>
              <h3>{s.name}</h3>
              <PhaseTag stage={s} />
              <Level level={s.band} />
              <small>{s.solved}/{s.total} solved</small>
              <button className="s-ghost" onClick={() => toProblems(s.slug)}>list ›</button>
            </div>
            {ps.map((p) => {
              const d = brief.get(p.id);
              const loading = p.status === "ready" && !d && details.some((x) => x.isPending);
              return (
                <article key={p.id} className={`rt-brief${p.progress === "started" ? " cur" : ""}`}>
                  <div className="rt-bh">
                    <StatusIcon progress={p.progress} draft={p.status === "draft"} />
                    <span className="rt-no">{pad2(p.order)}</span>
                    {p.status === "ready" ? (
                      <Link to="/p/$id" params={{ id: p.id }}>{p.title}</Link>
                    ) : (
                      <span className="rt-draft">{p.title}</span>
                    )}
                    <ModeTag mode={p.mode} />
                    <Level level={p.level} />
                    <small>{p.status === "draft" ? "not written" : progressLabel(p.progress)}</small>
                  </div>
                  {loading && <div className="rt-ph" aria-hidden="true" />}
                  {d && <p className="bk-dek" dangerouslySetInnerHTML={{ __html: marked.parseInline(firstSentence(d.statement), { async: false }) as string }} />}
                  {d && d.teaches.length > 0 && <Teaches items={d.teaches} />}
                  {d?.follow_up && (
                    <Fold title="Follow-up questions" n={d.follow_up.split(/(?:^|\s)\d+\)\s/).filter(Boolean).length}>
                      <FollowUps text={d.follow_up} label={d.mode === "fix" ? "ASKED AS" : "INTERVIEW FOLLOW-UP"} compact />
                    </Fold>
                  )}
                  {d && d.runs.length > 0 && <TestResults run={d.runs[d.runs.length - 1]!} />}
                  {d && d.related.length > 0 && (
                    <div className="rt-rel">
                      related:{" "}
                      {d.related.map((c) => {
                        const trk = (all.data ?? []).find((x) => x.code === c);
                        return trk && trk.ready > 0 ? (
                          <Link key={c} to="/t/$track" params={{ track: trk.slug }}>{c} {trk.name}</Link>
                        ) : (
                          <span key={c}>{c}</span>
                        );
                      })}
                    </div>
                  )}
                </article>
              );
            })}
          </section>
        );
      })}
    </div>
  );

  const cols = "30px 38px minmax(0,1fr) auto auto 112px";
  const list = (
    <>
      <div className="fbar">
        <label className="fbox">
          <span aria-hidden="true">⌕</span>
          <input ref={findBox} value={find} onChange={(e) => (setFind(e.target.value), setCur(-1))} placeholder="filter problems, tags, companies…" autoComplete="off" spellCheck={false} onKeyDown={(e) => e.key === "Escape" && e.currentTarget.blur()} aria-label="Filter problems" />
          <kbd>/</kbd>
        </label>
        <span className="fcount">
          {rows.length} of {t.problems.length}
        </span>
        <select className="sortsel" value={sort ? `${sort.k}${sort.dir}` : ""} onChange={(e) => setSort(e.target.value ? { k: e.target.value[0] as SortKey, dir: e.target.value[1] === "-" ? -1 : 1 } : null)} aria-label="Sort problems">
          <option value="">Sort: recommended order</option>
          {(Object.keys(SORT_LABELS) as SortKey[]).flatMap((k) => [
            <option key={k + "1"} value={`${k}1`}>Sort: {SORT_LABELS[k]} ↑</option>,
            <option key={k + "-"} value={`${k}-1`}>Sort: {SORT_LABELS[k]} ↓</option>,
          ])}
        </select>
        {sort && (
          <button className="sreset" onClick={() => setSort(null)}>
            RESET ORDER
          </button>
        )}
      </div>
      <div className="rt-list" ref={tableRef}>
        {rows.map((p, ri) => {
          const cells = (
            <>
              <StatusIcon progress={p.progress} draft={p.status === "draft"} />
              <span className="num">{pad2(p.order)}</span>
              <span className="rt-t">
                <b className="pt"><Hl text={p.title} find={needle} /></b>
                <small><Hl text={[...p.tags.slice(0, 3), ...p.companies.slice(0, 2)].join(" · ")} find={needle} /></small>
              </span>
              <ModeTag mode={p.mode} />
              <span className="lvl"><Level level={p.level} /></span>
              <span className="best stat" style={{ color: p.status === "draft" ? "var(--dim)" : p.progress === "not_started" ? "var(--mut)" : p.progress === "started" ? "var(--warn)" : "var(--grn)" }}>
                <StatusIcon progress={p.progress} draft={p.status === "draft"} />
                {p.status === "draft" ? "not written" : progressLabel(p.progress)}
              </span>
            </>
          );
          return p.status === "ready" ? (
            <Link key={p.id} className={`tr rt-row${p.progress === "started" ? " cur" : ""}${ri === cur ? " kb" : ""}`} data-row={ri} to="/p/$id" params={{ id: p.id }} style={{ gridTemplateColumns: cols }}>
              {cells}
            </Link>
          ) : (
            <div key={p.id} className={`tr rt-row tr-draft${ri === cur ? " kb" : ""}`} data-row={ri} style={{ gridTemplateColumns: cols }}>
              {cells}
            </div>
          );
        })}
        {rows.length === 0 && (
          <div className="tr tr-empty">
            No problem matches that filter.{" "}
            <button className="sreset" onClick={clearAll}>
              CLEAR IT
            </button>
          </div>
        )}
      </div>
    </>
  );

  const progress = (c: TrackSummary) => (c.total ? Math.round((c.solved / c.total) * 100) : 0);
  const linkTo = (c: TrackSummary) => ({ to: "/t/$track" as const, params: { track: c.slug } });
  const tabs: [Tab, string, string][] = [
    ["learn", "Learn", `${t.stages.length} stage${t.stages.length === 1 ? "" : "s"}`],
    ["problems", "Problems", `${t.solved}/${t.total}`],
  ];
  const bar = (
    <div className="ps-bar">
      <div className="ps-tabs" role="tablist">
        {tabs.map(([k, label, n], i) => (
          <button key={k} role="tab" aria-selected={k === tab} className={k === tab ? "on" : ""} onClick={() => setTab(k)}>
            {label}<small>{n}</small><kbd>{i + 1}</kbd>
          </button>
        ))}
      </div>
      <div className="ps-acts">
        {nextUp && (
          <Link className="ps-next" to="/p/$id" params={{ id: nextUp.id }} title={`Next unsolved problem in ${t.name}`}>
            <em>{nextUp.progress === "started" ? "Resume" : "Next"}</em> {nextUp.title}
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 8h10M9 4l4 4-4 4" /></svg>
          </Link>
        )}
        <button className="ps-reset" aria-expanded={resetOpen} onClick={() => setResetOpen(!resetOpen)} title="Forget your progress on this track">
          Reset progress
        </button>
        {written.length > 1 && (
          <span className="hintkeys">
            {prevTrack && <Link {...linkTo(prevTrack)} aria-label="Previous track" title="Previous track ([)">‹</Link>}
            <em><kbd>[</kbd> <kbd>]</kbd></em>
            {nextTrack && <Link {...linkTo(nextTrack)} aria-label="Next track" title="Next track (])">›</Link>}
          </span>
        )}
      </div>
    </div>
  );
  const center = (
    <>
      {written.length > 1 && (
        <div className="ps-strip" ref={strip} role="navigation" aria-label="Tracks">
          {prevTrack && <Link className="arr" {...linkTo(prevTrack)} title="Previous track" aria-label="Previous track">‹</Link>}
          {written.map((c, i) => (
            <Link key={c.code} className={`pchip${i === at ? " on" : ""}`} {...linkTo(c)} aria-current={i === at ? "page" : undefined}>
              <span className="ringm" style={{ "--p": `${progress(c)}%` } as CSSProperties}><span>{c.code.slice(1)}</span></span>
              {c.name}
            </Link>
          ))}
          {nextTrack && <Link className="arr" {...linkTo(nextTrack)} title="Next track" aria-label="Next track">›</Link>}
        </div>
      )}
      {bar}
      {resetOpen && <ResetTrack slug={slug} name={t.name} onDone={() => setResetOpen(false)} />}
      {tab === "learn" ? learn : list}
    </>
  );

  const nextDetail = nextUp ? brief.get(nextUp.id) : undefined;
  const right = (
    <div className="s-side">
      <div className="s-card">
        <h5>UP NEXT</h5>
        {nextUp ? (
          <>
            <Link className="rt-nx" to="/p/$id" params={{ id: nextUp.id }}>
              {nextUp.title}
            </Link>
            <div className="s-pills">
              <span className="s-pill"><Level level={nextUp.level} /></span>
              <span className="s-pill">{nextUp.mode === "fix" ? "fix this" : "write it"}</span>
              <span className="s-pill">~{MINUTES[nextUp.level]}m</span>
            </div>
            {nextDetail?.follow_up && <p className="s-none">After it works you are asked {nextDetail.follow_up.split(/(?:^|\s)\d+\)\s/).filter(Boolean).length} follow-up question(s).</p>}
          </>
        ) : (
          <p className="s-none">{t.ready > 0 ? "Everything written is solved. Re-solves keep it fresh." : "Nothing is written in this track yet."}</p>
        )}
      </div>
      <div className="s-card">
        <h5>
          RE-SOLVE DUE
          {due.length > 0 && <Link to="/progress" hash="reviews" style={{ color: "var(--vio)", float: "right" }}>{due.length} →</Link>}
        </h5>
        {due.length ? (
          <div className="s-list">
            {due.slice(0, 5).map((r) => (
              <Link key={r.problem_id} className="s-item" to="/p/$id" params={{ id: r.problem_id }}>
                <i style={{ background: "var(--vio)" }} />
                <span>{r.title}</span>
                <small>{r.days_overdue > 0 ? `${r.days_overdue}d late` : "today"}</small>
              </Link>
            ))}
          </div>
        ) : (
          <p className="s-none">Nothing due in this track.</p>
        )}
      </div>
      {written.length > 1 && (
        <div className="s-card">
          <h5>NEARBY TRACKS</h5>
          <div className="s-rd">
            {[prevTrack, nextTrack].filter((c): c is TrackSummary => !!c && c.slug !== slug).map((c) => (
              <Link key={c.code} className="rt-near" {...linkTo(c)}>
                <div className="s-l">
                  <span>{c.code} · {c.name}</span>
                  <b style={{ color: c.readiness > 0 ? pctColor(c.readiness) : "var(--dim)" }}>{Math.round(c.readiness)}%</b>
                </div>
                <div className="s-meter" style={{ margin: 0 }}>
                  <i style={{ width: `${c.readiness}%`, background: pctColor(c.readiness) }} />
                </div>
              </Link>
            ))}
          </div>
        </div>
      )}
    </div>
  );

  if (area === "rust") {
    const ready = t.problems.filter((p) => p.status === "ready");
    const color = AREA_COLOR[sectionOf(t.code)] ?? "var(--vio)";
    const kind = (m: string) => ready.filter((p) => p.mode === m).length;
    let n = 0;
    const parts = (
      <div className="rt-learn">
        <div className="sr-path">
          <PhaseBar stages={t.stages} />
          <details>
            <summary>The path, stage by stage</summary>
            <ReadingPath steps={steps} />
          </details>
        </div>
        {t.stages.map((stage) => {
          const ps = t.problems.filter((p) => p.stage === stage.slug);
          if (!ps.length) return null;
          return (
            <section key={stage.slug} className="sr-stage" id={`sr-${stage.slug}`}>
              <h3>
                <span>{stage.name}</span>
                <PhaseTag stage={stage} />
                <Level level={stage.band} />
                <small>{stage.solved} of {stage.total} solved</small>
                <button type="button" className="sr-list-btn" onClick={() => toProblems(stage.slug)}>list ›</button>
              </h3>
              <div className="sr-parts">
                {ps.map((p) => {
                  n += 1;
                  const d = brief.get(p.id);
                  const done = p.progress === "solved" || p.progress === "assisted";
                  const next = p.id === nextUp?.id;
                  const card = (
                    <>
                      <span className="no">{pad2(n)}</span>
                      <span className="body">
                        <span className="h">{p.title}</span>
                        {d && <span className="d" dangerouslySetInnerHTML={{ __html: marked.parseInline(firstSentence(d.statement), { async: false }) as string }} />}
                        <span className="rp-tags">{p.tags.slice(0, 5).map((x) => <code key={x}>{x}</code>)}</span>
                      </span>
                      <span className="state">
                        {/* "not started" is what a card is until it says otherwise */}
                        {(p.status === "draft" || next || p.progress !== "not_started") && (
                          <span className="box">{p.status === "draft" ? "Not written" : next && !done ? (p.progress === "started" ? "Resume" : "Start here") : progressLabel(p.progress)}</span>
                        )}
                        <span className="what"><ModeTag mode={p.mode} /><Level level={p.level} /></span>
                      </span>
                    </>
                  );
                  const cls = `sr-part${next ? " cur" : ""}${done ? " done" : ""}`;
                  return (
                    <article key={p.id} className="rt-brief sr-item">
                      {p.status === "ready" ? <Link className={cls} to="/p/$id" params={{ id: p.id }}>{card}</Link> : <div className={cls}>{card}</div>}
                    </article>
                  );
                })}
              </div>
            </section>
          );
        })}
      </div>
    );
    return (
      <>
        <Header area={area} />
        <MockRoot prefix="s-" className="s-rust s-3" style={{ "--s-ac": "var(--vio)", "--s-acb": "var(--vio-bg)", "--ca": "var(--vio)", "--cab": "var(--vio-bg)" } as CSSProperties}>
          <main className="page cb-page cb-sub bk-track rs-track" style={{ "--c": color } as CSSProperties}>
            <aside className="cb-side">
              <TrackRail tracks={written} slug={slug} here={t.stages} onStage={(x) => (tab === "learn" ? document.getElementById(`sr-${x}`)?.scrollIntoView({ block: "start" }) : toProblems(x))} />
            </aside>
            <div className="cb-main">
              <header className="cb-banner sr-hero">
                <div className="cb-dotline">
                  <b>Track {t.code}</b>
                  <Link to="/rust">{SECTION_NAMES[t.section]}</Link>
                  <span>{TIER[t.tier]}</span>
                  <span>{kind("write")} to write</span>
                  <span>{kind("fix")} to fix</span>
                </div>
                <h1 className="ly-h1"><Accented text={t.name} /></h1>
                {t.summary && <p dangerouslySetInnerHTML={{ __html: marked.parseInline(t.summary, { async: false }) as string }} />}
                <div className="sr-foot">
                  <div>
                    <div className="sr-phase"><span>{t.stages.length} stages</span><span>{t.solved} of {t.total} solved</span></div>
                    <span className="bk-segs" role="img" aria-label={`${t.solved} of ${t.total} solved`}>
                      {t.problems.map((p) => <b key={p.id} className={p.progress} />)}
                    </span>
                  </div>
                  <div className="sr-acts">
                    {nextUp && <Link className="cb-btn" to="/p/$id" params={{ id: nextUp.id }}>{t.solved || nextUp.progress === "started" ? "Continue" : "Start problem 1"}</Link>}
                    <Link className="sr-ghost" to="/cookbook">Cookbook</Link>
                  </div>
                </div>
              </header>
              {written.length > 1 && (
                <div className="ps-strip sr-strip" ref={strip} role="navigation" aria-label="Tracks">
                  {written.map((c, i) => (
                    <Link key={c.code} className={`pchip${i === at ? " on" : ""}`} {...linkTo(c)} aria-current={i === at ? "page" : undefined}>
                      <span className="ringm" style={{ "--p": `${progress(c)}%` } as CSSProperties}><span>{c.code.slice(1)}</span></span>
                      {c.name}
                    </Link>
                  ))}
                </div>
              )}
              {bar}
              {resetOpen && <ResetTrack slug={slug} name={t.name} onDone={() => setResetOpen(false)} />}
              {tab === "learn" ? parts : (
                <>
                  <details className="sr-filters" open={active > 0 || undefined}>
                    <summary>Filters{active > 0 ? ` · ${active} on` : ""}</summary>
                    <div className="sr-filter-grid">{left}</div>
                  </details>
                  {list}
                </>
              )}
              <div className="ly-tiles sr-tiles">{tiles}</div>
              <div className="sr-after">{right}</div>
            </div>
          </main>
        </MockRoot>
      </>
    );
  }

  return (
    <>
      <Header area={area} />
      <MockRoot prefix="s-" className={`s-${area} s-3`} style={{ "--s-ac": "var(--vio)", "--s-acb": "var(--vio-bg)", "--ca": "var(--vio)", "--cab": "var(--vio-bg)" } as CSSProperties}>
        <Page3
          crumb={[<Link to="/rust">{area === "dsa" ? "DSA" : "Rust"}</Link>, <Link to="/rust">{SECTION_NAMES[t.section]}</Link>, `${t.code} · ${TIER[t.tier]}`]}
          className="bk bk-track"
          title={<Accented text={t.name} />}
          sub={
            <>
              {t.summary && <span dangerouslySetInnerHTML={{ __html: marked.parseInline(t.summary, { async: false }) as string }} />}
              {/* one segment a problem: the colour is its real state */}
              <span className="bk-segs" role="img" aria-label={`${t.solved} of ${t.total} solved`}>
                {t.problems.map((p) => <b key={p.id} className={p.progress} />)}
              </span>
            </>
          }
          tiles={tiles}
          left={left}
          center={center}
          right={right}
          label={t.name}
        />
      </MockRoot>
    </>
  );
}
