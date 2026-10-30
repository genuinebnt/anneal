import { useRouterState } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { getPref, setPref } from "../prefs";

interface Tip {
    id: string;
    /** the page paths it belongs on; none means anywhere */
    on?: RegExp;
    keys: string[];
    text: string;
}

const TIPS: Tip[] = [
    { id: "palette", keys: ["⌘", "K"], text: "Search every page, problem and setting from anywhere." },
    { id: "help", keys: ["?"], text: "Shows every keyboard shortcut for the page you are on." },
    { id: "review-keys", on: /^\/dsa\/review/, keys: ["J", "K"], text: "Move through the review queue. 1 to 4 grade the one open." },
];

const KEY = "tips.seen";

/** One small, dismissible hint at a time for a shortcut not yet used. Each is shown until it is dismissed, once, and then never again
 *  (the choice follows the owner between computers). Nothing is shown to an automated browser, so tests are never covered by one. */
export function Tips() {
    const path = useRouterState({ select: (s) => s.location.pathname });
    const [seen, setSeen] = useState<string[]>(() => getPref<string[]>(KEY, []));
    const [ready, setReady] = useState(false);
    useEffect(() => {
        // wait for the first screen and for the saved choices to arrive before showing anything
        const t = window.setTimeout(() => { setSeen(getPref<string[]>(KEY, [])); setReady(true); }, 2500);
        return () => window.clearTimeout(t);
    }, []);
    // using a shortcut counts as knowing it
    useEffect(() => {
        const down = (e: KeyboardEvent) => {
            const used = (e.key === "k" && (e.metaKey || e.ctrlKey)) ? "palette" : e.key === "?" ? "help" : null;
            if (used) dismiss(used);
        };
        window.addEventListener("keydown", down);
        return () => window.removeEventListener("keydown", down);
    });
    const dismiss = (id: string) => {
        const next = [...new Set([...getPref<string[]>(KEY, []), id])];
        setPref(KEY, next);
        setSeen(next);
    };
    if (!ready || navigator.webdriver || path === "/login") return null;
    const tip = TIPS.find((t) => !seen.includes(t.id) && (!t.on || t.on.test(path)));
    if (!tip) return null;
    return (
        <div className="hint-tip" role="note" aria-label="Tip">
            <span className="hint-tip-k">{tip.keys.map((k) => <kbd key={k}>{k}</kbd>)}</span>
            <span className="hint-tip-t">{tip.text}</span>
            <button className="hint-tip-x" onClick={() => dismiss(tip.id)}>Got it</button>
        </div>
    );
}
