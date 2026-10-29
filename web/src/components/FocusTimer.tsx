// A focus timer for the DSA problem page and the Rust workspace (a card in the page's right column): Pomodoro (focus and break phases, a long break after the fourth), a countdown (the interview
// clock) and a stopwatch, all on one ring. It lives in the browser only: nothing about it is stored on the server, only the lengths you chose.

import { useEffect, useRef, useState } from "react";
import { getPref, setPref } from "../prefs";

type Mode = "pom" | "timer" | "sw";
type Phase = "focus" | "break" | "long";

interface Cfg {
    mode: Mode;
    focus: number;
    brk: number;
    timer: number;
    /** A short two-note chime when a phase or the countdown ends. Off unless chosen. */
    chime: boolean;
}

const DEFAULTS: Cfg = { mode: "pom", focus: 25, brk: 5, timer: 45, chime: false };
const MODES: [Mode, string][] = [
    ["pom", "POMODORO"],
    ["timer", "TIMER"],
    ["sw", "STOPWATCH"],
];

interface State {
    phase: Phase;
    /** Seconds left (pomodoro, timer) or elapsed (stopwatch). */
    secs: number;
    total: number;
    running: boolean;
    /** Which of the four focus sessions this is, and how many are done. */
    cycle: number;
    done: number;
    /** What just happened, for the status line. */
    note: string;
}

const lengthOf = (c: Cfg, mode: Mode, phase: Phase) => (mode === "timer" ? c.timer : phase === "focus" ? c.focus : phase === "break" ? c.brk : c.brk * 3) * 60;

function fresh(c: Cfg, mode: Mode = c.mode): State {
    const total = mode === "sw" ? 0 : lengthOf(c, mode, "focus");
    return { phase: "focus", secs: mode === "sw" ? 0 : total, total, running: false, cycle: 1, done: 0, note: "" };
}

/** The state `dt` seconds later. A finished phase moves to the next one (pomodoro) or stops (timer). */
export function advance(s: State, dt: number, c: Cfg, mode: Mode): State {
    if (!s.running) return s;
    if (mode === "sw") return { ...s, secs: s.secs + dt };
    const secs = s.secs - dt;
    if (secs > 0) return { ...s, secs };
    if (mode === "timer") return { ...s, secs: 0, running: false, note: "Time is up." };
    if (s.phase === "focus") {
        const done = s.done + 1;
        const phase: Phase = done % 4 === 0 ? "long" : "break";
        const total = lengthOf(c, mode, phase);
        return { ...s, phase, done, secs: total, total, note: phase === "long" ? "Four sessions done. Take a long break." : "Focus session done. Break time." };
    }
    const cycle = s.phase === "long" ? 1 : Math.min(4, s.cycle + 1);
    const total = lengthOf(c, mode, "focus");
    return { ...s, phase: "focus", cycle, done: s.phase === "long" ? 0 : s.done, secs: total, total, note: "Break over. Back to it." };
}

/** Two soft notes. The context is made on a click (browsers refuse sound before one), so a chime is only possible once the timer was used. */
function beep(ctx: AudioContext | null) {
    if (!ctx) return;
    void ctx.resume();
    const t = ctx.currentTime;
    [660, 880].forEach((freq, i) => {
        const at = t + i * 0.25;
        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.frequency.value = freq;
        gain.gain.setValueAtTime(0.0001, at);
        gain.gain.exponentialRampToValueAtTime(0.2, at + 0.03);
        gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.22);
        osc.connect(gain).connect(ctx.destination);
        osc.start(at);
        osc.stop(at + 0.25);
    });
}

const mmss = (n: number) => `${String(Math.floor(n / 60)).padStart(2, "0")}:${String(Math.floor(n % 60)).padStart(2, "0")}`;

