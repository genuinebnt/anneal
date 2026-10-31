// The stage page's Hints, Solution, Concepts and Last run tabs, in the mockup's markup (docs/mockups/course-motion.html, screen 2).

import { Link } from "@tanstack/react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, type CourseRun, type CourseStagePage as Page, type SolutionFile } from "../../api";
import { handleCodeClick, handleCodeKey, highlightDiffLines, initCodeTabs, renderMd } from "../courseMd";
import { MockCopy } from "../../components/mock";
import { toast } from "../../components/toasts";
import { ago } from "./shared";

/** Markdown rendered with the mockup's components; the code tabs measure their underline once they are in the page. */
export function Prose({ text, className = "k-prose" }: { text: string; className?: string }) {
    const ref = useRef<HTMLDivElement>(null);
    const html = renderMd(text);
    useLayoutEffect(() => {
        initCodeTabs(ref.current);
    }, [html]);
    return <div ref={ref} className={className} onClick={handleCodeClick} onKeyDown={handleCodeKey} dangerouslySetInnerHTML={{ __html: html }} />;
}

/* ---------------------------------------------------------------- hints */

export function HintsTab({ page, onOpen, pending }: { page: Page; onOpen: () => void; pending: boolean }) {
    const { hints, state } = page;
    // The hints you have already opened start open; the one you open now opens with the same slide.
    const [shut, setShut] = useState<Set<number>>(() => new Set());
    if (hints.total === 0) return <div className="k-empty">No hints written for this stage yet.</div>;
    return (
        <>
            <p className="k-quiet" style={{ margin: "0 0 14px", color: "var(--mut)", maxWidth: "62ch" }}>
                {hints.revealed.length} of {hints.total} opened. Each hint opened before the stage passes marks it as assisted. They get deeper: the first nudges the design, the last names the invariant to check.
            </p>
            {Array.from({ length: hints.total }, (_, i) => {
                const h = hints.revealed[i];
                const next = i === hints.revealed.length;
                const open = !!h && !shut.has(i);
                return (
                    <div key={i} className={`k-hint${open ? " k-open" : ""}`} id={`h${i + 1}`}>
                        <button
                            className="k-hh"
                            aria-expanded={open}
                            onClick={() => {
                                if (h) setShut((s) => (s.has(i) ? new Set([...s].filter((x) => x !== i)) : new Set(s).add(i)));
                                else if (next) onOpen();
                                else toast("in", "Open the previous hint first");
                            }}
                            disabled={pending && next}
                        >
                            <span>
                                Hint {i + 1}
                                {h ? ` · ${h.title}` : ""}
                            </span>
                            <span className={`k-cost${h ? "" : " k-lock"}`}>{h ? "opened" : next ? (state === "todo" ? "marks the stage as assisted" : "open it") : `🔒 open after hint ${i}`}</span>
                        </button>
                        <div className="k-hb">
                            <div inert={!open}>{h && <Prose text={h.md} className="k-prose k-hbody" />}</div>
                        </div>
                    </div>
                );
            })}
        </>
    );
}

/* ------------------------------------------------------------- solution */

