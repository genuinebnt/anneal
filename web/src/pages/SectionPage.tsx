import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { marked } from "marked";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type Activity, type Band, type NextUp, type ReviewItem, type Section, type Tier, type TrackSummary } from "../api";
import { Header } from "../components/Header";
import { LEVEL_COLOR, pad2, pctColor } from "../components/bits";
import { MockRoot, rise, useReady } from "../components/mock";
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
    <aside className="s-side" aria-label="Activity">
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
    </aside>
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
  const [view, setView] = useStored<"grid" | "list">("anneal-catalog-view", "grid");
  const search = useRef<HTMLInputElement>(null);
  const ready = useReady();
  const seg = useRef<HTMLDivElement>(null);
  const loading = tracks.isPending;

  useEffect(() => {
    setQ("");
    setFilter("all");
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
  // The thumb under the grid / list toggle slides to the one that is on.
  const placeThumb = () => {
    const box = seg.current;
    const on = box?.querySelector<HTMLElement>("button.s-on");
    const t = box?.querySelector<HTMLElement>(".s-thumb");
    if (!on || !t) return;
    t.style.width = `${on.offsetWidth}px`;
    t.style.transform = `translateX(${on.offsetLeft - 3}px)`;
  };
  useLayoutEffect(placeThumb);
  useEffect(() => {
    addEventListener("resize", placeThumb);
    return () => removeEventListener("resize", placeThumb);
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

  return (
    <>
      <Header area={area} />
      <MockRoot prefix="s-" className={`s-${area}`} style={{ "--s-ac": `var(--${copy.color})`, "--s-acb": `var(--${copy.color}-bg)`, "--ca": `var(--${copy.color})`, "--cab": `var(--${copy.color}-bg)` } as CSSProperties}>
        <main className={`s-wrap${loading ? " s-sk" : ""}`} id="page">
          <section className="s-hero">
            <div>
              <div className="s-eye s-rv" style={rise(0).style}>
                <b>{copy.eyebrow}</b> / TRACKS
              </div>
              <h1 className="s-rv" style={rise(1).style}>
                {copy.title[0]}
                <em>{copy.title[1]}</em>
              </h1>
            </div>
            <div className="s-stats s-rv" style={rise(3).style}>
              <div>
                <b>
                  <CountUp value={rows.length} />
                </b>
                <span>TRACKS</span>
              </div>
              <div>
                <b>
                  <CountUp value={problems} />
                </b>
                <span>PROBLEMS</span>
              </div>
              <div>
                <b>
                  <CountUp value={left} format={hours} />
                </b>
                <span>TO FINISH</span>
              </div>
            </div>
          </section>
          {tracks.isError && <p className="notice bad">Couldn't reach the API. Is `cargo run -p anneal-api` running?</p>}
          <div className="s-main">
            <div>
              {activity.data?.next && <NextUpCard next={activity.data.next} streak={activity.data.streak} ready={ready} />}
              <div className="s-tools s-rv" style={rise(5).style}>
                <label className="s-search">
                  <span>⌕</span>
                  <input ref={search} type="search" placeholder="search tracks, stages, topics…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search tracks" autoComplete="off" spellCheck={false} />
                  <kbd>/</kbd>
                </label>
                <div className="s-seg" ref={seg} role="group" aria-label="View">
                  <span className="s-thumb" />
                  {(["grid", "list"] as const).map((v) => (
                    <button key={v} className={view === v ? "s-on" : ""} aria-pressed={view === v} onClick={() => setView(v)}>
                      {v === "grid" ? "▦ grid" : "≡ list"}
                    </button>
                  ))}
                </div>
              </div>
              <div className="s-flab s-rv" style={rise(6).style}>
                FILTER BY
              </div>
              <div className="s-chips s-rv" style={rise(6).style}>
                {FILTERS.map(([key, label, fn]) => {
                  const n = rows.filter(fn).length;
                  if (!n && key !== "all") return null;
                  return (
                    <button key={key} className={`s-chip${filter === key ? " s-on" : ""}`} aria-pressed={filter === key} onClick={() => setFilter(key)}>
                      {label} <small>{n}</small>
                    </button>
                  );
                })}
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
                <button
                  className="s-ghost"
                  style={{ marginLeft: 8 }}
                  onClick={() => {
                    setQ("");
                    setFilter("all");
                  }}
                >
                  Clear filters
                </button>
              </div>
            </div>
            <Side activity={activity.data} sections={sections} live={tracks.data ?? []} due={due} ready={ready} />
          </div>
        </main>
      </MockRoot>
    </>
  );
}
