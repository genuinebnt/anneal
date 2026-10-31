import { marked } from "marked";

/** The callouts a system design page can use: `> [!NOTE]` and the like at the start of a quote. */
const CALLOUTS: Record<string, string> = { NOTE: "Note", TIP: "Tip", WARNING: "Watch out", INTERVIEW: "Say it like this", ASIDE: "Aside" };

/** Markdown for system design pages: tables, code, and callouts (`> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`, `> [!INTERVIEW]`,
 *  `> [!ASIDE]`) drawn as tinted boxes instead of quotes. The content is anneal's own, so the HTML is trusted. */
export function SdMd({ text }: { text: string }) {
    let html = marked.parse(text, { async: false });
    html = html.replace(/<blockquote>\s*<p>\[!(NOTE|TIP|WARNING|INTERVIEW|ASIDE)\]\s*([\s\S]*?)<\/blockquote>/g, (_m, kind: string, body: string) => `<aside class="sd-call sd-call-${kind.toLowerCase()}"><b>${CALLOUTS[kind]}</b><p>${body}</aside>`);
    return <div className="sd-md" dangerouslySetInnerHTML={{ __html: html }} />;
}