export function SolutionTab({ page, onOpen, pending }: { page: Page; onOpen: () => void; pending: boolean }) {
    const { solution, state } = page;
    const opened = solution.open && !!solution.files;
    // A stand-in for the code behind the blur until it is opened.
    const dummy: SolutionFile[] = [{ path: "", lines: [" pub fn solution(&mut self) -> Result<()> {", "     let page = self.pool.fetch(id)?;", "     page.write(offset, bytes);", "     Ok(())", " }"] }];
    const files = opened ? (solution.files as SolutionFile[]) : dummy;
    return (
        <>
            <p className="k-prose" style={{ color: "var(--mut)", maxWidth: "60ch", marginBottom: 14 }}>
                {opened ? "One way to write it, as a diff against your starter code. Yours only has to pass the tests." : "Seen it already? The solution is hidden until you ask, and opening it is recorded (assisted solve)."}
            </p>
            {files.map((f, n) => (
                <div className={`k-sol${opened ? " k-rev" : ""}`} key={f.path || n} style={{ marginBottom: 12 }}>
                    {f.path && <div className="k-solf">{f.path}</div>}
                    <pre aria-hidden={!opened}>
                        {highlightDiffLines(f.lines).map((html, i) => {
                            const l = f.lines[i] ?? "";
                            return (
                            <span key={i} className={`k-l${l.startsWith("+") ? " k-add" : l.startsWith("-") ? " k-del" : ""}`}>
                                {l.slice(0, 1) || " "}
                                <span dangerouslySetInnerHTML={{ __html: html || " " }} />
                            </span>
                            );
                        })}
                    </pre>
                    {!opened && n === 0 && (
                        <div className="k-gate">
                            {solution.available ? (
                                <>
                                    <p>{state === "todo" ? "Try the hints first. Opening the solution before the stage passes marks it as assisted." : "Unlocked now that the stage has passed."}</p>
                                    <button className="k-cta k-fx" style={{ justifySelf: "center" }} onClick={onOpen} disabled={pending}>
                                        <span className="k-lbl">Reveal solution</span>
                                    </button>
                                </>
                            ) : page.stage.kind === "challenge" ? (
                                <p>This is a challenge: there is no solution to reveal. The tests are properties, so any correct design passes. Work it out with what the modules before it taught.</p>
                            ) : page.stage.kind === "boss" ? (
                                <p>A boss stage has no code of its own to write: it runs the tests of the stages before it, and BusTub's own tests. There is no solution to reveal.</p>
                            ) : (
                                <p>
                                    Not uploaded to this app yet. From your clone of the repo: <code>anneal course login {location.origin}</code>, then <code>anneal course solutions</code>.
                                </p>
                            )}
                        </div>
                    )}
                </div>
            ))}
        </>
    );
}

/* ------------------------------------------------------------- concepts */

