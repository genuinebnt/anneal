import { Link } from "@tanstack/react-router";
import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from "react";
import type { SdConceptView, SdOverview } from "../../api";
import { AskPanel } from "../../components/AskPanel";
import { HowFound, ListCard, Page3, Panel } from "../../components/layout";
import { SdFigure } from "../../components/SdFigure";
import { SdMd, countNotes } from "../../components/SdMd";
import { getPref, setPref } from "../../prefs";
import { groupOf } from "./SdHome";

/** How the article is set: column width, text size, line spacing, and focus (the article alone). They follow the owner across
 *  laptops like other preferences. */
type ReadPrefs = { width: "n" | "s" | "w"; size: "s" | "m" | "l" | "xl"; space: "tight" | "normal" | "relaxed"; focus: boolean };
const DEFAULT_PREFS: ReadPrefs = { width: "s", size: "m", space: "normal", focus: false };
/** Characters per line for each width; the column is that many characters of the chosen text size. */
const WIDTH = { n: 60, s: 68, w: 80 };
const SIZE = { s: "15px", m: "16.5px", l: "18px", xl: "19.5px" };
const SPACE = { tight: 1.55, normal: 1.75, relaxed: 1.95 };

function useReadPrefs(): [ReadPrefs, (p: Partial<ReadPrefs>) => void] {
    const [prefs, setPrefs] = useState<ReadPrefs>(() => ({ ...DEFAULT_PREFS, ...getPref<Partial<ReadPrefs>>("sd.read", {}) }));
    const update = (p: Partial<ReadPrefs>) =>
        setPrefs((cur) => {
            const next = { ...cur, ...p };
            setPref("sd.read", next);
            return next;
        });
    return [prefs, update];
}

/** A section's anchor: its heading in lower case, words joined by hyphens. */
export const sectionId = (heading: string) => `s-${heading.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}`;

/** Which section is being read: the last one whose top has passed the upper third of the window. */
function useCurrentSection(ids: string[]): number {
    const [cur, setCur] = useState(0);
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
            setCur(at);
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
    return cur;
}

const go = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth", block: "start" });

/** What a source is, from its own `kind` or, failing that, its address. */
function sourceKind(s: { url: string; kind?: string | null }): string {
    if (s.kind) return s.kind;
    if (/usenix|vldb|acm\.org|arxiv|\.pdf$|research\.google|static\.googleusercontent/.test(s.url)) return "paper";
    if (/rfc-editor|ietf\.org|w3\.org/.test(s.url)) return "standard";
    if (/github\.com/.test(s.url)) return "guide";
    if (/docs?\.|\/docs\/|developer\.mozilla|redis\.io|postgresql\.org|kafka\.apache/.test(s.url)) return "docs";
    return "article";
}
const KIND_COLOR: Record<string, string> = { paper: "var(--vio)", docs: "var(--fn)", guide: "var(--grn)", book: "var(--acc)", standard: "var(--ty)", article: "var(--mut)" };

function Seg<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: [T, ReactNode][]; onChange: (v: T) => void }) {
    return (
        <div className="rd-set">
            <span>{label}</span>
            <div className="rd-seg" role="radiogroup" aria-label={label}>
                {options.map(([v, l]) => <button key={v} role="radio" aria-checked={value === v} className={value === v ? "on" : ""} onClick={() => onChange(v)}>{l}</button>)}
            </div>
        </div>
    );
}

function Outline({ headings, ids, cur, left, checks }: { headings: string[]; ids: string[]; cur: number; left: number; checks: number }) {
    const share = headings.length ? cur / headings.length : 0;
    return (
        <>
            <div className="rd-left">
                <svg viewBox="0 0 44 44" aria-hidden="true"><circle cx="22" cy="22" r="18" /><circle className="on" cx="22" cy="22" r="18" style={{ strokeDasharray: `${share * 113} 113` }} /></svg>
                <div><b>{left > 0 ? `${left} min left` : "almost done"}</b><small>section {cur + 1} of {headings.length}</small></div>
            </div>
            <ol className="rd-toc">
                {headings.map((h, i) => (
                    <li key={ids[i]}>
                        <a href={`#${ids[i]}`} className={i === cur ? "on" : i < cur ? "past" : ""} aria-current={i === cur ? "location" : undefined} onClick={(e) => { e.preventDefault(); go(ids[i]!); history.replaceState(null, "", `#${ids[i]}`); }}>
                            <em>{i < cur ? "✓" : String(i + 1).padStart(2, "0")}</em>
                            {h}
                        </a>
                    </li>
                ))}
                <li><a href="#checks" className="chk" onClick={(e) => { e.preventDefault(); go("checks"); }}><em>?</em>Check yourself · {checks}</a></li>
            </ol>
        </>
    );
}