/** `suggest`: the minutes the problem is estimated to take. In Timer mode the countdown starts there unless the owner chose a length. */
export function FocusTimer({ suggest }: { suggest?: number }) {
    const suggested = suggest ? Math.max(5, Math.round(suggest / 5) * 5) : undefined;
    const [cfg, setCfg] = useState<Cfg>(() => ({ ...DEFAULTS, ...(suggested ? { timer: suggested } : {}), ...getPref<Partial<Cfg>>("focus.cfg", {}) }));
    const [s, setS] = useState<State>(() => fresh(cfg));
    const [settings, setSettings] = useState(false);
    // Focus time spent since this page opened (nothing about it is stored).
    const [visit, setVisit] = useState({ focus: 0 });
    const cfgRef = useRef(cfg);
    cfgRef.current = cfg;
    const phaseRef = useRef<Phase>("focus");
    phaseRef.current = s.phase;
    const last = useRef(0);
    const audio = useRef<AudioContext | null>(null);
    const baseTitle = useRef<string | null>(null);
    const wake = () => {
        if (cfgRef.current.chime && !audio.current && typeof AudioContext !== "undefined") audio.current = new AudioContext();
    };

    // While the timer runs the tab title carries the time, so it can be read from the LeetCode tab. It never starts the timer.
    useEffect(() => {
        if (s.running) {
            baseTitle.current ??= document.title;
            document.title = `${mmss(s.secs)} ${cfg.mode === "sw" ? "elapsed" : "left"} · ${baseTitle.current}`;
        } else if (baseTitle.current !== null) {
            document.title = baseTitle.current;
            baseTitle.current = null;
        }
    }, [s.running, s.secs, cfg.mode]);
    useEffect(
        () => () => {
            if (baseTitle.current !== null) document.title = baseTitle.current;
        },
        [],
    );
    // A phase or the countdown ended (`note` is set only then).
    useEffect(() => {
        if (s.note && cfgRef.current.chime) beep(audio.current);
    }, [s.note]);

    useEffect(() => {
        if (!s.running) return;
        last.current = Date.now();
        const id = setInterval(() => {
            const now = Date.now();
            const dt = (now - last.current) / 1000;
            last.current = now;
            setS((prev) => advance(prev, dt, cfgRef.current, cfgRef.current.mode));
            if (cfgRef.current.mode !== "pom" || phaseRef.current === "focus") setVisit((v) => ({ focus: v.focus + dt }));
        }, 500);
        return () => clearInterval(id);
    }, [s.running]);

    const save = (next: Cfg) => {
        setCfg(next);
        // a countdown length that is only the problem's estimate is not stored as the owner's choice
        const { timer, ...rest } = next;
        setPref("focus.cfg", timer === suggested && getPref<Partial<Cfg>>("focus.cfg", {}).timer === undefined ? rest : next);
    };
    const setMode = (mode: Mode) => {
        const next = { ...cfg, mode };
        save(next);
        setS(fresh(next, mode));
    };
    const setLen = (key: "focus" | "brk" | "timer", v: number) => {
        const next = { ...cfg, [key]: v };
        save(next);
        // A change shows at once while the timer is stopped; a running one keeps its current phase.
        if (!s.running && s.secs === s.total) setS(fresh(next));
    };

    const sw = cfg.mode === "sw";
    const p = sw ? ((s.secs % 60) / 60) * 100 : s.total ? ((s.total - s.secs) / s.total) * 100 : 0;
    const frac = s.total ? s.secs / s.total : 1;
    const tone = sw ? "" : cfg.mode === "pom" ? (s.phase === "focus" ? "" : " ft-brk") : frac < 0.05 ? " ft-crit" : frac < 0.2 ? " ft-warn" : "";
    const label = sw ? "ELAPSED" : cfg.mode === "timer" ? "REMAINING" : s.phase === "focus" ? `FOCUS ${s.cycle} / 4` : s.phase === "long" ? "LONG BREAK" : "BREAK";
    const fresh0 = sw ? s.secs === 0 : s.secs === s.total;
    // A countdown that ran out offers to start again, not to resume at zero.
    const finished = cfg.mode === "timer" && !s.running && s.secs === 0;
    const range = (min: number, max: number, v: number) => ({ "--v": `${(100 * (v - min)) / (max - min)}%` }) as React.CSSProperties;
    const worked = Math.floor(visit.focus / 60);

    return (
        <div className={`ft${tone}`} aria-label="Focus timer">
            <h6 className="ft-h">
                <span>FOCUS</span>
                <span className="ft-modes" role="group" aria-label="Timer mode">
                    {MODES.map(([m, l]) => (
                        <button key={m} aria-pressed={cfg.mode === m} className={cfg.mode === m ? "ft-on" : ""} onClick={() => setMode(m)}>
                            {l}
                        </button>
                    ))}
                </span>
            </h6>
            <div className="ft-body">
                <div className={`ft-ring${s.running ? " ft-run" : ""}`} style={{ "--p": p } as React.CSSProperties} role="timer" aria-label={`${label} ${mmss(s.secs)}`}>
                    <span>
                        <b>{mmss(s.secs)}</b>
                        <small>{label}</small>
                    </span>
                </div>
                <div className="ft-ctl">
                    <button className="ft-go" onClick={() => { wake(); setS((x) => (finished ? { ...fresh(cfg), running: true } : { ...x, running: !x.running, note: "" })); }}>
                        {s.running ? "Pause" : finished ? "Restart" : fresh0 ? "Start" : "Resume"}
                    </button>
                    <div className="ft-btns">
                        <button className="ft-sb" aria-label="Reset" title="Reset" onClick={() => setS(fresh(cfg))}>
                            ↺
                        </button>
                        <button className="ft-sb" aria-label="Skip to the next phase" title="Skip to the next phase" disabled={sw || cfg.mode === "timer"} onClick={() => setS((x) => ({ ...advance({ ...x, running: true }, x.secs + 1, cfg, cfg.mode), running: x.running }))}>
                            ⏭
                        </button>
                        <button className="ft-sb" aria-label="Settings" aria-expanded={settings} title="Settings" onClick={() => setSettings(!settings)}>
                            ⚙
                        </button>
                    </div>
                </div>
            </div>
            {cfg.mode === "pom" && (
                <div className="ft-dots" aria-hidden="true">
                    {[1, 2, 3, 4].map((i) => (
                        <i key={i} className={i <= s.done ? "ft-done" : i === s.cycle && s.phase === "focus" ? "ft-cur" : ""} />
                    ))}
                </div>
            )}
            <div className={`ft-set${settings ? " ft-open" : ""}`}>
                <div inert={!settings}>
                    <label className="ft-rng">
                        <span className="ft-rh">
                            <span>Focus</span>
                            <b>{cfg.focus} min</b>
                        </span>
                        <input type="range" min={5} max={60} step={5} value={cfg.focus} style={range(5, 60, cfg.focus)} onChange={(e) => setLen("focus", +e.target.value)} />
                    </label>
                    <label className="ft-rng">
                        <span className="ft-rh">
                            <span>Break</span>
                            <b>{cfg.brk} min</b>
                        </span>
                        <input type="range" min={3} max={20} value={cfg.brk} style={range(3, 20, cfg.brk)} onChange={(e) => setLen("brk", +e.target.value)} />
                    </label>
                    <label className="ft-chk">
                        <input type="checkbox" checked={cfg.chime} onChange={(e) => { save({ ...cfg, chime: e.target.checked }); if (e.target.checked && !audio.current && typeof AudioContext !== "undefined") audio.current = new AudioContext(); }} />
                        <span>Chime when time is up</span>
                    </label>
                    {cfg.mode === "timer" && (
                        <label className="ft-rng">
                            <span className="ft-rh">
                                <span>Countdown</span>
                                <b>{cfg.timer} min</b>
                            </span>
                            <input type="range" min={5} max={90} step={5} value={cfg.timer} style={range(5, 90, cfg.timer)} onChange={(e) => setLen("timer", +e.target.value)} />
                        </label>
                    )}
                </div>
            </div>
            <div className="ft-sum" role="status" aria-live="polite">
                {s.note ? (
                    <span>{s.note}</span>
                ) : (
                    <>
                        <span>
                            this visit <b>{worked} min</b>
                        </span>
                        <span>
                            <b>{s.done}</b> pomodoro{s.done === 1 ? "" : "s"}
                        </span>
                    </>
                )}
            </div>
        </div>
    );
}
