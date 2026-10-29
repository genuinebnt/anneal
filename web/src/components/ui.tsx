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
