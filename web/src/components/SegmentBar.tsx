export type Segment = "done" | "part" | "";

/** One thin bar with a slice per item: done, partly done, or still empty. The state is a data attribute, so no other
 *  class in the app can reach a slice. Its look is in styles/design.css; `className` is only for placing it. */
export function SegmentBar({ segments, label, className = "", decorative = false }: { segments: Segment[]; label?: string; className?: string; decorative?: boolean }) {
    return (
        <span className={`seg-bar${className ? ` ${className}` : ""}`} {...(decorative ? { "aria-hidden": true } : { role: "img", "aria-label": label })}>
            {segments.map((s, i) => <b key={i} data-state={s || undefined} />)}
        </span>
    );
}
