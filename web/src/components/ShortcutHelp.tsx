import { useEffect, useRef, useState } from "react";
import { useDialog } from "./useDialog";

const GROUPS: { title: string; keys: [string[], string][] }[] = [
    { title: "Everywhere", keys: [[["⌘", "K"], "Search tracks, stages, concepts and problems"], [["?"], "This list"]] },
    {
        title: "A DSA problem",
        keys: [
            [["1"], "Log On my own"],
            [["2"], "Log With help"],
            [["3"], "Log Attempted"],
            [["4"], "Log Instant"],
            [["U"], "Undo the last log (or restore a reset) while its message is showing"],
        ],
    },
    {
        title: "Review",
        keys: [
            [["Space"], "Show the approach"],
            [["1", "2", "3", "4"], "Again, Hard, Good, Easy"],
            [["J", "K"], "Next and previous review (arrows work too)"],
            [["S"], "Skip to the next"],
            [["U"], "Undo the last grade"],
        ],
    },
    {
        title: "A pattern",
        keys: [
            [["1", "2", "3"], "Learn, Problems, Practice"],
            [["[", "]"], "Previous and next pattern"],
            [["Esc"], "Back to the problems"],
        ],
    },
    {
        title: "Calendar",
        keys: [
            [["←", "→"], "Previous and next month"],
            [["T"], "Today"],
            [["S", "P", "B", "R"], "Solve, practice, break or routine for the selected days"],
            [["Z"], "Undo the last edit"],
            [["Esc"], "Clear the selection"],
        ],
    },
    {
        title: "Tracks",
        keys: [
            [["/"], "Filter the table"],
            [["J", "K"], "Move down and up a row"],
            [["Enter"], "Open the row"],
        ],
    },
];

/** `?` opens the list of every key; Escape or a click outside closes it. It ignores the key while you type. */
export function ShortcutHelp() {
    const [open, setOpen] = useState(false);
    const close = useRef<HTMLButtonElement>(null);
    const box = useRef<HTMLDivElement>(null);
    useDialog(box, open, () => setOpen(false));
    useEffect(() => {
        const f = (e: KeyboardEvent) => {
            if (e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement | null)?.closest?.("input,textarea,select,[contenteditable],.cm-editor")) return;
            if (e.key === "?") {
                e.preventDefault();
                setOpen((o) => !o);
            } else if (e.key === "Escape") setOpen(false);
        };
        const show = () => setOpen(true);
        window.addEventListener("keydown", f);
        window.addEventListener("anneal:help", show);
        return () => {
            window.removeEventListener("keydown", f);
            window.removeEventListener("anneal:help", show);
        };
    }, []);
    useEffect(() => {
        if (open) close.current?.focus();
    }, [open]);
    if (!open) return null;
    return (
        <div className="sh-back" onMouseDown={(e) => e.target === e.currentTarget && setOpen(false)}>
            <div className="sh-box" ref={box} role="dialog" aria-modal="true" aria-label="Keyboard shortcuts">
                <div className="sh-head">
                    <h2>Keyboard shortcuts</h2>
                    <button ref={close} className="sh-x" aria-label="Close" onClick={() => setOpen(false)}>✕</button>
                </div>
                <div className="sh-grid">
                    {GROUPS.map((g) => (
                        <section key={g.title}>
                            <h3>{g.title}</h3>
                            <dl>
                                {g.keys.map(([keys, what]) => (
                                    <div key={what}>
                                        <dt>{keys.map((k) => <kbd key={k}>{k}</kbd>)}</dt>
                                        <dd>{what}</dd>
                                    </div>
                                ))}
                            </dl>
                        </section>
                    ))}
                </div>
                <p className="sh-foot">Keys are ignored while you type in a box or the editor. The editor has its own Vim keys when Vim mode is on.</p>
            </div>
        </div>
    );
}
