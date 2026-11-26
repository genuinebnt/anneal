// What only a hand-written lesson has: the "In short" block, the Connections box and the three blocks a section can use
// (a stepper, a code walkthrough and a wrong and fixed pair). The loader sends each block as JSON, already checked
// (crates/content/src/lesson.rs). Nothing moves on its own and nothing is stored.
import { marked } from "marked";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactElement, type ReactNode } from "react";
import type { CbConnection, CbPage } from "../../api";
import { CodeGroup, SdMd, StepControls } from "../../components/SdMd";
import { highlightLines } from "../../highlight";
import { lineRange } from "./lines";
import { anchorOf, scrollToId } from "./shared";
import "../../styles/cookbook-lesson.css";

const inline = (text: string) => ({ __html: marked.parseInline(text, { async: false }) });

/** Goes to a part of the page, and moves the keyboard there with the view. */
function jump(e: MouseEvent, anchor: string) {
    if (e.metaKey || e.ctrlKey || e.shiftKey) return;
    e.preventDefault();
    history.replaceState(history.state, "", `#${anchor}`);
    const el = document.getElementById(anchor);
    if (!el) return;
    el.tabIndex = -1;
    el.focus({ preventScroll: true });
    scrollToId(anchor);
}

/** The lesson's answer first, with ways straight to the code and to each part. */
export function InShort({ page }: { page: CbPage }) {
    const parts = [
        ...page.sections.map((s) => ({ label: s.heading, id: anchorOf(s.heading) })),
        ...(page.connections?.length ? [{ label: "Connections", id: "cb-connections" }] : []),
    ];
    return (
        <section className="cb-sum cb-short" id="cb-in-short" aria-labelledby="cb-short-h">
            <div>
                <h2 id="cb-short-h">In short</h2>
                <SdMd text={page.in_short ?? ""} id="cb-short" />
                {page.labs.length > 0 && (
                    <a className="cb-btn" href="#cb-lab-0" onClick={(e) => jump(e, "cb-lab-0")}>Jump to code</a>
                )}
            </div>
            <nav aria-label="Jump to">
                <h3>Jump to</h3>
                <ul>
                    {parts.map((p) => (
                        <li key={p.id}><a href={`#${p.id}`} onClick={(e) => jump(e, p.id)} dangerouslySetInnerHTML={inline(p.label)} /></li>
                    ))}
                </ul>
            </nav>
        </section>
    );
}

/** Full code that gives away much of an exercise: it says so, and starts closed. The viewer is made when it is first opened,
 *  and nothing remembers that it was. */
export function SpoilerGate({ spoils, children }: { spoils: { link: string; title: string }[]; children: ReactNode }) {
    const [opened, setOpened] = useState(false);
    return (
        <>
            <aside className="sd-call sd-call-warning cb-spoil-note">
                <b>Spoiler</b>
                <div>
                    <p>
                        This code gives away much of{" "}
                        {spoils.map((s, i) => (
                            <span key={s.link}>
                                {i > 0 && (i === spoils.length - 1 ? " and " : ", ")}
                                <a href={s.link}>{s.title}</a>
                            </span>
                        ))}
                        . Try {spoils.length === 1 ? "the exercise" : "the exercises"} first if you plan to do {spoils.length === 1 ? "it" : "them"}.
                    </p>
                </div>
            </aside>
            <details className="sd-deeper cb-spoil" onToggle={(e) => e.currentTarget.open && setOpened(true)}>
                <summary><span>Show the full code</span><em>spoiler</em></summary>
                {opened && children}
            </details>
        </>
    );
}

const GROUPS: [CbConnection["kind"], string][] = [["before", "Know first"], ["practise", "Practise"], ["returns", "Where it comes back"], ["project", "In real projects"]];

function Entry({ c }: { c: CbConnection }) {
    return (
        <li>
            <p className="cb-conn-t">
                {c.link ? <a href={c.link}>{c.title}</a> : <span>{c.title}</span>}
                {c.level && <em className="cb-kind">{c.level}</em>}
            </p>
            <p dangerouslySetInnerHTML={inline(c.why)} />
        </li>
    );
}

