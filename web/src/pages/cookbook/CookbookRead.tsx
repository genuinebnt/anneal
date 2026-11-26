// A cookbook page: the subject's tree on the left, the article, and its contents on the right. Both side panels can be
// hidden to give code more room. A long draft page opens with its first two sections and keeps the rest folded.
import { marked } from "marked";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useRouter } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useState, type MouseEvent } from "react";
import { ApiError, api, type CbLab, type CbMethod, type CbPage, type CbRow } from "../../api";
import { Header } from "../../components/Header";
import { Panel } from "../../components/layout";
import { FencesContext, LineStopsContext, SdMd, countNotes } from "../../components/SdMd";
import { getPref, setPref } from "../../prefs";
import { KIND_COLOR, sourceKind } from "../sd/SdRead";
import { CodeViewer } from "./CodeViewer";
import { Connections, InShort, SpoilerGate, lessonFences } from "./Lesson";
import { Shelf, anchorOf, isCodeKind, kindLabel, scrollToId, subjectPath, tint, topicsOf, useCookbook } from "./shared";

/** Sections shown open on a draft page with many: the answer comes first. */
const OPEN_FIRST = 2;
const FOLD_FROM = 6;

const inline = (text: string) => ({ __html: marked.parseInline(text, { async: false }) });

/** A lesson's code is the whole file its text walks through; other pages that are not about code call theirs a project. */
const labTitle = (page: CbPage, lab: CbLab) => (page.kind === "lesson" ? "Full code" : `Project: ${lab.title}`);

/** A project that goes with the page: its files in the viewer and, for a sample, the output recorded from running it. */
function Lab({ lab, n, page, wrap, onWrap }: { lab: CbLab; n: number; page: CbPage; wrap: boolean; onWrap: () => void }) {
    const code = isCodeKind(page.kind);
    if (!lab.contents.length) return null;
    const viewer = <CodeViewer files={lab.contents} label={code || page.kind === "lesson" ? `Code of ${page.title}` : lab.title} wrap={wrap} onWrap={onWrap} foldHeader={page.kind === "lesson" || (code && page.kind !== "project")} />;
    return (
        <section className="cb-lab" id={`cb-lab-${n}`} aria-label={code ? "The code" : labTitle(page, lab)}>
            {!code && <h2>{labTitle(page, lab)}</h2>}
            {page.code_spoils?.length ? <SpoilerGate spoils={page.code_spoils}>{viewer}</SpoilerGate> : viewer}
            {lab.recorded && (
                <details className="cb-out">
                    <summary>
                        Recorded output<small>{lab.recorded.passed} tests passed, {lab.recorded.stamp}</small>
                    </summary>
                    {lab.recorded.run && <><h4>cargo run</h4><pre>{lab.recorded.run}</pre></>}
                    {lab.recorded.tests && <><h4>cargo test</h4><pre>{lab.recorded.tests}</pre></>}
                </details>
            )}
        </section>
    );
}

/** Copies the address of a section; the button says so for a moment. */
function CopyLink({ anchor }: { anchor: string }) {
    const [done, setDone] = useState(false);
    const copy = () =>
        navigator.clipboard.writeText(`${window.location.origin}${window.location.pathname}#${anchor}`).then(
            () => {
                setDone(true);
                window.setTimeout(() => setDone(false), 1600);
            },
            () => {},
        );
    return <button type="button" className={`cb-copylink${done ? " done" : ""}`} onClick={copy} aria-label="Copy a link to this section" title="Copy a link to this section">{done ? "Copied" : "#"}</button>;
}

/** The other pages of this chapter, in order, and the pages that link here. */
function Around({ chapter, current, linked }: { chapter: CbRow[]; current: string; linked: CbRow[] }) {
    return (
        <>
            {chapter.length > 1 && (
                <Panel title="In this chapter" aside={<small className="cb-dim">{chapter.length}</small>}>
                    <ol className="cb-rail">
                        {chapter.map((r) => (
                            <li key={r.id}>
                                <Link to="/cookbook/p/$id" params={{ id: r.id }} className={r.id === current ? "on" : ""} aria-current={r.id === current ? "page" : undefined}>{r.title}</Link>
                            </li>
                        ))}
                    </ol>
                </Panel>
            )}
            {linked.length > 0 && (
                <Panel title="Linked from" aside={<small className="cb-dim">{linked.length}</small>}>
                    <ul className="cb-linked">{linked.map((r) => <li key={r.id}><Link to="/cookbook/p/$id" params={{ id: r.id }}>{r.title}</Link></li>)}</ul>
                </Panel>
            )}
        </>
    );
}

