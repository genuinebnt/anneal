import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { DEMOS, type DemoInput, type Frame, type Reading } from "../ideaDemos";
import { highlightPy } from "./dsaBits";
import { Segmented } from "./ui";

const CELL = 56;
const hue = (c: string) => (c.charCodeAt(0) * 47) % 360;

/** A number that counts to its new value instead of jumping (at once under reduced motion). */
function Num({ value }: { value: string | number }) {
    const target = typeof value === "number" ? value : Number.NaN;
    const [shown, setShown] = useState(target);
    const from = useRef(target);
    useEffect(() => {
        if (Number.isNaN(target)) return;
        const calm = document.documentElement.dataset.motion === "reduce" || window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
        if (calm || from.current === target) {
            from.current = target;
            setShown(target);
            return;
        }
        const start = from.current;
        const t0 = performance.now();
        let raf = 0;
        const step = (t: number) => {
            const k = Math.min(1, (t - t0) / 420);
            setShown(Math.round(start + (target - start) * (1 - (1 - k) ** 3)));
            if (k < 1) raf = requestAnimationFrame(step);
            else from.current = target;
        };
        raf = requestAnimationFrame(step);
        return () => cancelAnimationFrame(raf);
    }, [target]);
    return <>{Number.isNaN(target) ? value : shown}</>;
}

function Read({ r }: { r: Reading }) {
    return (
        <div className="id-read">
            <span>{r.label}</span>
            {r.chips ? (
                <div className="id-chips" aria-label={r.value ? `${r.label}: ${r.value}` : `${r.label}: empty`}>
                    {r.chips.map((c) => <i key={c} style={{ "--h": hue(c) } as CSSProperties}>{c}</i>)}
                    {r.chips.length === 0 && <em>empty</em>}
                </div>
            ) : (
                <b><Num value={r.value} /></b>
            )}
        </div>
    );
}

function Work({ f }: { f: Frame }) {
    const w = f.work;
    if (w.kind === "fixed") {
        const top = Math.max(w.brute, 1);
        return (
            <div className="id-work">
                <h3>ADDITIONS SO FAR</h3>
                <div className="id-wr"><span>Recompute each window</span><div className="id-bar id-bad"><b style={{ width: `${(w.brute / top) * 100}%` }} /></div><em>{w.brute}</em></div>
                <div className="id-wr"><span>Slide</span><div className="id-bar id-good"><b style={{ width: `${(w.slide / top) * 100}%` }} /></div><em>{w.slide}</em></div>
                <p>Recomputing adds k numbers for every window. Sliding adds one and takes one away. Make k bigger above and the gap grows.</p>
            </div>
        );
    }
    return (
        <div className="id-work">
            <h3>EACH LETTER COMES IN ONCE AND GOES OUT AT MOST ONCE</h3>
            <div className="id-wr"><span>Came in</span><div className="id-bar id-good"><b style={{ width: `${(w.enters / w.n) * 100}%` }} /></div><em>{w.enters}/{w.n}</em></div>
            <div className="id-wr"><span>Went out</span><div className="id-bar id-bad"><b style={{ width: `${(w.leaves / w.n) * 100}%` }} /></div><em>{w.leaves}/{w.n}</em></div>
            <p>The inner while loop looks like extra work, but the left end never moves back, so it can never move more than n times in total. That is why this is O(n).</p>
        </div>
    );
}

/** The step-through of an Idea tab: the input as cells with the window over them, the readings, the Python with the running lines lit,
 *  and what it saves. Step with the buttons, the ticks, or ← and → once it has focus; Space plays. */
