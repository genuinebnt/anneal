import { Fragment, type ReactNode } from "react";

// Titles, ledes and other one-line fields are written with inline Markdown. This draws the marks they use, `code`,
// **bold** and *emphasis*, as elements; everything else stays text (inlineMarks.ts says what counts as a mark).
import { marks } from "./inlineMarks.ts";

function parts(text: string): ReactNode[] {
    return marks(text).map((m, i) => {
        if (m.kind === "text") return m.text;
        if (m.kind === "code") return <code className="il" key={i}>{m.text}</code>;
        return m.kind === "bold" ? <strong key={i}>{parts(m.text)}</strong> : <em key={i}>{parts(m.text)}</em>;
    });
}

export const Inline = ({ text }: { text: string }) => <Fragment>{parts(text)}</Fragment>;

const escapeHtml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The same marks as an HTML string, for the pages built as HTML (course Markdown). */
export function inlineHtml(text: string): string {
    return marks(text)
        .map((m) => {
            if (m.kind === "text") return escapeHtml(m.text);
            if (m.kind === "code") return `<code class="il">${escapeHtml(m.text)}</code>`;
            const tag = m.kind === "bold" ? "strong" : "em";
            return `<${tag}>${inlineHtml(m.text)}</${tag}>`;
        })
        .join("");
}

/** The same text with the marks taken off, for places that hold no elements: attributes, tooltips and the search list. */
export const plain = (text: string): string =>
    marks(text)
        .map((m) => (m.kind === "bold" || m.kind === "em" ? plain(m.text) : m.text))
        .join("");
