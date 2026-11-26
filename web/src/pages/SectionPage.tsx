import { pctColor, LEVEL_COLOR } from "../components/bits";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { marked } from "marked";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type Band, type Section, type Tier, type TrackSummary, type Activity, type NextUp, type ReviewItem } from "../api";
import { Header } from "../components/Header";
import { MockRoot, SlidingSeg, rise, useReady } from "../components/mock";
import { Tile } from "../components/layout";
import "../styles/cookbook.css";
import { BLURB, DSA_ORDER, NAV_SECTIONS, PLANNED, SECTION_NAMES, type NavArea, sectionOf } from "../curriculum";
import { CountUp } from "../components/kit";

const COPY: Record<NavArea, { eyebrow: string; color: string; title: [string, string]; lead: ReactNode }> = {
  dsa: {
    eyebrow: "DSA",
    color: "acc",
    title: ["Fourteen tracks, ", "easy to hard."],
    lead: (
      <>
        Every track climbs easy → medium → hard, and every problem is solved the way Rust wants it: index-based graphs,{" "}
        <code>Option&lt;Box&lt;Node&gt;&gt;</code> lists, heaps with <code>Reverse&lt;T&gt;</code>.
      </>
    ),
  },
  rust: {
    eyebrow: "RUST",
    color: "vio",
    title: ["Close the reference. ", "Write the code."],
    lead: "Language, standard library, concurrency and systems. Each track climbs from using the API, to understanding why it behaves that way, to building a small version yourself.",
  },
};

/** Rough minutes per problem, for "to finish" and next-up estimates. */
const MINUTES: Record<Band, number> = { easy: 12, medium: 25, hard: 45 };

const MODE_LABEL = { write: ["write it", "var(--acc)"], fix: ["fix this", "var(--vio)"], stage: ["stage", "var(--grn)"] } as const;

type State = "cur" | "open" | "ahead" | "done" | "planned";

interface Row {
  code: string;
  name: string;
  tier: Tier;
  planned: number;
  n: number;
  live?: TrackSummary;
  state: State;
}

const FILTERS: [key: string, label: string, test: (r: Row) => boolean][] = [
  ["all", "all", () => true],
  ["progress", "in progress", (r) => r.state === "cur" || r.state === "open"],
  ["new", "not started", (r) => r.state === "ahead"],
  ["done", "done", (r) => r.state === "done"],
  ["core", "core", (r) => r.tier === "core"],
  ["sde3", "SDE-3", (r) => r.tier === "sde3"],
  ["planned", "planned", (r) => r.state === "planned"],
];

function stateOf(live: TrackSummary | undefined, current: string | undefined): State {
  if (!live || live.ready === 0) return "planned";
  if (live.solved >= live.ready) return "done";
  if (live.code === current) return "cur";
  return live.solved > 0 ? "open" : "ahead";
}

function minutesLeft(r: Row) {
  if (!r.live) return r.planned * MINUTES.medium;
  return r.live.stages.reduce((n, s) => n + (s.total - s.solved) * MINUTES[s.band], 0);
}

const hours = (m: number) => (m >= 600 ? `~${Math.round(m / 60)}h` : `~${(m / 60).toFixed(1).replace(/\.0$/, "")}h`);

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



const AREA_COLOR: Record<Section, string> = { D: "var(--acc)", L: "var(--vio)", S: "var(--acc)", C: "var(--fn)", Y: "var(--grn)", F: "var(--warn)", P: "var(--acc)" };

