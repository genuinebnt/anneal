// A focus timer for the DSA problem page and the Rust workspace (a card in the page's right column): Pomodoro (focus and break phases, a long break after the fourth), a countdown (the interview
// clock) and a stopwatch, all on one ring. It lives in the browser only: nothing about it is stored on the server, only the lengths you chose.

import { useEffect, useRef, useState } from "react";
import { getPref, setPref } from "../prefs";
import { advance, fresh, mmss, readyChime, setTimer, setTimerCfg, toggleTimer, useFocus, type Cfg, type Mode } from "../focusStore";

export { advance } from "../focusStore";

const MODES: [Mode, string][] = [
    ["pom", "POMODORO"],
    ["timer", "TIMER"],
    ["sw", "STOPWATCH"],
];

/** `suggest`: the minutes the problem is estimated to take. In Timer mode the countdown starts there unless the owner chose a length.
 *  The timer itself is shared with the header (`focusStore`), so it keeps running while you move between pages. */
export function FocusTimer({ suggest }: { suggest?: number }) {
    const suggested = suggest ? Math.max(5, Math.round(suggest / 5) * 5) : undefined;
    const { cfg, s, spent } = useFocus();
    const [settings, setSettings] = useState(false);
    const baseTitle = useRef<string | null>(null);
    const chosen = () => getPref<Partial<Cfg>>("focus.cfg", {}).timer !== undefined;

    // The problem's estimate is the countdown length unless the owner chose one; a running timer is left alone.
    useEffect(() => {
        if (!suggested || chosen() || cfg.timer === suggested || s.running) return;
        const next = { ...cfg, timer: suggested };
        setTimerCfg(next, false);
        if (cfg.mode === "timer" && s.secs === s.total) setTimer(fresh(next));
    }, [suggested]); // eslint-disable-line react-hooks/exhaustive-deps

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

    const save = (next: Cfg) => {
        // a countdown length that is only the problem's estimate is not stored as the owner's choice
        if (next.timer === suggested && !chosen()) {
            const { timer: _estimate, ...rest } = next;
            void _estimate;
            setTimerCfg(next, false);
            setPref("focus.cfg", rest);
        } else setTimerCfg(next);
    };
    const setMode = (mode: Mode) => {
        const next = { ...cfg, mode };
        save(next);
        setTimer(fresh(next, mode));
    };
    const setLen = (key: "focus" | "brk" | "timer", v: number) => {
        const next = { ...cfg, [key]: v };
        save(next);
        // A change shows at once while the timer is stopped; a running one keeps its current phase.
        if (!s.running && s.secs === s.total) setTimer(fresh(next));
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
    const worked = Math.floor(spent / 60);

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
                    <button className="ft-go" onClick={toggleTimer}>
                        {s.running ? (
                            <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3" width="3" height="10" rx="1" /><rect x="9.5" y="3" width="3" height="10" rx="1" /></svg>
                        ) : (
                            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.2v9.6a.6.6 0 0 0 .9.5l7.6-4.8a.6.6 0 0 0 0-1L5.9 2.7a.6.6 0 0 0-.9.5z" /></svg>
                        )}
                        {s.running ? "Pause" : finished ? "Restart" : fresh0 ? "Start" : "Resume"}
                    </button>
                    <div className="ft-btns">
                        <button className="ft-sb" aria-label="Reset" title="Reset" onClick={() => setTimer(fresh(cfg))}>
                            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 1 0 2.4-5.7" /><path d="M4 4v4.5h4.5" /></svg>
                        </button>
                        <button className="ft-sb" aria-label="Skip to the next phase" title="Skip to the next phase" disabled={sw || cfg.mode === "timer"} onClick={() => setTimer((x) => ({ ...advance({ ...x, running: true }, x.secs + 1, cfg, cfg.mode), running: x.running }))}>
                            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6.5v11l8-5.5-8-5.5z" /><path d="M18 6v12" /></svg>
                        </button>
                        <button className="ft-sb" aria-label="Settings" aria-expanded={settings} title="Settings" onClick={() => setSettings(!settings)}>
                            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h10M18 7h2M4 17h4M12 17h8" /><circle cx="16" cy="7" r="2" /><circle cx="10" cy="17" r="2" /></svg>
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
                        <input type="checkbox" checked={cfg.chime} onChange={(e) => { save({ ...cfg, chime: e.target.checked }); if (e.target.checked) readyChime(); }} />
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
                            this session <b>{worked} min</b>
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
