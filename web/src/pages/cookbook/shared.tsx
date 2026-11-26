// What the cookbook pages share: the index, each subject's colour, and the rail of subject "books" (after the wiki of the
// owner's blog): the open book lists its chapters and their pages.
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { api, type CbIndex, type CbRow, type CbSubject } from "../../api";
import "../../styles/cookbook.css";
import { Inline } from "../../components/InlineMd";

export const useCookbook = () => useQuery({ queryKey: ["cookbook"], queryFn: api.cookbook, staleTime: 5 * 60_000 });

const COLOR: Record<string, string> = {
    rust: "var(--acc)", algorithms: "var(--vio)", systems: "var(--fn)", data: "var(--mac)",
    distributed: "var(--lt)", compilers: "var(--grn)", testing: "var(--warn)", projects: "var(--bad)", samples: "var(--warn)",
};
export const colorOf = (subject: string) => COLOR[subject] ?? "var(--mac)";
export const tint = (subject: string) => ({ "--c": colorOf(subject) }) as CSSProperties;

export const KIND_LABEL: Record<string, string> = {
    guide: "Guide", recipe: "Example", capstone: "Capstone", questions: "Questions", project: "Project",
    lesson: "Lesson", api: "API", pattern: "Pattern", comparison: "Comparison", concept: "Concept", testing: "Testing", build: "Build",
};
export const kindLabel = (kind: string) => KIND_LABEL[kind] ?? kind;
/** Pages whose code is the point: the reading view shows it first. */
export const isCodeKind = (kind: string) => kind === "recipe" || kind === "capstone" || kind === "questions" || kind === "project";

export type CbModule = { title: string; chapters: { title: string; pages: CbRow[] }[] };

/** Rows in reading order, grouped by module and chapter (the server sends them in that order). */
export function grouped(rows: CbRow[]): CbModule[] {
    const out: CbModule[] = [];
    for (const r of rows) {
        let m = out.find((x) => x.title === r.module);
        if (!m) out.push((m = { title: r.module, chapters: [] }));
        let c = m.chapters.find((x) => x.title === r.chapter);
        if (!c) m.chapters.push((c = { title: r.chapter, pages: [] }));
        c.pages.push(r);
    }
    return out;
}

export const chaptersIn = (rows: CbRow[]) => grouped(rows).reduce((n, m) => n + m.chapters.length, 0);

/** Every page with the subject it is listed under. */
export function allPages(data: CbIndex): { row: CbRow; subject: string; title: string }[] {
    return [
        ...data.subjects.flatMap((s) => s.pages.map((row) => ({ row, subject: s.id, title: s.title }))),
        ...data.samples.map((row) => ({ row, subject: "samples", title: "Draft samples" })),
    ];
}

export const wordsOf = (q: string) => q.toLowerCase().split(/\s+/).filter(Boolean);
export const matches = (text: string, words: string[]) => {
    const t = text.toLowerCase();
    return words.every((w) => t.includes(w));
};
export const rowMatches = (r: CbRow, words: string[]) => matches(`${r.title} ${r.chapter} ${r.module} ${r.lede}`, words);

/** A guide page's description is a list of topics joined by dots; the page head shows them as labels. */
export const topicsOf = (lede: string) => {
    const parts = lede.split(" · ").map((x) => x.trim()).filter(Boolean);
    return parts.length >= 3 && parts.every((x) => x.length <= 90) ? parts : null;
};

export const subjectPath = (id: string) => `/cookbook/${id}` as "/cookbook";
export const anchorOf = (heading: string) => `cb-${heading.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}`;
export const scrollToId = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "start" });

export const SearchIcon = () => (
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
        <circle cx="7" cy="7" r="4.5" />
        <path d="M10.5 10.5 14 14" />
    </svg>
);

/** A search box; `/` moves to it from anywhere on the page. */
export function Search({ value, onChange, label, placeholder }: { value: string; onChange: (v: string) => void; label: string; placeholder: string }) {
    const box = useRef<HTMLInputElement>(null);
    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.key !== "/" || e.metaKey || e.ctrlKey || (e.target as HTMLElement).closest("input, textarea, [contenteditable]")) return;
            e.preventDefault();
            box.current?.focus();
        };
        addEventListener("keydown", key);
        return () => removeEventListener("keydown", key);
    }, []);
    return (
        <label className="cb-search">
            <SearchIcon />
            <input ref={box} type="search" value={value} onChange={(e) => onChange(e.target.value)} onKeyDown={(e) => e.key === "Escape" && onChange("")} placeholder={placeholder} aria-label={label} />
            <kbd>/</kbd>
        </label>
    );
}