/** A track as a book on the shelf: its code, its stages and how far you are. */
function BookCard({ row, i, gone, color }: { row: Row; i: number; gone: boolean; color: string }) {
  const live = row.live;
  const open = live && row.state !== "planned";
  const cls = `cb-book s-trk s-rv${open ? "" : " s-plan"}${row.state === "cur" ? " s-cur" : ""}${gone ? " s-gone" : ""}`;
  const style = { ...rise(Math.min(7 + i, 16)).style, "--c": color } as CSSProperties;
  const stages = live?.stages ?? [];
  const body = (
    <>
      <span className="cb-ghost" aria-hidden="true">{row.code}</span>
      <span className="rs-bh">
        <span className="rs-name">{live?.name ?? row.name}</span>
        <span className="rs-rule" aria-hidden="true"><i style={{ width: `${live && live.total ? (live.solved / live.total) * 100 : 0}%` }} /></span>
      </span>
      <span className="rs-stages">
        {stages.slice(0, 6).map((x) => <span key={x.slug}><span>{x.name}</span><small>{x.solved ? `${x.solved}/${x.total}` : x.total}</small></span>)}
      </span>
      <span className="cb-book-foot">
        <span>{live ? `${live.solved} of ${live.total} solved` : "not written yet"}</span>
        <span>{stages.length > 6 ? `all ${stages.length} stages` : "open the track"}</span>
      </span>
    </>
  );
  return open ? (
    <Link className={cls} style={style} to="/t/$track" params={{ track: live.slug }}>{body}</Link>
  ) : (
    <article className={cls} style={style} aria-disabled="true">{body}</article>
  );
}

/** The soft light that follows the pointer over a track card: its position is passed to the stylesheet. */
function pointerLight(e: React.PointerEvent<HTMLElement>) {
  const card = (e.target as HTMLElement).closest<HTMLElement>(".s-trk");
  if (!card) return;
  const r = card.getBoundingClientRect();
  card.style.setProperty("--mx", `${e.clientX - r.left}px`);
  card.style.setProperty("--my", `${e.clientY - r.top}px`);
}

const REASON: Record<NextUp["reason"], string> = {
  resume: "You started this one and haven't solved it yet.",
  current: "It's the next unsolved problem in the track you're working on.",
  start: "It's the first unsolved problem in the recommended order.",
};

const OUTCOME_COLOR = { solved: "var(--grn)", assisted: "var(--acc)", failing: "var(--bad)", started: "var(--dim)" } as const;

function Today({ due, next, doneToday }: { due: number; next?: NextUp | null; doneToday: number }) {
  const solved = doneToday > 0;
  const dueDone = due === 0;
  type Step = { key: string; label: string; detail: string; state: "done" | "now" | "wait"; to?: { to: "/progress"; hash: string } | { to: "/p/$id"; params: { id: string } } };
  const steps: Step[] = [
    { key: "rev", label: "Re-solves", detail: dueDone ? "none due" : `${due} due`, state: dueDone ? "done" : "now", to: dueDone ? undefined : { to: "/progress", hash: "reviews" } },
    {
      key: "new",
      label: "New problem",
      detail: solved ? `${doneToday} solved today` : next ? next.title : "nothing left",
      state: solved ? "done" : dueDone ? "now" : "wait",
      to: next && !solved ? { to: "/p/$id", params: { id: next.problem_id } } : undefined,
    },
    { key: "end", label: "Done for today", detail: dueDone && solved ? "all done" : "not yet", state: dueDone && solved ? "done" : "wait" },
  ];
  return (
    <nav className="d-today s-rv" aria-label="Today">
      {steps.map((s, i) => {
        const body = (
          <>
            <i className={`d-ts ${s.state}`} aria-hidden="true">{s.state === "done" ? "\u2713" : i + 1}</i>
            <span><b>{s.label}</b><small>{s.detail}</small></span>
          </>
        );
        return s.to ? (
          <Link key={s.key} className={`d-step ${s.state}`} {...(s.to as { to: "/progress" })}>{body}</Link>
        ) : (
          <div key={s.key} className={`d-step ${s.state}`}>{body}</div>
        );
      })}
    </nav>
  );
}

