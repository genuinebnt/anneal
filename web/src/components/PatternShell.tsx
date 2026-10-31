import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api } from "../api";
import { counted, statusOf } from "../dsa";
import { Header } from "./Header";
import { Page3, RingTile, Tile } from "./layout";
import { ResetTrack } from "./ResetTrack";

export type PatternTab = "idea" | "learn" | "problems" | "practice";
const KEYS: PatternTab[] = ["learn", "problems", "practice"];

/** The frame every pattern page shares (docs/DSA.md, decision 31): the title with the pattern at a glance, then the three panels. The centre
 *  starts with the strip of all patterns and the tabs (Idea, Learn, Problems, Practice) with Next, Reset and the pattern pager beside them.
 *  Changing pattern keeps the tab; I and 1 2 3 switch tabs, [ and ] step through the patterns, Esc goes home. A tab passes its own side panels. */
export function PatternShell({ code, tab, left, right, mini, children }: { code: string; tab: PatternTab; left?: ReactNode; right?: ReactNode; mini?: boolean; children: ReactNode }) {
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
    const dueHere = pattern && o ? o.problems.filter((p) => p.pattern === pattern.code && statusOf(p, o.today) === "due").length : 0;
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

    const tiles = pattern && (
        <>
            <RingTile p={total ? (done / total) * 100 : 0} center={`${done}/${total}`} em="In the lists" strong={total > 0 && done === total ? "Pattern done" : `${total - done} to go`} small="NeetCode 150" title="Solved of the NeetCode 150 problems in this pattern" />
            <Tile em="Techniques" strong={techniques} small="on the Learn page" />
            <Tile em="Practice" strong={<>{pattern.practice_solved}<span>/{pattern.practice_total}</span></>} small={pattern.practice_solved === 0 ? "not started" : "extra problems"} />
            <Tile em="Reviews due" strong={dueHere} hot={dueHere > 0} color="var(--vio)" to={{ to: "/dsa/review" }} small={dueHere > 0 ? <><span className="ly-dotp" aria-hidden="true" />Review now →</> : "nothing due here"} />
        </>
    );

    const center = (
        <>
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
            <div className="ps-bar">
                <div className="ps-tabs" role="tablist">
                    {tabs.map(([k, label, n, enabled]) =>
                        !enabled ? (
                            <span key={k} role="tab" aria-disabled="true" className="off">{label}<small>{n}</small></span>
                        ) : (
                            <Link key={k} role="tab" aria-selected={k === tab} className={k === tab ? "on" : ""} {...linkTo(pattern?.code ?? code, k)}>
                                {label}{n && <small>{n}</small>}<kbd>{k === "idea" ? "I" : KEYS.indexOf(k) + 1}</kbd>
                            </Link>
                        ),
                    )}
                </div>
                <div className="ps-acts">
                    {nextProblem && (
                        <Link className="ps-next" to="/d/$slug" params={{ slug: nextProblem.slug }} title={`Next unsolved problem in ${pattern?.name}`}>
                            <em>Next</em> {nextProblem.title}
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 8h10M9 4l4 4-4 4" /></svg>
                        </Link>
                    )}
                    {pattern && (
                        <button className="ps-reset" aria-expanded={resetOpen} onClick={() => setResetOpen(!resetOpen)} title="Forget your progress on this pattern">
                            Reset progress
                        </button>
                    )}
                    <span className="hintkeys">
                        {prev && <Link to={linkTo(prev.code, tab).to} params={linkTo(prev.code, tab).params} aria-label="Previous pattern" title="Previous pattern ([)">‹</Link>}
                        <em><kbd>[</kbd> <kbd>]</kbd></em>
                        {next && <Link to={linkTo(next.code, tab).to} params={linkTo(next.code, tab).params} aria-label="Next pattern" title="Next pattern (])">›</Link>}
                    </span>
                </div>
            </div>
            {pattern && resetOpen && <ResetTrack slug={pattern.slug} name={pattern.name} onDone={() => setResetOpen(false)} />}
            {o && !pattern ? (
                <p className="notice bad">
                    There is no pattern called "{code}". <Link to="/dsa">Back to the problems</Link>
                </p>
            ) : (
                children
            )}
        </>
    );

    const name = pattern?.name ?? code;
    return (
        <>
            <Header area="dsa" />
            <div className={mini ? "ps-mini" : undefined} style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <Page3
                    crumb={[<Link to="/dsa">DSA</Link>, <Link to="/dsa">Patterns</Link>, name]}
                    title={pattern?.name ?? "Patterns"}
                    tiles={tiles}
                    left={left}
                    right={right}
                    center={center}
                    label={name}
                />
            </div>
        </>
    );
}