/** Minutes to read the prose, and what else the page holds. */
function facts(page: CbPage) {
    const text = page.sections.map((s) => s.body).join("\n");
    const code = text.match(/^```\w/gm)?.length ?? 0;
    const words = text.replace(/^```[\s\S]*?^```$/gm, " ").replace(/<[^>]+>/g, " ").split(/\s+/).filter(Boolean).length;
    return { minutes: Math.max(1, Math.round(words / 220)), code };
}

const count = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

function Head({ page, subject, place }: { page: CbPage; subject: string; place: string }) {
    const topics = topicsOf(page.lede);
    const f = useMemo(() => facts(page), [page]);
    const lines = page.labs.reduce((n, l) => n + l.contents.reduce((m, x) => m + x.text.split("\n").length - 1, 0), 0);
    const files = page.labs.reduce((n, l) => n + l.contents.length, 0);
    return (
        <header className="cb-head">
            <p className="cb-where">
                {page.chapter}
                {place && <><i>/</i>{place}</>}
            </p>
            <h1>{page.title}</h1>
            <div className="cb-meta">
                <span className="cb-subj">{subject}</span>
                <span>{kindLabel(page.kind)}</span>
                {page.level && page.level !== page.kind && <span>{page.level}</span>}
                <span>{f.minutes} min read</span>
                {lines > 0 && <span>{files > 1 ? `${files} files, ` : ""}{lines.toLocaleString()} lines of code</span>}
                {page.sources.length > 0 && <span>{count(page.sources.length, "source")}</span>}
                {page.status === "draft" && <span className="cb-draft">Draft: not yet reviewed by a person</span>}
            </div>
            {topics ? <ul className="cb-topics" aria-label="What the page covers">{topics.map((t) => <li key={t}>{t}</li>)}</ul> : <p className="cb-dek" dangerouslySetInnerHTML={inline(page.lede)} />}
            {page.origin && <p className="cb-dim">From the {page.origin.toLowerCase()}. Its exercises are left out.</p>}
        </header>
    );
}

/** Which of the page's parts is being read, and how far down the article the window is. */
function usePlace(ids: string[]) {
    const [place, setPlace] = useState({ at: 0, done: 0 });
    useEffect(() => {
        let raf = 0;
        const update = () => {
            raf = 0;
            const line = window.innerHeight * 0.3;
            let at = 0;
            ids.forEach((id, i) => {
                const el = document.getElementById(id);
                if (el && el.getBoundingClientRect().top <= line) at = i;
            });
            const art = document.querySelector(".cb-art");
            const box = art?.getBoundingClientRect();
            const done = box ? Math.min(1, Math.max(0, (window.innerHeight - box.top) / Math.max(1, box.height))) : 0;
            setPlace((p) => (p.at === at && Math.abs(p.done - done) < 0.005 ? p : { at, done }));
        };
        const onScroll = () => {
            if (!raf) raf = requestAnimationFrame(update);
        };
        update();
        addEventListener("scroll", onScroll, { passive: true });
        addEventListener("resize", onScroll);
        return () => {
            removeEventListener("scroll", onScroll);
            removeEventListener("resize", onScroll);
            if (raf) cancelAnimationFrame(raf);
        };
    }, [ids]);
    return place;
}

function Contents({ page, open }: { page: CbPage; open: (i: number, anchor: string) => void }) {
    const parts = useMemo(() => {
        const sections = page.sections.map((s, i) => ({ label: s.heading, id: anchorOf(s.heading), section: i as number | null }));
        const labs = page.labs.map((l, n) => ({ label: isCodeKind(page.kind) ? "The code" : labTitle(page, l), id: `cb-lab-${n}`, section: null }));
        const after = [
            ...(page.connections?.length ? [{ label: "Connections", id: "cb-connections", section: null }] : []),
            ...(page.sources.length ? [{ label: "Go deeper", id: "cb-sources", section: null }] : []),
        ];
        return [...(isCodeKind(page.kind) ? [...labs, ...sections] : [...sections, ...labs]), ...after];
    }, [page]);
    const ids = useMemo(() => parts.map((p) => p.id), [parts]);
    const { at, done } = usePlace(ids);
    const left = Math.ceil(facts(page).minutes * (1 - done));
    // J and K go to the next and the previous part
    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            const step = e.key === "j" ? 1 : e.key === "k" ? -1 : 0;
            if (!step || e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement).closest("input, textarea, [contenteditable]")) return;
            const to = parts[Math.max(0, Math.min(parts.length - 1, at + step))];
            if (!to) return;
            if (to.section === null) scrollToId(to.id);
            else open(to.section, to.id);
        };
        addEventListener("keydown", key);
        return () => removeEventListener("keydown", key);
    }, [at, parts, open]);
    return (
        <Panel title="On this page" aside={<small className="cb-dim cb-left-time">{left > 0 && done < 0.98 ? `${left} min left` : "end"}</small>}>
            <div className="cb-meter" aria-hidden="true"><i style={{ width: `${done * 100}%` }} /></div>
            <nav className="cb-toc">
                {parts.map((p, i) => (
                    <button key={p.id} type="button" className={i === at ? "on" : ""} aria-current={i === at ? "location" : undefined} onClick={() => (p.section === null ? scrollToId(p.id) : open(p.section, p.id))}>
                        <i>{String(i + 1).padStart(2, "0")}</i>
                        <span>{p.label}</span>
                    </button>
                ))}
            </nav>
            <p className="cb-dim cb-keys"><kbd>J</kbd> next part · <kbd>K</kbd> back</p>
        </Panel>
    );
}

/** The methods of the types a page is about: the common ones first, all of them behind a fold. */
function Methods({ methods }: { methods: CbMethod[] }) {
    const table = (list: CbMethod[]) => (
        <div className="sd-tbl">
            <table>
                <thead><tr><th>Method</th><th>Use it when</th><th>Since</th></tr></thead>
                <tbody>
                    {list.map((m) => (
                        <tr key={`${m.type}::${m.name}:${m.signature}`}>
                            <td>
                                {m.anchor ? <button type="button" className="cb-link" onClick={() => scrollToId(m.anchor!)}><code>{m.type}::{m.name}</code></button> : <code>{m.type}::{m.name}</code>}
                                <pre>{m.signature}</pre>
                            </td>
                            <td dangerouslySetInnerHTML={inline(m.use_when)} />
                            <td className="cb-dim">{m.since}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
        </div>
    );
    const common = methods.filter((m) => m.common);
    return (
        <section className="cb-methods" id="cb-methods">
            <h2>Methods</h2>
            {common.length > 0 ? table(common) : table(methods)}
            {common.length > 0 && common.length < methods.length && (
                <details className="sd-deeper">
                    <summary><span>All {methods.length} methods</span><em>full list</em></summary>
                    {table(methods)}
                </details>
            )}
        </section>
    );
}

function Article({ page, wrap, onWrap }: { page: CbPage; wrap: boolean; onWrap: () => void }) {
    const labs = page.labs.map((l, n) => <Lab key={l.title} lab={l} n={n} page={page} wrap={wrap} onWrap={onWrap} />);
    const lesson = page.kind === "lesson";
    // a lesson's explanation is never folded
    const fold = !lesson && page.status === "draft" && page.sections.length >= FOLD_FROM;
    const fences = useMemo(() => (lesson ? lessonFences(page) : {}), [lesson, page]);
    const [open, setOpen] = useState<Set<number>>(() => new Set(page.sections.map((_, i) => i).filter((i) => !fold || i < OPEN_FIRST)));
    // opening a folded section from the contents or an address
    useEffect(() => {
        const show = (e: Event) => setOpen((s) => new Set(s).add((e as CustomEvent<number>).detail));
        addEventListener("cb-open", show);
        // an address that names a section opens it and goes there
        const wanted = window.location.hash.slice(1);
        const i = page.sections.findIndex((s) => anchorOf(s.heading) === wanted);
        if (i >= 0) {
            setOpen((s) => new Set(s).add(i));
            requestAnimationFrame(() => scrollToId(wanted));
        } else if (page.kind === "lesson" && wanted) {
            requestAnimationFrame(() => scrollToId(wanted));
        }
        return () => removeEventListener("cb-open", show);
    }, [page]);
    let notes = 0;
    return (
        <LineStopsContext.Provider value={!lesson}>
        <FencesContext.Provider value={fences}>
            {page.in_short && <InShort page={page} />}
            {page.summary.length > 0 && (
                <section className="cb-sum">
                    <h2>In short</h2>
                    <ul>{page.summary.map((s) => <li key={s} dangerouslySetInnerHTML={inline(s)} />)}</ul>
                </section>
            )}
            {page.glance.length > 0 && (
                <dl className="cb-glance">{page.glance.map((g) => <div key={g.label}><dt>{g.label}</dt><dd dangerouslySetInnerHTML={inline(g.value)} /></div>)}</dl>
            )}
            {fold && (
                <button type="button" className="cb-link cb-all" onClick={() => setOpen(open.size === page.sections.length ? new Set([0, 1]) : new Set(page.sections.map((_, i) => i)))}>
                    {open.size === page.sections.length ? "Fold the detail" : `Open all ${page.sections.length} sections`}
                </button>
            )}
            {isCodeKind(page.kind) && labs}
            {page.sections.map((s, i) => {
                const start = notes;
                notes += countNotes(s.body);
                const shown = open.has(i);
                const head = s.heading === "Overview" && i === 0 && !isCodeKind(page.kind) ? null : s.heading;
                return (
                    <section key={s.heading} id={anchorOf(s.heading)} className="cb-sec">
                        {head && (
                            <h2>
                                {fold && i >= OPEN_FIRST ? (
                                    <button type="button" className="cb-fold" aria-expanded={shown} onClick={() => setOpen((o) => { const n = new Set(o); if (!n.delete(i)) n.add(i); return n; })}>
                                        <i aria-hidden="true">{shown ? "▾" : "▸"}</i>
                                        {head}
                                    </button>
                                ) : head}
                                <CopyLink anchor={anchorOf(s.heading)} />
                            </h2>
                        )}
                        {shown && <SdMd text={s.body} id={anchorOf(s.heading)} notes={start} />}
                        {shown && i === 0 && page.methods.length > 0 && <Methods methods={page.methods} />}
                    </section>
                );
            })}
            {!isCodeKind(page.kind) && labs}
            {!!page.connections?.length && <Connections list={page.connections} />}
            {page.sources.length > 0 && (
                <section className="cb-sources" id="cb-sources">
                    <h2>Go deeper</h2>
                    <ul>
                        {page.sources.map((s) => (
                            <li key={s.url}>
                                <em style={{ color: KIND_COLOR[sourceKind(s)] }}>{sourceKind(s)}</em>
                                <a href={s.url} target="_blank" rel="noreferrer">{s.title}</a>
                                {s.note && <p>{s.note}</p>}
                                {s.read === false && <p className="cb-dim">Not read by the page's author.</p>}
                            </li>
                        ))}
                    </ul>
                </section>
            )}
        </FencesContext.Provider>
        </LineStopsContext.Provider>
    );
}

export function CookbookRead({ id }: { id: string }) {
    // a page that is not there will not appear by asking again; anything else is tried once more
    const view = useQuery({ queryKey: ["cookbook", id], queryFn: () => api.cookbookPage(id), staleTime: 5 * 60_000, retry: (n, e) => n < 1 && !(e instanceof ApiError && e.status === 404) });
    const missing = view.error instanceof ApiError && view.error.status === 404;
    const book = useCookbook();
    const nav = useNavigate();
    const router = useRouter();
    const [panels, setPanels] = useState(() => ({ left: true, right: true, wrap: false, big: false, ...getPref<{ left?: boolean; right?: boolean; wrap?: boolean; big?: boolean }>("cookbook.view", {}) }));
    const [drawer, setDrawer] = useState(false);
    const toggle = (side: "left" | "right" | "wrap" | "big") => setPanels((p) => { const n = { ...p, [side]: !p[side] }; setPref("cookbook.view", n); return n; });
    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement).closest("input, textarea, [contenteditable]")) return;
            if (e.key === "[") toggle("left");
            else if (e.key === "]") toggle("right");
            else if (e.key === "Escape") setDrawer(false);
        };
        addEventListener("keydown", key);
        return () => removeEventListener("keydown", key);
    }, []);

    const page = view.data?.page;
    useEffect(() => {
        if (!window.location.hash) window.scrollTo({ top: 0 });
    }, [id]);
    const rows = useMemo(() => {
        if (!book.data || !page) return [];
        return page.subject === "samples" ? book.data.samples : (book.data.subjects.find((s) => s.id === page.subject)?.pages ?? []);
    }, [book.data, page]);
    const subject = book.data?.subjects.find((s) => s.id === page?.subject);
    const chapter = useMemo(() => rows.filter((r) => r.chapter === page?.chapter && r.module === page?.module), [rows, page]);
    const at = chapter.findIndex((r) => r.id === page?.id);
    const open = useCallback((i: number, anchor: string) => {
        dispatchEvent(new CustomEvent("cb-open", { detail: i }));
        requestAnimationFrame(() => scrollToId(anchor));
    }, []);
    // links between pages inside the page's HTML move without reloading
    const follow = (e: MouseEvent) => {
        const a = (e.target as HTMLElement).closest("a");
        const href = a?.getAttribute("href") ?? "";
        // a lesson also links to problems, series and sections of other pages
        if (page?.kind === "lesson" && href.startsWith("/") && !e.defaultPrevented && !e.metaKey && !e.ctrlKey && !e.shiftKey && a?.target !== "_blank") {
            e.preventDefault();
            router.history.push(href);
            return;
        }
        const m = /^\/cookbook\/p\/([\w-]+)$/.exec(href);
        if (!m || e.metaKey || e.ctrlKey || e.shiftKey) return;
        e.preventDefault();
        void nav({ to: "/cookbook/p/$id", params: { id: m[1]! } });
    };
    return (
        <>
            <Header area="cb" />
            <main style={tint(page?.subject ?? "data")} className={`page cb-read${page?.kind === "lesson" ? " cb-lesson" : ""}${panels.left ? "" : " no-left"}${panels.right ? "" : " no-right"}${panels.wrap ? " wrap" : ""}${panels.big ? " big" : ""}${drawer ? " drawer" : ""}`}>
                <div className="cb-bar">
                    <button type="button" className="cb-drawer" onClick={() => setDrawer(true)} aria-expanded={drawer}>Pages</button>
                    <nav className="cb-crumb" aria-label="Where this page is">
                        <Link to="/cookbook">Cookbook</Link>
                        <i>/</i>
                        {subject ? <Link to={subjectPath(subject.id)}>{subject.title}</Link> : <span>{page?.subject === "samples" ? "Draft samples" : ""}</span>}
                        {page && <><i>/</i><span>{page.chapter}</span></>}
                    </nav>
                    <span className="cb-sp" />
                    <button type="button" className="cb-toggle" aria-pressed={panels.wrap} onClick={() => toggle("wrap")}>Wrap code</button>
                    <button type="button" className="cb-toggle" aria-pressed={panels.big} onClick={() => toggle("big")}>Larger text</button>
                    <button type="button" className="cb-toggle cb-wide" aria-pressed={panels.left} onClick={() => toggle("left")} aria-keyshortcuts="[">Pages <kbd>[</kbd></button>
                    <button type="button" className="cb-toggle cb-wide" aria-pressed={panels.right} onClick={() => toggle("right")} aria-keyshortcuts="]">Contents <kbd>]</kbd></button>
                </div>
                <div className="cb-lay">
                    <aside className="cb-left" onClick={(e) => (e.target as HTMLElement).closest("a") && setDrawer(false)}>
                        {page && book.data && <Shelf data={book.data} subject={page.subject} current={page.id} />}
                    </aside>
                    {drawer && <div className="cb-scrim" onClick={() => setDrawer(false)} />}
                    <article className={`cb-art${page && isCodeKind(page.kind) ? " cb-code-page" : ""}`} onClick={follow}>
                        {view.isError && missing && <p className="cb-note" role="alert">There is no cookbook page at this address. <Link to="/cookbook">Go to the cookbook</Link></p>}
                        {view.isError && !missing && <p className="cb-note" role="alert">This page could not be loaded. <button type="button" className="cb-link" onClick={() => void view.refetch()}>Try again</button></p>}
                        {view.isPending && <p className="cb-note">Loading…</p>}
                        {page && (
                            <>
                                <Head page={page} subject={subject?.title ?? "Draft samples"} place={chapter.length > 1 && at >= 0 ? `page ${at + 1} of ${chapter.length}` : ""} />
                                <Article key={page.id} page={page} wrap={panels.wrap} onWrap={() => toggle("wrap")} />
                                <nav className="cb-pn" aria-label="Previous and next page">
                                    {view.data?.previous ? <Link to="/cookbook/p/$id" params={{ id: view.data.previous.id }}><small>‹ Previous · {view.data.previous.chapter}</small><span>{view.data.previous.title}</span></Link> : <span />}
                                    {view.data?.next ? <Link to="/cookbook/p/$id" params={{ id: view.data.next.id }} className="next"><small>Up next · {view.data.next.chapter} ›</small><span>{view.data.next.title}</span></Link> : <span />}
                                </nav>
                            </>
                        )}
                    </article>
                    <aside className="cb-right">
                        {page && <Contents key={page.id} page={page} open={open} />}
                        {page && <Around chapter={chapter} current={page.id} linked={view.data?.linked_from ?? []} />}
                    </aside>
                </div>
            </main>
        </>
    );
}