/** Your week, what you did last and how ready each area is: beside the next problem. */
function Pulse({ activity, sections, live, due, ready }: { activity?: Activity; sections: readonly Section[]; live: TrackSummary[]; due: ReviewItem[]; ready: boolean }) {
  const shade = (n: number) => (n === 0 ? "var(--raise)" : `color-mix(in oklab, var(--grn) ${Math.min(100, 25 + n * 20)}%, var(--raise))`);
  const readiness = sections.map((s) => {
    const ts = live.filter((t) => t.section === s && t.ready > 0);
    const w = (t: TrackSummary) => (t.tier === "core" ? 2 : 1);
    const total = ts.reduce((n, t) => n + w(t), 0);
    return { s, pct: total ? Math.round(ts.reduce((n, t) => n + t.readiness * w(t), 0) / total) : null };
  });
  return (
    <div className="rs-pulse" aria-label="Activity">
      <section className="rs-box">
        <h2>This week<small>{activity?.week_unassisted ?? 0} of {activity?.week_solved ?? 0} unassisted</small></h2>
        <div className="rs-week">
          {(activity?.week ?? []).map((d) => <i key={d.date} style={d.solved ? { background: shade(d.solved) } : undefined} title={`${d.date}: ${d.solved} solved`} />)}
        </div>
      </section>
      {due.length > 0 && (
        <section className="rs-box">
          <h2>Re-solve due<Link to="/progress" hash="reviews">all {due.length}</Link></h2>
          {due.slice(0, 3).map((r) => (
            <Link key={r.problem_id} className="rs-line" to="/progress" hash="reviews">
              <i style={{ background: "var(--vio)" }} />
              <span>{r.title}</span>
              <small>{r.track} · {r.days_overdue > 0 ? `${r.days_overdue}d late` : "today"}</small>
            </Link>
          ))}
        </section>
      )}
      <section className="rs-box">
        <h2>Recent</h2>
        {activity?.recent.length ? (
          activity.recent.slice(0, 4).map((r) => (
            <Link key={r.problem_id + r.at} className="rs-line" to="/p/$id" params={{ id: r.problem_id }}>
              <i style={{ background: OUTCOME_COLOR[r.outcome] }} />
              <span>{r.title}</span>
              <small>{r.track} · {r.detail}</small>
            </Link>
          ))
        ) : (
          <p>Nothing yet. Your attempts show up here.</p>
        )}
      </section>
      <section className="rs-box">
        <h2>Readiness</h2>
        {readiness.map(({ s, pct }) => (
          <div key={s} className="rs-ready">
            <span>{SECTION_NAMES[s]}</span>
            <b style={{ color: pct === null ? "var(--dim)" : pctColor(pct) }}>{pct === null ? "—" : `${pct}%`}</b>
            <div className="s-meter"><i style={{ width: ready ? `${pct ?? 0}%` : 0, background: pct === null ? "transparent" : pctColor(pct) }} /></div>
          </div>
        ))}
      </section>
    </div>
  );
}

/** Solved and written problems of some tracks: the goal a filter stands for. */
function goalOf(rows: Row[]) {
  const written = rows.filter((r) => r.live && r.live.ready > 0);
  return { solved: written.reduce((n, r) => n + r.live!.solved, 0), total: written.reduce((n, r) => n + r.live!.ready, 0) };
}

