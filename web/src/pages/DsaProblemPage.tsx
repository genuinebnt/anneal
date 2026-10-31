import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, type DsaProblem, type Grade, type MySolution } from "../api";
import { Companies, Md, Mark, ProblemTags, PyCode, useLogger } from "../components/dsaBits";
import { Header } from "../components/Header";
import { Page3 } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { GradeGlyph } from "../components/icons";
import { RecallCurve } from "../components/RecallCurve";
import { ConfirmButton } from "../components/ui";
import { useEditorSettings } from "../settings";
import { Editor } from "../workspace/Editor";
import { FocusTimer } from "../components/FocusTimer";
import { getPref, setPref } from "../prefs";
import { useShowIdeas } from "../spoilers";
import { DIFF, MINUTES, inDays, leetcode, niceDate, statusOf, videoUrl } from "../dsa";

const DIFFS = ["easy", "medium", "hard"];
const LIST_NAME = { blind75: "Blind 75", neetcode150: "NeetCode 150", neetcode250: "NeetCode 250", all: "NeetCode All", practice: "Practice" };
const SAME_IDEA_SHOWN = 7;
/** The four log tiles: short labels, since each one also says when the problem comes back. */
const TILES: { grade: Grade; glyph: string; label: string; cls: string }[] = [
    { grade: "good", glyph: "✓", label: "On my own", cls: "ok" },
    { grade: "hard", glyph: "½", label: "With help", cls: "half" },
    { grade: "again", glyph: "!", label: "Attempted", cls: "no" },
    { grade: "easy", glyph: "⚡", label: "Instant", cls: "fast" },
];

const RESULT_GLYPH = { good: "✓", hard: "½", again: "!" } as const;
const RESULT_CLASS = { good: "ok", hard: "half", again: "no" } as const;
const RESULT_WORD = { good: "on my own", hard: "with help", again: "attempted" } as const;
const ORD = (n: number) => (n === 1 ? "st" : n === 2 ? "nd" : n === 3 ? "rd" : "th");

const dotOf = (p: DsaProblem) => (p.state.last_grade ? (p.state.last_grade === "again" ? "fail" : p.state.last_grade === "hard" ? "help" : "solo") : "");

/** A link to the owner's own notes for this problem (Notion, Obsidian, a file URL). Only the link is kept, as a preference, so it follows
 *  the owner to another laptop; anneal stores no notes. */
