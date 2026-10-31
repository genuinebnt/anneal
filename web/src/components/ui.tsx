// The shared controls: a switch, a segmented control, a button and a two-step confirm. Their look is defined once, in styles/ui.css, which the older
// one-off classes (`.st-sw`, `.d-sw`, `.k-sw`, `.st-seg`, `.rv-sort`, ...) are also pointed at; new code uses these.
import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";

export function Switch({ on, onChange, label, disabled, className = "" }: { on: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean; className?: string }) {
    return <button type="button" role="switch" aria-checked={on} aria-label={label} disabled={disabled} className={`ui-sw${className ? ` ${className}` : ""}`} onClick={() => onChange(!on)} />;
}

export function Segmented<T extends string>({ value, options, onChange, label, className = "" }: { value: T; options: readonly (readonly [T, string])[]; onChange: (v: T) => void; label: string; className?: string }) {
    return (
        <div className={`ui-seg${className ? ` ${className}` : ""}`} role="radiogroup" aria-label={label}>
            {options.map(([v, text]) => (
                <button key={v} type="button" role="radio" aria-checked={value === v} className={value === v ? "on" : ""} onClick={() => onChange(v)}>
                    {text}
                </button>
            ))}
        </div>
    );
}

type Variant = "primary" | "secondary" | "quiet" | "danger";

export function Btn({ variant = "secondary", className = "", ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
    return <button type="button" className={`ui-btn ${variant}${className ? ` ${className}` : ""}`} {...rest} />;
}

/** A destructive action asks once, in place: the button turns into "<what>?  Yes, <do it> / Keep". It goes back by itself after a few seconds. */
export function ConfirmButton({ label, confirm, onConfirm, note, variant = "danger" }: { label: string; confirm: string; onConfirm: () => void; note?: ReactNode; variant?: Variant }) {
    const [asking, setAsking] = useState(false);
    const timer = useRef(0);
    useEffect(() => () => window.clearTimeout(timer.current), []);
    if (!asking)
        return (
            <Btn variant="quiet" onClick={() => { setAsking(true); window.clearTimeout(timer.current); timer.current = window.setTimeout(() => setAsking(false), 8000); }}>
                {label}
            </Btn>
        );
    return (
        <span className="ui-confirm" role="group" aria-label={label}>
            {note && <small>{note}</small>}
            <Btn variant={variant} onClick={() => { setAsking(false); onConfirm(); }}>{confirm}</Btn>
            <Btn variant="quiet" onClick={() => setAsking(false)}>Keep</Btn>
        </span>
    );
}

/** A number with minus and plus in one pill, held between `min` and `max`. Holding a button repeats it; the number bumps when it changes. */
export function Stepper({ value, onChange, min, max, step = 1, label, compact = false, disabled = false }: { value: number; onChange: (n: number) => void; min: number; max: number; step?: number; label: string; compact?: boolean; disabled?: boolean }) {
    const latest = useRef(value);
    latest.current = value;
    const hold = useRef(0);
    const out = useRef<HTMLOutputElement>(null);
    const first = useRef(true);
    useEffect(() => {
        if (first.current) {
            first.current = false;
            return;
        }
        const el = out.current;
        if (!el) return;
        el.classList.remove("bump");
        void el.offsetWidth; // restart the animation
        el.classList.add("bump");
    }, [value]);
    const stop = () => window.clearTimeout(hold.current);
    useEffect(() => stop, []);
    const nudge = (dir: 1 | -1) => {
        const next = Math.max(min, Math.min(max, latest.current + dir * step));
        if (next !== latest.current) onChange(next);
        return next !== latest.current && next !== min && next !== max;
    };
    const press = (dir: 1 | -1) => {
        stop();
        let wait = 420;
        const again = () => {
            if (!nudge(dir)) return;
            wait = Math.max(60, wait * 0.8);
            hold.current = window.setTimeout(again, wait);
        };
        hold.current = window.setTimeout(again, wait);
    };
    const btn = (dir: 1 | -1) => (
        <button
            type="button"
            aria-label={`${dir < 0 ? "Less" : "More"} ${label}`}
            disabled={disabled || (dir < 0 ? value <= min : value >= max)}
            onClick={(e) => e.detail === 0 && nudge(dir)}
            onPointerDown={(e) => {
                if (e.button !== 0) return;
                nudge(dir);
                press(dir);
            }}
            onPointerUp={stop}
            onPointerLeave={stop}
            onPointerCancel={stop}
        >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
                <path d={dir < 0 ? "M3.5 8h9" : "M3.5 8h9M8 3.5v9"} />
            </svg>
        </button>
    );
    return (
        <span className={`ui-step${compact ? " compact" : ""}${disabled ? " off" : ""}`} role="group" aria-label={label}>
            {btn(-1)}
            <output ref={out} aria-live="polite">{value}</output>
            {btn(1)}
        </span>
    );
}

/** Closes a popover on a click outside it or Escape, and gives focus back to what opened it. */
function usePopover(open: boolean, close: () => void) {
    const box = useRef<HTMLDivElement>(null);
    useEffect(() => {
        if (!open) return;
        const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && close();
        const esc = (e: KeyboardEvent) => {
            if (e.key !== "Escape") return;
            e.stopPropagation();
            close();
            box.current?.querySelector<HTMLElement>(".ui-trigger")?.focus();
        };
        document.addEventListener("mousedown", away);
        document.addEventListener("keydown", esc);
        return () => {
            document.removeEventListener("mousedown", away);
            document.removeEventListener("keydown", esc);
        };
    }, [open, close]);
    return box;
}

/** A pick from a short list, drawn in the app's own look (the browser's list ignores the theme). Arrow keys move, Enter picks, Escape closes. */
export function Select<T extends string>({ value, options, onChange, label, className = "" }: { value: T; options: readonly (readonly [T, string])[]; onChange: (v: T) => void; label: string; className?: string }) {
    const [open, setOpen] = useState(false);
    const [at, setAt] = useState(0);
    const box = usePopover(open, () => setOpen(false));
    const current = options.find(([v]) => v === value);
    const show = () => {
        setAt(Math.max(0, options.findIndex(([v]) => v === value)));
        setOpen(true);
    };
    const pick = (v: T) => {
        onChange(v);
        setOpen(false);
        box.current?.querySelector<HTMLElement>(".ui-trigger")?.focus();
    };
    return (
        <div className={`ui-pop${className ? ` ${className}` : ""}`} ref={box}>
            <button
                type="button"
                className="ui-trigger"
                aria-label={label}
                aria-haspopup="listbox"
                aria-expanded={open}
                onClick={() => (open ? setOpen(false) : show())}
                onKeyDown={(e) => {
                    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                        e.preventDefault();
                        if (!open) show();
                        else setAt((i) => Math.min(options.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1))));
                    } else if (e.key === "Enter" && open) {
                        e.preventDefault();
                        const o = options[at];
                        if (o) pick(o[0]);
                    }
                }}
            >
                <span>{current?.[1] ?? "—"}</span>
                <svg viewBox="0 0 10 6" width="10" height="6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M1 1l4 4 4-4" /></svg>
            </button>
            {open && (
                <ul className="ui-list" role="listbox" aria-label={label}>
                    {options.map(([v, text], i) => (
                        <li key={v} role="option" aria-selected={v === value} className={`${v === value ? "on" : ""}${i === at ? " at" : ""}`} onMouseEnter={() => setAt(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => pick(v)}>
                            {text}
                            {v === value && <b aria-hidden="true">✓</b>}
                        </li>
                    ))}
                </ul>
            )}
        </div>
    );
}

