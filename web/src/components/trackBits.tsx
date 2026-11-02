// Pieces the Rust track pages and the workspace share: the interview follow-up questions as a list, and a problem brief (what it
// teaches, what you will be asked after). Styles: styles/rust.css.
import { marked } from "marked";
import { useState } from "react";
import type { RunView, StageView, TestOutcome } from "../api";

/** A problem's follow-up text, "1) ... 2) ...", as separate questions. Text without numbers is one question. */
export function splitQuestions(text: string): string[] {
    const parts = text.split(/(?:^|\s)\d+\)\s+/).map((s) => s.trim()).filter(Boolean);
    return parts.length ? parts : [text];
}

const inline = (s: string) => ({ __html: marked.parseInline(s, { async: false }) as string });

/** The follow-up questions an interviewer asks once the code works: a numbered list. `label` names them ("Interview follow-up", "Asked as"). */
export function FollowUps({ text, label = "INTERVIEW FOLLOW-UP", compact = false }: { text: string; label?: string; compact?: boolean }) {
    const qs = splitQuestions(text);
    return (
        <div className={`rt-fu${compact ? " compact" : ""}`}>
            <div className="lab">{label}</div>
            <ol>
                {qs.map((q, i) => (
                    <li key={i}>
                        <i>{i + 1}</i>
                        <span dangerouslySetInnerHTML={inline(q)} />
                    </li>
                ))}
            </ol>
        </div>
    );
}

/** What a problem teaches, as a list of short lines. */
export function Teaches({ items }: { items: string[] }) {
    return (
        <ul className="rt-teach">
            {items.map((t) => (
                <li key={t} dangerouslySetInnerHTML={inline(t)} />
            ))}
        </ul>
    );
}

/** A folded section: a title with a count, opened by a click. */
export function Fold({ title, n, children, start = false }: { title: string; n?: number; children: React.ReactNode; start?: boolean }) {
    const [open, setOpen] = useState(start);
    return (
        <div className={`rt-fold${open ? " open" : ""}`}>
            <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>
                <span>{title}</span>
                {n !== undefined && <small>{n}</small>}
                <i aria-hidden="true">{open ? "−" : "+"}</i>
            </button>
            {open && <div className="rt-fb">{children}</div>}
        </div>
    );
}

// ---------------------------------------------------------------- phases

export type Phase = "learn" | "build" | "practice";
export const PHASES: { key: Phase; name: string; note: string }[] = [
    { key: "learn", name: "Learn", note: "understand the idea by using it" },
    { key: "build", name: "Build", note: "write the structure yourself" },
    { key: "practice", name: "Practice", note: "apply it in new places" },
];

/** Which phase a stage belongs to, from its slug ("understand-it", "use-it", "build-it" in the written tracks). Stages that follow no such name have none. */
export function phaseOf(stage: Pick<StageView, "slug" | "name">): Phase | null {
    const s = stage.slug.toLowerCase();
    if (s.startsWith("understand")) return "learn";
    if (s.startsWith("build")) return "build";
    if (s.startsWith("use") || s.startsWith("practi")) return "practice";
    return null;
}

/** The three phases of a track as one bar: how much of each is solved. Phases with no stage in the track are left out. */
export function PhaseBar({ stages }: { stages: StageView[] }) {
    const rows = PHASES.map((ph) => {
        const ss = stages.filter((s) => phaseOf(s) === ph.key);
        return { ...ph, total: ss.reduce((n, s) => n + s.total, 0), solved: ss.reduce((n, s) => n + s.solved, 0), n: ss.length };
    }).filter((r) => r.n > 0);
    if (!rows.length) return null;
    return (
        <div className="rt-phases" role="list" aria-label="Phases">
            {rows.map((r, i) => (
                <div key={r.key} className={`rt-ph-c${r.total > 0 && r.solved === r.total ? " done" : ""}`} role="listitem">
                    <span className="rt-ph-n">{i + 1}</span>
                    <b>{r.name}</b>
                    <small>{r.note}</small>
                    <i className="rt-ph-bar"><u style={{ width: `${r.total ? (r.solved / r.total) * 100 : 0}%` }} /></i>
                    <em>{r.solved}/{r.total}</em>
                </div>
            ))}
        </div>
    );
}

export function PhaseTag({ stage }: { stage: Pick<StageView, "slug" | "name"> }) {
    const ph = phaseOf(stage);
    const meta = PHASES.find((p) => p.key === ph);
    return meta ? <span className={`rt-ptag ${meta.key}`}>{meta.name}</span> : null;
}

// ---------------------------------------------------------------- test results

const OUTCOME_NAME: Record<TestOutcome["outcome"], string> = { passed: "passed", failed: "failed", ignored: "skipped", timed_out: "timed out" };

/** The result of a run as a strip of one dot per test, a line saying what happened, and the failing cases on request. */
export function TestResults({ run }: { run: RunView }) {
    const failing = run.tests.filter((t) => t.outcome !== "passed" && t.outcome !== "ignored");
    const headline =
        run.status === "compile_error" ? "did not compile" : run.status === "timeout" ? "timed out" : run.status === "passed" ? "all tests pass" : `${failing.length} failing`;
    return (
        <div className={`rt-tr ${run.status}`}>
            <div className="rt-trh">
                <b>{run.kind === "submit" ? "Submitted" : "Last run"}</b>
                <span>{run.passed}/{run.total} · {headline}</span>
                <small>{new Date(run.created_at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</small>
            </div>
            {run.tests.length > 0 && (
                <div className="rt-dots" aria-hidden="true">
                    {run.tests.map((t, i) => (
                        <i key={i} className={`${t.outcome}${t.suite === "hidden" ? " hid" : ""}`} title={`${t.suite} · ${t.name}: ${OUTCOME_NAME[t.outcome]}`} />
                    ))}
                </div>
            )}
            {failing.length > 0 && (
                <Fold title="Failing cases" n={failing.length}>
                    <ul className="rt-fails">
                        {failing.map((t, i) => (
                            <li key={i}>
                                <code>{t.name}</code> <small>{t.suite} · {OUTCOME_NAME[t.outcome]}</small>
                                {t.check && (
                                    <dl>
                                        <dt>input</dt><dd><code>{t.check.input}</code></dd>
                                        <dt>expected</dt><dd><code>{t.check.expected}</code></dd>
                                        <dt>got</dt><dd><code>{t.check.got}</code></dd>
                                    </dl>
                                )}
                                {t.panic && <pre>{t.panic}</pre>}
                            </li>
                        ))}
                    </ul>
                </Fold>
            )}
        </div>
    );
}
