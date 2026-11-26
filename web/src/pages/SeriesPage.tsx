// /rust/series/<id>: an ordered path of small problems that ends in a capstone (docs/SERIES.md, docs/mockups/series.html).
// Everything on it comes from content/series/<id>.toml and the attempts already stored.
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ApiError, api, type SeriesItem, type SeriesRow, type SeriesView, type SeriesSummary } from "../api";
import { Header } from "../components/Header";
import { Skeleton } from "../components/Skeleton";
import { Segments, lowerFirst, shortDay } from "../components/seriesBits";
import "../styles/series.css";
import { Inline } from "../components/InlineMd";

const STATE: Record<SeriesItem["state"], string> = { solved: "Solved", assisted: "With help", next: "Next", started: "Started", open: "", to_write: "To write" };

const done = (s: SeriesItem["state"]) => s === "solved" || s === "assisted";

function PartRow({ item, series }: { item: SeriesItem; series: string }) {
  const body = (
    <>
      <b>{item.title}</b>
      <span>{item.idea}</span>
    </>
  );
  return (
    <li className={`sx-part is-${item.state}`}>
      <span className="sx-n" aria-hidden="true">{done(item.state) ? "✓" : item.number}</span>
      {item.id ? <Link to="/p/$id" params={{ id: item.id }} search={{ series }}>{body}</Link> : <div>{body}</div>}
      <span className="sx-min">{item.minutes ? `${item.minutes} min` : ""}</span>
      <span className="sx-state">{STATE[item.state]}</span>
    </li>
  );
}

function RepeatRow({ row }: { row: Extract<SeriesRow, { row: "repeat" }> }) {
  // The day is the re-solve schedule's own (docs/SPACED_REPETITION.md), so no number of days is promised here.
  const when =
    row.state === "waiting" ? "Once you solve it, the re-solve schedule gives it a day"
    : row.state === "done" ? "Done"
    : row.state === "due" ? "Due now"
    : row.due ? `Scheduled: its re-solve is due on ${shortDay(row.due)}` : "On the re-solve schedule";
  const body = (
    <>
      <span className="sx-n" aria-hidden="true">{row.state === "done" ? "✓" : "↻"}</span>
      <div><b>Repeat: part {row.number} from a blank starter</b><span>{when}{row.minutes ? ` · about ${row.minutes} minutes` : ""}</span></div>
    </>
  );
  return (
    <li className={`sx-revisit is-${row.state}`}>
      {row.state === "due" ? <Link to="/progress" hash="reviews">{body}</Link> : body}
    </li>
  );
}

function Capstone({ cap, series }: { cap: NonNullable<SeriesView["capstone"]>; series: string }) {
  const solved = cap.steps.filter((s) => done(s.state)).length;
  const written = cap.steps.filter((s) => s.state !== "to_write").length;
  return (
    <section className="sx-cap" aria-label="Capstone project">
      <div className="sx-caphead">
        <h2>{cap.title}</h2>
        <span>{written === 0 ? "To be written" : `${solved} of ${cap.steps.length} steps solved`}</span>
      </div>
      {cap.summary && <p>{cap.summary}</p>}
      <ol>
        {cap.steps.map((s) => (
          <li key={s.number} className={`is-${s.state}`}>
            <i aria-hidden="true">{done(s.state) ? "✓" : s.number}</i>
            {s.id ? <Link to="/p/$id" params={{ id: s.id }} search={{ series }}>{s.title}</Link> : <b>{s.title}</b>}
            <span>{[s.idea, s.state === "to_write" ? (written > 0 ? "To write" : "") : STATE[s.state]].filter(Boolean).join(" · ")}</span>
          </li>
        ))}
      </ol>
      {cap.note && <p className="sx-capnote">{cap.note}</p>}
      {cap.cookbook && (
        <details>
          <summary>Show the Cookbook solution<small>This shows the complete approach</small></summary>
          <Link to="/cookbook/p/$id" params={{ id: cap.cookbook.id }}><span>Open in the Cookbook: <Inline text={cap.cookbook.title} /></span></Link>
        </details>
      )}
    </section>
  );
}

