import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api } from "../api";
import { counted } from "../dsa";
import { Header } from "./Header";
import { ResetTrack } from "./ResetTrack";

export type PatternTab = "idea" | "learn" | "problems" | "practice";
const KEYS: PatternTab[] = ["learn", "problems", "practice"];

/** The frame every pattern page shares (docs/DSA.md, decision 31): the strip of all patterns, then Learn, Problems and
 *  Practice as tabs. Changing pattern keeps the tab; 1 2 3 switch tabs, [ and ] step through the patterns, Esc goes home. */
export function PatternShell({ code, tab, children }: { code: string; tab: PatternTab; children: ReactNode }) {
    const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const navigate = useNavigate();
    const o = overview.data;
    const patterns = o?.patterns ?? [];
    const at = patterns.findIndex((p) => p.code.toLowerCase() === code.toLowerCase() || p.slug === code);
    const pattern = at >= 0 ? patterns[at] : undefined;
    const strip = useRef<HTMLDivElement>(null);
    const [resetOpen, setResetOpen] = useState(false);

    const go = (c: string, t: PatternTab) => {
        if (t === "idea") void navigate({ to: "/dsa/patterns/$code/idea", params: { code: c } });
        else if (t === "learn") void navigate({ to: "/dsa/patterns/$code", params: { code: c } });
        else if (t === "problems") void navigate({ to: "/dsa/patterns/$code/problems", params: { code: c } });
        else void navigate({ to: "/dsa/practice/$code", params: { code: c } });
    };
    const keys = useRef<(e: KeyboardEvent) => void>(() => {});
    keys.current = (e) => {
        if ((e.target as HTMLElement | null)?.closest?.("input,textarea,select,[contenteditable]") || e.metaKey || e.ctrlKey || e.altKey || !pattern) return;
        if (e.key === "i" && pattern.has_idea) {
            go(pattern.code, "idea");
        } else if (e.key === "1" || e.key === "2" || e.key === "3") {
            const t = KEYS[Number(e.key) - 1]!;
            if (t === "practice" && pattern.practice_total === 0) return;
            go(pattern.code, t);
        } else if ((e.key === "[" || e.key === "]") && patterns.length) {
            const next = patterns[(at + (e.key === "]" ? 1 : patterns.length - 1)) % patterns.length]!;
            go(next.code, tab);
        } else if (e.key === "Escape") void navigate({ to: "/dsa" });
    };
    useEffect(() => {
        const f = (e: KeyboardEvent) => keys.current(e);
        window.addEventListener("keydown", f);
        return () => window.removeEventListener("keydown", f);
    }, []);
    // Keep the current pattern in view in the strip.
    useEffect(() => {
        const el = strip.current?.querySelector<HTMLElement>(".pchip.on");
        if (el && strip.current) strip.current.scrollLeft = el.offsetLeft - strip.current.clientWidth / 2 + el.offsetWidth / 2;
    }, [at]);

    // Progress is counted on the NeetCode 150, the same as the cards on the DSA home.
    const prog = (c: string): [number, number] => {
        const ps = (o?.problems ?? []).filter((p) => p.pattern === c && p.lists.includes("neetcode150") && counted(o!, p));
        return [ps.filter((p) => p.state.solved).length, ps.length];
    };
    const [done, total] = pattern ? prog(pattern.code) : [0, 0];
    const techniques = pattern && o ? o.techniques.filter((t) => t.pattern === pattern.name).length + (o.lesson_extras?.[pattern.name] ?? 0) : 0;
    const tabs: [PatternTab, string, string, boolean][] = [
        ...(pattern?.has_idea ? ([["idea", "Idea", "", true]] as [PatternTab, string, string, boolean][]) : []),
        ["learn", "Learn", techniques ? `${techniques} technique${techniques === 1 ? "" : "s"}` : "", true],
        ["problems", "Problems", pattern ? `${done}/${total}` : "", true],
        ["practice", "Practice", pattern ? `${pattern.practice_solved}/${pattern.practice_total}` : "", !pattern || pattern.practice_total > 0],
    ];
    const linkTo = (c: string, t: PatternTab) => (t === "idea" ? { to: "/dsa/patterns/$code/idea" as const, params: { code: c } } : t === "learn" ? { to: "/dsa/patterns/$code" as const, params: { code: c } } : t === "problems" ? { to: "/dsa/patterns/$code/problems" as const, params: { code: c } } : { to: "/dsa/practice/$code" as const, params: { code: c } });
    // The first problem of this pattern that is not solved yet, in the list's order: where to go next.
    const nextProblem = pattern && o ? o.problems.filter((p) => p.pattern === pattern.code && p.lists.includes("neetcode150") && !p.state.solved).sort((a, b) => a.order - b.order)[0] : undefined;
    const prev = patterns.length ? patterns[(at + patterns.length - 1) % patterns.length] : undefined;
    const next = patterns.length ? patterns[(at + 1) % patterns.length] : undefined;

    return (
        <>
            <Header area="dsa" />
            <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <div className="wrap ps-wrap">
                    <div className="eyebrow">
                        <Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link>
                        <span>/</span>
                        <Link to="/dsa" style={{ color: "var(--ca)" }}>PATTERNS</Link>
                        <span>/</span>
                        <span style={{ color: "var(--ca)" }}>{(pattern?.name ?? code).toUpperCase()}</span>
                    </div>
                    <h1 className="h1 md">{pattern?.name ?? "Patterns"}</h1>
                    {patterns.length > 0 && (
                        <div className="ps-strip" ref={strip} role="navigation" aria-label="Patterns">
                            {prev && <Link className="arr" {...linkTo(prev.code, tab)} title="Previous pattern" aria-label="Previous pattern">‹</Link>}
                            {patterns.map((p, i) => (
                                <Link key={p.code} className={`pchip${i === at ? " on" : ""}`} {...linkTo(p.code, tab)} aria-current={i === at ? "page" : undefined}>
                                    <span className="ringm" style={{ "--p": `${(() => { const [d, t] = prog(p.code); return t ? Math.round((d / t) * 100) : 0; })()}%` } as CSSProperties}><span>{i + 1}</span></span>
                                    {p.name}
                                </Link>
                            ))}
                            {next && <Link className="arr" {...linkTo(next.code, tab)} title="Next pattern" aria-label="Next pattern">›</Link>}
                        </div>
                    )}
                    <div className="ps-tabs" role="tablist">
                        {tabs.map(([k, label, n, enabled]) =>
                            !enabled ? (
                                <span key={k} role="tab" aria-disabled="true" className="off">{label}<small>{n}</small></span>
                            ) : (
                                <Link key={k} role="tab" aria-selected={k === tab} className={k === tab ? "on" : ""} {...linkTo(pattern?.code ?? code, k)}>
                                    {label}<small>{n}</small><kbd>{k === "idea" ? "I" : KEYS.indexOf(k) + 1}</kbd>
                                </Link>
                            ),
                        )}
                        {pattern && (
                            <button className="ps-reset" aria-expanded={resetOpen} onClick={() => setResetOpen(!resetOpen)} title="Forget your progress on this pattern">
                                Reset progress
                            </button>
                        )}
                        {nextProblem && (
                            <Link className="ps-next" to="/d/$slug" params={{ slug: nextProblem.slug }} title={`Next unsolved problem in ${pattern?.name}`}>
                                Next: {nextProblem.title} ›
                            </Link>
                        )}
                        <span className="hintkeys"><kbd>[</kbd> <kbd>]</kbd> change pattern</span>
                    </div>
                    {pattern && resetOpen && <ResetTrack slug={pattern.slug} name={pattern.name} onDone={() => setResetOpen(false)} />}
                    {o && !pattern ? (
                        <p className="notice bad">
                            There is no pattern called "{code}". <Link to="/dsa">Back to the problems</Link>
                        </p>
                    ) : (
                        children
                    )}
                </div>
            </main>
        </>
    );
}