export function SectionPage({ area }: { area: NavArea }) {
  const copy = COPY[area];
  const sections: readonly Section[] = NAV_SECTIONS[area];
  const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
  const activity = useQuery({ queryKey: ["activity", area], queryFn: () => api.activity(sections) });
  const reviews = useQuery({ queryKey: ["reviews"], queryFn: api.reviews });
  const due = (reviews.data?.queue ?? []).filter((r) => r.days_overdue >= 0 && sections.includes(sectionOf(r.track)));
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState("all");
  const [sec, setSec] = useState("all");
  const [view, setView] = useStored<"grid" | "list">("anneal-catalog-view", "grid");
  const search = useRef<HTMLInputElement>(null);
  const ready = useReady();
  const loading = tracks.isPending;

  useEffect(() => {
    setQ("");
    setFilter("all");
    setSec("all");
  }, [area]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, [contenteditable]");
      if (e.key === "/" && !typing) {
        e.preventDefault();
        search.current?.focus();
      }
      if (e.key === "Escape") (document.activeElement as HTMLElement | null)?.blur?.();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const live = new Map((tracks.data ?? []).map((t) => [t.code, t]));
  // Only a track you've actually worked in is "now on"; a first pick is just a suggestion.
  const next = activity.data?.next;
  const current = next && next.reason !== "start" ? next.track_code : undefined;
  const order = (code: string) => (area === "dsa" ? DSA_ORDER.indexOf(code) : PLANNED.findIndex((t) => t.code === code));
  const rows: Row[] = PLANNED.filter((t) => sections.includes(sectionOf(t.code)))
    .sort((a, b) => order(a.code) - order(b.code))
    .map((t, i) => {
      const l = live.get(t.code);
      return { code: t.code, name: t.name, tier: t.tier, planned: t.planned, n: i + 1, live: l, state: stateOf(l, current) };
    });

  const needle = q.trim().toLowerCase();
  const test = FILTERS.find((f) => f[0] === filter)?.[2] ?? (() => true);
  const shown = (r: Row) =>
    test(r) &&
    (sec === "all" || sectionOf(r.code) === sec) &&
    (!needle ||
      [r.code, r.live?.name ?? r.name, r.live?.summary ?? BLURB[r.code] ?? "", ...(r.live?.stages.map((s) => s.name) ?? [])]
        .join(" ")
        .toLowerCase()
        .includes(needle));
  const problems = rows.reduce((n, r) => n + Math.max(r.planned, r.live?.total ?? 0), 0);
  const left = rows.reduce((n, r) => n + (r.state === "done" ? 0 : minutesLeft(r)), 0);
  const visible = rows.filter(shown).length;
  const solved = rows.reduce((n, r) => n + (r.live?.solved ?? 0), 0);
  const clear = () => {
    setQ("");
    setFilter("all");
    setSec("all");
  };
  const lead = activity.data?.next;
  let index = 0;
  return (
    <>
      <Header area={area} />
      <MockRoot prefix="s-" className={`s-${area} s-3`} style={{ "--s-ac": `var(--${copy.color})`, "--s-acb": `var(--${copy.color}-bg)`, "--ca": `var(--${copy.color})`, "--cab": `var(--${copy.color}-bg)` } as CSSProperties}>
        <main className="page cb-page cb-home rs-home" style={{ "--c": "var(--vio)" } as CSSProperties}>
          <div className="cb-intro">
            <div>
              <h1>
                {copy.title[0]}
                <em className="cb-rgb">{copy.title[1]}</em>
              </h1>
              <p>Solve it yourself first. When you are stuck, look it up in the Cookbook.</p>
            </div>
            <div className="ly-tiles">
              <Tile em="Tracks" strong={<CountUp value={rows.length} />} small={`${rows.filter((r) => r.live && r.live.ready > 0).length} written`} />
              <Tile em="Problems" strong={<CountUp value={problems} />} small={`${solved} solved`} />
              <Tile em="To finish" strong={<CountUp value={left} format={hours} />} small="at your pace" hint="An estimate of the time left: 12 minutes for each unsolved easy problem, 25 for a medium, 45 for a hard." />
              <Tile em="Re-solves due" strong={due.length} hot={due.length > 0} color="var(--vio)" to={{ to: "/progress", hash: "reviews" }} small={due.length > 0 ? "re-solve now" : "nothing due"} />
            </div>
          </div>
          {tracks.isError && <p className="notice bad">Couldn't reach the API. Is `cargo run -p anneal-api` running?</p>}
          <Today due={due.length} next={lead} doneToday={activity.data?.week[activity.data.week.length - 1]?.solved ?? 0} />
          <section className="cb-feature rs-feature" aria-label="Next up">
            {lead ? (
              <article className="cb-lead">
                <div className="cb-dotline">
                  <b>Next up</b>
                  <span>{lead.track_code} {lead.track_name}</span>
                  <span>{lead.stage_name}</span>
                </div>
                <div className="rs-streak">
                  <strong>{activity.data!.streak > 0 ? `${activity.data!.streak}-day streak` : "No streak yet"}</strong>
                  <span>{activity.data!.streak > 0 ? "Keep it going." : "Solve one today to start it."} {REASON[lead.reason]}</span>
                </div>
                <div>
                  <h2 className="cb-lead-t">{lead.title}</h2>
                  {lead.excerpt && <p className="cb-lead-d" dangerouslySetInnerHTML={{ __html: marked.parseInline(lead.excerpt, { async: false }) as string }} />}
                  <div className="rs-pills">
                    <span style={{ color: LEVEL_COLOR[lead.level] }}>{lead.level}</span>
                    <span>{MODE_LABEL[lead.mode][0]}</span>
                    <span>~{MINUTES[lead.level]}m</span>
                  </div>
                  <div className="rs-ml">
                    <span>{lead.track_code} readiness {Math.round(lead.readiness)}%</span>
                    <b>+{lead.gain}% if unassisted</b>
                  </div>
                  <div className="s-meter"><i style={{ width: ready ? `${Math.round(lead.readiness)}%` : 0 }} /></div>
                  <div className="rs-acts">
                    <Link to="/p/$id" params={{ id: lead.problem_id }} className="cb-btn">Solve it</Link>
                    <Link to="/cookbook" className="sr-ghost">Cookbook</Link>
                  </div>
                </div>
              </article>
            ) : (
              <article className="cb-lead"><p className="cb-lead-d">{activity.isPending ? "Loading your next problem…" : "Nothing is waiting: every written problem is solved."}</p></article>
            )}
            <Pulse activity={activity.data} sections={sections} live={tracks.data ?? []} due={due} ready={ready} />
          </section>
          <div className="cb-kbar">
            <label className="cb-search">
              <span aria-hidden="true">⌕</span>
              <input ref={search} type="search" placeholder="Find a track, a stage or a topic…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search tracks" autoComplete="off" spellCheck={false} />
              <kbd>/</kbd>
            </label>
            <SlidingSeg label="View" value={view} onChange={setView} options={(["grid", "list"] as const).map((v) => ({ key: v, label: v === "grid" ? "▦ grid" : "≡ list" }))} />
            <div className="cb-chips rs-areas" role="group" aria-label="Area">
              <button type="button" className={sec === "all" ? "on" : ""} aria-pressed={sec === "all"} onClick={() => setSec("all")}>All areas<small>{rows.length}</small></button>
              {sections.map((x) => (
                <button key={x} type="button" className={sec === x ? "on" : ""} aria-pressed={sec === x} onClick={() => setSec(sec === x ? "all" : x)} style={{ "--c": AREA_COLOR[x] } as CSSProperties}>
                  {SECTION_NAMES[x]}<small>{rows.filter((r) => sectionOf(r.code) === x).length}</small>
                </button>
              ))}
            </div>
            <div className="cb-chips rs-status" role="group" aria-label="Status">
              {FILTERS.map(([key, label, fn]) => {
                const n = rows.filter(fn).length;
                if (!n && key !== "all") return null;
                const goal = key === "all" || key === "core" || key === "sde3" ? goalOf(rows.filter(fn)) : undefined;
                return (
                  <button key={key} type="button" className={filter === key ? "on" : ""} aria-pressed={filter === key} onClick={() => setFilter(key)}>
                    {label}
                    <small>{goal ? `${goal.solved}/${goal.total} solved` : n}</small>
                  </button>
                );
              })}
            </div>
          </div>
          {loading && <div className="cb-skel" aria-label="Loading"><i /><i /><i /></div>}
          {sections.map((x) => {
            const all = rows.filter((r) => sectionOf(r.code) === x);
            const count = all.filter(shown).length;
            if (!all.length) return null;
            return (
              <div key={x} style={{ display: count ? undefined : "none" }}>
                <h2 className="cb-secth">{SECTION_NAMES[x]}<small>{count} track{count === 1 ? "" : "s"}</small></h2>
                <div className={`cb-books s-grid${view === "list" ? " s-list" : ""}`} onPointerMove={pointerLight}>
                  {all.map((r) => <BookCard key={r.code} row={r} i={index++} gone={!shown(r)} color={AREA_COLOR[x]} />)}
                </div>
              </div>
            );
          })}
          <div className={`s-empty${visible === 0 && !loading ? " s-show" : ""}`}>
            No track matches that.{" "}
            <button className="s-ghost" style={{ marginLeft: 8 }} onClick={clear}>
              Clear filters
            </button>
          </div>
        </main>
      </MockRoot>
    </>
  );
}