/** One page as a row of a list. */
export function PageRow({ row, subject, where }: { row: CbRow; subject: string; where?: string }) {
    return (
        <li style={tint(subject)}>
            <Link to="/cookbook/p/$id" params={{ id: row.id }}>
                <span className="cb-row-t"><Inline text={row.title} /></span>
                <span className="cb-row-d"><Inline text={row.lede} /></span>
                <span className="cb-row-m">
                    {where && <small>{where}</small>}
                    <em className={`cb-kind cb-kind-${row.kind}`}>{kindLabel(row.kind)}</em>
                    <small>{row.lines > 0 ? `${row.lines.toLocaleString()} lines` : (row.level ?? "")}</small>
                </span>
            </Link>
        </li>
    );
}

/** The pages of the open book: chapters that fold, with the current page's chapter open. */
function Book({ rows, current, flat }: { rows: CbRow[]; current?: string; flat: boolean }) {
    const here = rows.find((r) => r.id === current);
    const link = (p: CbRow) => (
        <Link key={p.id} to="/cookbook/p/$id" params={{ id: p.id }} className={p.id === current ? "on" : ""} aria-current={p.id === current ? "page" : undefined}>
            <span><Inline text={p.title} /></span>
        </Link>
    );
    if (flat) return <div className="cb-book-pages">{rows.map(link)}</div>;
    return (
        <>
            {grouped(rows).map((m) => (
                <div key={m.title}>
                    <h6>{m.title}</h6>
                    {m.chapters.map((c) => (
                        <details key={`${c.title}:${here?.chapter === c.title && here.module === m.title}`} open={here?.chapter === c.title && here.module === m.title}>
                            <summary>
                                <span>{c.title}</span>
                                <small>{c.pages.length}</small>
                            </summary>
                            <div className="cb-book-pages">{c.pages.map(link)}</div>
                        </details>
                    ))}
                </div>
            ))}
        </>
    );
}

/** Every subject as a card; the open one shows its pages. Typing lists the matching pages of that book. */
export function Shelf({ data, subject, current, chapters }: { data: CbIndex; subject: string; current?: string; chapters?: boolean }) {
    const [q, setQ] = useState("");
    const list = useRef<HTMLDivElement>(null);
    const books: CbSubject[] = [...data.subjects, { id: "samples", title: "Draft samples", about: "Pages in the full format, waiting for review.", pages: data.samples }];
    const open = books.find((b) => b.id === subject);
    useEffect(() => {
        const box = list.current;
        const on = box?.querySelector<HTMLElement>(".cb-book-pages a.on") ?? box?.querySelector<HTMLElement>(".cb-bk.on");
        if (box && on) box.scrollTop = Math.max(0, on.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop - box.clientHeight / 3);
    }, [subject, current]);
    const words = wordsOf(q);
    // the list shows names only, so a match is one the name or the chapter explains; names come first
    const found = words.length && open ? open.pages.filter((r) => matches(`${r.title} ${r.chapter}`, words)).sort((a, b) => Number(matches(b.title, words)) - Number(matches(a.title, words))) : null;
    return (
        <nav className="cb-shelf" aria-label="Subjects and pages">
            <input type="search" value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Find a page in ${open?.title ?? "this book"}`} aria-label="Find a page in this subject" />
            <div className="cb-shelf-list" ref={list}>
                {found ? (
                    <div className="cb-book-pages cb-found">
                        {found.length === 0 && <p className="cb-dim">No page here matches. ⌘K searches everything.</p>}
                        {found.slice(0, 60).map((p) => (
                            <Link key={p.id} to="/cookbook/p/$id" params={{ id: p.id }} className={p.id === current ? "on" : ""}><span><Inline text={p.title} /></span></Link>
                        ))}
                    </div>
                ) : (
                    books.map((b) => (
                        <section key={b.id} className={`cb-bk${b.id === subject ? " on" : ""}`} style={tint(b.id)}>
                            <Link to={b.id === "samples" ? "/cookbook" : subjectPath(b.id)} className="cb-bk-h">
                                <strong>{b.title}</strong>
                                <small>{b.pages.length} pages</small>
                                {b.id === subject && <span>{b.about}</span>}
                            </Link>
                            {b.id === subject && !chapters && <Book rows={b.pages} current={current} flat={b.id === "samples"} />}
                            {b.id === subject && chapters && (
                                <div className="cb-book-pages cb-jumps">
                                    {grouped(b.pages).flatMap((m) =>
                                        m.chapters.map((c) => (
                                            <button key={`${m.title}/${c.title}`} type="button" onClick={() => scrollToId(anchorOf(`${m.title} ${c.title}`))}>
                                                <span>{c.title}</span>
                                                <small>{c.pages.length}</small>
                                            </button>
                                        )),
                                    )}
                                </div>
                            )}
                        </section>
                    ))
                )}
            </div>
        </nav>
    );
}