/** How the idea joins others. An exercise that is exactly this page's task stays closed until the reader asks. */
export function Connections({ list }: { list: CbConnection[] }) {
    return (
        <section className="cb-conn" id="cb-connections">
            <h2>Connections</h2>
            <div className="cb-conn-groups">
                {GROUPS.map(([kind, title]) => {
                    const mine = list.filter((c) => c.kind === kind);
                    if (!mine.length) return null;
                    const exact = mine.filter((c) => c.level === "exact");
                    const open = mine.filter((c) => c.level !== "exact");
                    return (
                        <div className="cb-conn-group" key={kind}>
                            <h3>{title}</h3>
                            {open.length > 0 && <ul>{open.map((c, i) => <Entry key={i} c={c} />)}</ul>}
                            {exact.length > 0 && (
                                <details>
                                    <summary>
                                        <span><span className="cb-when-closed">Show</span><span className="cb-when-open">Hide</span> {exact.length === 1 ? "the exact exercise" : `${exact.length} exact exercises`}</span>
                                    </summary>
                                    <ul>{exact.map((c, i) => <Entry key={i} c={c} />)}</ul>
                                </details>
                            )}
                        </div>
                    );
                })}
            </div>
        </section>
    );
}

// ---- the blocks ----

type Lane = { name: string; items: string[]; slots?: number };
type StepperData = { caption: string; frame: { title: string; text: string; lanes: Lane[] }[] };
type WalkData = { file: string; step: { title: string; lines: string; text: string }[] };
type Side = { title: string; code: string; result: string };
type CompareData = { takeaway: string; wrong: Side; fixed: Side };

/** Arrow keys, Home and End move between steps while a button of the block has the keyboard. */
function stepKey(e: KeyboardEvent, i: number, n: number): number | null {
    if (e.altKey || e.ctrlKey || e.metaKey || !(e.target as HTMLElement).closest("button")) return null;
    const to: Record<string, number> = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: n - 1 };
    const next = to[e.key];
    if (next === undefined) return null;
    e.preventDefault();
    return Math.max(0, Math.min(n - 1, next));
}

function LaneView({ lane }: { lane: Lane }) {
    const places = Math.max(lane.slots ?? 0, lane.items.length);
    return (
        <div className="cb-lane">
            <p className="cb-lane-n">
                <span dangerouslySetInnerHTML={inline(lane.name)} />
                {lane.slots !== undefined && <small>{lane.items.length} of {lane.slots} used</small>}
            </p>
            {places === 0 ? (
                <p className="cb-lane-none">nothing</p>
            ) : (
                <ul className={lane.slots !== undefined ? "bounded" : ""}>
                    {Array.from({ length: places }, (_, k) =>
                        k < lane.items.length ? <li key={k} dangerouslySetInnerHTML={inline(lane.items[k]!)} /> : <li key={k} className="free">empty</li>,
                    )}
                </ul>
            )}
        </div>
    );
}

/** A changing state, one frame at a time. Every frame stays in the page for screen readers; only the current one is drawn,
 *  and they share one cell, so the block keeps the height of the tallest and nothing below it moves. */
function Stepper({ data }: { data: StepperData }) {
    const [i, setI] = useState(0);
    const [said, setSaid] = useState("");
    const name = useId();
    const n = data.frame.length;
    const go = (k: number) => {
        setI(k);
        setSaid(`Step ${k + 1} of ${n}: ${data.frame[k]!.title}`);
    };
    return (
        <figure
            className="cb-step"
            aria-labelledby={name}
            onKeyDown={(e) => {
                const k = stepKey(e, i, n);
                if (k !== null) go(k);
            }}
        >
            <figcaption id={name} dangerouslySetInnerHTML={inline(data.caption)} />
            <StepControls i={i} n={n} set={go} />
            <ol className="cb-frames">
                {data.frame.map((f, k) => (
                    // a link in a frame that is not drawn brings its frame up when the keyboard reaches it
                    <li key={k} className={k === i ? "on" : ""} aria-current={k === i ? "step" : undefined} onFocus={() => setI(k)}>
                        <p className="cb-frame-t"><span className="cb-sr">Step {k + 1} of {n}: </span><span dangerouslySetInnerHTML={inline(f.title)} /></p>
                        <div className="cb-frame-say"><SdMd text={f.text} id={`${name}-f${k}`} /></div>
                        <div className="cb-lanes">{f.lanes.map((l) => <LaneView key={l.name} lane={l} />)}</div>
                    </li>
                ))}
            </ol>
            <p className="cb-sr" role="status">{said}</p>
        </figure>
    );
}

