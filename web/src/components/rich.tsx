import type { ReactNode } from "react";

/** The little markup the content files use: `**bold**`, `*italic*` and `code` in backticks. Nothing else, and never HTML. */
export function rich(text: string): ReactNode[] {
    const out: ReactNode[] = [];
    let last = 0;
    for (const m of text.matchAll(/\*\*(.+?)\*\*|`(.+?)`|\*(.+?)\*/g)) {
        const at = m.index ?? 0;
        if (at > last) out.push(text.slice(last, at));
        out.push(m[1] !== undefined ? <b key={at}>{m[1]}</b> : m[2] !== undefined ? <code key={at}>{m[2]}</code> : <i key={at}>{m[3]}</i>);
        last = at + m[0].length;
    }
    if (last < text.length) out.push(text.slice(last));
    return out;
}
