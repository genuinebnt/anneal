// The pieces of a series (docs/SERIES.md) that the problem workspace shows: the Connections block in the problem pane and
// the completion view in the centre pane. Links that may give the method away stay closed until asked.
import { Link } from "@tanstack/react-router";
import type { SeriesDone, SeriesLink, SeriesPlace } from "../api";
import "../styles/series.css";

/** `Part 2 of 21` or `Capstone step 3 of 10`. */
export const placeName = (s: SeriesPlace) => `${s.kind === "part" ? "Part" : "Capstone step"} ${s.number} of ${s.total}`;

/** A title inside a sentence: "A thread pool" reads "a thread pool", while "RAII guards" keeps its capitals. */
export const lowerFirst = (s: string) => (/^[A-Z](?![A-Z])/.test(s) ? s[0]!.toLowerCase() + s.slice(1) : s);

/** `29 Nov`, from `2026-11-29`. */
export const shortDay = (day: string) => new Date(`${day}T12:00:00`).toLocaleDateString("en-GB", { day: "numeric", month: "short" });

export function Segments({ on, label }: { on: boolean[]; label: string }) {
  return (
    <div className="sx-segs" role="img" aria-label={`${on.filter(Boolean).length} of ${on.length} ${label} done`}>
      {on.map((x, i) => <i key={i} className={x ? "on" : undefined} />)}
    </div>
  );
}

const named = (l: SeriesLink) => (l.number === null ? l.title : `${l.number}. ${l.title}`);

function Connection({ tag, link }: { tag: string; link: SeriesLink }) {
  const body = (
    <>
      <em>{tag}</em>
      <span>{named(link)}{link.id ? "" : " (still to be written)"}</span>
      {link.why && <small>{link.why}</small>}
    </>
  );
  return link.id ? <Link to="/p/$id" params={{ id: link.id }}>{body}</Link> : <div>{body}</div>;
}

const COOKBOOK_TAG = { exact: "Gives the solution away", similar: "Same technique", related: "Related" } as const;

export function Connections({ place, solved }: { place: SeriesPlace; solved: boolean }) {
  const closed = place.helpful.length + place.cookbook.length;
  const earlier = (l: SeriesLink) => (l.number === null ? "Another problem" : l.label.startsWith("Part") ? "Earlier part" : "Earlier step");
  return (
    <section className="sx-links" aria-label="Connections">
      <h2>Connections</h2>
      {(place.know_first || place.required.length > 0) && (
        <div className="sx-first">
          <b>Know first</b>
          {place.know_first && <p>{place.know_first}</p>}
          {place.required.map((l) => <Connection key={l.label} tag={l.label} link={l} />)}
        </div>
      )}
      {closed > 0 && (
        // after solving, nothing is left to give away; the key makes the block start closed again for the next problem
        <details key={`${place.series.id}:${place.kind}${place.number}:${solved}`} open={solved || undefined}>
          <summary>Show helpful ideas<small>{solved ? "You have solved this one" : "This may give away part of the approach"}</small></summary>
          {place.helpful.map((l) => (
            <Connection key={l.label + l.title} tag={`${l.number === null ? "Another problem" : l.number < place.number ? earlier(l) : l.label}${l.reveals_method ? " · gives away the method" : ""}`} link={l} />
          ))}
          {place.cookbook.map((c) => (
            <Link key={c.page} to="/cookbook/p/$id" params={{ id: c.page }}>
              <em>{COOKBOOK_TAG[c.level]}</em>
              <span>Cookbook: {c.title}</span>
            </Link>
          ))}
        </details>
      )}
    </section>
  );
}

/** What a passing Submit on a series problem leads to: what it practised, where it comes back and what is next. */
export function SeriesComplete({ place, done, how }: { place: SeriesPlace; done: SeriesDone; how: string }) {
  const next = done.next;
  return (
    <div className="sx-done" role="region" aria-label="Solved">
      <header>
        <h2>{done.title}</h2>
        <p>{how} {placeName(place)} is done.</p>
      </header>
      <Segments on={place.segments} label={place.kind === "part" ? "parts" : "capstone steps"} />
      {done.practised.length > 0 && (
        <section>
          <h3>What you practised</h3>
          <ul>
            {done.practised.map((l) => <li key={l.text}><b>{l.label}</b><span>{l.text}</span></li>)}
          </ul>
        </section>
      )}
      {done.returns.length > 0 && (
        <section>
          <h3>Where it returns</h3>
          <table>
            <tbody>{done.returns.map((l) => <tr key={l.label + l.text}><th scope="row">{l.label}</th><td>{l.text}</td></tr>)}</tbody>
          </table>
        </section>
      )}
      {done.repeat.length > 0 && (
        <section>
          <h3>Repeat later</h3>
          {done.repeat.map((r) => (
            <p key={r.text + (r.due ?? "")}>{r.text}{r.due ? <> It is due on {shortDay(r.due)} and will appear under <Link to="/progress" hash="reviews">Re-solves</Link>.</> : null}</p>
          ))}
        </section>
      )}
      {done.skipped.length > 0 && <p className="sx-skipped">Still to be written: {done.skipped.join(", ")}.</p>}
      <div className="sx-acts">
        {next?.id && (
          <Link className="sx-btn" to="/p/$id" params={{ id: next.id }}>
            Start {next.kind === "part" ? "part" : "capstone step"} {next.number}: {lowerFirst(next.title)}
          </Link>
        )}
        <Link className="sx-ghost" to="/rust/series/$id" params={{ id: place.series.id }}>Back to the series</Link>
      </div>
    </div>
  );
}