/** The room left above and below a walkthrough step's lines. */
const LEAD = 24;

/** Pick a step: its lines are highlighted in the file beside it. */
function Walkthrough({ data, page, id }: { data: WalkData; page: CbPage; id: string }) {
    const [i, setI] = useState(0);
    const pane = useRef<HTMLPreElement>(null);
    const list = useRef<HTMLOListElement>(null);
    const text = page.labs.flatMap((l) => l.contents).find((f) => f.path === data.file)?.text ?? "";
    const lines = useMemo(() => highlightLines(data.file.endsWith(".rs") ? "rust" : "text", text.replace(/\n$/, "")), [data.file, text]);
    const [first, last] = lineRange(data.step[i]!.lines);
    const name = data.file.split("/").pop();
    // The code area grows to hold the whole of a step's lines (the stylesheet sets the least and the most), and the first of
    // them comes near the top, without moving the page. A walkthrough inside a closed fold has no size yet, and a narrower
    // window wraps lines differently, so this runs again whenever the code's width changes.
    useLayoutEffect(() => {
        const box = pane.current;
        if (!box) return;
        const place = () => {
            const from = box.querySelector<HTMLElement>(`[data-line="${first}"]`);
            const to = box.querySelector<HTMLElement>(`[data-line="${last}"]`);
            if (!from || !to || !box.clientWidth) return;
            const tall = to.getBoundingClientRect().bottom - from.getBoundingClientRect().top;
            box.closest<HTMLElement>(".cb-walk")?.style.setProperty("--need", `${Math.ceil(tall) + LEAD * 2}px`);
            box.scrollTop += from.getBoundingClientRect().top - box.getBoundingClientRect().top - LEAD;
            // the chosen step's words stay in sight in the list
            const steps = list.current;
            const on = steps?.children[i];
            if (steps && on) {
                const over = on.getBoundingClientRect().bottom - steps.getBoundingClientRect().bottom;
                if (over > 0) steps.scrollTop += Math.min(over + 8, on.getBoundingClientRect().top - steps.getBoundingClientRect().top);
            }
        };
        let width = box.clientWidth;
        place();
        const watch = new ResizeObserver(() => {
            if (box.clientWidth === width) return;
            width = box.clientWidth;
            place();
        });
        watch.observe(box);
        return () => watch.disconnect();
    }, [first, last, i]);
    const say = (a: number, b: number) => (a === b ? `line ${a}` : `lines ${a} to ${b}`);
    return (
        <figure
            className="cb-view cb-walk"
            aria-label={`Code walkthrough of ${name}`}
            onKeyDown={(e) => {
                const k = stepKey(e, i, data.step.length);
                if (k === null) return;
                setI(k);
                list.current?.querySelectorAll<HTMLButtonElement>("button")[k]?.focus();
            }}
        >
            <div className="cb-view-bar">
                <span className="cb-view-name">Code walkthrough</span>
                <span className="cb-walk-file">{data.file}</span>
            </div>
            <div className="cb-walk-body">
                <ol className="cb-walk-steps" ref={list}>
                    {data.step.map((s, k) => {
                        const [a, b] = lineRange(s.lines);
                        return (
                            <li key={k} className={k === i ? "on" : ""} onFocus={() => setI(k)}>
                                <button type="button" aria-current={k === i ? "step" : undefined} onClick={() => setI(k)}>
                                    <i aria-hidden="true">{k + 1}</i>
                                    <span>
                                        <b dangerouslySetInnerHTML={inline(s.title)} />
                                        <small>{say(a, b)}</small>
                                    </span>
                                </button>
                                <div className="cb-walk-say">
                                    <p className="cb-walk-t" aria-hidden="true"><b dangerouslySetInnerHTML={inline(s.title)} /> <small>{say(a, b)}</small></p>
                                    <SdMd text={s.text} id={`${id}-w${k}`} />
                                </div>
                            </li>
                        );
                    })}
                </ol>
                <div className="cb-view-code sd-code">
                    <pre ref={pane} tabIndex={0} role="region" aria-label={`${data.file}, ${say(first, last)} highlighted`}>
                        {lines.map((html, k) => (
                            <div key={k} data-line={k + 1} className={`sd-ln${k + 1 >= first && k + 1 <= last ? " sd-hl" : ""}`}>
                                <span className="sd-lnum" aria-hidden="true">{k + 1}</span>
                                <span dangerouslySetInnerHTML={{ __html: html || " " }} />
                            </div>
                        ))}
                    </pre>
                </div>
            </div>
        </figure>
    );
}