export function ConceptsTab({ course, page, queryKey }: { course: string; page: Page; queryKey: unknown[] }) {
    const qc = useQueryClient();
    const [flt, setFlt] = useState<"all" | "req" | "opt">("all");
    const [open, setOpen] = useState<string | null>(null);
    const mark = useMutation({
        mutationFn: ({ id, read }: { id: string; read: boolean }) => api.setConceptRead(course, id, read),
        onMutate: async ({ id, read }) => {
            // A poll that is already on its way would bring back the old value over this change.
            await qc.cancelQueries({ queryKey });
            qc.setQueryData<Page>(queryKey, (old) => old && { ...old, concepts: old.concepts.map((k) => (k.id === id ? { ...k, read } : k)) });
            toast(read ? "ok" : "in", read ? "Marked as read" : "Marked as unread");
        },
        onSettled: () => qc.invalidateQueries({ queryKey }),
    });
    const req = page.concepts.filter((k) => k.required);
    const done = req.filter((k) => k.read).length;
    const left = req.filter((k) => !k.read).reduce((n, k) => n + k.minutes, 0);
    const list = page.concepts.filter((k) => flt === "all" || (flt === "req") === k.required);
    const { module } = page;
    return (
        <>
            {page.concepts.length > 0 && (
                <>
                    <div className="k-chdr">
                        <div className="k-cring2" style={{ "--p": req.length ? Math.round((100 * done) / req.length) : 100 } as React.CSSProperties}>
                            <span>
                                {done}/{req.length}
                            </span>
                        </div>
                        <div style={{ flex: 1, minWidth: 0 }}>
                            <b style={{ fontSize: 17, letterSpacing: "-.01em" }}>{req.length === 0 ? "Rust and systems notes: all optional" : done === req.length ? "Required reading done" : `Required reading: ${done} of ${req.length} done`}</b>
                            <div style={{ color: "var(--dim)", fontSize: 13.5, marginTop: 2 }}>
                                {req.length === 0 ? "Pull one in when you want it: an idea you have not met, a compiler error you do not understand, or a design question. The stage works without them." : done === req.length ? "Nice. The optional articles go deeper when you want them." : `About ${left} minutes left. You can pass the stage without it; it saves you the hints.`}
                            </div>
                        </div>
                        {page.concepts.some((k) => !k.required) && (
                            <div className="k-grp" role="group" aria-label="Show">
                                {([["all", "All"], ["req", "Required"], ["opt", "Optional"]] as const).map(([k, l]) => (
                                    <button key={k} aria-pressed={flt === k} className={flt === k ? "k-on" : ""} onClick={() => setFlt(k)}>
                                        {l}
                                    </button>
                                ))}
                            </div>
                        )}
                    </div>
                    {list.map((k) => (
                        <article key={k.id} className={`k-ccard${k.read ? " k-isread" : ""}${open === k.id ? " k-open" : ""}`} data-id={k.id}>
                            <div className="k-cch" onClick={(e) => !(e.target as HTMLElement).closest(".k-cact") && setOpen(open === k.id ? null : k.id)}>
                                <span className="k-cic">{k.read ? "✓" : page.concepts.indexOf(k) + 1}</span>
                                <div style={{ minWidth: 0 }}>
                                    <h4>
                                        {k.title} <span className={`k-badge2 ${k.required ? "k-req" : "k-op"}`}>{k.required ? "REQUIRED" : "OPTIONAL"}</span>
                                    </h4>
                                    <p>{k.summary}</p>
                                    <div className="k-cmeta">
                                        <span>{k.minutes} min read</span>
                                        <span className="k-mp2">
                                            <i style={{ width: k.read ? "100%" : "0%" }} />
                                        </span>
                                        <span>{k.read ? "read" : "not read yet"}</span>
                                    </div>
                                </div>
                                <div className="k-cact">
                                    <button className="k-cta k-sm k-sec k-fx" aria-expanded={open === k.id} onClick={() => setOpen(open === k.id ? null : k.id)}>
                                        <span className="k-lbl">{open === k.id ? "Close" : "Preview"}</span>
                                    </button>
                                    <Link className="k-cta k-sm k-fx" to="/courses/$course/concept/$id" params={{ course, id: k.id }}>
                                        <span className="k-lbl">{k.read ? "Open" : "Read"}</span>
                                        <span className="k-ar">›</span>
                                    </Link>
                                </div>
                            </div>
                            <div className="k-cprev">
                                <div inert={open !== k.id}>
                                    <div className="k-cpi">
                                        <div>
                                            <h6>IN SHORT</h6>
                                            <p style={{ margin: 0, color: "var(--mut)", fontSize: 14, maxWidth: "62ch" }}>{k.summary}</p>
                                        </div>
                                    </div>
                                    <div className="k-cfoot">
                                        <label style={{ display: "flex", gap: 10, alignItems: "center", fontSize: 13.5, color: "var(--mut)" }}>
                                            <button type="button" role="switch" aria-checked={k.read} className={`k-sw${k.read ? " k-on" : ""}`} aria-label={`Mark "${k.title}" as read`} onClick={() => mark.mutate({ id: k.id, read: !k.read })} />
                                            Mark as read
                                        </label>
                                        <span className="k-spacer" />
                                        <Link className="k-cta k-sm k-fx" to="/courses/$course/concept/$id" params={{ course, id: k.id }}>
                                            <span className="k-lbl">Open article</span>
                                            <span className="k-ar">→</span>
                                        </Link>
                                    </div>
                                </div>
                            </div>
                        </article>
                    ))}
                </>
            )}
            <div className="k-flab2">FURTHER READING · {module.code.toUpperCase()}</div>
            <div className="k-rowf">
                {module.lectures.map((l) => (
                    <a key={l.id} className="k-cp" href={l.video ?? l.slides} target="_blank" rel="noreferrer" title={[l.slides && "slides", l.notes && "notes", l.video && "video"].filter(Boolean).join(" · ")}>
                        CMU 15-445 · {l.title}
                    </a>
                ))}
                {module.resources.map((r) => (
                    <a key={r.url} className="k-cp" href={r.url} target="_blank" rel="noreferrer">
                        {r.title}
                    </a>
                ))}
                {module.bustub.map((u) => (
                    <a key={u} className="k-cp" href={u} target="_blank" rel="noreferrer" title={u.replace("https://github.com/cmu-db/bustub/blob/master/", "")}>
                        BusTub · {u.split("/").slice(-1)[0]}
                    </a>
                ))}
            </div>
        </>
    );
}