function NotesLink({ id }: { id: string }) {
    const key = `notes:${id}`;
    const [url, setUrl] = useState(() => getPref(key, ""));
    const [edit, setEdit] = useState(false);
    const [draft, setDraft] = useState(url);
    useEffect(() => {
        const u = getPref(key, "");
        setUrl(u);
        setDraft(u);
        setEdit(false);
    }, [key]);
    const ok = draft.trim() === "" || /^https?:\/\//i.test(draft.trim());
    const save = () => {
        if (!ok) return;
        setPref(key, draft.trim());
        setUrl(draft.trim());
        setEdit(false);
    };
    return (
        <div className="pp-card pp-notes-card">
            <h3><span>MY NOTES</span><span>a link, kept here</span></h3>
            {edit ? (
                <div className="pp-nrow">
                    <input id="pp-notes-url" aria-label="Link to my notes" placeholder="https://…" value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") save(); else if (e.key === "Escape") setEdit(false); }} autoFocus />
                    <button className="pp-link" disabled={!ok} onClick={save}>Save</button>
                    <button className="pp-link" onClick={() => { setDraft(url); setEdit(false); }}>Cancel</button>
                    {!ok && <small style={{ color: "var(--bad)" }}>Start with http:// or https://</small>}
                </div>
            ) : url ? (
                <div className="pp-nrow">
                    <a className="pp-act alt" href={url} target="_blank" rel="noreferrer">Open my notes ↗</a>
                    <button className="pp-link" onClick={() => setEdit(true)}>Edit</button>
                </div>
            ) : (
                <button className="pp-link" onClick={() => setEdit(true)}>+ Link to my notes</button>
            )}
        </div>
    );
}

/** The owner's own solutions: any number, kept so they can be read later. Written in the same editor as the workspace, so Vim mode works. */
function MySolutions({ p, template }: { p: DsaProblem; template: string }) {
    const qc = useQueryClient();
    const { editor } = useEditorSettings();
    const list = useQuery({ queryKey: ["dsa-solutions", p.id], queryFn: () => api.solutions(p.id) });
    const [editing, setEditing] = useState<number | "new" | null>(null);
    const [draft, setDraft] = useState({ label: "", code: "", notes: "" });
    const [confirm, setConfirm] = useState<number | null>(null);
    const done = () => {
        void qc.invalidateQueries({ queryKey: ["dsa-solutions", p.id] });
        setEditing(null);
    };
    const save = useMutation({
        mutationFn: (code: string) => (editing === "new" ? api.addSolution(p.id, { ...draft, code }) : api.saveSolution(editing as number, { ...draft, code })),
        onSuccess: done,
    });
    const remove = useMutation({ mutationFn: (sid: number) => api.deleteSolution(sid), onSuccess: () => { setConfirm(null); done(); } });
    const open = (s: MySolution | null) => {
        setDraft(s ? { label: s.label, code: s.code, notes: s.notes } : { label: "", code: template, notes: "" });
        setEditing(s ? s.id : "new");
        save.reset();
    };
    const solutions = list.data ?? [];
    const when = (t: string) => new Date(t).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
    return (
        <section className="pp-sec" id="pp-mine">
            <h3><span>MY SOLUTIONS</span><em>{solutions.length ? `${solutions.length} saved` : "yours, kept here"}</em></h3>
            {list.isError && <p className="rempty">Couldn't load your solutions.</p>}
            {solutions.map((s, i) =>
                editing === s.id ? null : (
                    <div key={s.id} className="pp-mine">
                        <div className="pp-mh">
                            <b>{s.label || `Solution ${i + 1}`}</b>
                            <small>{s.updated_at !== s.created_at ? `edited ${when(s.updated_at)}` : `added ${when(s.created_at)}`}</small>
                            <span className="sp" />
                            {confirm === s.id ? (
                                <>
                                    <small>Delete it?</small>
                                    <button className="pp-link bad" onClick={() => remove.mutate(s.id)}>Yes, delete</button>
                                    <button className="pp-link" onClick={() => setConfirm(null)}>Keep</button>
                                </>
                            ) : (
                                <>
                                    <button className="pp-link" onClick={() => open(s)}>Edit</button>
                                    <button className="pp-link" onClick={() => setConfirm(s.id)}>Delete</button>
                                </>
                            )}
                        </div>
                        {s.notes && <Md text={s.notes} />}
                        <PyCode code={s.code} lineNumbers />
                    </div>
                ),
            )}
            {editing !== null && (
                <div className="pp-mine edit">
                    <input className="pp-label" placeholder="Label, e.g. two pointers, or the one I wrote in the interview" maxLength={80} value={draft.label} onChange={(e) => setDraft({ ...draft, label: e.target.value })} />
                    <div className="edit-host pp-ed">
                        <Editor language="python" vim={editor.vim} value={draft.code} docKey={`mine:${p.id}:${editing}`} onChange={(code) => setDraft((d) => ({ ...d, code }))} onSave={(code) => save.mutate(code)} />
                    </div>
                    <textarea className="pp-notes" placeholder="Notes, in markdown: what you'd say about it, what tripped you up" rows={3} value={draft.notes} onChange={(e) => setDraft({ ...draft, notes: e.target.value })} />
                    <div className="pp-mrow">
                        <button className="pp-save" disabled={save.isPending || !draft.code.trim()} onClick={() => save.mutate(draft.code)}>{save.isPending ? "Saving…" : editing === "new" ? "Save solution" : "Save changes"}</button>
                        <button className="pp-link" onClick={() => setEditing(null)}>Cancel</button>
                        <small>{editor.vim ? ":w or ⌘S also saves" : "⌘S also saves"}</small>
                        {save.isError && <small style={{ color: "var(--bad)" }}>{(save.error as Error).message}</small>}
                    </div>
                </div>
            )}
            {editing === null && (
                <button className="pp-add" onClick={() => open(null)}>+ {solutions.length ? "Add another solution" : "Add my solution"}</button>
            )}
            {!solutions.length && editing === null && !list.isLoading && <p className="rempty">Save the solution you submitted, with notes.</p>}
        </section>
    );
}

/** A NeetCode problem: the statement and lesson on the left, every action in a sticky rail on the right. */
export function DsaProblemPage({ slug }: { slug: string }) {
    const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const o = overview.data;
    const { log, reset, undo, toast, busy } = useLogger(o?.today);
    // a problem outside the NeetCode lists is not in the overview: ask for it by itself
    const listed = o?.problems.find((x) => x.slug === slug);
    const outside = useQuery({ queryKey: ["dsa-row", slug], queryFn: () => api.dsaRow(`lc-${slug}`), enabled: !!o && !listed, retry: false });
    const p = listed ?? outside.data;
    const lesson = useQuery({ queryKey: ["dsa-page", p?.id], queryFn: () => api.dsaPage(p!.id), enabled: !!p?.has_page });
    const statement = useQuery({ queryKey: ["dsa-statement", p?.id], queryFn: () => api.statement(p!.id), enabled: !!p, staleTime: Infinity, retry: false });
    const preview = useQuery({ queryKey: ["dsa-preview", p?.id], queryFn: () => api.preview(p!.id), enabled: !!p });
    const [tab, setTab] = useState(0);
    const [practiceMode, setPracticeMode] = useState(false);
    useEffect(() => setPracticeMode(false), [p?.id]);
    const [showIdeas] = useShowIdeas();
    const [revealIdea, setRevealIdea] = useState(false);
    useEffect(() => setRevealIdea(false), [p?.id]);
    const ideaShown = showIdeas || revealIdea;
    const [reading, setReading] = useState(() => getPref("pp.reading", false));
    const [hintsShown, setHintsShown] = useState(0);
    useEffect(() => setHintsShown(0), [p?.id]);
    const [more, setMore] = useState(false);
    // 1 to 4 log, U takes the last log back. The handler is set below, once the page knows what it can log.
    const keys = useRef<(e: KeyboardEvent) => void>(() => {});
    useEffect(() => {
        const f = (e: KeyboardEvent) => keys.current(e);
        window.addEventListener("keydown", f);
        return () => window.removeEventListener("keydown", f);
    }, []);
    const [section, setSection] = useState("problem");
    const hasHints = (statement.data?.hints.length ?? 0) > 0;
    const hasLesson = !!lesson.data;
    const sections = [
        { id: "problem", label: "Problem" },
        ...(hasHints ? [{ id: "hints", label: "Hints" }] : []),
        { id: "idea", label: "Idea" },
        ...(hasLesson ? [{ id: "approaches", label: "Approaches" }] : []),
        { id: "mine", label: "My solutions" },
        ...(hasLesson ? [{ id: "tips", label: "Tips" }] : []),
    ];
    const ids = sections.map((s) => s.id).join();

    // The sub-nav follows the reader: the last section whose top has scrolled past the top of the page.
    useEffect(() => {
        const onScroll = () => {
            let current = "problem";
            for (const id of ids.split(",")) {
                const el = document.getElementById(`pp-${id}`);
                if (el && el.getBoundingClientRect().top < 140) current = id;
            }
            setSection(current);
        };
        onScroll();
        window.addEventListener("scroll", onScroll, { passive: true });
        return () => window.removeEventListener("scroll", onScroll);
    }, [ids]);

    const frame = (body: ReactNode) => (
        <>
            <Header area="dsa" />
            <main className={`page pp${reading ? " reading" : ""}`} style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                {body}
            </main>
            {toast}
        </>
    );
    if (!o || (!p && outside.isLoading)) return frame(<div className="pp-pad">{overview.isError ? <p className="rempty">Couldn't reach the API.</p> : <Skeleton lines={6} />}</div>);
    if (!p) return frame(<p className="notice bad pp-pad">No problem called {slug}. <Link to="/dsa">Back to the list</Link></p>);

    const pattern = o.patterns.find((x) => x.code === p.pattern);
    const technique = o.techniques.find((t) => t.id === p.technique);
    const teacher = p.practice_of ? o.problems.find((x) => x.id === p.practice_of) : undefined;
    const mustLearn = technique ? o.problems.find((x) => x.id === technique.must_learn) : undefined;
    const sameIdea = technique ? o.problems.filter((x) => x.technique === technique.id).sort((a, b) => a.order - b.order) : [];
    const inPattern = o.problems.filter((x) => x.pattern === p.pattern).sort((a, b) => a.order - b.order);
    const at = inPattern.findIndex((x) => x.id === p.id);
    const prev = at > 0 ? inPattern[at - 1] : undefined;
    const next = at >= 0 && at < inPattern.length - 1 ? inPattern[at + 1] : undefined;
    // The class and method line of the first approach, as LeetCode's template gives it, so a new solution starts in the right shape.
    const first = lesson.data?.approaches[0]?.code.split("\n") ?? [];
    const defAt = first.findIndex((l) => l.trimStart().startsWith("def "));
    const template = defAt >= 0 ? `${first.slice(first.findIndex((l) => l.startsWith("class ")), defAt + 1).join("\n")}\n        pass\n` : "class Solution:\n    pass\n";
    keys.current = (e) => {
        if (e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement | null)?.closest?.("input,textarea,select,[contenteditable],.cm-editor")) return;
        const i = "1234".indexOf(e.key);
        if (i >= 0 && e.key) {
            const g = TILES[i]!;
            if (!busy && !(practising && g.grade === "easy")) log(p, g.grade, practising, hintsShown);
        } else if (e.key === "u" || e.key === "U") undo();
    };
    const status = statusOf(p, o.today);
    const s = p.state;
    const previews = preview.data?.previews;
    const recall = s.retrievability != null ? Math.round(s.retrievability * 100) : null;
    const today = o.today;
    const go = (id: string) => document.getElementById(`pp-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
    // Practice is a repeat of a problem already logged; the first log is always a first try, and a LeetCode practice problem schedules nothing anyway.
    const history = preview.data?.history ?? [];
    const canPractice = previews !== undefined && previews !== null && history.length > 0;
    const practising = practiceMode && canPractice;
    const tryNo = history.filter((h) => h.kind !== "practice").length + 1;
    const practised = history.filter((h) => h.kind === "practice").length;
    const gradeNote = (g: Grade) => {
        if (practising) return g === "easy" ? "review grade" : g === "again" ? "logged as a miss" : "+1 practice";
        if (previews === undefined) return "…";
        if (previews === null) return "no review";
        const v = previews[g];
        return v.days === 0 ? "back today" : `back ${inDays(v.days)}`;
    };
    const standing = s.last_grade
        ? status === "due"
            ? [`Review due ${s.due && s.due < today ? "now" : "today"}`, `${s.reps} log${s.reps === 1 ? "" : "s"} · ${s.lapses} lapse${s.lapses === 1 ? "" : "s"}`]
            : s.last_grade === "again"
                ? ["Attempted, not solved", s.due ? `Retry ${niceDate(s.due, today)}` : "Retry soon"]
                : [s.last_grade === "hard" ? "Solved with help" : s.last_grade === "easy" ? "Solved instantly" : "Solved on my own", s.due ? `Next review ${niceDate(s.due, today)}` : `${s.reps} logs`]
        : ["Not started", p.list_tag ? "Solve it on LeetCode, then log how it went. The next review is scheduled for you." : "Solve it on LeetCode, then log how it went. It is tracked, but not part of your goal, the reviews or the calendar."];
    const lists = p.lists.filter((l) => l !== "practice" || p.lists.length === 1).map((l) => LIST_NAME[l]);

    const crumb = (
        <span className="pp-crumbs">
            <Link to="/dsa">DSA</Link>
            <i>/</i>
            {pattern ? <Link to="/dsa/patterns/$code" params={{ code: pattern.code }}>{ideaShown ? pattern.name.toUpperCase() : "PATTERN"}</Link> : <span>?</span>}
            <i>/</i>
            <span>#{p.number}</span>
        </span>
    );
    const title = (
        <span className="pp-titlerow">
            <Mark p={p} today={today} />
            <span>{p.title}</span>
        </span>
    );
    const sub = (
        <span className="pp-sub">
            <span className="pp-badges">
                <ProblemTags p={p} />
                <span className="pp-b">~{MINUTES[p.difficulty]} min</span>
                {lists.map((l) => <span key={l} className="pp-b">{l}</span>)}
                {technique && ideaShown && <span className="pp-b tech">{technique.name}</span>}
            </span>
            {p.tags.length > 0 && <span className="pp-tags">{p.tags.map((t) => <span key={t}>{t}</span>)}</span>}
            {p.insight && (ideaShown ? <span className="pp-ins"><b>Key insight</b>{p.insight}</span> : <span className="pp-ins"><b>Key insight</b><button className="d-reveal" onClick={() => setRevealIdea(true)}>Show</button></span>)}
        </span>
    );
    const center = (
        <div className="pp-main">
            <nav className="pp-subnav" aria-label="Sections">
                {sections.map((x) => (
                    <button key={x.id} className={section === x.id ? "on" : ""} aria-current={section === x.id} onClick={() => go(x.id)}>{x.label}</button>
                ))}
                <button className="pp-read" aria-pressed={reading} title="Hide the side panels and read at a comfortable width" onClick={() => { setReading(!reading); setPref("pp.reading", !reading); }}>Reading mode</button>
            </nav>
                    <section className="pp-sec" id="pp-problem">
                        <h3><span>PROBLEM</span><em>from LeetCode{statement.data?.stale ? ", an older copy" : ""}</em></h3>
                        {statement.isLoading && <p className="rempty">Fetching the statement from LeetCode…</p>}
                        {statement.isError && (
                            <p className="rempty">
                                Couldn't fetch the statement from LeetCode right now. <a href={leetcode(p.slug)} target="_blank" rel="noreferrer" style={{ color: "var(--ca)" }}>Read it on LeetCode ↗</a>
                            </p>
                        )}
                        {statement.data?.locked && (
                            <p className="rempty">
                                This is a LeetCode Premium problem, so LeetCode doesn't share its statement. <a href={leetcode(p.slug)} target="_blank" rel="noreferrer" style={{ color: "var(--ca)" }}>Open it on LeetCode ↗</a>
                            </p>
                        )}
                        {statement.data?.html && <div className="d-statement" dangerouslySetInnerHTML={{ __html: statement.data.html }} />}
                    </section>

                    {hasHints && statement.data && (
                        <section className="pp-sec" id="pp-hints">
                            <h3><span>HINTS</span><em>LeetCode's, one at a time</em></h3>
                            <div className="d-hints">
                                {statement.data.hints.map((h, i) =>
                                    i < hintsShown ? (
                                        <div key={i} className="d-hint open">
                                            <b>Hint {i + 1}</b>
                                            <div className="d-statement" dangerouslySetInnerHTML={{ __html: h }} />
                                        </div>
                                    ) : (
                                        <button key={i} className="d-hint" disabled={i > hintsShown} onClick={() => setHintsShown(i + 1)}>
                                            <b>Hint {i + 1}</b>
                                            <span>{i === hintsShown ? "click to reveal" : "reveal the one before first"}</span>
                                        </button>
                                    ),
                                )}
                            </div>
                        </section>
                    )}

                    <section className="pp-sec" id="pp-idea">
                        <h3><span>THE IDEA</span>{technique && <em>{technique.name}</em>}</h3>
                        {technique && (
                            <p className="pp-lead">
                                <b>{technique.name}</b>.{" "}
                                {p.role === "must_learn" ? "This is the problem that teaches it first." : teacher ? <>Practice for <Link to="/d/$slug" params={{ slug: teacher.slug }} style={{ color: "var(--ca)" }}>{teacher.title}</Link>, which teaches it.</> : null}{" "}
                                {technique.problems} problem{technique.problems === 1 ? "" : "s"} use it.
                                {pattern && <> <Link to="/dsa/patterns/$code" params={{ code: pattern.code }} style={{ color: "var(--ca)" }}>Read the {pattern.name} lesson ›</Link></>}
                            </p>
                        )}
                        {mustLearn && mustLearn.id !== p.id && !teacher && (
                            <p className="pp-lead">Starts with <Link to="/d/$slug" params={{ slug: mustLearn.slug }} style={{ color: "var(--ca)" }}>{mustLearn.title}</Link>.</p>
                        )}
                        {lesson.data && <Md text={lesson.data.intuition} />}
                        {!p.has_page && <p className="rempty">No written lesson yet. Solve it on LeetCode, then log it.</p>}
                    </section>

                    {lesson.data && (
                        <>
                            <section className="pp-sec" id="pp-approaches">
                                <h3><span>APPROACHES</span><em>Python, pastes into LeetCode</em></h3>
                                <div className="pp-tabs" role="tablist">
                                    {lesson.data.approaches.map((a, i) => (
                                        <button key={a.name} role="tab" aria-selected={i === tab} className={i === tab ? "on" : ""} onClick={() => setTab(i)}>{a.label}</button>
                                    ))}
                                </div>
                                {lesson.data.approaches.map((a, i) =>
                                    i === tab ? (
                                        <div key={a.name} className="d-ap">
                                            <h3 className="pp-apname">{a.name}</h3>
                                            <Md text={a.idea} />
                                            <PyCode code={a.code} lineNumbers />
                                            <div className="pp-cx">
                                                <span>time <b>{a.time}</b></span>
                                                <span>space <b>{a.space}</b></span>
                                            </div>
                                            {(a.time_why || a.space_why || a.note) && (
                                                <dl className="pp-why">
                                                    {a.time_why && <><dt>Why time is {a.time}</dt><dd><Md inline text={a.time_why} /></dd></>}
                                                    {a.space_why && <><dt>Why space is {a.space}</dt><dd><Md inline text={a.space_why} /></dd></>}
                                                    {a.note && <><dt>Note</dt><dd><Md inline text={a.note} /></dd></>}
                                                </dl>
                                            )}
                                        </div>
                                    ) : null,
                                )}
                            </section>
                            <MySolutions p={p} template={template} />
                            <section className="pp-sec" id="pp-tips">
                                <h3><span>TIPS AND PITFALLS</span></h3>
                                <ul className="d-tips">
                                    {lesson.data.tips.map((t) => (
                                        <li key={t}><Md inline text={t} /></li>
                                    ))}
                                </ul>
                            </section>
                        </>
                    )}

                    {!lesson.data && <MySolutions p={p} template={template} />}

        </div>
    );
    const left = (
        <>
                    {sameIdea.length > 1 && (
                        <div className="pp-card">
                            <h3><span>SAME IDEA</span><span>{sameIdea.length} problems</span></h3>
                            <div className="pp-rel">
                                {(more ? sameIdea : sameIdea.slice(0, SAME_IDEA_SHOWN)).map((x) => (
                                    <Link key={x.id} to="/d/$slug" params={{ slug: x.slug }} className={x.id === p.id ? "now" : ""}>
                                        <i className={dotOf(x)} />
                                        <span>{x.title}</span>
                                        <small>{x.id === p.id ? "THIS" : x.role === "must_learn" ? "MUST" : DIFF[x.difficulty][0][0]}</small>
                                    </Link>
                                ))}
                            </div>
                            {sameIdea.length > SAME_IDEA_SHOWN && (
                                <button className="pp-more" onClick={() => setMore(!more)}>{more ? "show fewer" : `+${sameIdea.length - SAME_IDEA_SHOWN} more`}</button>
                            )}
                        </div>
                    )}

                    {p.premium && (() => {
                        const free = (technique ? o.problems.filter((x) => x.technique === technique.id && !x.premium && x.id !== p.id) : o.problems.filter((x) => x.pattern === p.pattern && !x.premium && x.id !== p.id)).sort((a, b) => Math.abs(DIFFS.indexOf(a.difficulty) - DIFFS.indexOf(p.difficulty)) - Math.abs(DIFFS.indexOf(b.difficulty) - DIFFS.indexOf(p.difficulty)) || a.order - b.order).slice(0, 3);
                        return free.length ? (
                            <div className="pp-card">
                                <h3><span>FREE INSTEAD</span><span>same idea, no Premium</span></h3>
                                <div className="pp-rel">
                                    {free.map((x) => (
                                        <Link key={x.id} to="/d/$slug" params={{ slug: x.slug }}>
                                            <i className={dotOf(x)} />
                                            <span>{x.title}</span>
                                            <small>{DIFF[x.difficulty][0][0]}</small>
                                        </Link>
                                    ))}
                                </div>
                            </div>
                        ) : null;
                    })()}

            {(prev || next) && (
                <div className="pp-pn">
                    {prev ? <Link to="/d/$slug" params={{ slug: prev.slug }}><small>← PREVIOUS IN {ideaShown ? pattern?.name.toUpperCase() : "THIS PATTERN"}</small><b>{prev.title}</b></Link> : <span />}
                    {next ? <Link to="/d/$slug" params={{ slug: next.slug }}><small>NEXT IN {ideaShown ? pattern?.name.toUpperCase() : "THIS PATTERN"} →</small><b>{next.title}</b></Link> : <span />}
                </div>
            )}
        </>
    );
    const right = (
        <aside className="pp-rail">
                    <div className="pp-card">
                        <div className="pp-solve">
                            <a className="pp-act pri" href={leetcode(p.slug)} target="_blank" rel="noreferrer">Solve on LeetCode ↗</a>
                            {p.video && <a className="pp-act alt" href={videoUrl(p.video)} target="_blank" rel="noreferrer" title="NeetCode's video">▶ Video</a>}
                        </div>
                        <h3><span>LOG IT</span>{previews === null ? <span>schedules no review</span> : practising ? <span>practice · schedule unchanged</span> : null}</h3>
                        {canPractice && (
                            <>
                                <div className="pp-seg" role="group" aria-label="This time">
                                    <button className={practising ? "" : "on"} aria-pressed={!practising} onClick={() => setPracticeMode(false)}>Try {tryNo}<small>scheduled review</small></button>
                                    <button className={practising ? "on" : ""} aria-pressed={practising} onClick={() => setPracticeMode(true)}>Practice<small>extra repeat, +1</small></button>
                                </div>
                            </>
                        )}
                        <div className="pp-log">
                            {TILES.map((g) => (
                                <button key={g.grade} className={`lg ${g.cls}${!practising && s.last_grade === g.grade ? " on" : ""}${hintsShown > 0 && g.grade === "hard" ? " sug" : ""}`} disabled={busy || (practising && g.grade === "easy")} onClick={() => log(p, g.grade, practising, hintsShown)}>
                                    <b><GradeGlyph grade={g.grade} /></b><span>{g.label}</span><small>{gradeNote(g.grade)}</small><kbd>{TILES.indexOf(g) + 1}</kbd>
                                </button>
                            ))}
                        </div>
                        {history.length > 0 && (
                            <div className="pp-hist" aria-label="This problem's tries">
                                <span>THIS PROBLEM</span>
                                {history.map((h, i) => (
                                    <i key={i} className={`${h.kind === "practice" ? "pr" : RESULT_CLASS[h.result]}`} title={`${h.kind === "first" ? "First try" : h.kind === "later" ? "Later try" : "Practice"}: ${RESULT_WORD[h.result]}`}>
                                        {h.kind === "practice" ? "P " : `${history.slice(0, i + 1).filter((x) => x.kind !== "practice").length}${ORD(history.slice(0, i + 1).filter((x) => x.kind !== "practice").length)} `}
                                        {RESULT_GLYPH[h.result]}
                                    </i>
                                ))}
                                {practised > 0 && <em>practised {practised}×</em>}
                            </div>
                        )}
                        <div className="pp-status">
                            <div className="pp-gauge" style={{ "--p": `${recall ?? 0}%` } as CSSProperties}><span>{recall != null ? `${recall}%` : "–"}</span></div>
                            <div><b>{standing[0]}</b><small>{standing[1]}{s.last_grade && recall != null && status !== "due" ? ` · ${recall}% recall now` : ""}</small></div>
                        </div>
                        {(s.last_grade || history.length > 0) && (
                            <div className="pp-reset">
                                <ConfirmButton label="Reset progress" confirm="Yes, reset" note="Remove this problem's log and review? Your saved solutions stay." onConfirm={() => reset(p)} />
                            </div>
                        )}
                    </div>

                    <NotesLink id={p.id} />

                    {s.last_grade && previews !== null && <RecallCurve id={p.id} version={s.reps} />}

                    <FocusTimer suggest={MINUTES[p.difficulty]} />

                    {p.companies.length > 0 && (
                        <div className="pp-card">
                            <h3><span>ASKED AT</span><span>highlighted: last six months</span></h3>
                            <Companies companies={p.companies} limit={10} />
                        </div>
                    )}
        </aside>
    );

    return (
        <>
            <Header area="dsa" />
            <div style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <Page3 className={`pp${reading ? " reading" : ""}`} crumb={[crumb]} title={title} sub={sub} left={reading ? undefined : left} center={center} right={reading ? undefined : right} label={`#${p.number}`} />
            </div>
            {toast}
        </>
    );
}