function CompareSide({ side, wrong, id }: { side: Side; wrong: boolean; id: string }) {
    const block = useMemo(() => [{ k: "code" as const, lang: "rust", hl: new Set<number>(), code: side.code.replace(/^\n+|\n+$/g, "") }], [side.code]);
    const box = useRef<HTMLDivElement>(null);
    // code wider than its box can be scrolled by keyboard too, and fades at the edge while there is more of it
    useEffect(() => {
        const pre = box.current?.querySelector("pre");
        if (!pre) return;
        const check = () => {
            const wide = pre.scrollWidth > pre.clientWidth + 1;
            if (wide) {
                pre.tabIndex = 0;
                pre.setAttribute("role", "region");
                pre.setAttribute("aria-label", `${wrong ? "Wrong" : "Fixed"} code, scrolls sideways`);
            } else {
                for (const a of ["tabindex", "role", "aria-label"]) pre.removeAttribute(a);
            }
            pre.toggleAttribute("data-more", wide && pre.scrollLeft + pre.clientWidth < pre.scrollWidth - 1);
        };
        check();
        const watch = new ResizeObserver(check);
        watch.observe(pre);
        pre.addEventListener("scroll", check, { passive: true });
        return () => {
            watch.disconnect();
            pre.removeEventListener("scroll", check);
        };
    }, [block, wrong]);
    return (
        <div ref={box} className={`cb-cmp-side ${wrong ? "wrong" : "fixed"}`}>
            <p className="cb-cmp-t">
                <em className="cb-kind">{wrong ? "✕ Wrong" : "✓ Fixed"}</em>
                <b dangerouslySetInnerHTML={inline(side.title)} />
            </p>
            <CodeGroup blocks={block} id={`${id}-${wrong ? "wrong" : "fixed"}`} />
            <div className="cb-cmp-result">
                <h4>{wrong ? "What goes wrong" : "What happens now"}</h4>
                <SdMd text={side.result} id={`${id}-${wrong ? "wr" : "fr"}`} />
            </div>
        </div>
    );
}

function Compare({ data, id }: { data: CompareData; id: string }) {
    return (
        <div className="cb-cmp" role="group" aria-label={`Wrong and fixed: ${data.wrong.title}, then ${data.fixed.title}`}>
            <div className="cb-cmp-pair">
                <CompareSide side={data.wrong} wrong id={id} />
                <CompareSide side={data.fixed} wrong={false} id={id} />
            </div>
            <div className="cb-cmp-take">
                <h4>What changed</h4>
                <SdMd text={data.takeaway} id={`${id}-take`} />
            </div>
        </div>
    );
}

/** A block's JSON, or a note in its place: a page must not go blank over one block. */
function block<T>(code: string, draw: (data: T) => ReactElement): ReactElement {
    try {
        return draw(JSON.parse(code) as T);
    } catch {
        return <p className="cb-note">This part of the lesson could not be shown.</p>;
    }
}

/** How a lesson draws its three fences. */
export const lessonFences = (page: CbPage): Record<string, (code: string, id: string) => ReactElement> => ({
    stepper: (code) => block<StepperData>(code, (d) => <Stepper data={d} />),
    walkthrough: (code, id) => block<WalkData>(code, (d) => <Walkthrough data={d} page={page} id={id} />),
    compare: (code, id) => block<CompareData>(code, (d) => <Compare data={d} id={id} />),
});
