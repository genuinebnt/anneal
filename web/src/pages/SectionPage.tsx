import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { marked } from "marked";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type Activity, type Band, type NextUp, type ReviewItem, type Section, type Tier, type TrackSummary } from "../api";
import { Header } from "../components/Header";
import { LEVEL_COLOR, pad2, pctColor } from "../components/bits";
import { MockRoot, SlidingSeg, rise, useReady } from "../components/mock";
import { Opt } from "../components/filterBits";
import { Page3, Panel, Tile } from "../components/layout";
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
    title: ["The Rust interviewers ", "actually probe."],
    lead: "Language, standard library, concurrency and systems. Each track climbs from using the API, to understanding why it behaves that way, to building a small version yourself.",
  },
};

const TIER: Record<Tier, string> = { core: "CORE", light: "LIGHT", sde3: "SDE-3" };
/** Rough minutes per problem, for "to finish" and next-up estimates. */
const MINUTES: Record<Band, number> = { easy: 12, medium: 25, hard: 45 };
const BAND_COLOR: Record<Band, string> = LEVEL_COLOR;
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

function minutesTotal(r: Row) {
  if (!r.live) return r.planned * MINUTES.medium;
  return r.live.stages.reduce((n, s) => n + s.total * MINUTES[s.band], 0);
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

const TIER_LABEL: Record<Tier, string> = TIER;

/** Five segments for how far through a track you are, as the mockup draws them. */
function Segs({ row }: { row: Row }) {
  const live = row.live;
  const filled = live && live.total ? Math.round((live.solved / live.total) * 5) : 0;
  return (
    <div className="s-segs" title={live ? `${live.solved} of ${live.total} solved` : undefined}>
      {row.state === "planned" ? null : Array.from({ length: 5 }, (_, k) => <i key={k} className={k < filled ? "s-f" : ""} style={{ "--k": k } as CSSProperties} />)}
    </div>
  );
}

function Tag({ row }: { row: Row }) {
  if (row.state === "cur") return <span className="s-tag s-now">NOW ON</span>;
  if (row.state === "done") return <span className="s-tag" style={{ color: "var(--grn)", background: "var(--grn-bg)", borderColor: "transparent" }}>DONE</span>;
  if (row.state === "planned") return <span className="s-tag">PLANNED</span>;
  if (row.tier === "sde3") return <span className="s-tag" style={{ color: "var(--vio)", background: "var(--vio-bg)", borderColor: "transparent" }}>SDE-3</span>;
  return null;
}

function Foot({ row }: { row: Row }) {
  const live = row.live;
  if (row.state === "planned" || !live) return <span>not written yet · {row.planned} planned</span>;
  if (row.state === "done")
    return (
      <>
        <span>all {live.solved} solved · re-solves keep it fresh</span>
        <span className="s-ghost">review <span className="s-ar">›</span></span>
      </>
    );
  if (row.state === "ahead")
    return (
      <>
        <span>start fresh · {live.stages[0]?.name}</span>
        <span className="s-ghost">start <span className="s-ar">›</span></span>
      </>
    );
  const stage = live.stages.find((s) => s.solved < s.ready);
  return (
    <>
      <span>resume · {stage?.name}</span>
      <span className="s-ghost s-solid">resume <span className="s-ar">›</span></span>
    </>
  );
}

function TrackCard({ row, i, gone, blank }: { row: Row; i: number; gone: boolean; blank: boolean }) {
  const live = row.live;
  const total = live?.total ?? row.planned;
  const cls = `s-trk s-rv${row.state === "planned" ? " s-plan" : ""}${row.state === "cur" ? " s-cur" : ""}${gone ? " s-gone" : ""}`;
  const rv = rise(Math.min(7 + i, 16));
  const body = (
    <>
      <div className="s-th">
        <span className="s-no">{pad2(row.n)}</span>
        <span className="s-tk">
          TRACK / {TIER_LABEL[row.tier]} · {row.code}
        </span>
        <Tag row={row} />
      </div>
      <h4>{live?.name ?? row.name}</h4>
      <p dangerouslySetInnerHTML={{ __html: blank ? "" : marked.parseInline(live?.summary || BLURB[row.code] || "", { async: false }) }} />
      <div className="s-meta s-x">
        <b>{total}</b> problems · {hours(minutesTotal(row))} · easy → hard
      </div>
      <Segs row={row} />
      <div className="s-foot">
        <Foot row={row} />
      </div>
    </>
  );
  return live && row.state !== "planned" ? (
    <Link className={cls} style={rise(Math.min(7 + i, 16)).style} to="/t/$track" params={{ track: live.slug }}>
      {body}
    </Link>
  ) : (
    <article className={cls} style={rv.style} aria-disabled="true">
      {body}
    </article>
  );
}

const REASON: Record<NextUp["reason"], string> = {
  resume: "You started this one and haven't solved it yet.",
  current: "It's the next unsolved problem in the track you're working on.",
  start: "It's the first unsolved problem in the recommended order.",
};

function NextUpCard({ next, streak, ready }: { next: NextUp; streak: number; ready: boolean }) {
  const [mode] = MODE_LABEL[next.mode];
  const readiness = Math.round(next.readiness);
  return (
    <article className="s-card s-next s-rv" style={rise(4).style} aria-label="Next up">
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
            NEXT UP · {next.track_code} · {next.stage_name.toUpperCase()}
          </div>
          <div style={{ marginTop: 4, color: "var(--mut)", fontSize: 14 }}>
            {streak > 0 ? (
              <>
                <b style={{ color: "var(--fg)" }}>{streak}-day streak</b>, keep it going.
              </>
            ) : (
              <>
                <b style={{ color: "var(--fg)" }}>No streak yet.</b> Solve one today to start it.
              </>
            )}{" "}
            {REASON[next.reason]}
          </div>
        </div>
      </div>
      <h3>{next.title}</h3>
      {next.excerpt && <p dangerouslySetInnerHTML={{ __html: marked.parseInline(next.excerpt, { async: false }) }} />}
      <div className="s-pills">
        <span className={`s-pill${next.level === "easy" ? " s-g" : ""}`} style={next.level === "easy" ? undefined : { color: BAND_COLOR[next.level] }}>
          {next.level}
        </span>
        <span className="s-pill">{mode}</span>
        <span className="s-pill">{next.track_name}</span>
        <span className="s-pill">~{MINUTES[next.level]}m</span>
      </div>
      <div className="s-ml">
        <span>
          {next.track_code} READINESS · {readiness}%
        </span>
        <b>+{next.gain}% IF UNASSISTED</b>
      </div>
      <div className="s-meter">
        <i style={{ width: ready ? `${readiness}%` : 0 }} />
      </div>
      <Link className="s-cta" to="/p/$id" params={{ id: next.problem_id }}>
        Solve in the workspace <span className="s-ar">→</span>
      </Link>
    </article>
  );
}

const OUTCOME_COLOR = { solved: "var(--grn)", assisted: "var(--acc)", failing: "var(--bad)", started: "var(--dim)" } as const;

function Side({ activity, sections, live, due, ready }: { activity?: Activity; sections: readonly Section[]; live: TrackSummary[]; due: ReviewItem[]; ready: boolean }) {
  const shade = (n: number) => (n === 0 ? "var(--raise)" : `color-mix(in oklab, var(--grn) ${Math.min(100, 25 + n * 20)}%, var(--raise))`);
  const readiness = sections.map((s) => {
    const ts = live.filter((t) => t.section === s && t.ready > 0);
    const w = (t: TrackSummary) => (t.tier === "core" ? 2 : 1);
    const total = ts.reduce((n, t) => n + w(t), 0);
    return { s, pct: total ? Math.round(ts.reduce((n, t) => n + t.readiness * w(t), 0) / total) : null };
  });
  return (
    <div className="s-side" aria-label="Activity">
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
        <h5>RECENT</h5>
        {activity?.recent.length ? (
          <div className="s-list">
            {activity.recent.map((r) => (
              <Link key={r.problem_id + r.at} className="s-item" to="/p/$id" params={{ id: r.problem_id }}>
                <i style={{ background: OUTCOME_COLOR[r.outcome] }} />
                <span>{r.title}</span>
                <small>
                  {r.track} · {r.detail}
                </small>
              </Link>
            ))}
          </div>
        ) : (
          <p className="s-none">Nothing yet. Your attempts show up here.</p>
        )}
      </div>
      {due.length > 0 && (
        <div className="s-card s-rv" style={rise(6).style}>
          <h5>
            RE-SOLVE DUE
            <Link to="/progress" hash="reviews" style={{ color: "var(--vio)", float: "right" }}>
              {due.length} →
            </Link>
          </h5>
          <div className="s-list">
            {due.slice(0, 4).map((r) => (
              <Link key={r.problem_id} className="s-item" to="/progress" hash="reviews">
                <i style={{ background: "var(--vio)" }} />
                <span>{r.title}</span>
                <small>
                  {r.track} · {r.days_overdue > 0 ? `${r.days_overdue}d late` : "today"}
                </small>
              </Link>
            ))}
          </div>
        </div>
      )}
      <div className="s-card s-rv" style={rise(7).style}>
        <h5>READINESS</h5>
        <div className="s-rd">
          {readiness.map(({ s, pct }) => (
            <div key={s}>
              <div className="s-l">
                <span>{SECTION_NAMES[s]}</span>
                <b style={{ color: pct === null ? "var(--dim)" : pctColor(pct) }}>{pct === null ? "—" : `${pct}%`}</b>
              </div>
              <div className="s-meter" style={{ margin: 0 }}>
                <i style={{ width: ready ? `${pct ?? 0}%` : 0, background: pct === null ? "transparent" : pctColor(pct) }} />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
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
  let index = 0;

  const groups = (sections.length > 1 ? sections : [null]).map((s) => {
    const all = rows.filter((r) => !s || sectionOf(r.code) === s);
    const count = all.filter(shown).length;
    if (!all.length) return null;
    return (
      <div key={s ?? "all"} style={{ display: count ? undefined : "none" }}>
        <div className="s-flab" style={{ marginTop: 28 }}>
          {s ? `${SECTION_NAMES[s].toUpperCase()} · ${count}` : `ALL TRACKS · IN RECOMMENDED ORDER · ${count}`}
        </div>
        <div className={`s-grid${view === "list" ? " s-list" : ""}`} onPointerMove={pointerLight}>
          {all.map((r) => (
            <TrackCard key={r.code} row={r} i={index++} gone={!shown(r)} blank={false} />
          ))}
        </div>
      </div>
    );
  });

  const sectionRows = (sections.length > 1 ? sections : []).map((sec) => [sec, rows.filter((r) => sectionOf(r.code) === sec)] as const);
  const tiles = (
    <>
      <Tile em="Tracks" strong={<CountUp value={rows.length} />} small={`${rows.filter((r) => r.live && r.live.ready > 0).length} written`} />
      <Tile em="Problems" strong={<CountUp value={problems} />} small={`${rows.reduce((n, r) => n + (r.live?.ready ?? 0), 0)} ready to solve`} />
      <Tile em="To finish" strong={<CountUp value={left} format={hours} />} small="at your pace" hint="An estimate of the time left: 12 minutes for each unsolved easy problem, 25 for a medium, 45 for a hard." />
      <Tile em="Re-solves due" strong={due.length} hot={due.length > 0} color="var(--vio)" to={{ to: "/progress", hash: "reviews" }} small={due.length > 0 ? <><span className="ly-dotp" aria-hidden="true" />Re-solve now →</> : "nothing due"} />
    </>
  );
  const clear = () => {
    setQ("");
    setFilter("all");
    setSec("all");
  };
  const active = (filter !== "all" ? 1 : 0) + (sec !== "all" ? 1 : 0);
  const leftPanel = (
    <>
      <Panel title="Status" aside={active > 0 ? <button className="d-clearall" onClick={clear}>clear {active}</button> : undefined}>
        <div className="d-rail-in">
          <div className="rbox">
            {FILTERS.map(([key, label, fn]) => {
              const n = rows.filter(fn).length;
              if (!n && key !== "all") return null;
              return <Opt key={key} label={label} n={n} on={filter === key} onClick={() => setFilter(key)} />;
            })}
          </div>
        </div>
      </Panel>
      {sectionRows.length > 0 && (
        <Panel title="Area">
          <div className="d-rail-in">
            <div className="rbox">
              <Opt label="All areas" n={rows.length} on={sec === "all"} onClick={() => setSec("all")} />
              {sectionRows.map(([s, rs]) => (
                <Opt key={s} label={SECTION_NAMES[s]} n={rs.length} on={sec === s} onClick={() => setSec(sec === s ? "all" : s)} />
              ))}
            </div>
          </div>
        </Panel>
      )}
      <Panel title="Keys">
        <p className="s-none"><kbd>/</kbd> search the tracks. Open a track, then <kbd>1</kbd> <kbd>2</kbd> <kbd>3</kbd> switch its tabs and <kbd>[</kbd> <kbd>]</kbd> step through the tracks.</p>
      </Panel>
    </>
  );
  const center = (
    <>
      {tracks.isError && <p className="notice bad">Couldn't reach the API. Is `cargo run -p anneal-api` running?</p>}
      {activity.data?.next && <NextUpCard next={activity.data.next} streak={activity.data.streak} ready={ready} />}
      <div className="s-tools">
        <label className="s-search">
          <span>⌕</span>
          <input ref={search} type="search" placeholder="search tracks, stages, topics…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search tracks" autoComplete="off" spellCheck={false} />
          <kbd>/</kbd>
        </label>
        <SlidingSeg
          label="View"
          value={view}
          onChange={setView}
          options={(["grid", "list"] as const).map((v) => ({ key: v, label: v === "grid" ? "▦ grid" : "≡ list" }))}
        />
      </div>
      {loading ? (
        <div className="s-grid" style={{ marginTop: 28 }}>
          {Array.from({ length: 6 }, (_, i) => (
            <article key={i} className="s-trk" style={{ minHeight: 220 }} aria-hidden="true">
              <div className="s-th">
                <span className="s-no">··</span>
              </div>
              <h4>Loading</h4>
            </article>
          ))}
        </div>
      ) : (
        groups
      )}
      <div className={`s-empty${visible === 0 && !loading ? " s-show" : ""}`}>
        No track matches that.{" "}
        <button className="s-ghost" style={{ marginLeft: 8 }} onClick={clear}>
          Clear filters
        </button>
      </div>
    </>
  );
  return (
    <>
      <Header area={area} />
      <MockRoot prefix="s-" className={`s-${area} s-3`} style={{ "--s-ac": `var(--${copy.color})`, "--s-acb": `var(--${copy.color}-bg)`, "--ca": `var(--${copy.color})`, "--cab": `var(--${copy.color}-bg)` } as CSSProperties}>
        <Page3
          crumb={[copy.eyebrow, "TRACKS"]}
          title={<>{copy.title[0]}<em className="d-acc">{copy.title[1]}</em></>}
          tiles={tiles}
          left={leftPanel}
          center={center}
          right={<Side activity={activity.data} sections={sections} live={tracks.data ?? []} due={due} ready={ready} />}
          label="Filters"
        />
      </MockRoot>
    </>
  );
}
