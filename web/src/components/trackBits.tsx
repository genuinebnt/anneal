// Pieces the Rust track pages and the workspace share: the interview follow-up questions as a list, and a problem brief (what it
// teaches, what you will be asked after). Styles: styles/rust.css.
import { marked } from "marked";
import { useState } from "react";

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
