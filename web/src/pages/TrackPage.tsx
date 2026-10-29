import { useEffect, useRef, useState } from "react";
import { Skeleton } from "../components/Skeleton";
import { createPortal } from "react-dom";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { api, ApiError, type Band, type StageView } from "../api";
import { Header } from "../components/Header";
import { ResetTrack } from "../components/ResetTrack";
import { Level, ModeTag, Phase, Sech, Segs, StatusIcon, pad2, progressLabel } from "../components/bits";
import { BLURB, NAV_SECTIONS, SECTION_NAMES, type NavArea } from "../curriculum";
import { TRACK_SLUGS } from "../trackSlugs";

const BANDS: Band[] = ["easy", "medium", "hard"];
const TIER = { core: "CORE", light: "LIGHT", sde3: "SDE-3" } as const;

type SortKey = "n" | "t" | "m" | "l" | "s";
const BAND_RANK: Record<Band, number> = { easy: 0, medium: 1, hard: 2 };
const PROGRESS_RANK = { not_started: 0, started: 1, solved: 2, assisted: 2 } as const;
const SORT_LABELS: Record<SortKey, string> = { n: "#", t: "Problem", m: "Mode", l: "Level", s: "Status" };

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

function stageKind(s: StageView, isCurrent: boolean) {
  if (s.total > 0 && s.solved === s.total) return "done" as const;
  if (isCurrent) return "cur" as const;
  return s.solved > 0 ? ("open" as const) : ("ahead" as const);
}

