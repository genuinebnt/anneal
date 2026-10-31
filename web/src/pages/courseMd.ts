// Markdown for course stages: syntax-highlighted code, colour-coded C/C++ vs Rust tables, and callouts.
//
// Authors write plain markdown. Three conventions give it structure:
//   > [!TIP] An optional title        a GitHub-style alert; types: TIP, NOTE, WARNING, PORT, WHY, BUSTUB
//   **Port rule:** …                  a paragraph that starts with one of the known bold leads becomes a callout
//   a table whose header says "C / C++", "C++", "BusTub" or "Rust", "here"   columns are tinted to match
//   ```svg                            a diagram: inline SVG, optionally starting with a `caption: …` line
//   (raw HTML works too: animated figures, small interactive demos)

import { rust } from "@codemirror/lang-rust";
import { highlightTree, tagHighlighter, tags as t } from "@lezer/highlight";
import { Marked, type Tokens, type TokenizerAndRendererExtension } from "marked";

export const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const span = (cls: string, text: string) => `<span class="${cls}">${esc(text)}</span>`;

// ---------- highlighting ----------

const rustHighlighter = tagHighlighter([
    { tag: [t.keyword, t.self, t.modifier, t.controlKeyword, t.definitionKeyword, t.moduleKeyword, t.operatorKeyword], class: "t-k" },
    { tag: [t.typeName, t.className, t.namespace, t.standard(t.typeName)], class: "t-t" },
    { tag: [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName))], class: "t-f" },
    { tag: [t.string, t.character, t.special(t.string)], class: "t-s" },
    { tag: [t.number, t.bool, t.atom], class: "t-n" },
    { tag: t.macroName, class: "t-m" },
    { tag: [t.labelName, t.special(t.variableName)], class: "t-l" },
    { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "t-c" },
]);

export function highlightRust(code: string): string {
    const tree = rust().language.parser.parse(code);
    let out = "";
    let at = 0;
    highlightTree(tree, rustHighlighter, (from, to, cls) => {
        if (from > at) out += esc(code.slice(at, from));
        out += `<span class="${cls}">${esc(code.slice(from, to))}</span>`;
        at = to;
    });
    return out + esc(code.slice(at));
}

/** Highlights the lines of a diff (each starting with ' ', '+' or '-') as Rust, as one file so that a comment or string over several lines stays one token. Gives each line's HTML, without the leading character. */
export function highlightDiffLines(lines: string[]): string[] {
    const code = lines.map((l) => l.slice(1)).join("\n");
    const out: string[] = [""];
    const put = (text: string, cls?: string) => {
        text.split("\n").forEach((piece, i) => {
            if (i > 0) out.push("");
            if (piece) out[out.length - 1] += cls ? `<span class="${cls}">${esc(piece)}</span>` : esc(piece);
        });
    };
    let at = 0;
    highlightTree(rust().language.parser.parse(code), rustHighlighter, (from, to, cls) => {
        if (from > at) put(code.slice(at, from));
        put(code.slice(from, to), cls);
        at = to;
    });
    put(code.slice(at));
    return lines.map((_, i) => out[i] ?? "");
}

const CPP_KEYWORDS = new Set(
    "alignas auto break case catch class const constexpr continue default delete do else enum explicit extern for friend goto if inline mutable namespace new noexcept operator override private protected public register return sizeof static static_assert struct switch template this throw try typedef typename union using virtual volatile while final".split(" "),
);
const CPP_TYPES = new Set("void bool char short int long float double unsigned signed size_t ssize_t off_t string vector unordered_map map set mutex thread atomic optional unique_ptr shared_ptr".split(" "));
const CPP_LITERALS = new Set(["true", "false", "nullptr", "NULL"]);

