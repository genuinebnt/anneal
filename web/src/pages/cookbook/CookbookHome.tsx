// The cookbook's front page, laid out after the owner's blog: a headline with the totals, a featured page beside three more,
// one search over everything with a filter by subject, the shelf of subjects, and the draft samples.
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { Header } from "../../components/Header";
import { PageRow, Search, allPages, chaptersIn, grouped, anchorOf, rowMatches, subjectPath, tint, useCookbook, wordsOf, kindLabel } from "./shared";

/** Pages shown first: chosen by hand. Each is skipped if it no longer exists. */
const FEATURED: { id: string; art: string[] }[] = [
    { id: "join-algorithms", art: ["nested loop  M + m·N", "sort-merge   M + N", "hash join    build, then probe"] },
    { id: "mvcc-snapshot-isolation", art: ["begin_ts ≤ t < end_ts", "first committer wins"] },
    { id: "cb-compiler", art: ["source → tokens", "→ typed IR → VM"] },
    { id: "cow-borrow-until-you-must-own", art: ["Cow::Borrowed", "to_mut() → Owned"] },
];
/** The most search results shown at once. */
const SHOWN = 60;

export function CookbookHome() {
    const book = useCookbook();
    const data = book.data;
    const [q, setQ] = useState("");
    const [subject, setSubject] = useState("all");
    const words = wordsOf(q);
    const all = data ? allPages(data) : [];
    const searching = words.length > 0 || subject !== "all";
    const found = searching ? all.filter((x) => (subject === "all" || x.subject === subject) && rowMatches(x.row, words)) : [];
    // a match in the name comes before one in the description
    if (words.length) found.sort((a, b) => Number(rowMatches({ ...b.row, lede: "", chapter: "", module: "" }, words)) - Number(rowMatches({ ...a.row, lede: "", chapter: "", module: "" }, words)));
    const featured = FEATURED.flatMap((f) => {
        const hit = all.find((x) => x.row.id === f.id);
        return hit ? [{ ...hit, art: f.art }] : [];
    });
    const [lead, ...side] = featured;
    const lines = all.reduce((n, x) => n + x.row.lines, 0);
    return (
        <>
            <Header area="cb" />
            <main className="page cb-page cb-home">
                <div className="cb-intro">
                    <div>
                        <h1>
                            Look it up, then <em className="cb-rgb">read the code.</em>
                        </h1>
                        <p>Rust, algorithms, systems, databases, distributed services, compilers and testing: explanations beside code you can read from the first line to the last.</p>
                    </div>
                    {data && (
                        <dl className="cb-tiles">
                            <div><dt>Pages</dt><dd>{all.length}</dd></div>
                            <div><dt>Chapters</dt><dd>{data.subjects.reduce((n, s) => n + chaptersIn(s.pages), 0)}</dd></div>
                            <div><dt>Lines of code</dt><dd>{Math.round(lines / 1000).toLocaleString()}k</dd></div>
                        </dl>
                    )}
                </div>
                {book.isError && (
                    <p className="cb-note" role="alert">
                        The cookbook could not be loaded. <button type="button" className="cb-link" onClick={() => void book.refetch()}>Try again</button>
                    </p>
                )}
                {!data && !book.isError && <div className="cb-skel" aria-label="Loading"><i /><i /><i /></div>}
                {data && !searching && lead && (
                    <section className="cb-feature" aria-label="Featured pages">
                        <Link to="/cookbook/p/$id" params={{ id: lead.row.id }} className="cb-lead" style={tint(lead.subject)}>
                            <span className="cb-dotline"><b>Featured</b><span>{lead.title}</span><span>{kindLabel(lead.row.kind)}</span></span>
                            <span className="cb-art-chips" aria-hidden="true">{lead.art.map((a) => <code key={a}>{a}</code>)}</span>
                            <span>
                                <span className="cb-lead-t">{lead.row.title}</span>
                                <span className="cb-lead-d">{lead.row.lede.replace(/`/g, "")}</span>
                                <span className="cb-btn">Read the page</span>
                            </span>
                        </Link>
                        <div className="cb-minis">
                            {side.map((f) => (
                                <Link key={f.row.id} to="/cookbook/p/$id" params={{ id: f.row.id }} className="cb-mini" style={tint(f.subject)}>
                                    <span>
                                        <span className="cb-dotline"><span>{f.title}</span><span>{kindLabel(f.row.kind)}</span></span>
                                        <span className="cb-mini-t">{f.row.title}</span>
                                        <span className="cb-mini-d">{f.row.lede.replace(/`/g, "")}</span>
                                    </span>
                                    <span className="cb-thumb" aria-hidden="true"><code>{f.art.join("\n")}</code></span>
                                </Link>
                            ))}
                        </div>
                    </section>
                )}
                {data && (
                    <div className="cb-kbar">
                        <Search value={q} onChange={setQ} label="Search the cookbook" placeholder="Search every page: join, MVCC, parser, Cow, io_uring…" />
                        <div className="cb-chips" role="group" aria-label="Subject">
                            {[{ id: "all", title: "All", n: all.length }, ...data.subjects.map((s) => ({ id: s.id, title: s.title, n: s.pages.length }))].map((s) => (
                                <button key={s.id} type="button" className={subject === s.id ? "on" : ""} aria-pressed={subject === s.id} onClick={() => setSubject(s.id)} style={s.id === "all" ? undefined : tint(s.id)}>
                                    {s.title}<small>{s.n}</small>
                                </button>
                            ))}
                        </div>
                    </div>
                )}
                {data && searching && (
                    <section className="cb-results" aria-live="polite">
                        <h2 className="cb-secth">Results<small>{found.length}</small></h2>
                        {found.length === 0 && (
                            <p className="cb-note">
                                No page matches. <button type="button" className="cb-link" onClick={() => { setQ(""); setSubject("all"); }}>Clear the search</button>
                            </p>
                        )}
                        <ul className="cb-rows">{found.slice(0, SHOWN).map((x) => <PageRow key={x.row.id} row={x.row} subject={x.subject} where={`${x.title}, ${x.row.chapter}`} />)}</ul>
                        {found.length > SHOWN && <p className="cb-dim">The first {SHOWN} of {found.length} are shown. Add a word to narrow them.</p>}
                    </section>
                )}
                {data && !searching && (
                    <>
                        <h2 className="cb-secth">The shelf<small>{data.subjects.length} subjects</small></h2>
                        <div className="cb-books">
                            {data.subjects.map((s, i) => {
                                const chapters = grouped(s.pages).flatMap((m) => m.chapters.map((c) => ({ ...c, at: anchorOf(`${m.title} ${c.title}`) })));
                                return (
                                    <section key={s.id} className="cb-book" style={tint(s.id)}>
                                        <span className="cb-ghost" aria-hidden="true">{String(i + 1).padStart(2, "0")}</span>
                                        <div>
                                            <h3><Link to={subjectPath(s.id)}>{s.title}</Link></h3>
                                            <p>{s.about}</p>
                                        </div>
                                        <ol>
                                            {chapters.slice(0, 6).map((c) => (
                                                <li key={c.at}><Link to={subjectPath(s.id)} hash={c.at}><span>{c.title}</span><small>{c.pages.length}</small></Link></li>
                                            ))}
                                        </ol>
                                        <Link to={subjectPath(s.id)} className="cb-book-foot">
                                            <span>{s.pages.length} pages</span>
                                            <span>{chapters.length > 6 ? `all ${chapters.length} chapters` : "open the book"}</span>
                                        </Link>
                                    </section>
                                );
                            })}
                        </div>
                        {data.samples.length > 0 && (
                            <>
                                <h2 className="cb-secth">Draft samples<small>written in the full page format, waiting for review</small></h2>
                                <ul className="cb-rows">{data.samples.map((p) => <PageRow key={p.id} row={p} subject="samples" />)}</ul>
                            </>
                        )}
                    </>
                )}
            </main>
        </>
    );
}