/* -------------------------------------------------------------- last run */

/** A test's source, folded: the result row's own button opens it. Looked up by the full name, then by its last path segment. */
function sourceOf(sources: Record<string, string>, name: string): string | undefined {
    return sources[name] ?? sources[name.split("::").pop() ?? name];
}

function TestCode({ src }: { src: string }) {
    return (
        <div className="k-tcode" data-testid="test-code">
            <Prose text={"```rust\n" + src + "\n```"} />
        </div>
    );
}

export function RunTab({ runs, stageId, sources = {} }: { runs: CourseRun[]; stageId: string; sources?: Record<string, string> }) {
    // Tests whose code the learner flipped: a failed test shows its code until it is hidden, a passed one only once it is opened.
    const [flipped, setFlipped] = useState<Set<string>>(new Set());
    const toggleCode = (name: string) => setFlipped((prev) => (prev.has(name) ? new Set([...prev].filter((x) => x !== name)) : new Set(prev).add(name)));
    const codeShown = (name: string, bad: boolean) => bad !== flipped.has(name);
    const [sel, setSel] = useState<number | null>(null);
    const newest = runs[0];
    // A run that arrives while the tab is open brings you back to the newest.
    useEffect(() => {
        setSel(null);
    }, [newest?.id]);
    const [flt, setFlt] = useState<"all" | "bad" | "ok">("all");
    const [cmp, setCmp] = useState(false);
    const [passOpen, setPassOpen] = useState(false);
    if (!newest) return null;
    const run = runs.find((r) => r.id === sel) ?? newest;
    const idx = runs.findIndex((r) => r.id === run.id);
    const prev = runs[idx + 1];
    const failed = run.tests.map((t, i) => ({ ...t, i })).filter((t) => !t.ok);
    const passed = run.tests.filter((t) => t.ok);
    const before = new Map((prev?.tests ?? []).map((t) => [t.name, t.ok]));
    const change = (name: string, ok: boolean) => (!prev || prev.problem || !before.has(name) ? "" : before.get(name) === ok ? "same" : ok ? "fix" : "new");
    const prevPassed = prev && !prev.problem ? prev.passed : null;
    const newlyFailing = failed.filter((t) => change(t.name, false) === "new").length;
    const same = new Map<string, number>();
    for (const t of failed) same.set(t.detail.trim(), (same.get(t.detail.trim()) ?? 0) + 1);
    const first = failed[0];
    const cmd = run.problem ? `anneal course test ${stageId}` : first ? `anneal course test ${stageId} --only -f ${first.name}` : null;
    const errors = run.problem ? (run.problem.match(/^error(\[|:)/gm) ?? []).length : 0;
    const tone = run.problem ? "k-wr" : failed.length > 0 ? "k-bad" : "k-ok";
    const shown = [...runs].reverse();
    return (
        <>
            {runs.length > 1 && (
                <div className="k-rhist">
                    <div className="k-rl" role="group" aria-label="Recent runs">
                        {shown.map((r) => (
                            <div key={r.id} className={`k-rn${r.id === run.id ? " k-on" : ""}`} role="button" tabIndex={0} aria-pressed={r.id === run.id} onClick={() => setSel(r.id)} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), setSel(r.id))} aria-label={`Run ${ago(r.at)}: ${r.problem ? "did not run" : `${r.passed} of ${r.total} passed`}`}>
                                <div className="k-bar3">
                                    <i className={r.problem ? "k-bad" : ""} style={{ height: r.problem ? "100%" : `${r.total ? Math.round((100 * r.passed) / r.total) : 0}%` }} />
                                </div>
                                <small>{r.problem ? "—" : `${r.passed}/${r.total}`}</small>
                                <em>{ago(r.at)}</em>
                            </div>
                        ))}
                    </div>
                </div>
            )}
            <div className={`k-rsm ${tone}`}>
                <div className="k-rtp">
                    <span className="k-rbig">
                        {run.problem ? (
                            <em>Did not run</em>
                        ) : failed.length > 0 ? (
                            <>
                                <em>{failed.length} failed</em> · {passed.length} passed
                            </>
                        ) : (
                            <em>
                                {run.passed} of {run.total} passed
                            </em>
                        )}
                    </span>
                    <span className="k-rmeta">
                        {ago(run.at)}
                        {run.commit_sha ? ` · commit ${run.commit_sha.slice(0, 7)}` : ""} · {(run.duration_ms / 1000).toFixed(1)}s
                    </span>
                </div>
                {!run.problem && prev && (
                    <div className="k-dlt">
                        {prev.problem && <span className="k-eq">first run that compiled</span>}
                        {prevPassed !== null && run.passed > prevPassed && <span className="k-up">▲ {run.passed - prevPassed} more passing</span>}
                        {prevPassed !== null && run.passed < prevPassed && <span className="k-dn">▼ {prevPassed - run.passed} fewer passing</span>}
                        {prevPassed !== null && run.passed === prevPassed && <span className="k-eq">same result as the run before</span>}
                        {newlyFailing > 0 && <span className="k-dn">{newlyFailing} newly failing</span>}
                    </div>
                )}
                {!run.problem && run.tests.length > 0 && (
                    <div className="k-rsb" aria-hidden="true">
                        {run.tests.map((t, i) => (
                            <i key={i} className={t.ok ? "" : "k-f"} style={{ "--k": i } as React.CSSProperties} />
                        ))}
                    </div>
                )}
                {cmd && (
                    <div className="k-rcmd">
                        <span>{run.problem ? "compile locally" : "run again"}</span>
                        <code>{cmd}</code>
                        <MockCopy text={cmd} className="k-copy k-fx" />
                    </div>
                )}
            </div>
            <div className="k-rtool">
                {!run.problem && run.tests.length > 0 && (
                    <div className="k-grp" role="group" aria-label="Show">
                        {([["all", "All"], ["bad", "Failed"], ["ok", "Passed"]] as const).map(([k, l]) => (
                            <button key={k} aria-pressed={flt === k} className={flt === k ? "k-on" : ""} onClick={() => setFlt(k)}>
                                {l}
                            </button>
                        ))}
                    </div>
                )}
                <span className="k-spacer" />
                {prev && !prev.problem && !run.problem && (
                    <label className="k-cmpl">
                        <button type="button" role="switch" aria-checked={cmp} className={`k-sw${cmp ? " k-on" : ""}`} onClick={() => setCmp(!cmp)} aria-label="Compare with the previous run" />
                        Compare with previous run
                    </label>
                )}
            </div>
            <div>
                {run.problem && (
                    <div className="k-rcomp">
                        <div className="k-rth">
                            <span className="k-ico" style={{ color: "var(--warn)" }}>!</span>
                            <b>{errors > 0 ? `${errors} error${errors === 1 ? "" : "s"}` : "no tests ran"}</b>
                            <span className="k-rmeta">as the compiler printed it</span>
                        </div>
                        <pre>
                            {run.problem.split("\n").map((l, i) => (
                                <span key={i} className={/^error(\[|:)/.test(l) ? "k-er" : /^\s*-->/.test(l) ? "k-pt2" : /^\s*(\d+\s*)?\|/.test(l) ? "k-dm2" : undefined}>
                                    {l}
                                    {"\n"}
                                </span>
                            ))}
                        </pre>
                    </div>
                )}
                {failed.length > 0 && flt !== "ok" && (
                    <>
                        <div className="k-flab2" style={{ color: "var(--bad)", marginTop: 6 }}>FAILED · {failed.length}</div>
                        {failed.map((t) => {
                            const n = (same.get(t.detail.trim()) ?? 1) - 1;
                            const ch = change(t.name, false);
                            return (
                                <div className="k-rt k-bad" key={t.name}>
                                    <div className={`k-rth${sourceOf(sources, t.name) ? " k-rclick" : ""}`} onClick={() => sourceOf(sources, t.name) && toggleCode(t.name)}>
                                        <span className="k-ico k-bad">✕</span>
                                        <b>{t.name}</b>
                                        {cmp && ch ? <span className={`k-cm2 k-${ch}`}>{ch === "new" ? "NEWLY FAILING" : "STILL FAILING"}</span> : <span className="k-rmeta">test {t.i + 1} of {run.tests.length}</span>}
                                        {sourceOf(sources, t.name) && (
                                            <button type="button" className="k-tcb" aria-expanded={codeShown(t.name, true)} onClick={(e) => { e.stopPropagation(); toggleCode(t.name); }}>
                                                {codeShown(t.name, true) ? "hide test code" : "show test code"}
                                            </button>
                                        )}
                                    </div>
                                    {t.detail && (
                                        <div className="k-rtb">
                                            {n > 0 && (
                                                <div className="k-same2">
                                                    same message as {n} other test{n === 1 ? "" : "s"}
                                                </div>
                                            )}
                                            <pre>{t.detail}</pre>
                                        </div>
                                    )}
                                    {codeShown(t.name, true) && sourceOf(sources, t.name) && <TestCode src={sourceOf(sources, t.name)!} />}
                                </div>
                            );
                        })}
                    </>
                )}
                {!run.problem && failed.length === 0 && flt === "bad" && (
                    <div className="k-rt">
                        <div className="k-rth">
                            <span className="k-ico k-ok">✓</span>
                            <b>Nothing failed in this run</b>
                            <span />
                        </div>
                    </div>
                )}
                {passed.length > 0 && flt !== "bad" && (
                    <>
                        <div className="k-flab2" style={{ color: "var(--grn)" }}>PASSED · {passed.length}</div>
                        <div className={`k-rpass${passOpen || flt === "ok" ? " k-open" : ""}`}>
                            <button className="k-rph" aria-expanded={passOpen || flt === "ok"} onClick={() => setPassOpen(!passOpen)}>
                                <span className="k-ico k-ok">✓</span>
                                <span>{flt === "ok" ? "all passing tests" : failed.length === 0 ? "all tests" : passed.slice(0, 2).map((t) => t.name).join(", ") + (passed.length > 2 ? ` and ${passed.length - 2} more` : "")}</span>
                                <span className="k-rmeta">{flt === "ok" ? "" : "show"}</span>
                            </button>
                            <div className="k-rpl">
                                <div inert={!(passOpen || flt === "ok")}>
                                    {passed.map((t) => (
                                        <div className="k-rwrap" key={t.name}>
                                            <div className={`k-rrow${sourceOf(sources, t.name) ? " k-rclick" : ""}`} onClick={() => sourceOf(sources, t.name) && toggleCode(t.name)}>
                                                <span className="k-ico k-ok">✓</span>
                                                <span>{t.name}</span>
                                                {cmp && change(t.name, true) === "fix" ? <span className="k-cm2 k-fix">FIXED SINCE THE RUN BEFORE</span> : <span />}
                                                {sourceOf(sources, t.name) && (
                                                    <button type="button" className="k-tcb" aria-expanded={codeShown(t.name, false)} onClick={(e) => { e.stopPropagation(); toggleCode(t.name); }}>
                                                        {codeShown(t.name, false) ? "hide test code" : "show test code"}
                                                    </button>
                                                )}
                                            </div>
                                            {codeShown(t.name, false) && sourceOf(sources, t.name) && <TestCode src={sourceOf(sources, t.name)!} />}
                                        </div>
                                    ))}
                                </div>
                            </div>
                        </div>
                    </>
                )}
            </div>
        </>
    );
}
