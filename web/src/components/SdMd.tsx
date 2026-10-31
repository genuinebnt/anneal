import { marked } from "marked";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type ReactElement } from "react";
import { CODE_LANGS, highlightLines } from "../highlight";

/** The callouts a system design page can use: `> [!NOTE]` and the like at the start of a quote. */
const CALLOUTS: Record<string, { label: string; icon: string }> = {
    NOTE: { label: "Note", icon: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.01"/>' },
    TIP: { label: "Tip", icon: '<path d="M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.5c.8.8 1 1.5 1 2.5h6c0-1 .2-1.7 1-2.5A6 6 0 0 0 12 3z"/>' },
    WARNING: { label: "Watch out", icon: '<path d="M12 3 2 20h20L12 3z"/><path d="M12 10v4M12 17v.01"/>' },
    INTERVIEW: { label: "Say it in the interview", icon: '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>' },
    ASIDE: { label: "Aside", icon: '<path d="M4 6h16M4 12h10M4 18h7"/>' },
};
const icon = (paths: string) => `<svg class="sd-ci" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;

type CodeSeg = { k: "code"; lang: string; title?: string; hl: Set<number>; group?: string; code: string };
type Seg = { k: "md"; text: string } | CodeSeg | { k: "math"; tex: string } | { k: "steps"; title: string; items: { who: string; what: string; say?: string }[] } | { k: "deeper"; title: string; meta?: string; text: string };

/** `hl=13,15-16` as a set of line numbers. */
function lineSet(spec: string | undefined): Set<number> {
    const out = new Set<number>();
    for (const part of (spec ?? "").split(",").filter(Boolean)) {
        const [a, b] = part.split("-").map(Number);
        for (let n = a!; n <= (b ?? a!); n++) if (Number.isFinite(n)) out.add(n);
    }
    return out;
}

/** Splits a page's Markdown into prose and the blocks drawn by components: fenced code (```rust title=… hl=… group=…),
 *  display math ($$ … $$), step-throughs (:::steps title, then `- who | what | narration` lines, then :::) and folds
 *  (:::deeper title | time, Markdown, :::). The content checker in crates/content/src/sd.rs enforces the same grammar. */
export function segments(text: string): Seg[] {
    const out: Seg[] = [];
    const lines = text.split("\n");
    let md: string[] = [];
    const flush = () => {
        if (md.join("").trim()) out.push({ k: "md", text: md.join("\n") });
        md = [];
    };
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i]!;
        const fence = /^```(\w*)\s*(.*)$/.exec(line);
        if (fence) {
            flush();
            const body: string[] = [];
            for (i++; i < lines.length && !/^```\s*$/.test(lines[i]!); i++) body.push(lines[i]!);
            const meta = Object.fromEntries([...(fence[2] ?? "").matchAll(/(\w+)=(\S+)/g)].map((m) => [m[1], m[2]]));
            out.push({ k: "code", lang: fence[1] || "text", title: meta.title, hl: lineSet(meta.hl), group: meta.group, code: body.join("\n") });
            continue;
        }
        const one = /^\$\$(.+)\$\$\s*$/.exec(line);
        if (one) {
            flush();
            out.push({ k: "math", tex: one[1]!.trim() });
            continue;
        }
        if (/^\$\$\s*$/.test(line)) {
            flush();
            const body: string[] = [];
            for (i++; i < lines.length && !/^\$\$\s*$/.test(lines[i]!); i++) body.push(lines[i]!);
            out.push({ k: "math", tex: body.join("\n") });
            continue;
        }
        const dir = /^:::(steps|deeper)\s+(.*)$/.exec(line);
        if (dir) {
            flush();
            const body: string[] = [];
            for (i++; i < lines.length && !/^:::\s*$/.test(lines[i]!); i++) body.push(lines[i]!);
            if (dir[1] === "steps") {
                const items = body
                    .map((l) => /^-\s+(.+?)\s+\|\s+(.+?)(?:\s+\|\s+(.+))?$/.exec(l))
                    .filter((m): m is RegExpExecArray => !!m)
                    .map((m) => ({ who: m[1]!, what: m[2]!, say: m[3] }));
                out.push({ k: "steps", title: dir[2]!.trim(), items });
            } else {
                const [title, meta] = dir[2]!.split(/\s+\|\s+/);
                out.push({ k: "deeper", title: title!.trim(), meta: meta?.trim(), text: body.join("\n") });
            }
            continue;
        }
        md.push(line);
    }
    flush();
    return out;
}