function Body({ s }: { s: SeriesView }) {
  const next = s.next;
  const total = s.segments.length;
  return (
    <>
      <header className="sx-hero">
        <div>
          <h1>{s.title}</h1>
          <p>{s.summary}</p>
        </div>
        <div className="sx-progress">
          <b>{s.solved} of {total} parts solved</b>
          <Segments on={s.segments} label="parts" />
          <span>
            {s.to_write ? `${s.to_write} part${s.to_write === 1 ? "" : "s"} still to be written` : "Every part is written"}
            {s.steps_solved > 0 ? ` · ${s.steps_solved} of ${s.steps} capstone steps solved` : ""}
          </span>
        </div>
      </header>
      {next?.id ? (
        <section className="sx-next" aria-label="Next">
          <div>
            <h2>Next: {next.kind === "part" ? "part" : "capstone step"} {next.number}, {lowerFirst(next.title)}</h2>
            <p>{s.next_why}</p>
          </div>
          <Link className="sx-btn" to="/p/$id" params={{ id: next.id }} search={{ series: s.id }}>Start {next.kind === "part" ? "part" : "step"} {next.number}</Link>
        </section>
      ) : (
        <section className="sx-next" aria-label="Next">
          <div>
            <h2>Everything written so far is solved</h2>
            <p>New parts appear here when they are added.</p>
          </div>
        </section>
      )}
      <div className="sx-cols">
        <section>
          <h2>Your path</h2>
          {s.groups.map((g) => (
            <div key={g.name}>
              <h3 className="sx-stage">{g.name}</h3>
              <ol className="sx-path">
                {g.rows.map((r) => (r.row === "item" ? <PartRow key={`p${r.number}`} item={r} series={s.id} /> : <RepeatRow key={`r${r.id}${r.number}`} row={r} />))}
              </ol>
            </div>
          ))}
          {s.capstone && <Capstone cap={s.capstone} series={s.id} />}
          <p className="sx-open">Every preparation part has its own starter code. You can open any written part at any time.{s.note ? ` ${s.note}` : ""}</p>
        </section>
        <aside>
          {s.needs.length > 0 && (
            <section className="sx-box">
              <h2>Ready for the capstone?</h2>
              <p className="sx-small">What the project needs, and which parts teach it. Open a line for the details, which may name techniques.</p>
              <table className="sx-need">
                <thead><tr><th scope="col">You will need</th><th scope="col">Parts</th></tr></thead>
                <tbody>
                  {s.needs.map((n) => (
                    <tr key={n.need}>
                      <td>
                        {/* the single things under a heading name techniques, so they stay closed */}
                        <details>
                          <summary>{n.need}</summary>
                          <ul>
                            {n.concepts.map((c) => (
                              <li key={c.concept}>
                                {c.concept}
                                <small>{[c.parts && `Parts ${c.parts}`, c.steps && `capstone ${c.steps}`, c.note].filter(Boolean).join(" · ")}</small>
                              </li>
                            ))}
                          </ul>
                        </details>
                      </td>
                      <td>{n.parts || "–"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          {s.ideas.length > 0 && (
            <section className="sx-box">
              <h2>How the ideas connect</h2>
              <ul className="sx-map">
                {s.ideas.map((t) => (
                  <li key={t.name}>
                    <b>{t.name}</b>
                    {t.learn && <span><em>Learn</em>{t.learn}</span>}
                    {t.again && <span><em>Again</em>{t.again}</span>}
                    {t.apply && <span><em>Apply</em>{t.apply}</span>}
                  </li>
                ))}
              </ul>
            </section>
          )}
          {s.optional.length > 0 && (
            <section className="sx-box sx-side">
              <h2>Optional side paths</h2>
              {s.optional.map((o) => (
                <Link key={o.id} to="/p/$id" params={{ id: o.id }}>{o.title}<small>{o.why}</small></Link>
              ))}
            </section>
          )}
        </aside>
      </div>
    </>
  );
}

export function SeriesPage({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["series", id], queryFn: () => api.series(id), retry: (n, e) => !(e instanceof ApiError && e.status === 404) && n < 2 });
  return (
    <>
      <Header area="rust" />
      <main className="page sx-page">
        <nav className="sx-back" aria-label="Breadcrumb"><Link to="/rust">Rust</Link> / <Link to="/rust/series">Series</Link></nav>
        {q.data ? <Body s={q.data} /> : q.isError ? (
          <p className="notice bad">{q.error instanceof ApiError && q.error.status === 404 ? "There is no series here." : "Couldn't load the series."} <Link to="/rust">Back to Rust</Link></p>
        ) : <Skeleton lines={8} />}
      </main>
    </>
  );
}

/** One band for a series: its progress and where to go on. */
function SeriesBand({ s }: { s: SeriesSummary }) {
  return (
    <Link className="sx-entry" to="/rust/series/$id" params={{ id: s.id }}>
      <div>
        <h3>{s.title}</h3>
        <p>{s.summary}</p>
      </div>
      <div className="sx-entry-p">
        <b>{s.solved} of {s.segments.length} parts solved</b>
        <Segments on={s.segments} label="parts" />
        <span>{s.next ? `Next: ${s.next.kind === "part" ? "part" : "capstone step"} ${s.next.number}, ${lowerFirst(s.next.title)}` : "Everything written so far is solved"}</span>
      </div>
      <span className="sx-btn">Open the series</span>
    </Link>
  );
}

/** The entry on the Rust home: one band for each series. */
export function SeriesEntries() {
  const q = useQuery({ queryKey: ["series"], queryFn: api.seriesList });
  if (!q.data?.length) return null;
  return (
    <section aria-label="Series">
      <h2 className="cb-secth">Series</h2>
      {q.data.map((s) => <SeriesBand key={s.id} s={s} />)}
    </section>
  );
}

/** The Series tab: every series in one place. */
export function SeriesListPage() {
  const q = useQuery({ queryKey: ["series"], queryFn: api.seriesList });
  return (
    <>
      <Header area="rust" />
      <main className="page sx-page">
        <nav className="sx-back" aria-label="Breadcrumb"><Link to="/rust">Rust</Link> / Series</nav>
        <header className="sx-hero">
          <div>
            <h1>Series</h1>
            <p>Small exercises in order, each building toward a project you write yourself.</p>
          </div>
        </header>
        <section className="sx-list" aria-label="All series">
          {q.data ? (
            q.data.length ? q.data.map((s) => <SeriesBand key={s.id} s={s} />) : <p className="notice">There is no series yet. <Link to="/rust">Back to the tracks</Link></p>
          ) : q.isError ? (
            <p className="notice bad">Couldn't load the series. <button type="button" onClick={() => void q.refetch()}>Try again</button></p>
          ) : <Skeleton lines={6} />}
        </section>
      </main>
    </>
  );
}