const pad = (n: number) => String(n).padStart(2, "0");
const iso = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`;
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** A date picked from a month grid in the app's own look. The value is `YYYY-MM-DD` or "". Weeks start on Monday. */
export function DateField({ value, onChange, label, min, clearable = true, className = "" }: { value: string; onChange: (v: string) => void; label: string; min?: string; clearable?: boolean; className?: string }) {
    const [open, setOpen] = useState(false);
    const box = usePopover(open, () => setOpen(false));
    const todayIso = new Date().toLocaleDateString("en-CA");
    const start = value || todayIso;
    const [view, setView] = useState({ y: Number(start.slice(0, 4)), m: Number(start.slice(5, 7)) - 1 });
    const show = () => {
        const s = value || todayIso;
        setView({ y: Number(s.slice(0, 4)), m: Number(s.slice(5, 7)) - 1 });
        setOpen(true);
    };
    const move = (by: number) => setView(({ y, m }) => {
        const t = m + by;
        return { y: y + Math.floor(t / 12), m: ((t % 12) + 12) % 12 };
    });
    const lead = (new Date(view.y, view.m, 1).getDay() + 6) % 7;
    const days = new Date(view.y, view.m + 1, 0).getDate();
    const cells = Array.from({ length: Math.ceil((lead + days) / 7) * 7 }, (_, i) => (i < lead || i >= lead + days ? 0 : i - lead + 1));
    const shown = value ? new Date(`${value}T00:00`).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short", year: "numeric" }) : "Pick a date";
    const pick = (v: string) => {
        onChange(v);
        setOpen(false);
        box.current?.querySelector<HTMLElement>(".ui-trigger")?.focus();
    };
    return (
        <div className={`ui-pop${className ? ` ${className}` : ""}`} ref={box}>
            <button type="button" className={`ui-trigger${value ? "" : " empty"}`} aria-label={label} aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? setOpen(false) : show())}>
                <span>{shown}</span>
                <svg viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="1.5" y="2.5" width="11" height="10" rx="2" /><path d="M1.5 5.5h11M4.5 1v3M9.5 1v3" /></svg>
            </button>
            {open && (
                <div className="ui-cal" role="dialog" aria-label={`${label}: choose a date`}>
                    <div className="ui-cal-head">
                        <button type="button" aria-label="Previous year" onClick={() => move(-12)}>«</button>
                        <button type="button" aria-label="Previous month" onClick={() => move(-1)}>‹</button>
                        <b>{MONTHS[view.m]} {view.y}</b>
                        <button type="button" aria-label="Next month" onClick={() => move(1)}>›</button>
                        <button type="button" aria-label="Next year" onClick={() => move(12)}>»</button>
                    </div>
                    <div className="ui-cal-grid">
                        {["M", "T", "W", "T", "F", "S", "S"].map((d, i) => <span key={i} className="dow">{d}</span>)}
                        {cells.map((d, i) => {
                            if (!d) return <span key={i} />;
                            const v = iso(view.y, view.m, d);
                            const blocked = !!min && v < min;
                            return (
                                <button key={i} type="button" disabled={blocked} className={`${v === value ? "on" : ""}${v === todayIso ? " today" : ""}`} aria-label={new Date(`${v}T00:00`).toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" })} aria-pressed={v === value} onClick={() => pick(v)}>
                                    {d}
                                </button>
                            );
                        })}
                    </div>
                    <div className="ui-cal-foot">
                        {clearable && value ? <button type="button" onClick={() => pick("")}>Clear</button> : <span />}
                        <button type="button" disabled={!!min && todayIso < min} onClick={() => pick(todayIso)}>Today</button>
                    </div>
                </div>
            )}
        </div>
    );
}
