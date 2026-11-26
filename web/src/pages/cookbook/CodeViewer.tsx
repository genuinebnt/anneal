// The cookbook's code viewer, read-only and made for reading: a file tree for a project, the file's lines with highlighting,
// a list of what the file declares, text size, wrapping and a full-window view.
import { useEffect, useMemo, useRef, useState } from "react";
import { highlightLines } from "../../highlight";
import { getPref, setPref } from "../../prefs";

export type ViewerFile = { path: string; text: string };

const LANG: Record<string, string> = { rs: "rust", sh: "sh", json: "json", sql: "sql", py: "python" };
const SIZES = [12.5, 13.5, 14.5, 16, 18];
/** A longer file opens showing this many lines of height; the rest scrolls inside it. */
const TALL = 34;

export type Sym = { kind: string; name: string; line: number; depth: number };

/** The items a Rust file declares, by their first line. */
export function symbolsOf(text: string): Sym[] {
    const out: Sym[] = [];
    const item = /^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:(?:async|unsafe|const|extern(?:\s+"[^"]*")?)\s+)*(fn|struct|enum|trait|mod|type|union|macro_rules!|impl)(?=[\s<!])\s*(.*)$/;
    text.split("\n").forEach((line, i) => {
        const m = item.exec(line);
        if (!m) return;
        const [, indent, kind, rest] = m as unknown as [string, string, string, string];
        const depth = indent.length / 4;
        if (depth > 1) return;
        const name = kind === "impl" ? rest.replace(/^<[^{]*?>\s+/, "").replace(/\s*(\{|where\b).*$/, "").trim() : (/^[A-Za-z0-9_]+/.exec(rest)?.[0] ?? "");
        if (name) out.push({ kind: kind.replace("macro_rules!", "macro"), name, line: i + 1, depth });
    });
    return out;
}

function Tree({ files, current, pick }: { files: ViewerFile[]; current: string; pick: (p: string) => void }) {
    const folders = new Map<string, ViewerFile[]>();
    for (const f of files) {
        const at = f.path.lastIndexOf("/");
        const dir = at < 0 ? "" : f.path.slice(0, at);
        folders.set(dir, [...(folders.get(dir) ?? []), f]);
    }
    return (
        <nav className="cb-ftree" aria-label="Files">
            {[...folders.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([dir, list]) => (
                <div key={dir}>
                    {dir && <h6>{dir}/</h6>}
                    {list.map((f) => (
                        <button key={f.path} type="button" className={f.path === current ? "on" : ""} aria-current={f.path === current ? "true" : undefined} onClick={() => pick(f.path)} title={f.path}>
                            {f.path.slice(dir ? dir.length + 1 : 0)}
                        </button>
                    ))}
                </div>
            ))}
        </nav>
    );
}

/** How many lines the file's opening `//!` notes take, with the attributes and blank lines among them. Zero for a short one. */
export function headerLines(lines: string[]): number {
    let end = 0;
    for (let i = 0; i < lines.length; i++) {
        const t = lines[i]!.trim();
        if (t.startsWith("//!")) end = i + 1;
        else if (!(t === "" || t.startsWith("#![") || t.startsWith("//"))) break;
    }
    return end >= 8 && end < lines.length ? end : 0;
}

/** `foldHeader`: the page already shows the file's opening notes as text, so the viewer starts at the code. */
export function CodeViewer({ files, label, wrap, onWrap, note, foldHeader = false }: { files: ViewerFile[]; label: string; wrap: boolean; onWrap: () => void; note?: string; foldHeader?: boolean }) {
    const [header, setHeader] = useState(false);
    const [path, setPath] = useState(files[0]?.path ?? "");
    const [size, setSize] = useState(() => Math.min(SIZES.length - 1, Math.max(0, getPref<number>("cookbook.codeSize", 2))));
    const [full, setFull] = useState(false);
    const [tall, setTall] = useState(false);
    const [outline, setOutline] = useState(false);
    const [lit, setLit] = useState<number | null>(null);
    const [copied, setCopied] = useState(false);
    const scroller = useRef<HTMLDivElement>(null);
    const file = files.find((f) => f.path === path) ?? files[0]!;
    const code = file.text.replace(/\n$/, "");
    const lines = useMemo(() => highlightLines(LANG[file.path.split(".").pop() ?? ""] ?? "text", code), [file.path, code]);
    const symbols = useMemo(() => (file.path.endsWith(".rs") ? symbolsOf(code) : []), [file.path, code]);
    const folded = useMemo(() => (foldHeader && file.path.endsWith(".rs") ? headerLines(code.split("\n")) : 0), [foldHeader, file.path, code]);
    const hidden = header ? 0 : folded;
    const resize = (d: number) => setSize((s) => { const n = Math.min(SIZES.length - 1, Math.max(0, s + d)); setPref("cookbook.codeSize", n); return n; });
    const pick = (p: string) => {
        setPath(p);
        setLit(null);
        scroller.current?.scrollTo({ top: 0 });
    };
    const go = (line: number) => {
        if (line <= hidden) setHeader(true);
        setLit(line);
        const el = scroller.current?.querySelector<HTMLElement>(`[data-line="${line}"]`);
        if (el && scroller.current) scroller.current.scrollTo({ top: el.offsetTop - 48 });
    };
    useEffect(() => {
        if (!full) return;
        const esc = (e: KeyboardEvent) => e.key === "Escape" && setFull(false);
        addEventListener("keydown", esc);
        document.documentElement.classList.add("cb-noscroll");
        return () => {
            removeEventListener("keydown", esc);
            document.documentElement.classList.remove("cb-noscroll");
        };
    }, [full]);
    const copy = () =>
        navigator.clipboard.writeText(file.text).then(
            () => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1600);
            },
            () => {},
        );
    const project = files.length > 1;
    const long = lines.length > TALL;
    return (
        <figure className={`cb-view${full ? " full" : ""}${tall ? " tall" : ""}${wrap ? " wrap" : ""}${project ? " project" : ""}`} aria-label={label}>
            <div className="cb-view-bar">
                <span className="cb-view-name" title={file.path}>{file.path.split("/").pop()}</span>
                <span className="cb-view-sp" />
                {symbols.length > 1 && <button type="button" aria-pressed={outline} onClick={() => setOutline((v) => !v)}>Symbols <small>{symbols.length}</small></button>}
                <button type="button" onClick={() => resize(-1)} disabled={size === 0} aria-label="Smaller code">A−</button>
                <button type="button" onClick={() => resize(1)} disabled={size === SIZES.length - 1} aria-label="Larger code">A+</button>
                <button type="button" aria-pressed={wrap} onClick={onWrap}>Wrap</button>
                {long && !full && <button type="button" aria-pressed={tall} onClick={() => setTall((v) => !v)}>{tall ? "Shorter" : "All lines"}</button>}
                <button type="button" aria-pressed={full} onClick={() => setFull((v) => !v)}>{full ? "Close" : "Full window"}</button>
                <button type="button" onClick={copy}>{copied ? "Copied" : "Copy file"}</button>
            </div>
            <div className="cb-view-body">
                {project && <Tree files={files} current={file.path} pick={pick} />}
                <div className="cb-view-code sd-code" ref={scroller} tabIndex={0} aria-label={`${file.path}, ${lines.length} lines`}>
                    <pre style={{ fontSize: SIZES[size] }}>
                        {folded > 0 && (
                            <button type="button" className="cb-view-fold" aria-expanded={header} onClick={() => setHeader((v) => !v)}>
                                {header ? "▾ Hide" : "▸ Show"} the file's opening notes, lines 1 to {folded}
                            </button>
                        )}
                        {lines.slice(hidden).map((html, k) => { const i = k + hidden; return (
                            <div key={i} data-line={i + 1} className={`sd-ln${lit === i + 1 ? " sd-lit" : ""}`}>
                                <button type="button" className="sd-lnum" tabIndex={-1} aria-label={`Mark line ${i + 1}`} onClick={() => setLit(lit === i + 1 ? null : i + 1)}>{i + 1}</button>
                                <span dangerouslySetInnerHTML={{ __html: html || " " }} />
                            </div>
                        ); })}
                    </pre>
                </div>
                {outline && symbols.length > 1 && (
                    <nav className="cb-syms" aria-label="In this file">
                        {symbols.map((s) => (
                            <button key={s.line} type="button" className={`${lit === s.line ? "on" : ""}${s.depth ? " in" : ""}`} onClick={() => go(s.line)}>
                                <i data-kind={s.kind}>{s.kind}</i>
                                <span>{s.name}</span>
                            </button>
                        ))}
                    </nav>
                )}
            </div>
            <figcaption>
                <span>{file.path}</span>
                <span>{lines.length.toLocaleString()} lines</span>
                <span>{(file.text.length / 1024).toFixed(1)} KB</span>
                <span>Read-only{note ? `. ${note}` : ""}</span>
            </figcaption>
        </figure>
    );
}
