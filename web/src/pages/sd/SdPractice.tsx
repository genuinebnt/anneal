import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, type SdAttempt, type SdDiagram, type SdMark, type SdProblem } from "../../api";
import { HowFound, Page3, Panel, Tile, PageLoading } from "../../components/layout";
import { SdMd } from "../../components/SdMd";
import { mmss } from "../../focusStore";
import { SdFrame } from "./SdHome";

type Phase = "brief" | "drill" | "compare";
type Answers = Record<string, { answer: string; seconds: number }>;
const DRAFT = (id: string) => `anneal-sd-draft:${id}`;
const MARKS: [SdMark, string][] = [["got", "Got it"], ["part", "Partly"], ["miss", "Missed"]];
const LEVEL: Record<string, string> = { mid: "mid", senior: "senior", staff: "staff" };

/** A saved draft of a drill in this browser, so a reload does not lose an answer in progress. */
function readDraft(id: string): { answers: Answers; diagram: SdDiagram; started: string; hints: number } | null {
    try {
        const raw = localStorage.getItem(DRAFT(id));
        return raw ? JSON.parse(raw) : null;
    } catch {
        return null;
    }
}

function writeDraft(id: string, v: unknown) {
    try {
        if (v === null) localStorage.removeItem(DRAFT(id));
        else localStorage.setItem(DRAFT(id), JSON.stringify(v));
    } catch {
        // storage can be refused; the drill still works for this visit
    }
}

