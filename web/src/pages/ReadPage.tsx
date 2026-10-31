import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { api, type DsaOverview, type DsaProblem, type Grade } from "../api";
import { Header } from "../components/Header";
import { IdeaDemo } from "../components/IdeaDemo";
import { HowFound, Page3, PageLoading, Panel, Tile } from "../components/layout";
import { Md, PyCode } from "../components/dsaBits";
import { DIFF, boxExamples } from "../dsa";
import { Select } from "../components/ui";

/** How the idea came back: these map onto FSRS's ratings on the reading schedule, apart from the coding reviews. */
const MARKS: [Grade, string, string, string][] = [
    ["again", "Forgot", "1", "var(--bad)"],
    ["hard", "Fuzzy", "2", "var(--warn)"],
    ["good", "Remembered", "3", "var(--grn)"],
];

/** One card: the problem first, so you try to recall the move; then the idea, the approach, and the pattern's drawing. */
function ReadCard({ p, o, onGraded, due }: { p: DsaProblem; o: DsaOverview; onGraded: () => void; due: boolean }) {
    const [shown, setShown] = useState(false);
    const statement = useQuery({ queryKey: ["dsa-statement", p.id], queryFn: () => api.statement(p.id), staleTime: Infinity, retry: false });
    const page = useQuery({ queryKey: ["dsa-page", p.id], queryFn: () => api.dsaPage(p.id), enabled: shown && p.has_page, staleTime: Infinity });
    const pattern = o.patterns.find((x) => x.code === p.pattern);
    const lessons = useQuery({ queryKey: ["pattern-lessons", p.pattern], queryFn: () => api.patternLessons(p.pattern), enabled: shown, staleTime: Infinity });
    const technique = o.techniques.find((t) => t.id === p.technique);
    const lesson = lessons.data?.techniques.find((t) => t.id === p.technique)?.lesson;
    const grade = useMutation({ mutationFn: (g: Grade) => api.readGrade(p.id, g), onSuccess: onGraded });
    useEffect(() => {
        const k = (e: KeyboardEvent) => {
            if ((e.target as HTMLElement | null)?.closest?.("input,textarea")) return;
            if (e.key === " " && !shown) {
                e.preventDefault();
                setShown(true);
            } else if (shown && ["1", "2", "3"].includes(e.key) && !grade.isPending) grade.mutate(MARKS[Number(e.key) - 1]![0]);
        };
        addEventListener("keydown", k);
        return () => removeEventListener("keydown", k);
    }, [shown, grade]);
    const approach = page.data?.approaches[page.data.approaches.length - 1];
    return (
        <>
            <section className="ly-pn rd-card">
                <div className="rd-top">
                    <span className="rd-k">{due ? "DUE AGAIN" : "NEW TO READ"} · {pattern?.name.toUpperCase()}</span>
                    <span className="rd-diff" style={{ color: DIFF[p.difficulty][1] }}>{DIFF[p.difficulty][0]}</span>
                </div>
                <h2>{p.title}</h2>
                <div className={`rd-stmt${shown ? " folded" : ""}`}>
                    {statement.data?.html ? <div className="d-statement" dangerouslySetInnerHTML={{ __html: boxExamples(statement.data.html) }} /> : <p className="rd-dim">{statement.isError ? "The statement could not be loaded; recall from the title." : "Loading the statement…"}</p>}
                </div>
                {!shown ? (
                    <div className="rd-recall">
                        <p>Before you reveal it: what is the move? Which pattern, what you keep track of, and the cost.</p>
                        <button className="rd-go" onClick={() => setShown(true)}>Reveal the idea <kbd>Space</kbd></button>
                    </div>
                ) : (
                    <div className="rd-idea">
                        <div className="rd-ins"><span>KEY INSIGHT</span><p>{p.insight}</p></div>
                        {technique && <p className="rd-tech">Technique: <b>{technique.name}</b>{lesson?.signals?.length ? <> · spot it when {lesson.signals[0]!.replace(/\.$/, "").toLowerCase()}</> : null}</p>}
                        {page.data?.intuition && <div className="rd-sec"><span>THE IDEA</span><Md text={page.data.intuition} /></div>}
                        {approach ? (
                            <div className="rd-sec">
                                <span>{approach.name.toUpperCase()} · {approach.time} TIME · {approach.space} SPACE</span>
                                <PyCode code={approach.code} />
                            </div>
                        ) : lesson?.template ? (
                            <div className="rd-sec"><span>THE TEMPLATE FOR THIS TECHNIQUE</span><PyCode code={lesson.template} /></div>
                        ) : null}
                        {lessons.data?.idea && lessons.data.idea.demos.length > 0 && (
                            <div className="rd-sec"><span>SEE IT MOVE · {pattern?.name.toUpperCase()}</span><IdeaDemo names={lessons.data.idea.demos.slice(0, 1)} /></div>
                        )}
                    </div>
                )}
            </section>
            {shown && (
                <div className="rd-marks" role="group" aria-label="How did the idea come back">
                    {MARKS.map(([g, label, key, c]) => (
                        <button key={g} style={{ "--c": c } as CSSProperties} disabled={grade.isPending} onClick={() => grade.mutate(g)}>
                            <b>{label}</b><kbd>{key}</kbd>
                        </button>
                    ))}
                    <Link className="rd-open" to="/d/$slug" params={{ slug: p.slug }}>Open the problem page</Link>
                </div>
            )}
        </>
    );
}