/** Prose as HTML: callouts as tinted panels with an icon, `^[note]` as numbered margin notes, `\( … \)` as inline math
 *  (rendered once KaTeX has loaded). The content is anneal's own, so the HTML is trusted. */
/** How many margin notes a piece of Markdown holds, so the numbers can run across a page. */
export const countNotes = (text: string) => (text.match(/\^\[/g) ?? []).length;

function proseHtml(text: string, id: string, start: number): string {
    let n = start;
    const maths: string[] = [];
    let src = text.replace(/\\\((.+?)\\\)/g, (_m, tex: string) => `@@TEX${maths.push(tex) - 1}@@`);
    src = src.replace(/\^\[((?:[^[\]]|\[[^\]]*\])+)\]/g, (_m, note: string) => {
        n += 1;
        const nid = `${id}-sn${n}`;
        return `<label class="sn-ref" for="${nid}">${n}</label><input type="checkbox" id="${nid}" class="sn-toggle"><span class="sn"><b>${n}</b>${marked.parseInline(note, { async: false })}</span>`;
    });
    let html = marked.parse(src, { async: false });
    html = html.replace(/<blockquote>\s*<p>\[!(NOTE|TIP|WARNING|INTERVIEW|ASIDE)\]\s*([\s\S]*?)<\/blockquote>/g, (_m, kind: string, body: string) => {
        const c = CALLOUTS[kind]!;
        return `<aside class="sd-call sd-call-${kind.toLowerCase()}">${icon(c.icon)}<b>${c.label}</b><div><p>${body}</div></aside>`;
    });
    // a table can be wider than the column: it scrolls inside its own box
    html = html.replace(/<table>/g, '<div class="sd-tbl"><table>').replace(/<\/table>/g, "</table></div>");
    return html.replace(/@@TEX(\d+)@@/g, (_m, i: string) => `<span class="sd-tex" data-tex="${(maths[Number(i)] ?? "").replace(/&/g, "&amp;").replace(/"/g, "&quot;")}"></span>`);
}

function Prose({ text, id, start }: { text: string; id: string; start: number }) {
    const ref = useRef<HTMLDivElement>(null);
    const html = useMemo(() => proseHtml(text, id, start), [text, id, start]);
    useEffect(() => {
        const el = ref.current;
        const spots = el ? [...el.querySelectorAll<HTMLElement>(".sd-tex[data-tex]")] : [];
        if (!spots.length) return;
        let live = true;
        void import("../katex").then(({ renderTex }) => {
            if (!live) return;
            for (const s of spots) s.innerHTML = renderTex(s.dataset.tex ?? "", false);
        });
        return () => {
            live = false;
        };
    }, [html]);
    return <div ref={ref} className="sd-md" dangerouslySetInnerHTML={{ __html: html }} />;
}

function MathBlock({ tex }: { tex: string }) {
    const [html, setHtml] = useState<string | null>(null);
    useEffect(() => {
        let live = true;
        void import("../katex").then(({ renderTex }) => live && setHtml(renderTex(tex, true)));
        return () => {
            live = false;
        };
    }, [tex]);
    return html ? <div className="sd-math" dangerouslySetInnerHTML={{ __html: html }} /> : <div className="sd-math sd-math-wait">{tex}</div>;
}

/** Code with its language, a file name, tabs when several blocks form a group, Copy, and line numbers that copy a link to the
 *  line (`#<block>-L13`); a link like that opens with the line lit. */