/** A small C/C++ tokenizer: comments, strings, numbers, preprocessor lines, keywords, types and calls. */
function highlightCpp(code: string): string {
    const re = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')|(^[ \t]*#[ \t]*\w+)|(\b0x[0-9a-fA-F']+[uUlL]*\b|\b\d[\d']*(?:\.\d+)?[uUlLfF]*\b)|([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)/gm;
    let out = "";
    let at = 0;
    for (let m = re.exec(code); m; m = re.exec(code)) {
        out += esc(code.slice(at, m.index));
        at = m.index + m[0].length;
        if (m[1]) out += span("t-c", m[0]);
        else if (m[2]) out += span("t-s", m[0]);
        else if (m[3]) out += span("t-m", m[0]);
        else if (m[4]) out += span("t-n", m[0]);
        else {
            const id = m[5] ?? "";
            const last = id.split("::").pop() ?? id;
            const call = /^\s*\(/.test(code.slice(at, at + 3));
            if (CPP_LITERALS.has(id)) out += span("t-n", id);
            else if (CPP_KEYWORDS.has(id)) out += span("t-k", id);
            else if (CPP_TYPES.has(last) || /_t$/.test(last) || /^[A-Z][A-Za-z0-9]*$/.test(last) && !call && /^[A-Z][a-z]/.test(last)) out += span("t-t", id);
            else if (id.includes("::") && !call) out += span("t-t", id);
            else if (call) out += span("t-f", id);
            else out += esc(id);
        }
    }
    return out + esc(code.slice(at));
}

/** Shell sessions: the prompt, the command and its flags. */
export function highlightShell(code: string): string {
    return code
        .split("\n")
        .map((line) => {
            const m = /^(\s*)(\$|#)(\s)(.*)$/.exec(line);
            if (!m) return /^\s*#/.test(line) ? span("t-c", line) : esc(line);
            const [, ind, prompt, sp, rest] = m;
            if (prompt === "#") return esc(ind ?? "") + span("t-c", `# ${rest}`);
            const words = (rest ?? "").split(/(\s+)/).map((w, i) => (i === 0 ? span("t-f", w) : /^--?[A-Za-z]/.test(w) ? span("t-n", w) : /^["'].*["']$/.test(w) ? span("t-s", w) : esc(w)));
            return `${esc(ind ?? "")}${span("t-c", prompt ?? "")}${sp ?? ""}${words.join("")}`;
        })
        .join("\n");
}

function highlightToml(code: string): string {
    return code
        .split("\n")
        .map((line) => {
            if (/^\s*#/.test(line)) return span("t-c", line);
            if (/^\s*\[/.test(line)) return span("t-t", line);
            const m = /^(\s*)([\w.-]+)(\s*=\s*)(.*)$/.exec(line);
            if (!m) return esc(line);
            const v = m[4] ?? "";
            return `${esc(m[1] ?? "")}${span("t-f", m[2] ?? "")}${esc(m[3] ?? "")}${/^["']/.test(v) ? span("t-s", v) : /^\d|^(true|false)\b/.test(v) ? span("t-n", v) : esc(v)}`;
        })
        .join("\n");
}

const LANGS: Record<string, { label: string; kind: "rust" | "cpp" | "shell" | "plain"; fn: (c: string) => string }> = {
    rust: { label: "Rust", kind: "rust", fn: highlightRust },
    rs: { label: "Rust", kind: "rust", fn: highlightRust },
    cpp: { label: "C++", kind: "cpp", fn: highlightCpp },
    "c++": { label: "C++", kind: "cpp", fn: highlightCpp },
    c: { label: "C", kind: "cpp", fn: highlightCpp },
    sh: { label: "Shell", kind: "shell", fn: highlightShell },
    bash: { label: "Shell", kind: "shell", fn: highlightShell },
    console: { label: "Shell", kind: "shell", fn: highlightShell },
    toml: { label: "TOML", kind: "plain", fn: highlightToml },
    text: { label: "Text", kind: "plain", fn: esc },
};

/** A diagram: an ```svg fence holds inline SVG (trusted: it comes from the repo). An optional first line `caption: …` becomes the
 *  figure caption. Diagrams use the classes in course.css (box, live, free, hot, ln, dim, grow, …) so they follow the theme. */
function diagram(text: string): string {
    const m = /^caption:\s*(.*)\n/.exec(text);
    const svg = m ? text.slice(m[0].length) : text;
    return `<figure class="cx-diagram">${svg}${m ? `<figcaption>${esc(m[1] ?? "")}</figcaption>` : ""}</figure>`;
}

// ---------- code ----------

/** A code block longer than this many lines starts folded. */
const FOLD_LINES = 12;

/** The words after the language on a fence: `hl=3,5-7` marks lines, `file=name.rs` shows a file name, `tab` joins it with the next
 *  `tab` block into one block with a tab for each language. */
interface Info {
    name: string;
    hl: Set<number>;
    file?: string;
    tab: boolean;
}

function parseInfo(info: string | undefined): Info {
    const parts = (info ?? "").trim().split(/\s+/).filter(Boolean);
    const out: Info = { name: (parts[0] ?? "").toLowerCase(), hl: new Set(), tab: false };
    for (const w of parts.slice(1)) {
        if (w === "tab") out.tab = true;
        else if (w.startsWith("file=")) out.file = w.slice(5);
        else if (w.startsWith("hl=")) {
            for (const r of w.slice(3).split(",")) {
                const [x, y] = r.split("-").map(Number);
                if (x === undefined || !Number.isFinite(x)) continue;
                for (let n = x; n <= (y !== undefined && Number.isFinite(y) ? y : x); n++) out.hl.add(n);
            }
        }
    }
    return out;
}

function specFor(info: Info, code: string) {
    let spec = info.name ? LANGS[info.name] : undefined;
    // A bare fence that is a shell session ("$ cargo test") reads as one.
    if (!spec && /^\s*\$ /m.test(code)) spec = LANGS.sh;
    return spec;
}

/** Splits highlighted HTML into lines, closing and reopening the highlight spans that run across a line break. */
function htmlLines(html: string): string[] {
    const lines: string[] = [];
    let cur = "";
    const open: string[] = [];
    for (const tok of html.split(/(<span[^>]*>|<\/span>|\n)/)) {
        if (tok === "\n") {
            lines.push(cur + "</span>".repeat(open.length));
            cur = open.join("");
        } else {
            if (tok.startsWith("<span")) open.push(tok);
            else if (tok === "</span>") open.pop();
            cur += tok;
        }
    }
    lines.push(cur + "</span>".repeat(open.length));
    return lines;
}

/** The numbered lines of one code block: a gutter of numbers, and each line its own block so a marked line can be tinted. */
function codeLines(code: string, info: Info, extra = ""): string {
    const spec = specFor(info, code);
    const lines = htmlLines(spec ? spec.fn(code) : esc(code));
    const gutter = lines.map((_, i) => `<span>${i + 1}</span>`).join("");
    const pre = lines.map((l, i) => `<span class="k-${info.hl.has(i + 1) ? "hl" : "l"}">${l || " "}</span>`).join("");
    return `<div class="k-cb${extra}"><div class="k-ln" aria-hidden="true">${gutter}</div><pre><code>${pre}</code></pre></div>`;
}

function codeBlock(text: string, lang: string | undefined): string {
    const info = parseInfo(lang);
    if (info.name === "svg") return diagram(text);
    const code = text.replace(/\n$/, "");
    const spec = specFor(info, code);
    const lines = code.split("\n").length;
    const fold = lines > FOLD_LINES;
    return `<div class="k-code${fold ? " k-folded" : ""}"><div class="k-ch"><div class="k-tb"><button class="k-on" type="button" tabindex="-1">${esc(spec?.label ?? "Code")}</button></div>${info.file ? `<span class="k-fn">${esc(info.file)}</span>` : ""}<button class="k-copy" type="button" data-c="${esc(code)}">COPY</button></div>${codeLines(code, info, fold ? " k-fold" : "")}${fold ? `<button class="k-more" type="button" data-lines="${lines}" aria-expanded="false">SHOW ALL ${lines} LINES ▾</button>` : ""}</div>`;
}

/** Fenced blocks marked `tab` that follow each other become one block with a tab for each: the C++ and the Rust of the same thing. */
function tabGroup(blocks: { info: Info; code: string }[]): string {
    const tabs = blocks.map((b, i) => `<button class="${i === 0 ? "k-on" : ""}" type="button" role="tab" data-i="${i}" aria-selected="${i === 0}">${esc(specFor(b.info, b.code)?.label ?? (b.info.name || "Code"))}</button>`);
    const panels = blocks.map((b, i) => `<div class="k-tp" role="tabpanel" data-i="${i}" data-c="${esc(b.code)}"${i === 0 ? "" : " hidden"}>${codeLines(b.code, b.info)}</div>`);
    const file = blocks.find((b) => b.info.file)?.info.file;
    return `<div class="k-code k-tabbed"><div class="k-ch"><div class="k-tb" role="tablist">${tabs.join("")}<span class="k-ul2" aria-hidden="true"></span></div>${file ? `<span class="k-fn">${esc(file)}</span>` : ""}<button class="k-copy" type="button" data-c="${esc(blocks[0]?.code ?? "")}">COPY</button></div>${panels.join("")}</div>`;
}

// ---------- callouts ----------

type CalloutKind = "tip" | "note" | "warn" | "danger" | "bus" | "rule" | "aside" | "check";

const ICON: Record<string, string> = { tip: "✓", note: "i", warn: "!", danger: "×", bus: "B", rule: "⇄" };

const ALERTS: Record<string, [CalloutKind, string]> = {
    TIP: ["tip", "TIP"],
    TRICK: ["tip", "TRICK"],
    NOTE: ["note", "NOTE"],
    WARNING: ["warn", "WATCH OUT"],
    PITFALL: ["warn", "PITFALL"],
    PORT: ["rule", "PORT RULE"],
    WHY: ["rule", "WHY"],
    BUSTUB: ["bus", "IN BUSTUB"],
    ASIDE: ["aside", "ASIDE"],
    DANGER: ["danger", "DON'T"],
    CHECK: ["check", "CHECK YOURSELF"],
};

/** Bold leads at the start of a paragraph that turn it into a callout. */
const LEADS: [RegExp, CalloutKind][] = [
    [/^Port rule\b/i, "rule"],
    [/^Where this fits\b/i, "note"],
    [/^Don't\b/i, "danger"],
    [/^(Pitfall|The classic bug|Careful|Watch out)\b/i, "warn"],
    [/^Why\b/i, "rule"],
    [/^(Tip|Trick|Tips and tricks)\b/i, "tip"],
];

function callout(kind: CalloutKind, title: string, body: string): string {
    const dismiss = kind === "danger" || kind === "bus" || kind === "rule" ? "" : `<button class="k-x" type="button" aria-label="Dismiss this note">×</button>`;
    return `<div class="k-co k-${kind}"><span class="k-ic" aria-hidden="true">${ICON[kind] ?? "i"}</span><div><b class="k-t0">${esc(title)}</b>${body}</div>${dismiss}</div>`;
}

/** An aside: a longer tangent (prose, code, a table) that opens when asked. `> [!ASIDE] Its title`, then the body. */
function aside(title: string, body: string): string {
    return `<div class="k-asd"><button class="k-ah" type="button" aria-expanded="false"><span class="k-badge2 k-as">ASIDE</span><b>${esc(title || "More on this")}</b><span class="k-r"><span>optional</span><span class="k-chev" aria-hidden="true">›</span></span></button><div class="k-ab"><div><div class="k-in3">${body}</div></div></div></div>`;
}

/** A question to try before looking: `> [!CHECK] The question`, then the answer in `||spoiler||` marks, then a list of nudges that open one at a time. */
/** `question` is HTML (inline code and emphasis already rendered). */
function check(question: string, answerHtml: string, nudges: string[]): string {
    const ladder = nudges.length ? `<div class="k-lad">${nudges.map(() => `<span class="k-st2" aria-hidden="true"></span>`).join("")}<button class="k-cta k-xs k-sec k-nb" type="button" data-nudges="${esc(JSON.stringify(nudges))}" data-used="0" style="margin-left:6px">Need a nudge?</button></div><div class="k-nudge" role="status"></div>` : "";
    return `<div class="k-rev2"><div class="k-q">${question}</div><div>${answerHtml}</div>${ladder}</div>`;
}

// ---------- sidenotes, spoilers ----------

/** The notes of the paragraph being rendered; the paragraph renderer puts them next to it. */
let paraNotes: string[] = [];
/** Rich sidenotes of the current render: the HTML of each `> [!SIDENOTE]` block, put into its note by the renderer. */
let noteBlocks: string[] = [];

/** `^[a short note]` in a paragraph: a numbered marker in the text and the note itself, which the stage page shows in its side panel (or under the
 *  paragraph on a narrow screen). The numbers are given after rendering, in the order the markers appear on the page. */
const sidenote: TokenizerAndRendererExtension = {
    name: "sidenote",
    level: "inline",
    start: (src: string) => src.indexOf("^["),
    tokenizer(this, src: string) {
        const m = /^\^\[((?:[^\[\]\\]|\\.|\[[^\]]*\](?:\([^)]*\))?)+)\]/.exec(src);
        if (!m) return undefined;
        return { type: "sidenote", raw: m[0], text: m[1] ?? "", tokens: this.lexer.inlineTokens(m[1] ?? "") };
    },
    renderer(this, token) {
        const html = this.parser.parseInline(token.tokens ?? []).replace(/%%CXSN(\d+)%%/g, (_, k) => noteBlocks[Number(k)] ?? "");
        paraNotes.push(html);
        const n = paraNotes.length;
        return `<span class="k-snm" data-sn="${n}" tabindex="0" role="button" aria-expanded="false" aria-label="Show the note">${n}</span>`;
    },
};

/** `||the answer||`: hidden until it is clicked. */
const spoiler: TokenizerAndRendererExtension = {
    name: "spoiler",
    level: "inline",
    start: (src: string) => src.indexOf("||"),
    tokenizer(this, src: string) {
        const m = /^\|\|([^|]+)\|\|/.exec(src);
        if (!m) return undefined;
        return { type: "spoiler", raw: m[0], text: m[1] ?? "", tokens: this.lexer.inlineTokens(m[1] ?? "") };
    },
    renderer(this, token) {
        return `<span class="k-spoil" role="button" tabindex="0" aria-pressed="false" title="Click to show">${this.parser.parseInline(token.tokens ?? [])}</span>`;
    },
};

// ---------- tables ----------

type Col = "c" | "rust" | "other";

function columnKind(head: string): Col {
    const h = head.trim().toLowerCase();
    if (/^(c\s*\/\s*c\+\+|c\+\+|c|c \(.*\)|bustub|c\+\+ \(.*\))$/.test(h)) return "c";
    if (/^(rust|here|rust \(.*\))$/.test(h)) return "rust";
    return "other";
}

const marked: Marked = new Marked({
    extensions: [sidenote, spoiler],
    gfm: true,
    renderer: {
        code({ text, lang }: Tokens.Code) {
            return codeBlock(text, lang);
        },
        blockquote(this: { parser: { parse: (t: Tokens.Generic[]) => string; parseInline: (t: Tokens.Generic[]) => string } }, { tokens }: Tokens.Blockquote): string {
            const first = tokens[0];
            const m = first?.type === "paragraph" ? /^\[!(\w+)\][ \t]*(.*)(?:\n|$)/.exec((first as Tokens.Paragraph).text) : null;
            const alert = m ? ALERTS[(m[1] ?? "").toUpperCase()] : undefined;
            if (m && alert && first) {
                const rest = (first as Tokens.Paragraph).text.slice(m[0].length).trim();
                // inline text goes through this renderer's own extensions (spoilers, sidenotes), which the static `Lexer.lexInline` does not know
                const para = (text: string) => ({ type: "html", raw: text, text: `<p>${marked.parseInline(text, { async: false })}</p>`, block: true, pre: false }) as unknown as Tokens.Paragraph;
                if (alert[0] === "aside") return aside(m[2] ?? "", this.parser.parse((rest ? [para(rest)] : []).concat(tokens.slice(1) as Tokens.Paragraph[])));
                if (alert[0] === "check") {
                    const list = tokens.slice(1).find((t) => t.type === "list") as Tokens.List | undefined;
                    const others = tokens.slice(1).filter((t) => t.type !== "list");
                    const nudges = (list?.items ?? []).map((it) => this.parser.parseInline(it.tokens.flatMap((t) => ("tokens" in t && t.tokens ? (t.tokens as Tokens.Generic[]) : [t as Tokens.Generic]))));
                    const answer = this.parser.parse((rest ? [para(rest)] : []).concat(others as Tokens.Paragraph[]));
                    return check(marked.parseInline(m[2] ?? "", { async: false }) as string, answer.replace(/^<p>/, "<p>Answer: "), nudges);
                }
                const body: string = (m[2] ? `<p><strong>${esc(m[2])}</strong></p>` : "") + this.parser.parse(rest ? [para(rest), ...tokens.slice(1)] : tokens.slice(1));
                return callout(alert[0], alert[1], body);
            }
            return `<blockquote>${this.parser.parse(tokens)}</blockquote>`;
        },
        paragraph(this: { parser: { parseInline: (t: Tokens.Generic[]) => string } }, { tokens }: Tokens.Paragraph): string {
            const lead = tokens[0];
            if (lead?.type === "strong") {
                const text = (lead as Tokens.Strong).text;
                for (const [re, kind] of LEADS) {
                    if (re.test(text)) {
                        const title = text.replace(/[.:!?]+$/, "").toUpperCase();
                        return callout(kind, title, `<p>${this.parser.parseInline(tokens.slice(1)).replace(/^\s+/, "")}</p>`);
                    }
                }
            }
            paraNotes = [];
            const html = this.parser.parseInline(tokens);
            if (paraNotes.length === 0) return `<p>${html}</p>\n`;
            // a paragraph with notes: the notes wait next to it, numbered once the page is assembled
            const notes = paraNotes.map((n, i) => `<aside class="k-sn" data-sn="${i + 1}"><b>${i + 1}</b>${n}</aside>`).join("");
            paraNotes = [];
            return `<div class="k-snr"><p>${html}</p><div>${notes}</div></div>\n`;
        },
        table(this: { parser: { parseInline: (t: Tokens.Generic[]) => string } }, token: Tokens.Table): string {
            const heads = token.header.map((h) => h.text);
            const headless = heads.every((h) => !h.trim());
            const kinds: Col[] = heads.map(columnKind);
            const versus = kinds.includes("c") && kinds.includes("rust");
            // In a C++/Rust comparison, a cell that is only a code span is highlighted as that language and gets a copy button.
            const cell = (c: Tokens.TableCell, i = -1) => {
                const only = c.tokens.length === 1 ? c.tokens[0] : undefined;
                const kind = kinds[i];
                if (versus && only?.type === "codespan" && (kind === "c" || kind === "rust")) {
                    const src = only.raw.replace(/^`+ ?/, "").replace(/ ?`+$/, "");
                    return `<code>${(kind === "c" ? highlightCpp : highlightRust)(src)}</code>${kind === "rust" ? `<button class="k-copy" type="button" data-c="${esc(src)}">COPY</button>` : ""}`;
                }
                return this.parser.parseInline(c.tokens);
            };
            const cls = (i: number) => {
                const k = kinds[i] ?? "other";
                return k === "other" ? (headless && i === 0 ? "w" : "") : k === "rust" ? "r" : k;
            };
            const thead = headless ? "" : `<thead><tr>${token.header.map((h, i) => `<th class="${cls(i) ? `k-${cls(i)}` : ""}">${cell(h)}</th>`).join("")}</tr></thead>`;
            const rows = token.rows.map((r) => `<tr>${r.map((c, i) => `<td class="${cls(i) && cls(i) !== "r" ? `k-${cls(i)}` : ""}">${cell(c, i)}</td>`).join("")}</tr>`).join("");
            return `<div class="k-vt"><table>${thead}<tbody>${rows}</tbody></table></div>`;
        },
    },
});

/** Rich sidenotes and tabbed code need to be found in the markdown before it is parsed. */
function pullSidenoteBlocks(md: string, blocks: string[]): string {
    const lines = md.split("\n");
    const out: string[] = [];
    let fence = false;
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i] ?? "";
        if (/^\s*```/.test(line)) fence = !fence;
        if (!fence && /^>\s*\[!SIDENOTE\]/i.test(line)) {
            const body: string[] = [];
            for (; i < lines.length && /^>/.test(lines[i] ?? ""); i++) {
                const l = (lines[i] ?? "").replace(/^>\s?/, "");
                body.push(body.length === 0 ? l.replace(/^\[!SIDENOTE\][ \t]*/i, "") : l);
            }
            i--;
            const title = (body[0] ?? "").trim();
            blocks.push(marked.parse(`${title ? `${title}\n\n` : ""}${body.slice(1).join("\n")}`, { async: false }));
            // attach to the closest non-empty line above (the end of its paragraph)
            let k = out.length - 1;
            while (k >= 0 && (out[k] ?? "").trim() === "") k--;
            if (k >= 0) out[k] = `${out[k]}^[%%CXSN${blocks.length - 1}%%]`;
            continue;
        }
        out.push(line);
    }
    return out.join("\n");
}

function pullTabs(md: string, groups: string[]): string {
    const lines = md.split("\n");
    const out: string[] = [];
    let i = 0;
    const readBlock = (at: number): { info: Info; code: string; end: number } | null => {
        const open = /^```(.*)$/.exec(lines[at] ?? "");
        if (!open) return null;
        const info = parseInfo(open[1]);
        if (!info.tab) return null;
        let j = at + 1;
        const body: string[] = [];
        while (j < lines.length && !/^```\s*$/.test(lines[j] ?? "")) body.push(lines[j++] ?? "");
        return { info, code: body.join("\n"), end: j + 1 };
    };
    while (i < lines.length) {
        const first = readBlock(i);
        if (!first) {
            out.push(lines[i] ?? "");
            i++;
            continue;
        }
        const blocks = [first];
        let end = first.end;
        for (;;) {
            let next = end;
            while (next < lines.length && (lines[next] ?? "").trim() === "") next++;
            const b = readBlock(next);
            if (!b) break;
            blocks.push(b);
            end = b.end;
        }
        if (blocks.length < 2) {
            out.push(lines[i] ?? "");
            i++;
            continue;
        }
        groups.push(tabGroup(blocks));
        out.push("", `%%CXTABS${groups.length - 1}%%`, "");
        i = end;
    }
    return out.join("\n");
}

export function renderMd(md: string): string {
    const savedBlocks = noteBlocks;
    const savedPara = paraNotes;
    noteBlocks = [];
    paraNotes = [];
    const groups: string[] = [];
    try {
        const prepared = pullTabs(pullSidenoteBlocks(md, noteBlocks), groups);
        const html = marked.parse(prepared, { async: false });
        return html.replace(/<p>%%CXTABS(\d+)%%<\/p>/g, (_, k) => groups[Number(k)] ?? "");
    } finally {
        noteBlocks = savedBlocks;
        paraNotes = savedPara;
    }
}

// ---------- behaviour of the rendered blocks ----------

/** Slides the underline of a tabbed code block under its active tab. */
export function placeTabUnderline(code: Element) {
    const on = code.querySelector<HTMLElement>(".k-tb .k-on");
    const ul = code.querySelector<HTMLElement>(".k-ul2");
    if (!on || !ul) return;
    ul.style.width = `${on.offsetWidth}px`;
    ul.style.transform = `translateX(${on.offsetLeft}px)`;
}

/** Call after rendered markdown is in the page: tabbed blocks get their underline. */
export function initCodeTabs(root: ParentNode | null) {
    root?.querySelectorAll(".k-tabbed").forEach(placeTabUnderline);
}

function toggleSpoiler(el: HTMLElement) {
    el.setAttribute("aria-pressed", String(el.classList.toggle("k-on")));
}

/** A click inside rendered course markdown: copy, unfold a long block, switch a code tab, dismiss a note, open an aside, ask for a nudge or
 *  show a spoiler (the HTML is static, so the click is caught by the element that holds it). */
export function handleCodeClick(e: { target: EventTarget | null }) {
    const el = e.target as HTMLElement | null;
    const tab = el?.closest<HTMLButtonElement>(".k-tb button[data-i]");
    if (tab) {
        const code = tab.closest(".k-code");
        if (!code) return;
        code.querySelectorAll<HTMLElement>(".k-tb button[data-i]").forEach((t) => {
            t.classList.toggle("k-on", t === tab);
            t.setAttribute("aria-selected", String(t === tab));
        });
        code.querySelectorAll<HTMLElement>(".k-tp").forEach((p) => {
            p.hidden = p.dataset.i !== tab.dataset.i;
            if (!p.hidden) code.querySelector<HTMLElement>(".k-ch > .k-copy")?.setAttribute("data-c", p.dataset.c ?? "");
        });
        placeTabUnderline(code);
        return;
    }
    const x = el?.closest<HTMLButtonElement>(".k-co > .k-x");
    if (x) {
        x.parentElement?.classList.add("k-gone");
        return;
    }
    const ah = el?.closest<HTMLButtonElement>(".k-ah");
    if (ah) {
        const open = ah.closest(".k-asd")?.classList.toggle("k-open");
        ah.setAttribute("aria-expanded", String(!!open));
        return;
    }
    const nb = el?.closest<HTMLButtonElement>(".k-nb");
    if (nb) {
        const list: string[] = JSON.parse(nb.dataset.nudges ?? "[]");
        const used = Number(nb.dataset.used) + 1;
        nb.dataset.used = String(used);
        const box = nb.closest(".k-rev2");
        const target = box?.querySelector<HTMLElement>(".k-nudge");
        if (target) {
            target.innerHTML = list[used - 1] ?? "";
            target.classList.add("k-on");
        }
        box?.querySelectorAll<HTMLElement>(".k-st2").forEach((b, i) => b.classList.toggle("k-on", i < used));
        nb.textContent = used < list.length ? `Another nudge (${used}/${list.length})` : "No more nudges";
        nb.disabled = used >= list.length;
        return;
    }
    const spoil = el?.closest<HTMLElement>(".k-spoil");
    if (spoil) {
        toggleSpoiler(spoil);
        return;
    }
    const more = el?.closest<HTMLButtonElement>(".k-more");
    if (more) {
        const code = more.closest(".k-code");
        const open = code?.querySelector(".k-cb")?.classList.toggle("k-fold") === false;
        more.setAttribute("aria-expanded", String(open));
        more.textContent = open ? "SHOW LESS ▴" : `SHOW ALL ${more.dataset.lines} LINES ▾`;
        return;
    }
    const b = el?.closest<HTMLButtonElement>(".k-copy");
    if (!b) return;
    const label = b.textContent;
    const done = () => {
        b.classList.add("k-ok");
        b.textContent = "COPIED";
        setTimeout(() => {
            b.classList.remove("k-ok");
            b.textContent = label;
        }, 1100);
    };
    try {
        navigator.clipboard.writeText(b.dataset.c ?? "").then(done, done);
    } catch {
        done();
    }
}

/** Enter or Space on a spoiler shows it. */
export function handleCodeKey(e: { target: EventTarget | null; key: string; preventDefault: () => void }) {
    const spoil = (e.target as HTMLElement | null)?.closest<HTMLElement>(".k-spoil");
    if (spoil && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        toggleSpoiler(spoil);
    }
}