export function IdeaDemo({ names }: { names: string[] }) {
    const known = names.filter((n) => DEMOS[n]);
    const [name, setName] = useState(known[0] ?? "");
    const demo = DEMOS[name];
    const [inputs, setInputs] = useState<Record<string, DemoInput>>({});
    const [i, setI] = useState(0);
    const [playing, setPlaying] = useState(false);
    const [text, setText] = useState("");
    const [k, setK] = useState("");
    const [invalid, setInvalid] = useState(false);
    const input = demo ? (inputs[name] ?? demo.initial) : null;
    const frames = useMemo(() => (demo && input ? demo.simulate(input) : []), [demo, input]);
    const last = frames.length - 1;

    // the typed text follows the demo shown
    useEffect(() => {
        if (!input) return;
        setText(input.items.join(input.type === "numbers" ? "," : ""));
        setK(input.type === "numbers" ? String(input.k) : "");
        setInvalid(false);
    }, [name]); // eslint-disable-line react-hooks/exhaustive-deps
    useEffect(() => {
        if (!playing) return;
        const t = window.setInterval(() => setI((x) => (x >= last ? x : x + 1)), 1500);
        return () => window.clearInterval(t);
    }, [playing, last]);
    useEffect(() => {
        if (i >= last) setPlaying(false);
    }, [i, last]);

    if (!demo || !input || frames.length === 0) return null;
    const f = frames[Math.min(i, last)]!;
    const cells = input.items;
    const right = f.probe ?? (f.r > f.l ? f.r - 1 : null);
    const apply = () => {
        const next = demo.parse(text, k);
        setInvalid(!next);
        if (!next) return;
        setInputs((m) => ({ ...m, [name]: next }));
        setI(0);
        setPlaying(false);
    };
    const key = (e: KeyboardEvent) => {
        if ((e.target as HTMLElement).closest("input")) return;
        if (e.key === "ArrowRight") setI((x) => Math.min(last, x + 1));
        else if (e.key === "ArrowLeft") setI((x) => Math.max(0, x - 1));
        else if (e.key === " ") {
            e.preventDefault();
            setPlaying((p) => !p);
        } else return;
        if (e.key !== " ") setPlaying(false);
    };

    return (
        <div className="id-lab" onKeyDown={key}>
            <div className="id-col">
                {known.length > 1 && <Segmented label="Demo" value={name} onChange={(v) => { setName(v); setI(0); setPlaying(false); }} options={known.map((n) => [n, DEMOS[n]!.title] as const)} />}
                <div className="id-try">
                    <label>Try your own <input value={text} onChange={(e) => setText(e.target.value)} onBlur={apply} onKeyDown={(e) => e.key === "Enter" && apply()} className={invalid ? "id-bad" : ""} aria-label={input.type === "numbers" ? "Numbers, separated by commas" : "A string of lowercase letters"} maxLength={input.type === "numbers" ? 40 : 14} /></label>
                    {input.type === "numbers" && <label>k <input className="id-k" type="number" min={1} value={k} onChange={(e) => setK(e.target.value)} onBlur={apply} onKeyDown={(e) => e.key === "Enter" && apply()} aria-label="Window size k" /></label>}
                </div>
                <div className="id-stage" role="img" aria-label={`The window over ${cells.join(", ")}, from index ${f.l} to ${f.r - 1}`}>
                    <div className="id-row" style={{ width: cells.length * CELL, "--cell": `${CELL}px` } as CSSProperties}>
                        <div className="id-brace" style={{ left: f.l * CELL, width: Math.max(f.r - f.l, 1) * CELL, opacity: f.r > f.l ? 1 : 0.35 }} />
                        {right !== null && <div className="id-ptr id-r" style={{ transform: `translateX(${right * CELL}px)` }}><span>R</span></div>}
                        <div className="id-ptr id-l" style={{ transform: `translateX(${f.l * CELL}px)` }}><span>L</span></div>
                        {cells.map((v, j) => {
                            const inside = j >= f.l && j < f.r;
                            const letter = input.type === "letters";
                            return (
                                <div key={j} className={`id-cell${inside ? " id-in" : ""}${f.dup === j ? " id-dup" : ""}${f.enter === j ? " id-enter" : ""}${f.leave === j ? " id-leave" : ""}`} style={{ left: j * CELL, ...(letter ? { "--h": hue(v) } : {}) } as CSSProperties}>
                                    <b>{v}</b>
                                    <small>{j}</small>
                                    {f.enter === j && <u className="id-tag id-e">IN</u>}
                                    {f.leave === j && <u className="id-tag id-l">OUT</u>}
                                </div>
                            );
                        })}
                    </div>
                </div>
                <div className="id-reads">{f.read.map((r) => <Read key={r.label} r={r} />)}</div>
                <div className={`id-say${f.result ? " id-done" : ""}`} aria-live="polite" key={i}>
                    {f.result && <span className="id-res" aria-label={`Answer ${f.result}`}><small>ANSWER</small><b>{f.result}</b>{Array.from({ length: 8 }, (_, n) => <i key={n} style={{ "--a": `${n * 45}deg` } as CSSProperties} />)}</span>}
                    <span>{f.say}</span>
                </div>
                <div className="id-ctl">
                    <button onClick={() => { setPlaying(false); setI(Math.max(0, i - 1)); }} disabled={i === 0}>← Back</button>
                    <button onClick={() => { setPlaying(false); setI(Math.min(last, i + 1)); }} disabled={i === last}>Next →</button>
                    <button className={playing ? "id-on" : ""} onClick={() => { if (!playing && i >= last) setI(0); setPlaying(!playing); }}>{playing ? "Pause" : "Play"}</button>
                    <button onClick={() => { setPlaying(false); setI(0); }}>Restart</button>
                    <span className="id-n">step {i + 1} of {frames.length}</span>
                </div>
                <div className="id-ticks" role="group" aria-label="Steps">
                    {frames.map((_, n) => <button key={n} className={n < i ? "id-past" : n === i ? "id-now" : ""} aria-label={`Step ${n + 1}`} aria-current={n === i ? "step" : undefined} onClick={() => { setPlaying(false); setI(n); }} />)}
                </div>
                <span className="id-keys">Keys: <kbd>←</kbd> <kbd>→</kbd> step, <kbd>Space</kbd> play, once the demo has focus.</span>
            </div>
            <div className="id-col">
                <div className="id-code" aria-label="The code, with the lines that run lit">
                    {demo.code.map((line, n) => (
                        <div key={n} className={`id-ln${f.lines.includes(n + 1) ? " id-on" : ""}`}><i>{n + 1}</i><span>{highlightPy(line)}</span></div>
                    ))}
                </div>
                <Work f={f} />
            </div>
        </div>
    );
}