export function TrackPage({ slug }: { slug: string }) {
  const [resetOpen, setResetOpen] = useState(false);
  const q = useQuery({ queryKey: ["track", slug], queryFn: () => api.track(slug), retry: (n, e) => n < 2 && !(e instanceof ApiError && e.status === 404) });
  const [stage, setStage] = useState<string | null>(null);
  const [tag, setTag] = useState<string | null>(null);
  const [group, setGroup] = useState<string | null>(null);
  const [company, setCompany] = useState<string | null>(null);
  const [find, setFind] = useState("");
  const [sort, setSort] = useState<{ k: SortKey; dir: 1 | -1 } | null>(null);
  const [cur, setCur] = useState(-1);
  const navigate = useNavigate();
  const findBox = useRef<HTMLInputElement>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  // the rows on screen, for the keyboard handler below (set on each render once the track has loaded)
  const shownRef = useRef<{ id: string; ready: boolean }[]>([]);
  const curRef = useRef(-1);
  curRef.current = cur;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
        const el = document.activeElement as HTMLElement | null;
        const typing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
        if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
        const rows = shownRef.current;
        if (e.key === "/") {
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
            if (r?.ready) navigate({ to: "/p/$id", params: { id: r.id } });
        }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);
  const t = q.data;
  const area = t ? ((Object.keys(NAV_SECTIONS) as NavArea[]).find((a) => (NAV_SECTIONS[a] as readonly string[]).includes(t.section)) ?? "dsa") : undefined;

  if (!t) {
    return (
      <>
        <Header />
        <main className="page">
          <div className="wrap">
            {q.isError && TRACK_SLUGS[slug] ? (
              <div className="notice" data-testid="planned-track">
                <b>
                  {TRACK_SLUGS[slug].code} · {TRACK_SLUGS[slug].name}
                </b>{" "}
                is planned and not written yet. The BusTub course already links here, so the link will work once the track exists.
                {BLURB[TRACK_SLUGS[slug].code] && <p>{BLURB[TRACK_SLUGS[slug].code]}</p>}
                <p>
                  <Link to="/rust">Back to the Rust tracks</Link>
                </p>
              </div>
            ) : (
              q.isError ? <p className="notice bad">{`Couldn't load track ${slug}.`}</p> : <Skeleton lines={6} />
            )}
          </div>
        </main>
      </>
    );
  }

  // The stage you're on: the first with something written and unsolved; otherwise the first unfinished one.
  const workable = t.stages.findIndex((s) => s.ready > 0 && s.solved < s.total);
  const current = workable >= 0 ? workable : t.stages.findIndex((s) => s.solved < s.total);
  const selected = stage ?? null;
  const inGroup = (p: { companies: string[] }, g: string) => p.companies.some((c) => t.company_groups.find((x) => x.name === g)?.companies.includes(c));
  const byCompany = (p: { companies: string[] }) => (company ? p.companies.includes(company) : group ? inGroup(p, group) : true);
  const inStage = t.problems.filter((p) => !selected || p.stage === selected);
  const needle = find.trim().toLowerCase();
  const matches = (p: { title: string; tags: string[]; companies: string[] }) => !needle || `${p.title} ${p.tags.join(" ")} ${p.companies.join(" ")}`.toLowerCase().includes(needle);
  const filtered = inStage.filter((p) => (!tag || p.tags.includes(tag)) && byCompany(p) && matches(p));
  const rows = sort
    ? [...filtered].sort((a, b) => {
          const key = { n: (p: typeof a) => p.order, t: (p: typeof a) => p.title.toLowerCase(), m: (p: typeof a) => p.mode as string, l: (p: typeof a) => BAND_RANK[p.level], s: (p: typeof a) => (p.status === "draft" ? 9 : PROGRESS_RANK[p.progress]) }[sort.k];
          const x = key(a);
          const y = key(b);
          return (x < y ? -1 : x > y ? 1 : a.order - b.order) * sort.dir;
      })
    : filtered;
  shownRef.current = rows.map((p) => ({ id: p.id, ready: p.status === "ready" }));
  const clickSort = (k: SortKey) => setSort(!sort || sort.k !== k ? { k, dir: 1 } : sort.dir === 1 ? { k, dir: -1 } : null);
  const sortHead = (k: SortKey) => (
    <button className="sorth" onClick={() => clickSort(k)} aria-sort={sort?.k === k ? (sort.dir === 1 ? "ascending" : "descending") : "none"} title={`Sort by ${SORT_LABELS[k].toLowerCase()}`}>
      {SORT_LABELS[k]}
      <span className={`sar${sort?.k === k ? " on" : ""}`} aria-hidden="true">
        {sort?.k === k && sort.dir === -1 ? "▼" : "▲"}
      </span>
    </button>
  );
  const faang = new Set(t.company_groups.find((g) => g.name === "FAANG")?.companies ?? []);
  const tagged = inStage.filter((p) => p.companies.length > 0);
  const tags = [...new Set(t.problems.filter((p) => !selected || p.stage === selected).flatMap((p) => p.tags))];
  const nextUp = t.problems.find((p) => p.status === "ready" && p.progress !== "solved" && p.progress !== "assisted");
  const cols = "40px 52px minmax(0,1.35fr) 100px 90px minmax(0,.9fr) minmax(0,1.25fr) 110px";

  return (
    <>
      <Header area={area} />
      <main className="page">
        <div className="wrap">
          <section className="hero solo">
            <div>
              <div className="eyebrow">
                <span style={{ color: "var(--acc)" }}>{t.code}</span>
                <span>/</span>
                <span>{SECTION_NAMES[t.section].toUpperCase()}</span>
                <span>/</span>
                <span>{TIER[t.tier]}</span>
              </div>
              <h1 className="h1 md">{t.name}</h1>
              {t.summary && <p className="lead">{t.summary}</p>}
              <div className="btns">
                {nextUp && (
                  <Link className="btn go" to="/p/$id" params={{ id: nextUp.id }}>
                    {nextUp.progress === "started" ? "Resume" : "Start"} · {nextUp.title} →
                  </Link>
                )}
                <button className="btn sm danger" aria-expanded={resetOpen} onClick={() => setResetOpen(!resetOpen)} title="Forget your progress on this track">
                  Reset progress
                </button>
              </div>
              {resetOpen && <ResetTrack slug={slug} name={t.name} onDone={() => setResetOpen(false)} />}
            </div>
          </section>
          <section className="strip">
            <div>
              <span className="k">SOLVED</span>
              <span className="v" style={{ color: "var(--grn)" }}>
                {t.solved} of {t.total}
              </span>
              <span className="s">{t.ready} ready to solve</span>
            </div>
            <div>
              <span className="k">NOW ON</span>
              <span className="v" style={{ color: "var(--acc)" }}>
                {t.stages[current]?.name ?? "Done"}
              </span>
              <span className="s">{current >= 0 ? `stage ${current + 1} of ${t.stages.length}` : "every stage solved"}</span>
            </div>
            <div>
              <span className="k">STAGES</span>
              <span className="v">{t.stages.length}</span>
              <span className="s">easy → medium → hard</span>
            </div>
            <div>
              <span className="k">NEXT UP</span>
              <span className="v" style={{ fontSize: 18 }}>
                {nextUp?.title ?? "—"}
              </span>
              <span className="s">
                {nextUp ? (
                  <>
                    <Level level={nextUp.level} /> · {nextUp.mode === "fix" ? "fix this" : "write it"}
                  </>
                ) : (
                  "nothing ready yet"
                )}
              </span>
            </div>
          </section>

          <section className="sec" style={{ paddingTop: 56 }}>
            <Sech title="THE STAGES" caption="click a stage to filter the problems" />
            <div className="bandcols">
              {BANDS.map((b) => {
                const list = t.stages.map((s, i) => [s, i] as const).filter(([s]) => s.band === b);
                return (
                  <div className="bandcol" key={b}>
                    <div className="bandh">
                      <Level level={b} upper />
                      <span>{list.length} stages</span>
                    </div>
                    {list.map(([s, i]) => {
                      const kind = stageKind(s, i === current);
                      const label =
                        kind === "done" ? `SOLVED · ${s.solved}/${s.total}` : kind === "cur" ? `IN PROGRESS · ${s.solved}/${s.total}` : kind === "open" ? `OPEN · ${s.solved}/${s.total}` : `AHEAD · ${s.total}`;
                      const on = selected === s.slug;
                      return (
                        <button
                          key={s.slug}
                          className={`scard${on ? " sel" : ""}`}
                          aria-pressed={on}
                          onClick={() => {
                            setStage(on ? null : s.slug);
                            setTag(null);
                          }}
                        >
                          <span className="sn">
                            <span>{s.name}</span>
                            <span className="m" style={{ fontSize: 10, color: "var(--dim)" }}>
                              {pad2(i + 1)}
                            </span>
                          </span>
                          <Phase kind={kind}>{label}</Phase>
                          <Segs n={s.total} filled={s.solved} color={kind === "ahead" ? "var(--line2)" : "var(--grn)"} />
                          <span className="m" style={{ fontSize: 10.5, color: "var(--dim)" }}>
                            {s.ready} of {s.total} written
                          </span>
                        </button>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          </section>

          <section className="sec" style={{ paddingTop: 48 }}>
            <Sech
              title={selected ? `STAGE · ${t.stages.find((s) => s.slug === selected)?.name.toUpperCase()}` : "ALL PROBLEMS"}
              caption={`${rows.length} problems · ${rows.filter((p) => p.mode === "fix").length} fix-this${company ? ` · asked at ${company}` : group ? ` · asked at ${group}` : ""}`}
            />
            <div className="chips" hidden={!selected}>
              {[null, ...tags].map((x) => (
                <button key={x ?? "all"} className={`chip${tag === x ? " on" : ""}`} onClick={() => setTag(x)}>
                  {x ?? "all"}
                </button>
              ))}
            </div>
            {tagged.length > 0 && (
              <div className="cofilter">
                <div className="corow">
                  <span className="colabel">GROUP</span>
                  <button className={`chip grp${!group && !company ? " on" : ""}`} onClick={() => (setGroup(null), setCompany(null))}>
                    all
                  </button>
                  {t.company_groups.map((g) => {
                    const n = inStage.filter((p) => inGroup(p, g.name)).length;
                    return n > 0 ? (
                      <button key={g.name} className={`chip grp${group === g.name && !company ? " on" : ""}`} onClick={() => (setGroup(g.name), setCompany(null))}>
                        {g.name} <small>{n}</small>
                      </button>
                    ) : null;
                  })}
                </div>
                <div className="corow">
                  <span className="colabel">COMPANY</span>
                  {t.company_groups
                    .filter((g) => !group || g.name === group)
                    .flatMap((g) => g.companies)
                    .map((c) => {
                      const n = inStage.filter((p) => p.companies.includes(c)).length;
                      return n > 0 ? (
                        <button key={c} className={`chip${company === c ? " on" : ""}`} onClick={() => setCompany(company === c ? null : c)}>
                          {c} <small>{n}</small>
                        </button>
                      ) : null;
                    })}
                </div>
              </div>
            )}
            <div className="fbar">
              <label className="fbox">
                <span aria-hidden="true">⌕</span>
                <input ref={findBox} value={find} onChange={(e) => (setFind(e.target.value), setCur(-1))} placeholder="filter problems, tags, companies…" autoComplete="off" spellCheck={false} onKeyDown={(e) => e.key === "Escape" && e.currentTarget.blur()} aria-label="Filter problems" />
                <kbd>/</kbd>
              </label>
              <span className="fcount">
                {rows.length} of {inStage.length}
              </span>
              <select className="sortsel" value={sort ? `${sort.k}${sort.dir}` : ""} onChange={(e) => setSort(e.target.value ? { k: e.target.value[0] as SortKey, dir: e.target.value[1] === "-" ? -1 : 1 } : null)} aria-label="Sort problems">
                <option value="">Sort: recommended order</option>
                {(Object.keys(SORT_LABELS) as SortKey[]).flatMap((k) => [
                  <option key={k + "1"} value={`${k}1`}>
                    Sort: {SORT_LABELS[k]} ↑
                  </option>,
                  <option key={k + "-"} value={`${k}-1`}>
                    Sort: {SORT_LABELS[k]} ↓
                  </option>,
                ])}
              </select>
              {sort && (
                <button className="sreset" onClick={() => setSort(null)}>
                  RESET ORDER
                </button>
              )}
            </div>
            <div className="tblw" ref={tableRef}>
              <div className="tbl tbl-co">
                <div className="tr th" style={{ gridTemplateColumns: cols }}>
                  <span />
                  {sortHead("n")}
                  {sortHead("t")}
                  {sortHead("m")}
                  {sortHead("l")}
                  <span>TAGS</span>
                  <span>COMPANIES</span>
                  <span style={{ textAlign: "right", display: "flex", justifyContent: "flex-end" }}>{sortHead("s")}</span>
                </div>
                {rows.map((p, ri) => {
                  const cells = (
                    <>
                      <span style={{ color: "var(--dim)", display: "inline-flex" }}><StatusIcon progress={p.progress} draft={p.status === "draft"} /></span>
                      <span className="num">{pad2(p.order)}</span>
                      <span className="pt"><Hl text={p.title} find={needle} /></span>
                      <span>
                        <ModeTag mode={p.mode} />
                      </span>
                      <span className="lvl">
                        <Level level={p.level} />
                      </span>
                      <span className="tags"><Hl text={p.tags.join(" · ")} find={needle} /></span>
                      {p.companies.length > 0 ? (
                        <span className="cos">
                          {p.companies.slice(0, 3).map((c) => (
                            <span key={c} className={`cc${faang.has(c) || c === company ? " faang" : ""}`}>
                              {c}
                            </span>
                          ))}
                          {p.companies.length > 3 && <MoreCompanies names={p.companies.slice(3)} highlight={(c) => faang.has(c) || c === company} />}
                        </span>
                      ) : (
                        <span className="tags">—</span>
                      )}
                      <span className="best stat" style={{ color: p.status === "draft" ? "var(--dim)" : p.progress === "not_started" ? "var(--mut)" : p.progress === "started" ? "var(--warn)" : "var(--grn)" }}>
                        <StatusIcon progress={p.progress} draft={p.status === "draft"} />
                        {p.status === "draft" ? "not written" : progressLabel(p.progress)}
                      </span>
                    </>
                  );
                  return p.status === "ready" ? (
                    <Link key={p.id} className={`tr${p.progress === "started" ? " cur" : ""}${ri === cur ? " kb" : ""}`} data-row={ri} to="/p/$id" params={{ id: p.id }} style={{ gridTemplateColumns: cols }}>
                      {cells}
                    </Link>
                  ) : (
                    <div key={p.id} className={`tr tr-draft${ri === cur ? " kb" : ""}`} data-row={ri} style={{ gridTemplateColumns: cols }}>
                      {cells}
                    </div>
                  );
                })}
                {rows.length === 0 && (
                  <div className="tr tr-empty">
                    No problem matches that filter.{" "}
                    {find && (
                      <button className="sreset" onClick={() => setFind("")}>
                        CLEAR IT
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
            {tagged.length > 0 && <p className="conote">Company tags are approximate, from reported questions.</p>}
          </section>
        </div>
      </main>
    </>
  );
}

/**
 * The "+N" chip for companies that didn't fit. Hovering shows them, and so does clicking, which doesn't open the
 * problem (the whole row is a link). The popover is portalled to <body>: the table scrolls sideways, which would
 * clip anything hanging out of a row.
 */
function MoreCompanies({ names, highlight }: { names: string[]; highlight: (c: string) => boolean }) {
  const chip = useRef<HTMLSpanElement>(null);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const show = () => {
    const r = chip.current?.getBoundingClientRect();
    if (r) setAt({ left: Math.min(r.left, window.innerWidth - 260), top: r.bottom + 6 });
  };
  useEffect(() => {
    if (!at) return;
    const hide = () => setAt(null);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", hide);
    return () => {
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("resize", hide);
    };
  }, [at]);
  return (
    <>
      <span
        ref={chip}
        className="cc more"
        role="button"
        tabIndex={0}
        aria-label={`${names.length} more: ${names.join(", ")}`}
        onMouseEnter={show}
        onMouseLeave={() => setAt(null)}
        onFocus={show}
        onBlur={() => setAt(null)}
        onClick={(e) => {
          // Open (a tap on a touch screen has no hover); leaving or tapping elsewhere closes it.
          e.preventDefault();
          e.stopPropagation();
          show();
        }}
      >
        +{names.length}
      </span>
      {at &&
        createPortal(
          <div className="cc-pop" style={{ left: at.left, top: at.top }} role="tooltip">
            {names.map((c) => (
              <span key={c} className={`cc${highlight(c) ? " faang" : ""}`}>
                {c}
              </span>
            ))}
          </div>,
          document.body,
        )}
    </>
  );
}
