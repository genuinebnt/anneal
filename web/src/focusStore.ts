// The focus timer's state, shared by the card on the problem page and the small timer in the header, so both show the same timer and it
// keeps running while you move between pages. It lives in the browser only: nothing about it is stored on the server, only the lengths you chose.

import { useSyncExternalStore } from "react";
import { getPref, setPref } from "./prefs";

export type Mode = "pom" | "timer" | "sw";
export type Phase = "focus" | "break" | "long";

export interface Cfg {
    mode: Mode;
    focus: number;
    brk: number;
    timer: number;
    /** A short two-note chime when a phase or the countdown ends. Off unless chosen. */
    chime: boolean;
}

export interface State {
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

export const DEFAULTS: Cfg = { mode: "pom", focus: 25, brk: 5, timer: 45, chime: false };

const lengthOf = (c: Cfg, mode: Mode, phase: Phase) => (mode === "timer" ? c.timer : phase === "focus" ? c.focus : phase === "break" ? c.brk : c.brk * 3) * 60;

export function fresh(c: Cfg, mode: Mode = c.mode): State {
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

export const mmss = (n: number) => `${String(Math.floor(n / 60)).padStart(2, "0")}:${String(Math.floor(n % 60)).padStart(2, "0")}`;

interface Snapshot {
    cfg: Cfg;
    s: State;
    /** Focus seconds spent since the app was opened. */
    spent: number;
}

const startCfg: Cfg = { ...DEFAULTS, ...getPref<Partial<Cfg>>("focus.cfg", {}) };
let snap: Snapshot = { cfg: startCfg, s: fresh(startCfg), spent: 0 };
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
let audio: AudioContext | null = null;
let tick: ReturnType<typeof setInterval> | null = null;
let last = 0;

/** Two soft notes. The context is made on a click (browsers refuse sound before one), so a chime is only possible once the timer was used. */
function beep() {
    if (!audio) return;
    void audio.resume();
    const t = audio.currentTime;
    [660, 880].forEach((freq, i) => {
        const at = t + i * 0.25;
        const osc = audio!.createOscillator();
        const gain = audio!.createGain();
        osc.frequency.value = freq;
        gain.gain.setValueAtTime(0.0001, at);
        gain.gain.exponentialRampToValueAtTime(0.2, at + 0.03);
        gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.22);
        osc.connect(gain).connect(audio!.destination);
        osc.start(at);
        osc.stop(at + 0.25);
    });
}

function run() {
    if (snap.s.running && !tick) {
        last = Date.now();
        tick = setInterval(() => {
            const now = Date.now();
            const dt = (now - last) / 1000;
            last = now;
            const before = snap.s;
            const s = advance(before, dt, snap.cfg, snap.cfg.mode);
            const focusing = snap.cfg.mode !== "pom" || before.phase === "focus";
            snap = { ...snap, s, spent: snap.spent + (focusing ? dt : 0) };
            if (s.note && s.note !== before.note && snap.cfg.chime) beep();
            if (!s.running) run();
            emit();
        }, 500);
    } else if (!snap.s.running && tick) {
        clearInterval(tick);
        tick = null;
    }
}

/** Replaces the timer state (a function gets the current one). */
export function setTimer(next: State | ((s: State) => State)) {
    snap = { ...snap, s: typeof next === "function" ? next(snap.s) : next };
    run();
    emit();
}

/** Changes the lengths or the mode; `store` false keeps it out of the saved preference (a length that is only a problem's estimate). */
export function setTimerCfg(cfg: Cfg, store = true) {
    snap = { ...snap, cfg };
    if (store) setPref("focus.cfg", cfg);
    emit();
}

/** Start or pause; a finished countdown starts again. Called from a click, so the chime can be made ready here. */
export function toggleTimer() {
    if (snap.cfg.chime && !audio && typeof AudioContext !== "undefined") audio = new AudioContext();
    const finished = snap.cfg.mode === "timer" && !snap.s.running && snap.s.secs === 0;
    setTimer((x) => (finished ? { ...fresh(snap.cfg), running: true } : { ...x, running: !x.running, note: "" }));
}

export function readyChime() {
    if (!audio && typeof AudioContext !== "undefined") audio = new AudioContext();
}

const subscribe = (l: () => void) => {
    listeners.add(l);
    return () => listeners.delete(l);
};

export function useFocus(): Snapshot {
    return useSyncExternalStore(subscribe, () => snap);
}