/** Reading to remember: problems to read on any device, the idea revealed after you try to recall it, on a schedule of its own. */
export function ReadPage() {
    const qc = useQueryClient();
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const [scope, setScope] = useState<"solved" | "all">("solved");
    const [pattern, setPattern] = useState<string>("");
    const q = useQuery({ queryKey: ["read", scope, pattern], queryFn: () => api.readQueue(scope, pattern || undefined) });
    const [done, setDone] = useState(0);
    const o = dsa.data;
    const v = q.data;
    const byId = useMemo(() => new Map((o?.problems ?? []).map((p) => [p.id, p])), [o]);
    if (!o || !v) return <><Header area="dsa" /><div style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}><PageLoading crumb={["DSA", "READ"]} title="Read" /></div></>;
    const queue = [...v.due.map((d) => ({ id: d.id, due: true })), ...v.new.map((id) => ({ id, due: false }))].filter((x) => byId.has(x.id));
    const cur = queue[0];
    const p = cur ? byId.get(cur.id) : undefined;
    const graded = () => {
        setDone((n) => n + 1);
        void qc.invalidateQueries({ queryKey: ["read"] });
    };
    const tiles = (
        <>
            <Tile em="Due again" strong={v.due.length} hot={v.due.length > 0} color="var(--vio)" small={`${v.week} more this week`} />
            <Tile em="Read" strong={`${v.read}`} small={`of ${v.in_scope} in scope`} />
            <Tile em="This visit" strong={done} small="read and marked" />
        </>
    );
    const left = (
        <>
            <Panel title="What to read">
                <div className="ui-seg rd-scope" role="radiogroup" aria-label="Scope">
                    {(["solved", "all"] as const).map((k) => <button key={k} role="radio" aria-checked={scope === k} className={scope === k ? "on" : ""} onClick={() => setScope(k)}>{k === "solved" ? "What I solved" : "Everything"}</button>)}
                </div>
                <div className="rd-pat">
                    <span>Pattern</span>
                    <Select label="Pattern" value={pattern} onChange={setPattern} options={[["", "All patterns"] as const, ...o.patterns.map((x) => [x.code, x.name] as const)]} />
                </div>
            </Panel>
            <Panel title={`Up next · ${queue.length}`}>
                <div className="sd-list">
                    {queue.slice(1, 9).map((x) => <span key={x.id} className="sd-att">{byId.get(x.id)?.title}{x.due ? " · due" : ""}</span>)}
                    {queue.length <= 1 && <p className="rd-dim">Nothing else in this scope.</p>}
                </div>
            </Panel>
        </>
    );
    const center = p && cur ? (
        <ReadCard key={p.id} p={p} o={o} due={cur.due} onGraded={graded} />
    ) : (
        <section className="ly-pn rd-card">
            <span className="rd-k">ALL READ</span>
            <h2>{done ? `${done} read. Nothing else is due in this scope.` : "Nothing to read in this scope."}</h2>
            <p className="rd-dim">{scope === "solved" ? "Read everything, not only what you solved, to study ahead." : "Pick another pattern, or come back when cards are due."}</p>
            {scope === "solved" && <button className="rd-go" onClick={() => setScope("all")}>Read everything</button>}
        </section>
    );
    const right = (
        <HowFound
            title="How reading works"
            text="Made for a phone or a spare minute: read the problem, try to recall the move, then reveal the key insight, the approach and the pattern moving. Mark how it came back; reading has its own schedule, so it never changes when your coding reviews are due."
            rows={[["Remembered", "back in days to weeks"], ["Fuzzy", "back sooner"], ["Forgot", "back tomorrow"]]}
        />
    );
    return (
        <>
            <Header area="dsa" />
            <div style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <Page3 crumb={["DSA", "READ"]} title={<>Read, <em className="rd-acc">remember.</em></>} sub="The problem, your recall, then the idea: a minute per card, on any device." tiles={tiles} left={left} center={center} right={right} label="What to read" />
            </div>
        </>
    );
}