/** Boxes and arrows: add a box, a store or a queue, drag them, and join two by clicking one then the other. */
function Diagram({ value, onChange, readOnly = false }: { value: SdDiagram; onChange?: (d: SdDiagram) => void; readOnly?: boolean }) {
    const nodes = value.nodes ?? [];
    const edges = value.edges ?? [];
    const [sel, setSel] = useState<string | null>(null);
    const [linking, setLinking] = useState(false);
    const [label, setLabel] = useState("");
    const drag = useRef<{ id: string; dx: number; dy: number } | null>(null);
    const svg = useRef<SVGSVGElement>(null);
    const at = (e: React.PointerEvent) => {
        const r = svg.current!.getBoundingClientRect();
        return { x: ((e.clientX - r.left) / r.width) * 720, y: ((e.clientY - r.top) / r.height) * 300 };
    };
    const add = (kind: "box" | "store" | "queue") => {
        const name = label.trim() || (kind === "store" ? "Database" : kind === "queue" ? "Queue" : "Service");
        const id = `n${Date.now().toString(36)}`;
        onChange?.({ nodes: [...nodes, { id, x: 60 + ((nodes.length * 130) % 600), y: 40 + Math.floor((nodes.length * 130) / 600) * 90, label: name.slice(0, 18), kind }], edges });
        setLabel("");
    };
    const remove = () => {
        if (!sel) return;
        onChange?.({ nodes: nodes.filter((n) => n.id !== sel), edges: edges.filter((e) => e.from !== sel && e.to !== sel) });
        setSel(null);
    };
    const click = (id: string) => {
        if (readOnly) return;
        if (linking && sel && sel !== id) {
            if (!edges.some((e) => e.from === sel && e.to === id)) onChange?.({ nodes, edges: [...edges, { from: sel, to: id }] });
            setLinking(false);
        }
        setSel(id);
    };
    const pos = (id: string) => nodes.find((n) => n.id === id);
    return (
        <div className="sd-canvas">
            {!readOnly && (
                <div className="sd-tool">
                    <input value={label} onChange={(e) => setLabel(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add("box")} placeholder="Label, e.g. Redirect service" aria-label="Label for the next box" maxLength={18} />
                    <button onClick={() => add("box")}>+ Service</button>
                    <button onClick={() => add("store")}>+ Store</button>
                    <button onClick={() => add("queue")}>+ Queue</button>
                    <button className={linking ? "on" : ""} disabled={!sel} onClick={() => setLinking(!linking)}>{linking ? "Click the target…" : "Arrow from selected"}</button>
                    <button disabled={!sel} onClick={remove}>Delete</button>
                </div>
            )}
            <svg
                ref={svg}
                viewBox="0 0 720 300"
                className="sd-svg"
                onPointerMove={(e) => {
                    if (!drag.current || readOnly) return;
                    const p = at(e);
                    onChange?.({ nodes: nodes.map((n) => (n.id === drag.current!.id ? { ...n, x: Math.max(0, Math.min(600, p.x - drag.current!.dx)), y: Math.max(0, Math.min(256, p.y - drag.current!.dy)) } : n)), edges });
                }}
                onPointerUp={() => (drag.current = null)}
                onPointerLeave={() => (drag.current = null)}
            >
                <defs><marker id="sd-ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 10 5 0 10z" fill="currentColor" /></marker></defs>
                {edges.map((e, i) => { const a = pos(e.from), b = pos(e.to); return a && b ? <path key={i} className="sd-edge" d={`M${a.x + 60} ${a.y + 22} L${b.x + 60} ${b.y + 22}`} markerEnd="url(#sd-ar)" /> : null; })}
                {nodes.map((n) => (
                    <g
                        key={n.id}
                        className={`sd-node ${n.kind}${sel === n.id ? " sel" : ""}`}
                        onPointerDown={(e) => {
                            if (readOnly) return;
                            const p = at(e);
                            drag.current = { id: n.id, dx: p.x - n.x, dy: p.y - n.y };
                            click(n.id);
                        }}
                    >
                        <rect x={n.x} y={n.y} width={120} height={44} rx={n.kind === "store" ? 22 : n.kind === "queue" ? 4 : 10} />
                        <text x={n.x + 60} y={n.y + 27}>{n.label}</text>
                    </g>
                ))}
                {nodes.length === 0 && <text className="sd-empty" x={360} y={150}>{readOnly ? "No diagram drawn." : "Add a service, a store or a queue to start."}</text>}
            </svg>
        </div>
    );
}

function Brief({ p, attempts, onStart }: { p: SdProblem; attempts: SdAttempt[]; onStart: () => void }) {
    return (
        <section className="ly-pn sd-art">
            <span className="sd-k">THE QUESTION</span>
            <h2>{p.title}</h2>
            <p className="sd-lede">“{p.prompt}”</p>
            <ol className="sd-steps">
                {p.steps.map((s) => <li key={s.name}><b>{s.name}</b><span>{s.minutes} min</span><small>{s.points.length} rubric point{s.points.length === 1 ? "" : "s"}</small></li>)}
            </ol>
            <div className="sd-row">
                <button className="sd-go" onClick={onStart}>Start the clock · {p.minutes} min</button>
                {attempts.length > 0 && <span className="sd-dim">{attempts.length} earlier attempt{attempts.length === 1 ? "" : "s"}; the rubric and references open after you finish</span>}
            </div>
        </section>
    );
}

/** A drill: the delivery framework one step at a time, each on its own clock, then Compare. */
export function SdPractice({ id }: { id: string }) {
    const qc = useQueryClient();
    const q = useQuery({ queryKey: ["sd-problem", id], queryFn: () => api.sdProblem(id) });
    const draft = useMemo(() => readDraft(id), [id]);
    const [phase, setPhase] = useState<Phase>(draft ? "drill" : "brief");
    const [step, setStep] = useState(0);
    const [answers, setAnswers] = useState<Answers>(draft?.answers ?? {});
    const [diagram, setDiagram] = useState<SdDiagram>(draft?.diagram ?? {});
    const [started, setStarted] = useState<string>(draft?.started ?? new Date().toISOString());
    const [hints, setHints] = useState(draft?.hints ?? 0);
    const [probe, setProbe] = useState(0);
    const [now, setNow] = useState(Date.now());
    const [attemptId, setAttemptId] = useState<number | null>(null);
    const [marks, setMarks] = useState<Record<string, SdMark>>({});
    const [saved, setSaved] = useState<{ score: number | null; cards: number } | null>(null);
    useEffect(() => {
        if (phase !== "drill") return;
        const t = window.setInterval(() => setNow(Date.now()), 1000);
        return () => window.clearInterval(t);
    }, [phase]);
    useEffect(() => {
        if (phase === "drill") writeDraft(id, { answers, diagram, started, hints });
    }, [id, phase, answers, diagram, started, hints]);
    const stepStart = useRef(Date.now());
    const finish = useMutation({
        mutationFn: (p: SdProblem) => {
            const seconds = Math.round((Date.now() - Date.parse(started)) / 1000);
            const steps: Answers = Object.fromEntries(p.steps.map((s) => [s.name, answers[s.name] ?? { answer: "", seconds: 0 }]));
            return api.sdAttempt({ problem_id: id, started_at: started, seconds, steps, diagram, hints });
        },
        onSuccess: (r) => {
            setAttemptId(r.id);
            writeDraft(id, null);
            setPhase("compare");
            void qc.invalidateQueries({ queryKey: ["sd"] });
            window.scrollTo(0, 0);
        },
    });
    const submit = useMutation({
        mutationFn: () => api.sdMark(attemptId!, marks),
        onSuccess: (r) => {
            setSaved(r);
            for (const k of [["sd"], ["sd-problem", id], ["sd-review"]]) void qc.invalidateQueries({ queryKey: k });
        },
    });

    const v = q.data;
    if (!v) return <SdFrame><PageLoading crumb={["SYSTEM DESIGN", "PRACTICE"]} title="Practice" error={q.isError ? <p className="notice bad">No problem called {id}.</p> : undefined} /></SdFrame>;
    const p = v.problem;
    const cur = p.steps[step]!;
    const elapsed = Math.round((now - Date.parse(started)) / 1000);
    const stepSecs = (answers[cur.name]?.seconds ?? 0) + (phase === "drill" ? Math.round((now - stepStart.current) / 1000) : 0);
    const isDesign = /high-level/i.test(cur.name);
    const isDeep = /deep/i.test(cur.name);
    const setAnswer = (text: string) => setAnswers((a) => ({ ...a, [cur.name]: { answer: text, seconds: a[cur.name]?.seconds ?? 0 } }));
    const go = (to: number) => {
        const spent = Math.round((Date.now() - stepStart.current) / 1000);
        setAnswers((a) => ({ ...a, [cur.name]: { answer: a[cur.name]?.answer ?? "", seconds: (a[cur.name]?.seconds ?? 0) + spent } }));
        stepStart.current = Date.now();
        setStep(to);
    };
    const points = p.steps.flatMap((s) => s.points);
    const markedAll = points.every((pt) => marks[pt.id]);

    const tiles =
        phase === "drill" ? (
            <>
                <Tile em="Step" strong={`${step + 1}/${p.steps.length}`} small={`${cur.name} · ${mmss(stepSecs)} of ${cur.minutes}:00`} hot color="var(--warn)" />
                <Tile em="Time" strong={mmss(elapsed)} small={`of ${p.minutes}:00${elapsed > p.minutes * 60 ? " · over" : ""}`} />
            </>
        ) : phase === "compare" ? (
            <>
                <Tile em="Marked" strong={`${Object.keys(marks).length}/${points.length}`} small="rubric points" hot={!markedAll} color="var(--warn)" />
                {saved?.score != null && <Tile em="Score" strong={`${Math.round(saved.score * 100)}%`} small={`${saved.cards} cards in Review`} color="var(--grn)" />}
            </>
        ) : (
            <Tile em="Time" strong={`${p.minutes} min`} small={`${points.length} rubric points`} />
        );

    const left =
        phase === "brief" ? undefined : (
            <Panel title="The framework">
                <ol className="sd-steps">
                    {p.steps.map((s, i) => (
                        <li key={s.name} className={phase === "drill" && i === step ? "cur" : (answers[s.name]?.answer ?? "").trim() ? "done" : ""}>
                            <button onClick={() => (phase === "drill" ? go(i) : document.getElementById(`sd-step-${i}`)?.scrollIntoView({ behavior: "smooth" }))}>
                                <b>{s.name}</b>
                                <span>{s.minutes} min</span>
                                <small>{answers[s.name]?.seconds ? mmss(answers[s.name]!.seconds) + " used" : phase === "compare" ? `${s.points.filter((pt) => marks[pt.id]).length}/${s.points.length} marked` : ""}</small>
                            </button>
                        </li>
                    ))}
                </ol>
                {phase === "drill" && <div className="sd-clock"><b>{mmss(elapsed)}</b><span>of {p.minutes}:00 · {elapsed > p.minutes * 60 ? "over time" : "on the clock"}</span></div>}
            </Panel>
        );

    const center =
        phase === "brief" ? (
            <Brief p={p} attempts={v.attempts} onStart={() => { setStarted(new Date().toISOString()); stepStart.current = Date.now(); setPhase("drill"); }} />
        ) : phase === "drill" ? (
            <>
                <section className="ly-pn sd-art">
                    <span className="sd-k">STEP {step + 1} OF {p.steps.length} · {cur.name.toUpperCase()}</span>
                    <h2>{p.title}</h2>
                    <p className="sd-lede">{cur.prompt}</p>
                    <textarea className={isDesign || /api/i.test(cur.name) ? "sd-code" : ""} value={answers[cur.name]?.answer ?? ""} onChange={(e) => setAnswer(e.target.value)} placeholder="Write it as you would say it…" rows={8} spellCheck={!isDesign} />
                    <div className="sd-row">
                        {step < p.steps.length - 1 ? <button className="sd-go" onClick={() => go(step + 1)}>Next: {p.steps[step + 1]!.name}</button> : <button className="sd-go" disabled={finish.isPending} onClick={() => { go(step); finish.mutate(p); }}>I'm done, compare</button>}
                        {step < p.steps.length - 1 && <button className="sd-ghost" disabled={finish.isPending} onClick={() => { go(step); finish.mutate(p); }}>Finish early and compare</button>}
                        {finish.isError && <span className="sd-bad">{(finish.error as Error).message}</span>}
                    </div>
                </section>
                {isDesign && (
                    <Panel title="High-level design" aside={<span className="sd-dim">saved with the attempt</span>}>
                        <Diagram value={diagram} onChange={setDiagram} />
                    </Panel>
                )}
                {isDeep && p.probes.length > 0 && (
                    <Panel title="The interviewer pushes" aside={<span className="sd-dim">one at a time</span>}>
                        {p.probes.slice(0, probe).map((pr) => <p key={pr.question} className="sd-probe">“{pr.question}”</p>)}
                        {probe < p.probes.length ? <button className="sd-ghost" onClick={() => setProbe(probe + 1)}>{probe === 0 ? "Ask me a follow-up" : "Next follow-up"}</button> : <span className="sd-dim">That is every follow-up; answer them in your deep dives.</span>}
                    </Panel>
                )}
            </>
        ) : (
            <>
                <section className="ly-pn sd-art">
                    <span className="sd-k">COMPARE · {mmss(Math.round((Date.now() - Date.parse(started)) / 1000))} USED OF {p.minutes}:00</span>
                    <h2>How your design measures up</h2>
                    <p className="sd-lede">Your answer and the reference, step by step. Mark each point honestly: a miss comes back in Review today, a partly in two days, a got it in a week.</p>
                </section>
                {p.steps.map((s, i) => (
                    <section key={s.name} id={`sd-step-${i}`} className="ly-pn sd-cmp">
                        <h4>{s.name}</h4>
                        <div className="sd-two">
                            <div><span className="sd-k">YOURS</span>{(answers[s.name]?.answer ?? "").trim() ? <pre className="sd-yours">{answers[s.name]!.answer}</pre> : <p className="sd-dim">Nothing written.</p>}</div>
                            <div><span className="sd-k sd-k-ref">REFERENCE</span><SdMd text={s.reference} /></div>
                        </div>
                        {/high-level/i.test(s.name) && (diagram.nodes?.length ?? 0) > 0 && <Diagram value={diagram} readOnly />}
                        <div className="sd-rub">
                            {s.points.map((pt) => (
                                <div key={pt.id}>
                                    <span>{pt.text}<em className={`sd-lvl ${pt.level}`}>{LEVEL[pt.level]}</em></span>
                                    <div className="sd-seg" role="group" aria-label={pt.text}>
                                        {MARKS.map(([m, l]) => <button key={m} data-m={m} className={marks[pt.id] === m ? "on" : ""} disabled={!!saved} onClick={() => setMarks((x) => ({ ...x, [pt.id]: m }))}>{l}</button>)}
                                    </div>
                                </div>
                            ))}
                        </div>
                    </section>
                ))}
                {p.probes.length > 0 && (
                    <section className="ly-pn sd-cmp">
                        <h4>The follow-ups, answered</h4>
                        {p.probes.map((pr) => <div key={pr.question} className="sd-ref"><b>{pr.question}</b><SdMd text={pr.answer} /></div>)}
                    </section>
                )}
                <div className="sd-row sd-submit">
                    <button className="sd-go" disabled={!markedAll || submit.isPending || !!saved} onClick={() => submit.mutate()}>{saved ? "Marked" : markedAll ? "Save my marks" : `Mark every point (${Object.keys(marks).length}/${points.length})`}</button>
                    {saved && <Link className="sd-ghost" to="/sd/review">Go to Review</Link>}
                    {saved && <Link className="sd-ghost" to="/sd/problems">Next problem</Link>}
                </div>
            </>
        );

    const right =
        phase === "drill" ? (
            <>
                <Panel title="Rubric"><p className="sd-dim">{points.length} points a strong answer covers, hidden until you finish, so you design first and compare after.</p></Panel>
                {p.hints.length > 0 && (
                    <Panel title="Hints" aside={<span className="sd-dim">{hints}/{p.hints.length} used</span>}>
                        {p.hints.slice(0, hints).map((h) => <p key={h} className="sd-hint">{h}</p>)}
                        {hints < p.hints.length && <button className="sd-ghost" onClick={() => setHints(hints + 1)}>Show a hint</button>}
                    </Panel>
                )}
                <HowFound title="How a drill works" text="Each step keeps its own time. Your answers are kept in this browser until you finish, then saved with the attempt; the rubric opens in Compare." rows={[["Steps", p.steps.length], ["Points", points.length]]} />
            </>
        ) : (
            <>
                {v.uses.length > 0 && (
                    <Panel title="Leans on">
                        <div className="sd-list">{v.uses.map((u) => <Link key={u.id} to="/sd/learn/$id" params={{ id: u.id }}>{u.title}</Link>)}</div>
                    </Panel>
                )}
                {v.attempts.length > 0 && (
                    <Panel title="Your attempts">
                        <div className="sd-list">
                            {v.attempts.slice(0, 6).map((a) => {
                                const got = Object.values(a.marks).reduce((n, m) => n + (m === "got" ? 1 : m === "part" ? 0.5 : 0), 0);
                                return <span key={a.id} className="sd-att">{new Date(a.finished_at).toLocaleDateString()} · {mmss(a.seconds)} · {Object.keys(a.marks).length ? `${Math.round((got / points.length) * 100)}%` : "not marked"}</span>;
                            })}
                        </div>
                    </Panel>
                )}
                <HowFound title="Where the rubric comes from" text="Written for anneal from the delivery framework: what a strong answer says at each step, each point tagged with the level expected to say it." rows={[["Companies", p.companies.join(", ") || "–"]]} />
            </>
        );

    return (
        <SdFrame>
            <Page3 crumb={[<Link to="/sd">SYSTEM DESIGN</Link>, <Link to="/sd/problems">PROBLEMS</Link>, p.kind.toUpperCase()]} title={p.title} sub={phase === "compare" ? "Mark each rubric point: got it, partly or missed" : `${p.kind === "hld" ? "High-level design" : p.kind.toUpperCase()} · ${p.minutes} minutes`} tiles={tiles} left={left} center={center} right={right} label="Framework" />
        </SdFrame>
    );
}