function Settings({ prefs, set }: { prefs: ReadPrefs; set: (p: Partial<ReadPrefs>) => void }) {
    return (
        <div className="rd-sets">
            <Seg label="Width" value={prefs.width} onChange={(width) => set({ width })} options={[["n", "N"], ["s", "S"], ["w", "W"]]} />
            <Seg label="Size" value={prefs.size} onChange={(size) => set({ size })} options={[["s", "S"], ["m", "M"], ["l", "L"], ["xl", "XL"]]} />
            <Seg label="Spacing" value={prefs.space} onChange={(space) => set({ space })} options={[["tight", "Tight"], ["normal", "Normal"], ["relaxed", "Relaxed"]]} />
            <button className={`rd-focus${prefs.focus ? " on" : ""}`} aria-pressed={prefs.focus} onClick={() => set({ focus: !prefs.focus })}>Focus mode: only the article <kbd>F</kbd></button>
            <p className="rd-keys"><kbd>J</kbd> next section · <kbd>K</kbd> back</p>
        </div>
    );
}

/** A concept page as a reading view: an outline that follows you, reading settings, numbered sections with links, a summary,
 *  margin notes, step-throughs and highlighted code (SdMd), the key terms of the section being read, sources, and the checks. */
export function ConceptReader({ id, v, all, checks }: { id: string; v: SdConceptView; all?: SdOverview; checks: ReactNode }) {
    const c = v.concept;
    const g = groupOf(c.group);
    const [prefs, setPrefs] = useReadPrefs();
    const [sheet, setSheet] = useState(false);
    const [copied, setCopied] = useState<string | null>(null);
    const headings = useMemo(() => c.sections.map((s) => s.heading), [c]);
    const ids = useMemo(() => headings.map(sectionId), [headings]);
    const cur = useCurrentSection(ids);
    // minutes left, from how much text remains after the section being read
    const words = useMemo(() => c.sections.map((s) => s.body.split(/\s+/).length), [c]);
    const total = words.reduce((a, b) => a + b, 0) || 1;
    const left = Math.round((c.minutes * words.slice(cur).reduce((a, b) => a + b, 0)) / total);
    // margin notes are numbered across the page
    const notesBefore = useMemo(() => c.sections.map((_, i) => c.sections.slice(0, i).reduce((n, s) => n + countNotes(s.body), 0)), [c]);
    const hasNotes = notesBefore.length > 0 && c.sections.some((s) => countNotes(s.body) > 0);

    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.metaKey || e.ctrlKey || e.altKey) return;
            const t = e.target as HTMLElement | null;
            if (t?.closest("input, textarea, select, [contenteditable=true], .hd-pal")) return;
            const k = e.key.toLowerCase();
            if (k === "j") go(ids[Math.min(ids.length - 1, cur + 1)]!);
            else if (k === "k") go(ids[Math.max(0, cur - 1)]!);
            else if (k === "f") setPrefs({ focus: !prefs.focus });
            else return;
            e.preventDefault();
        };
        addEventListener("keydown", key);
        return () => removeEventListener("keydown", key);
    }, [ids, cur, prefs.focus, setPrefs]);

    useEffect(() => {
        if (!sheet) return;
        const esc = (e: KeyboardEvent) => e.key === "Escape" && setSheet(false);
        addEventListener("keydown", esc);
        return () => removeEventListener("keydown", esc);
    }, [sheet]);

    // a link to a section scrolls to it once the page has drawn
    useEffect(() => {
        const h = window.location.hash.slice(1);
        if (h && ids.includes(h)) window.setTimeout(() => go(h), 50);
    }, [ids]);

    const copyLink = (sid: string) => {
        navigator.clipboard.writeText(`${window.location.origin}${window.location.pathname}#${sid}`).then(
            () => {
                setCopied(sid);
                window.setTimeout(() => setCopied(null), 1500);
            },
            () => {},
        );
    };
    const siblings = (all?.concepts ?? []).filter((x) => x.group === c.group).sort((a, b) => a.order - b.order);
    const titleOf = (cid: string) => all?.concepts.find((x) => x.id === cid)?.title ?? cid;
    const terms = (c.terms ?? []).filter((t) => t.section === headings[cur]);
    const sources = c.sources ?? [];

    const outline = <Outline headings={headings} ids={ids} cur={cur} left={left} checks={c.checks.length} />;
    const settings = <Settings prefs={prefs} set={setPrefs} />;
    const left_ = (
        <>
            <Panel title="On this page">{outline}</Panel>
            <Panel title="Reading">{settings}</Panel>
            <Panel title={g.name}>
                <div className="sd-list">
                    {siblings.map((s) => (
                        <Link key={s.id} to="/sd/learn/$id" params={{ id: s.id }} className={s.id === id ? "on" : ""}>
                            <i className={s.checked >= s.checks && s.checks ? "done" : ""} />
                            {s.title}
                        </Link>
                    ))}
                </div>
            </Panel>
        </>
    );
    const style = { "--rd-w": WIDTH[prefs.width], "--rd-fs": SIZE[prefs.size], "--rd-lh": SPACE[prefs.space] } as CSSProperties;
    const center = (
        <div className={`rd-wrap${hasNotes ? " has-sn" : ""}`} style={style}>
            <article className="ly-pn sd-art rd-art">
                <span className="sd-k" style={{ color: g.color }}>{g.name.toUpperCase()}</span>
                <h2>{c.title}</h2>
                <p className="sd-lede">{c.lede}</p>
                <div className="rd-meta">
                    <span>{c.minutes} min read</span>
                    <span>{c.checks.length} checks</span>
                    {sources.length > 0 && <span>{sources.length} sources</span>}
                    {c.credit && <span className="ok">parts adapted · openly licensed</span>}
                </div>
                {c.summary && c.summary.length > 0 && (
                    <section className="rd-sum" aria-label="In 60 seconds">
                        <span>IN 60 SECONDS</span>
                        <ul>{c.summary.map((s, i) => <li key={i}><SdMd text={s} id={`sum${i}`} /></li>)}</ul>
                    </section>
                )}
                {c.sections.map((s, i) => (
                    <section key={s.heading} id={ids[i]} className="rd-sec">
                        <div className="rd-h">
                            <em className={i === cur ? "on" : ""}>{String(i + 1).padStart(2, "0")}</em>
                            <h3>{s.heading}</h3>
                            <button className="rd-anchor" aria-label={`Copy a link to ${s.heading}`} onClick={() => copyLink(ids[i]!)}>{copied === ids[i] ? "✓" : "#"}</button>
                        </div>
                        <SdMd text={s.body} id={ids[i]} notes={notesBefore[i]} />
                        {c.figures.filter((f) => f.after === s.heading).map((f) => <SdFigure key={f.name} name={f.name} caption={f.caption} />)}
                    </section>
                ))}
                <aside className="sd-call sd-call-interview rd-say"><b>Say it like this</b><div><p>“{c.say}”</p></div></aside>
                {(sources.length > 0 || c.credit) && (
                    <section className="sd-srcs">
                        <h4>Sources and further reading</h4>
                        {sources.length > 0 && (
                            <div className="rd-srcs">
                                {sources.map((x) => {
                                    const k = sourceKind(x);
                                    return (
                                        <a key={x.url} href={x.url} target="_blank" rel="noreferrer">
                                            <small style={{ color: KIND_COLOR[k] }}>{k.toUpperCase()}</small>
                                            <b>{x.title}</b>
                                            {x.note && <span>{x.note}</span>}
                                        </a>
                                    );
                                })}
                            </div>
                        )}
                        {c.credit && <div className="sd-credit"><SdMd text={c.credit} id="credit" /></div>}
                    </section>
                )}
            </article>
            <div id="checks">{checks}</div>
        </div>
    );
    const right = (
        <>
            <AskPanel kind="sd-concept" id={id} />
            {terms.length > 0 && (
                <Panel title="Key terms here">
                    <div className="rd-terms">{terms.map((t) => <div key={t.term}><b>{t.term}</b><span>{t.means}</span></div>)}</div>
                </Panel>
            )}
            {v.used_in.length > 0 && (
                <Panel title="Used in">
                    <div className="ly-cards">
                        {v.used_in.map((p, i) => <ListCard key={p.id} i={i} title={p.title} note={p.kind.toUpperCase()} to={{ to: "/sd/practice/$id", params: { id: p.id } }} />)}
                    </div>
                </Panel>
            )}
            {c.related.length > 0 && (
                <Panel title="Read next">
                    <div className="sd-list">{c.related.map((r) => <Link key={r} to="/sd/learn/$id" params={{ id: r }}>{titleOf(r)}</Link>)}</div>
                </Panel>
            )}
            <HowFound title="About this page" text={`${c.credit ? "Builds on openly licensed material, credited at the end." : "Written for anneal."}${sources.length ? " Facts are checked against the sources listed at the end." : ""} The checks are open questions: write, compare with the answer, and mark yourself; each mark becomes a review card.`} rows={[["Reading", `${c.minutes} min`], ["Checks", `${v.marked.length}/${c.checks.length} marked`]]} />
        </>
    );
    return (
        <>
            <Page3
                className={prefs.focus ? "rd-focused" : ""}
                crumb={[<Link to="/sd">SYSTEM DESIGN</Link>, <Link to="/sd/learn">LEARN</Link>, g.name.toUpperCase()]}
                title={c.title}
                sub={`${g.name} · ${c.minutes} minutes · ${c.checks.length} checks`}
                left={prefs.focus ? undefined : left_}
                center={center}
                right={prefs.focus ? undefined : right}
                label="Outline and reading"
            />
            {prefs.focus && <button className="rd-unfocus" onClick={() => setPrefs({ focus: false })}>Leave focus mode <kbd>F</kbd></button>}
            <button className="rd-fab" aria-label="Outline and reading settings" aria-expanded={sheet} onClick={() => setSheet(true)}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true"><path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /></svg>
            </button>
            {sheet && (
                <>
                    <div className="rd-scrim" onClick={() => setSheet(false)} aria-hidden="true" />
                    <div className="rd-sheet" role="dialog" aria-modal="true" aria-label="Outline and reading settings" onClick={(e) => (e.target as HTMLElement).closest("a") && setSheet(false)}>
                        <div className="rd-sheet-h"><b>On this page</b><button onClick={() => setSheet(false)} aria-label="Close">✕</button></div>
                        {outline}
                        <b className="rd-sheet-sub">Reading</b>
                        {settings}
                    </div>
                </>
            )}
        </>
    );
}
