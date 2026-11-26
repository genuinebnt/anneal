// Where inline Markdown's marks are in a line of text. These fields are mostly prose about code, so the rule for `*` is
// stricter than CommonMark, which would italicise the text between two `count(*)`. A mark opens only at the start of
// the text, after a space, an opening bracket or quote, or a dash, and straight before a letter, digit or code (quotes
// and brackets, or a number's sign, may come between). It closes only straight after something other than a space, an opening bracket or a slash, and not before a
// letter or digit. What it holds must have its brackets in pairs. So `count(*)`, `*.rs`, `src/*`, `src/**/*.rs`,
// `a * b`, `SELECT *`, `*const T` and `O(n*m)` stay as typed. Underscores are never marks.
export type Mark = { kind: "text" | "code" | "bold" | "em"; text: string };

const OPEN_AFTER = String.raw`(?<![^\s([{"'“‘—–-])`;
const OPEN_BEFORE = String.raw`(?=["'“‘([{]*(?:[\p{L}\p{N}\x60]|[-+−~]\p{N}))`;
const CLOSE_AFTER = String.raw`(?<![\s([{/\\])`;
const CLOSE_BEFORE = String.raw`(?![\p{L}\p{N}*])`;

const CODE = String.raw`(\x60+)(.+?)\1`;
// inside a mark, a code span is taken whole, so an asterisk in it closes nothing
const BOLD = `${OPEN_AFTER}\\*\\*${OPEN_BEFORE}((?:\\x60[^\\x60]*\\x60|[^\\x60])+?)${CLOSE_AFTER}\\*\\*${CLOSE_BEFORE}`;
const EM = `${OPEN_AFTER}\\*${OPEN_BEFORE}((?:\\x60[^\\x60]*\\x60|[^\\x60*])+?)${CLOSE_AFTER}\\*${CLOSE_BEFORE}`;
const ANY = new RegExp(`${CODE}|${BOLD}|${EM}`, "gu");

/** Whether every bracket in the text, outside code, is closed where it should be. */
function paired(text: string): boolean {
    const open: string[] = [];
    for (const c of text.replace(/`[^`]*`/g, "")) {
        if (c === "(" || c === "[" || c === "{") open.push(c);
        else if (c === ")" || c === "]" || c === "}") {
            if (open.pop() !== { ")": "(", "]": "[", "}": "{" }[c]) return false;
        }
    }
    return open.length === 0;
}

/** The text as plain runs and marks, in order. */
export function marks(text: string): Mark[] {
    const out: Mark[] = [];
    let at = 0;
    const re = new RegExp(ANY);
    for (let m = re.exec(text); m; m = re.exec(text)) {
        const [all, , code, bold, em] = m;
        const held = bold ?? em;
        if (held !== undefined && !paired(held)) {
            // not a mark: look again from the next character
            re.lastIndex = m.index + 1;
            continue;
        }
        if (m.index > at) out.push({ kind: "text", text: text.slice(at, m.index) });
        out.push(code !== undefined ? { kind: "code", text: code } : { kind: bold !== undefined ? "bold" : "em", text: held! });
        at = m.index + all.length;
    }
    if (at < text.length) out.push({ kind: "text", text: text.slice(at) });
    return out;
}