function CodeGroup({ blocks, id }: { blocks: CodeSeg[]; id: string }) {
    const [tab, setTab] = useState(0);
    const [copied, setCopied] = useState<string | null>(null);
    const [lit, setLit] = useState<number | null>(null);
    const b = blocks[Math.min(tab, blocks.length - 1)]!;
    const lines = useMemo(() => highlightLines(b.lang, b.code), [b]);
    const ref = useRef<HTMLElement>(null);
    useEffect(() => {
        const m = new RegExp(`^#${id}-L(\\d+)$`).exec(window.location.hash);
        if (!m) return;
        setLit(Number(m[1]));
        ref.current?.scrollIntoView({ block: "center" });
    }, [id]);
    const copy = (text: string, what: string) => {
        navigator.clipboard.writeText(text).then(
            () => {
                setCopied(what);
                window.setTimeout(() => setCopied(null), 1600);
            },
            () => {},
        );
    };
    return (
        <figure ref={ref} className="sd-code" id={id}>
            <div className="sd-code-h">
                <span className="sd-code-l">{CODE_LANGS[b.lang]?.label ?? b.lang}</span>
                {b.title && <span className="sd-code-t">{b.title}</span>}
                <span className="sd-code-sp" />
                {blocks.length > 1 && (
                    <div className="sd-code-tabs" role="tablist" aria-label="Language">
                        {blocks.map((x, i) => (
                            <button key={i} role="tab" aria-selected={i === tab} className={i === tab ? "on" : ""} onClick={() => setTab(i)}>
                                {CODE_LANGS[x.lang]?.label ?? x.lang}
                            </button>
                        ))}
                    </div>
                )}
                <button className="sd-code-copy" onClick={() => copy(b.code, "code")}>{copied === "code" ? "Copied" : "Copy"}</button>
            </div>
            <pre>
                {lines.map((html, i) => (
                    <div key={i} className={`sd-ln${b.hl.has(i + 1) ? " sd-hl" : ""}${lit === i + 1 ? " sd-lit" : ""}`}>
                        <button
                            className="sd-lnum"
                            aria-label={`Copy a link to line ${i + 1}`}
                            onClick={() => {
                                setLit(i + 1);
                                copy(`${window.location.origin}${window.location.pathname}#${id}-L${i + 1}`, `L${i + 1}`);
                            }}
                        >
                            {copied === `L${i + 1}` ? "✓" : i + 1}
                        </button>
                        <span dangerouslySetInnerHTML={{ __html: html || " " }} />
                    </div>
                ))}
            </pre>
        </figure>
    );
}

/** The controls the Idea tab's lab uses: back, next, play or pause, restart, the step count and a tick per step. */
export function StepControls({ i, n, playing, set, play }: { i: number; n: number; playing: boolean; set: (i: number) => void; play: (on: boolean) => void }) {
    return (
        <>
            <div className="id-ctl">
                <button onClick={() => { play(false); set(Math.max(0, i - 1)); }} disabled={i === 0}>← Back</button>
                <button onClick={() => { play(false); set(Math.min(n - 1, i + 1)); }} disabled={i === n - 1}>Next →</button>
                <button className={playing ? "id-on" : ""} onClick={() => { if (!playing && i >= n - 1) set(0); play(!playing); }}>{playing ? "Pause" : "Play"}</button>
                <button onClick={() => { play(false); set(0); }}>Restart</button>
                <span className="id-n">step {i + 1} of {n}</span>
            </div>
            <div className="id-ticks" role="group" aria-label="Steps">
                {Array.from({ length: n }, (_, k) => <button key={k} className={k < i ? "id-past" : k === i ? "id-now" : ""} aria-label={`Step ${k + 1}`} aria-current={k === i ? "step" : undefined} onClick={() => { play(false); set(k); }} />)}
            </div>
        </>
    );
}

