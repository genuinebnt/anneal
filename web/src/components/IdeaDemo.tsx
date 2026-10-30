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
    return (
        <div className="id-work">
            <h3>{w.title}</h3>
            {w.bars.map((b) => (
                <div className="id-wr" key={b.label}><span>{b.label}</span><div className={`id-bar id-${b.tone}`}><b style={{ width: `${(b.value / Math.max(b.of, 1)) * 100}%` }} /></div><em>{b.value}{b.ratio ? `/${b.of}` : ""}</em></div>
            ))}
            <p>{w.note}</p>
        </div>
    );
}

/** Markers on one cell stack, so two never sit on top of each other. */
function placed(f: Frame) {
    const count = new Map<string, number>();
    return f.pointers.map((p) => {
        const key = `${p.at}:${p.below ? 1 : 0}`;
        const k = count.get(key) ?? 0;
        count.set(key, k + 1);
        return { ...p, k };
    });
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
        setText(input.items.join(demo?.letters || demo?.compact ? "" : ","));
        setK(input.param !== undefined ? String(input.param) : "");
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
    const cells = f.cells ?? input.items;
    const pointers = placed(f);
    // room for the markers that stack: the most any step has above and below the row
    const rooms = frames.reduce((m, fr) => {
        const above = new Map<number, number>();
        const below = new Map<number, number>();
        for (const p of fr.pointers) { const mm = p.below ? below : above; mm.set(p.at, (mm.get(p.at) ?? 0) + 1); }
        return [Math.max(m[0], ...above.values(), 1), Math.max(m[1], ...below.values(), 1)] as const;
    }, [1, 1] as readonly [number, number]);
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
                    <label>Try your own <input value={text} onChange={(e) => setText(e.target.value)} onBlur={apply} onKeyDown={(e) => e.key === "Enter" && apply()} className={invalid ? "id-bad" : ""} aria-label={demo.fields.text} maxLength={demo.letters || demo.compact ? 14 : 40} /></label>
                    {demo.fields.param && <label>{demo.fields.param.label.replace(/^Window size /, "")} <input className="id-k" type="number" min={demo.fields.param.min} max={demo.fields.param.max} value={k} onChange={(e) => setK(e.target.value)} onBlur={apply} onKeyDown={(e) => e.key === "Enter" && apply()} aria-label={demo.fields.param.label} /></label>}
                </div>
                <div className="id-stage" role="img" aria-label={`${cells.join(", ")}. ${f.say}`} style={{ paddingTop: 22 + rooms[0] * 27, paddingBottom: 30 + rooms[1] * 27 }}>
                    <div className="id-body">
                        <div className="id-row" style={{ width: cells.length * CELL, "--cell": `${CELL}px` } as CSSProperties}>
                            {f.win && <div className="id-brace" style={{ left: f.win[0] * CELL, width: Math.max(f.win[1] - f.win[0], 1) * CELL, opacity: f.win[1] > f.win[0] ? 1 : 0.35 }} />}
                            {pointers.map((p) => (
                                <div key={`${p.label}${p.below ? "b" : "a"}`} className={`id-ptr id-${p.tone}${p.below ? " id-below" : ""}`} style={{ transform: `translateX(${p.at * CELL}px)`, "--k": p.k } as CSSProperties}><span>{p.label}</span></div>
                            ))}
                            {cells.map((v, j) => {
                                const inside = !!f.win && j >= f.win[0] && j < f.win[1];
                                const cls = [inside && "id-in", f.dim?.includes(j) && "id-gone", f.cur?.includes(j) && "id-cur", f.hit?.includes(j) && "id-hit", f.ok?.includes(j) && "id-ok", f.nope?.includes(j) && "id-nope", f.dup === j && "id-dup", f.enter === j && "id-enter", f.leave === j && "id-leave"].filter(Boolean).join(" ");
                                return (
                                    <div key={j} className={`id-cell ${cls}`} style={{ left: j * CELL, ...(demo.letters ? { "--h": hue(v) } : {}) } as CSSProperties}>
                                        <b>{v}</b>
                                        <small>{j}</small>
                                        {f.enter === j && <u className="id-tag id-e">IN</u>}
                                        {f.leave === j && <u className="id-tag id-l">OUT</u>}
                                    </div>
                                );
                            })}
                        </div>
                        {f.stack && (
                            <div className="id-stackwrap">
                                <span className="id-stl">{f.stack.label}</span>
                                <div className="id-stack" aria-label={`${f.stack.label}: ${f.stack.items.join(" ") || "empty"}`}>
                                    {f.stack.items.map((v, n) => <div key={`${n}${v}`} className={`id-cell id-s${f.stack?.top === "push" && n === f.stack.items.length - 1 ? " id-enter" : ""}`}><b>{v}</b></div>)}
                                </div>
                            </div>
                        )}
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
