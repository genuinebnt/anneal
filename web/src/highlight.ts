// Syntax colours for code on the system design pages: Rust and shell reuse the course pages' highlighters; Python, SQL, HTTP and
// Redis commands get small tokenizers here. All of them emit the shared `t-*` classes (course.css), so code looks the same everywhere.

import { esc, highlightRust, highlightShell } from "./pages/courseMd";

const span = (cls: string, text: string) => `<span class="${cls}">${esc(text)}</span>`;

/** One regex pass: each alternative is a token class, in the order given; anything unmatched is plain text. */
function tokens(code: string, re: RegExp, classes: (m: RegExpExecArray) => string | null): string {
    let out = "";
    let at = 0;
    re.lastIndex = 0;
    for (let m = re.exec(code); m; m = re.exec(code)) {
        if (m[0] === "") {
            re.lastIndex += 1;
            continue;
        }
        out += esc(code.slice(at, m.index));
        const cls = classes(m);
        out += cls ? span(cls, m[0]) : esc(m[0]);
        at = m.index + m[0].length;
    }
    return out + esc(code.slice(at));
}

const PY_KW = new Set("and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield".split(" "));
const PY_LIT = new Set(["True", "False", "None"]);

function highlightPython(code: string): string {
    const re = /(#[^\n]*)|("""[\s\S]*?"""|'''[\s\S]*?'''|[rbf]?"(?:\\.|[^"\\\n])*"|[rbf]?'(?:\\.|[^'\\\n])*')|(\b\d[\d_]*(?:\.\d+)?\b)|(@\w+)|([A-Za-z_]\w*)(?=\s*\()|([A-Za-z_]\w*)/g;
    return tokens(code, re, (m) => {
        if (m[1]) return "t-c";
        if (m[2]) return "t-s";
        if (m[3]) return "t-n";
        if (m[4]) return "t-m";
        const w = m[5] ?? m[6] ?? "";
        if (PY_KW.has(w)) return "t-k";
        if (PY_LIT.has(w)) return "t-n";
        if (m[5]) return "t-f";
        return /^[A-Z]/.test(w) ? "t-t" : null;
    });
}

const SQL_KW = new Set(
    "select from where and or not in is null as on join left right inner outer full cross group by order having limit offset insert into values update set delete create table index unique primary key foreign references begin commit rollback with distinct union all exists case when then else end returning for skip locked nowait share default constraint check alter add drop if cascade asc desc between like ilike over partition window lateral using".split(" "),
);
const SQL_TY = new Set("int integer bigint smallint text varchar char uuid timeuuid timestamptz timestamp date boolean bool jsonb json real double numeric serial bigserial bytea".split(" "));

function highlightSql(code: string): string {
    const re = /(--[^\n]*)|('(?:''|[^'])*')|(\b\d+(?:\.\d+)?\b)|(\$\d+|:\w+|\?)|([A-Za-z_]\w*)(?=\s*\()|([A-Za-z_]\w*)/g;
    return tokens(code, re, (m) => {
        if (m[1]) return "t-c";
        if (m[2]) return "t-s";
        if (m[3]) return "t-n";
        if (m[4]) return "t-l";
        const w = (m[5] ?? m[6] ?? "").toLowerCase();
        if (SQL_KW.has(w)) return "t-k";
        if (SQL_TY.has(w)) return "t-t";
        return m[5] ? "t-f" : null;
    });
}

function highlightHttp(code: string): string {
    return code
        .split("\n")
        .map((line, i) => {
            const req = /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)(\s+)(\S+)(.*)$/.exec(line);
            if (req) return span("t-k", req[1]!) + req[2] + span("t-s", req[3]!) + esc(req[4] ?? "");
            const status = /^(HTTP\/[\d.]+)(\s+)(\d{3})(.*)$/.exec(line);
            if (status) return span("t-t", status[1]!) + status[2] + span("t-n", status[3]!) + esc(status[4] ?? "");
            const header = /^([A-Za-z][\w-]*)(:)(.*)$/.exec(line);
            if (header && i > 0) return span("t-f", header[1]!) + esc(header[2]!) + esc(header[3] ?? "");
            return highlightJson(line);
        })
        .join("\n");
}

function highlightJson(code: string): string {
    const re = /("(?:\\.|[^"\\\n])*")(\s*:)?|(\b-?\d+(?:\.\d+)?\b)|\b(true|false|null)\b/g;
    let out = "";
    let at = 0;
    for (let m = re.exec(code); m; m = re.exec(code)) {
        out += esc(code.slice(at, m.index));
        if (m[1]) out += span(m[2] ? "t-f" : "t-s", m[1]) + esc(m[2] ?? "");
        else if (m[3]) out += span("t-n", m[3]);
        else out += span("t-k", m[4] ?? "");
        at = m.index + m[0].length;
    }
    return out + esc(code.slice(at));
}

/** Redis commands, one per line: the command word, then keys and arguments; numbers and quoted strings are coloured. */
function highlightRedis(code: string): string {
    return code
        .split("\n")
        .map((line) => {
            if (/^\s*#/.test(line)) return span("t-c", line);
            const m = /^(\s*)([A-Za-z]+)(.*)$/.exec(line);
            if (!m) return esc(line);
            const rest = tokens(m[3] ?? "", /("(?:\\.|[^"\\])*")|(\b\d+\b)|\b(NX|XX|EX|PX|KEEPTTL|GET|WITHSCORES|LIMIT|COUNT|BLOCK|STREAMS|MAXLEN)\b/g, (t) => (t[1] ? "t-s" : t[2] ? "t-n" : "t-k"));
            return esc(m[1] ?? "") + span("t-f", m[2]!) + rest;
        })
        .join("\n");
}

/** The languages a page may name after its ``` fence, with the label its code block shows. */
export const CODE_LANGS: Record<string, { label: string; fn: (code: string) => string }> = {
    rust: { label: "Rust", fn: highlightRust },
    rs: { label: "Rust", fn: highlightRust },
    python: { label: "Python", fn: highlightPython },
    py: { label: "Python", fn: highlightPython },
    sql: { label: "SQL", fn: highlightSql },
    http: { label: "HTTP", fn: highlightHttp },
    json: { label: "JSON", fn: highlightJson },
    redis: { label: "Redis", fn: highlightRedis },
    sh: { label: "Shell", fn: highlightShell },
    bash: { label: "Shell", fn: highlightShell },
    text: { label: "Text", fn: esc },
};

/** The code as highlighted HTML split into lines (a token over several lines is closed and reopened at each line break). */
export function highlightLines(lang: string, code: string): string[] {
    const html = (CODE_LANGS[lang]?.fn ?? esc)(code);
    const lines: string[] = [];
    let open: string[] = [];
    for (const raw of html.split("\n")) {
        const line = open.join("") + raw;
        // keep the spans that are still open at the end of this line, so the next line starts inside them
        const stack: string[] = [...open];
        for (const m of raw.matchAll(/<span class="[^"]*">|<\/span>/g)) {
            if (m[0] === "</span>") stack.pop();
            else stack.push(m[0]);
        }
        lines.push(line + "</span>".repeat(stack.length));
        open = stack;
    }
    return lines;
}