/** Arrow keys step and Space plays, once the figure has focus (never from the page as a whole). */
export function stepKeys(i: number, n: number, set: (i: number) => void, play: (f: (p: boolean) => boolean) => void) {
    return (e: ReactKeyboardEvent) => {
        if ((e.target as HTMLElement).closest("button") && e.key === " ") return;
        if (e.key === "ArrowRight") set(Math.min(n - 1, i + 1));
        else if (e.key === "ArrowLeft") set(Math.max(0, i - 1));
        else if (e.key === " ") play((p) => !p);
        else return;
        e.preventDefault();
        if (e.key !== " ") play(() => false);
    };
}

function Steps({ title, items }: { title: string; items: { who: string; what: string; say?: string }[] }) {
    const [i, setI] = useState(0);
    const [playing, setPlaying] = useState(false);
    useEffect(() => {
        if (!playing) return;
        if (i >= items.length - 1) {
            setPlaying(false);
            return;
        }
        const t = window.setTimeout(() => setI((x) => x + 1), 1400);
        return () => window.clearTimeout(t);
    }, [playing, i, items.length]);
    const cur = items[i];
    return (
        <figure className="sd-steps-fig" tabIndex={0} onKeyDown={stepKeys(i, items.length, setI, setPlaying)} aria-label={`${title}: step ${i + 1} of ${items.length}. ${cur?.who}: ${cur?.what}`}>
            <div className="sd-steps-h"><span>STEP THROUGH</span><b>{title}</b></div>
            <div className="sd-steps-row" style={{ "--n": items.length } as CSSProperties}>
                {items.map((s, k) => (
                    <button key={k} className={k < i ? "past" : k === i ? "now" : ""} onClick={() => { setPlaying(false); setI(k); }}>
                        <small>{k + 1} · {s.who}</small>
                        <span>{s.what}</span>
                    </button>
                ))}
            </div>
            {cur?.say && <figcaption className="sd-steps-say">{cur.say}</figcaption>}
            <StepControls i={i} n={items.length} playing={playing} set={setI} play={setPlaying} />
        </figure>
    );
}

function Deeper({ title, meta, text, id, start }: { title: string; meta?: string; text: string; id: string; start: number }) {
    return (
        <details className="sd-deeper">
            <summary>
                <span>{title}</span>
                <em>{meta ? `optional · ${meta}` : "optional"}</em>
            </summary>
            <Blocks text={text} id={`${id}-d`} start={start} />
        </details>
    );
}

function Blocks({ text, id, start }: { text: string; id: string; start: number }) {
    const segs = useMemo(() => segments(text), [text]);
    const out: ReactElement[] = [];
    let code = 0;
    let notes = start;
    for (let i = 0; i < segs.length; i++) {
        const s = segs[i]!;
        if (s.k === "md") {
            out.push(<Prose key={i} text={s.text} id={`${id}-p${i}`} start={notes} />);
            notes += countNotes(s.text);
        }
        else if (s.k === "math") out.push(<MathBlock key={i} tex={s.tex} />);
        else if (s.k === "steps") out.push(<Steps key={i} title={s.title} items={s.items} />);
        else if (s.k === "deeper") {
            out.push(<Deeper key={i} title={s.title} meta={s.meta} text={s.text} id={`${id}-${i}`} start={notes} />);
            notes += countNotes(s.text);
        }
        else {
            // consecutive code blocks that name the same group are one block with a tab per language
            const group: CodeSeg[] = [s];
            while (s.group && segs[i + 1]?.k === "code" && (segs[i + 1] as CodeSeg).group === s.group) group.push(segs[++i] as CodeSeg);
            code += 1;
            out.push(<CodeGroup key={i} blocks={group} id={`${id}-c${code}`} />);
        }
    }
    return <>{out}</>;
}

/** Markdown for system design pages: prose with tables, callouts (`> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`, `> [!INTERVIEW]`,
 *  `> [!ASIDE]`), margin notes (`^[…]`) and inline math (`\( … \)`), plus highlighted code, display math, step-throughs and
 *  folds (see `segments`). `id` names code blocks and notes; `notes` is how many margin notes come before this text on the page. */
export function SdMd({ text, id = "md", notes = 0 }: { text: string; id?: string; notes?: number }) {
    return <Blocks text={text} id={id} start={notes} />;
}
