import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api, type Grade } from "../api";
import { useLogger } from "./dsaBits";

const KEY = "anneal:leetcode.pending";
const AWAY_MS = 5000;
const OUTCOMES: { grade: Grade; label: string }[] = [
    { grade: "good", label: "On my own" },
    { grade: "hard", label: "With help" },
    { grade: "again", label: "Attempted" },
    { grade: "easy", label: "Instant" },
];

interface Pending {
    slug: string;
    at: number;
}

const read = (): Pending | null => {
    try {
        return JSON.parse(sessionStorage.getItem(KEY) ?? "null") as Pending | null;
    } catch {
        return null;
    }
};

/** After you open a problem on LeetCode (any "Solve on LeetCode" link) and come back to this tab, a slim bar asks how it went. It never logs by
 *  itself, never starts the timer, and "Not now" drops it. The problem is remembered for this tab only. */
export function WelcomeBack() {
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa, staleTime: 30_000 });
    const o = dsa.data;
    const { log, toast, busy } = useLogger(o?.today);
    const [pending, setPending] = useState<Pending | null>(null);

    useEffect(() => {
        const click = (e: MouseEvent) => {
            const a = (e.target as HTMLElement | null)?.closest?.("a");
            const m = a?.getAttribute("href")?.match(/leetcode\.com\/problems\/([^/?#]+)/);
            if (!m?.[1]) return;
            try {
                sessionStorage.setItem(KEY, JSON.stringify({ slug: m[1], at: Date.now() } satisfies Pending));
            } catch {
                // without storage the bar simply does not appear
            }
        };
        const back = () => {
            if (document.visibilityState !== "visible") return;
            const p = read();
            setPending(p && Date.now() - p.at >= AWAY_MS ? p : null);
        };
        document.addEventListener("click", click, true);
        document.addEventListener("visibilitychange", back);
        window.addEventListener("focus", back);
        return () => {
            document.removeEventListener("click", click, true);
            document.removeEventListener("visibilitychange", back);
            window.removeEventListener("focus", back);
        };
    }, []);

    const drop = () => {
        try {
            sessionStorage.removeItem(KEY);
        } catch {
            // nothing to do
        }
        setPending(null);
    };
    const problem = pending && o ? o.problems.find((p) => p.slug === pending.slug) : undefined;
    return (
        <>
            {problem && !problem.state.solved && (
                <div className="wb-bar" role="region" aria-label="Back from LeetCode">
                    <span>Done with <b>{problem.title}</b>?</span>
                    <span className="wb-btns">
                        {OUTCOMES.map((g) => (
                            <button key={g.grade} disabled={busy} onClick={() => { log(problem, g.grade); drop(); }}>{g.label}</button>
                        ))}
                        <button className="wb-no" onClick={drop}>Not now</button>
                    </span>
                </div>
            )}
            {toast}
        </>
    );
}
