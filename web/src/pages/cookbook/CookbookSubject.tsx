// One subject, laid out like a series on the owner's blog: the books on the left, a header for this one, then its chapters
// in order, each a list of pages. Filters by words and by kind of page.
import { Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Header } from "../../components/Header";
import { PageRow, Search, Shelf, anchorOf, grouped, kindLabel, rowMatches, scrollToId, tint, useCookbook, wordsOf } from "./shared";
import { Inline } from "../../components/InlineMd";

export function CookbookSubject({ id }: { id: string }) {
    const book = useCookbook();
    const [q, setQ] = useState("");
    const [kind, setKind] = useState("all");
    const subject = book.data?.subjects.find((s) => s.id === id);
    const words = wordsOf(q);
    const all = subject?.pages ?? [];
    const rows = all.filter((p) => (kind === "all" || p.kind === kind) && rowMatches(p, words));
    const kinds = [...new Set(all.map((p) => p.kind))];
    const modules = grouped(rows);
    const lines = all.reduce((n, p) => n + p.lines, 0);
    // a chapter link from the front page lands on its chapter once the index has arrived
    const loaded = !!subject;
    useEffect(() => {
        const at = window.location.hash.slice(1);
        if (loaded && at) requestAnimationFrame(() => scrollToId(at));
    }, [loaded]);
    const filtered = words.length > 0 || kind !== "all";
    // the title's last word carries the accent, as on the blog's series pages
    const name = subject?.title.split(" ") ?? [];
    const last = name.pop();
    let chapterNo = 0;
    return (
        <>
            <Header area="cb" />
            <main className="page cb-page cb-sub" style={tint(id)}>
                {book.data && <aside className="cb-side"><Shelf data={book.data} subject={id} chapters /></aside>}
                <div className="cb-main">
                    {!book.data && !book.isError && <div className="cb-skel" aria-label="Loading"><i /><i /><i /></div>}
                    {book.isError && <p className="cb-note" role="alert">The cookbook could not be loaded. <button type="button" className="cb-link" onClick={() => void book.refetch()}>Try again</button></p>}
                    {book.data && !subject && <p className="cb-note" role="alert">There is no subject at this address. <Link to="/cookbook">Go to the cookbook</Link></p>}
                    {subject && (
                        <>
                            <header className="cb-banner">
                                <span className="cb-dotline"><b>Subject</b><span>{subject.pages.length} pages</span><span>{grouped(all).reduce((n, m) => n + m.chapters.length, 0)} chapters</span>{lines > 0 && <span>{Math.round(lines / 1000)}k lines of code</span>}</span>
                                <h1>{name.join(" ")}{name.length > 0 && " "}<em className="cb-rgb">{last}.</em></h1>
                                <p>{subject.about}</p>
                                {all[0] && <Link to="/cookbook/p/$id" params={{ id: all[0].id }} className="cb-btn"><span>Start with <Inline text={all[0].title} /></span></Link>}
                            </header>
                            <div className="cb-kbar">
                                <Search value={q} onChange={setQ} label="Filter the pages" placeholder={`Filter ${subject.title}: a name or a word in the description`} />
                                <div className="cb-chips" role="group" aria-label="Kind of page">
                                    {["all", ...kinds].map((k) => (
                                        <button key={k} type="button" className={kind === k ? "on" : ""} aria-pressed={kind === k} onClick={() => setKind(k)}>
                                            {k === "all" ? "All" : kindLabel(k)}<small>{k === "all" ? all.length : all.filter((p) => p.kind === k).length}</small>
                                        </button>
                                    ))}
                                </div>
                            </div>
                            {filtered && <p className="cb-count" aria-live="polite">{rows.length} of {all.length} pages</p>}
                            {rows.length === 0 && <p className="cb-note">No page matches. <button type="button" className="cb-link" onClick={() => { setQ(""); setKind("all"); }}>Clear the filter</button></p>}
                            {modules.map((m) => (
                                <section key={m.title}>
                                    <h2 className="cb-secth">{m.title}<small>{m.chapters.reduce((n, c) => n + c.pages.length, 0)} pages</small></h2>
                                    {m.chapters.map((c) => {
                                        chapterNo += 1;
                                        return (
                                            <section key={c.title} className="cb-chapter" id={anchorOf(`${m.title} ${c.title}`)}>
                                                <h3>
                                                    <i aria-hidden="true">{String(chapterNo).padStart(2, "0")}</i>
                                                    <span>{c.title}</span>
                                                    <small>{c.pages.length}</small>
                                                </h3>
                                                <ul className="cb-rows">{c.pages.map((p) => <PageRow key={p.id} row={p} subject={id} />)}</ul>
                                            </section>
                                        );
                                    })}
                                </section>
                            ))}
                        </>
                    )}
                </div>
            </main>
        </>
    );
}
