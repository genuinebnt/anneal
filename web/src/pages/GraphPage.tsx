import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api, type DsaOverview } from "../api";
import { Header } from "../components/Header";
import { HowFound, ListCard, Page3, Panel, ReadingPath, Tile } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { counted, statusOf } from "../dsa";
import { buildGraph, cosine, fastRP, rng, sharedTags, shortestPath, type EdgeKind, type GNode, type Graph } from "../graph";

/** The Graph tab: every pattern, technique and problem of your goal list and the companies that ask most, drawn live. The layout settles in
 *  front of you, dragging a node reheats it, a selected node keeps its neighbours and the rest recede, labels never sit on one another, and
 *  the legend filters. Beside it: where to start, what looks like a problem, and the path between two things. */
export function GraphPage() {
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const o = dsa.data;
    return (
        <>
            <Header area="dsa" />
            <div className="gr-3" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                {o ? <Workpage o={o} /> : <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Graph"]} title="Graph" center={dsa.isError ? <p className="notice bad">Couldn't reach the API.</p> : <Skeleton lines={8} />} />}
            </div>
        </>
    );
}

const HUES = [150, 250, 40, 300, 200, 20, 120, 330, 80, 270, 180, 0, 100, 220, 60, 310, 160, 350];
const colOf = (i: number) => `oklch(0.74 0.11 ${HUES[i % HUES.length]})`;
type Mode = "start" | "like" | "path";
type Sim = "tags" | "graph" | "meaning";
const KIND_NAME = { pat: "pattern", tec: "technique", pro: "problem", co: "company" } as const;
const EDGE_NAME: [EdgeKind, string][] = [["uses", "uses (written)"], ["tag", "shares tags (inferred)"], ["asks", "asked by a company"]];

function Workpage({ o }: { o: DsaOverview }) {
    const navigate = useNavigate();
    const [scopeAll, setScopeAll] = useState(false);
    const scope = useMemo(() => o.problems.filter((p) => counted(o, p) && (scopeAll ? true : p.lists.includes(o.settings.goal.list))), [o, scopeAll]);
    const g = useMemo(() => buildGraph(o, scope), [o, scope]);
    const vectors = useMemo(() => fastRP(g), [g]);
    const byId = useMemo(() => new Map(o.problems.map((p) => [p.id, p] as const)), [o]);
    const [mode, setMode] = useState<Mode>("start");
    const [sim, setSim] = useState<Sim>("tags");
    const [selected, setSelected] = useState<number | null>(null);
    const [path, setPath] = useState<number[] | null>(null);
    const [picking, setPicking] = useState<number | "first" | null>(null);
    const [query, setQuery] = useState("");
    const [colourBy, setColourBy] = useState<"pattern" | "state">("pattern");
    const [hiddenPats, setHiddenPats] = useState<Set<number>>(() => new Set());
    const [hiddenEdges, setHiddenEdges] = useState<Set<EdgeKind>>(() => new Set());
    const canvas = useRef<HTMLCanvasElement>(null);
    const engine = useRef<Engine | null>(null);

    // the canvas engine lives outside React; React tells it what to show
    useEffect(() => {
        const cv = canvas.current;
        if (!cv) return;
        const e = new Engine(cv, g, (id) => {
            if (pickRef.current !== null) {
                if (id === null) return setPicking(null);
                if (pickRef.current === "first") return setPicking(id);
                if (id !== pickRef.current) {
                    setPath(shortestPath(g, pickRef.current, id));
                    setPicking(null);
                    setMode("path");
                }
                return;
            }
            setSelected(id);
        });
        engine.current = e;
        return () => e.stop();
    }, [g]);
    const pickRef = useRef<number | "first" | null>(null);
    pickRef.current = picking;
    useEffect(() => {
        engine.current?.set({ selected, path, query: query.trim().toLowerCase(), colourBy, hiddenPats, hiddenEdges });
    }, [selected, path, query, colourBy, hiddenPats, hiddenEdges]);
    // keys: L start, S looks like, P path, / find
    const find = useRef<HTMLInputElement>(null);
    useEffect(() => {
        const k = (e: KeyboardEvent) => {
            if ((e.target as HTMLElement | null)?.closest?.("input,textarea,select") || e.metaKey || e.ctrlKey || e.altKey) return;
            const m = ({ l: "start", s: "like", p: "path" } as const)[e.key.toLowerCase() as "l" | "s" | "p"];
            if (m) setMode(m);
            else if (e.key === "/") {
                e.preventDefault();
                find.current?.focus();
            } else if (e.key === "Escape") {
                setSelected(null);
                setPath(null);
                setPicking(null);
            }
        };
        addEventListener("keydown", k);
        return () => removeEventListener("keydown", k);
    }, []);

    const node = selected !== null ? g.nodes[selected] : undefined;
    const selProblem = node?.kind === "pro" ? byId.get(node.key.slice(4)) : undefined;

    // Where to start: techniques with problems left, ranked by facts the graph already holds
    const leads = useMemo(() => {
        const out: { node: GNode; score: number; why: string[] }[] = [];
        for (const n of g.nodes) {
            if (n.kind !== "tec") continue;
            const ps = scope.filter((p) => `tec:${p.technique}` === n.key);
            const left = ps.filter((p) => !p.state.solved).length;
            if (!left) continue;
            const due = ps.filter((p) => statusOf(p, o.today) === "due").length;
            const solvedNear = scope.filter((p) => p.pattern === o.patterns[n.pat]?.code && p.state.solved).length;
            const recent = ps.filter((p) => p.recent).length;
            const must = ps.filter((p) => p.priority === "must" && !p.state.solved).length;
            const score = left * 2 + due * 3 + must * 3 + recent + Math.min(solvedNear, 4);
            const why = [`${left} problem${left === 1 ? "" : "s"} left to solve`, ...(must ? [`${must} MUST in the list, not solved`] : []), ...(due ? [`${due} review${due === 1 ? "" : "s"} due here`] : []), ...(recent ? [`${recent} asked in the last six months`] : []), ...(solvedNear ? [`you solved ${solvedNear} in ${o.patterns[n.pat]?.name}`] : [])];
            out.push({ node: n, score, why });
        }
        return out.sort((a, b) => b.score - a.score).slice(0, 5);
    }, [g, scope, o]);
    const topScore = leads[0]?.score ?? 1;

    // Looks like: by shared tags over every problem, or by surroundings in the graph
    const alike = useMemo(() => {
        if (!node) return [];
        if (sim === "tags" && selProblem) {
            return o.problems
                .filter((q) => q.id !== selProblem.id && counted(o, q))
                .map((q) => ({ key: q.id, label: q.title, slug: q.slug, score: sharedTags(selProblem, q).length, note: sharedTags(selProblem, q).join(" · ") }))
                .filter((x) => x.score > 0)
                .sort((a, b) => b.score - a.score || (byId.get(a.key)?.order ?? 0) - (byId.get(b.key)?.order ?? 0))
                .slice(0, 6)
                .map((x) => ({ ...x, w: x.score / Math.max(1, selProblem.tags.length), noteTop: `${x.score} shared` }));
        }
        if (sim === "graph") {
            const v = vectors[node.id]!;
            return g.nodes
                .filter((m) => m.id !== node.id && m.kind === node.kind)
                .map((m) => ({ m, c: cosine(v, vectors[m.id]!) }))
                .sort((a, b) => b.c - a.c)
                .slice(0, 6)
                .map(({ m, c }) => ({ key: m.key, label: m.label, slug: m.slug, score: c, note: `${KIND_NAME[m.kind]} · ${o.patterns[m.pat]?.name ?? ""}`, w: Math.max(0, c), noteTop: `${c.toFixed(2)} alike` }));
        }
        return [];
    }, [node, sim, selProblem, o, byId, vectors, g]);

    const nodesN = g.nodes.length;
    const solved = scope.filter((p) => p.state.solved).length;
    const due = scope.filter((p) => statusOf(p, o.today) === "due").length;
    const togglePat = (i: number) => setHiddenPats((s) => { const n = new Set(s); if (!n.delete(i)) n.add(i); return n; });
    const toggleEdge = (k: EdgeKind) => setHiddenEdges((s) => { const n = new Set(s); if (!n.delete(k)) n.add(k); return n; });
    const open = (n: GNode) => {
        if (n.kind === "pro" && n.slug) void navigate({ to: "/d/$slug", params: { slug: n.slug } });
        else if (n.code) void navigate({ to: "/dsa/patterns/$code", params: { code: n.code }, hash: n.kind === "tec" ? `t-${n.key.split(":").pop()}` : undefined });
    };

    const tiles = (
        <>
            <Tile em="Nodes" strong={nodesN.toLocaleString()} small="patterns, techniques, problems, companies" />
            <Tile em="Links" strong={g.edges.length.toLocaleString()} small="uses · shares tags · asks" />
            {leads[0] && <Tile em="Where to start" strong={<span className="gr-tlead">{leads[0].node.label}</span>} hot color="var(--acc)" small={`score ${leads[0].score}`} onClick={() => { setSelected(leads[0]!.node.id); setMode("start"); }} />}
        </>
    );

    const left = (
        <>
            <div className="gr-modes" role="tablist" aria-label="Modes">
                {([["start", "Start", "L"], ["like", "Looks like", "S"], ["path", "Path", "P"]] as const).map(([k, l, key]) => (
                    <button key={k} role="tab" aria-selected={mode === k} className={mode === k ? "on" : ""} onClick={() => setMode(k)}>{l}<kbd>{key}</kbd></button>
                ))}
            </div>
            {mode === "start" && (
                <Panel title="Ranked from the graph">
                    <h3 className="gr-h">Where to start</h3>
                    <p>Ranked from what the graph already holds: problems left, MUST problems not solved, reviews due, recent asks and what you solved nearby. A lead says where to look; it is never a verdict.</p>
                    <div className="ly-cards">
                        {leads.map((l, i) => (
                            <ListCard key={l.node.key} i={i} title={l.node.label} note={String(l.score)} w={(l.score / topScore) * 100} color={colOf(l.node.pat)} on={selected === l.node.id} text={<span className="gr-why">{l.why.map((w) => <span key={w}>{w}</span>)}</span>} onClick={() => setSelected(l.node.id)} />
                        ))}
                        {leads.length === 0 && <p>Every problem in scope is solved.</p>}
                    </div>
                </Panel>
            )}
            {mode === "like" && (
                <Panel title="Starting from">
                    <h3 className="gr-h">{node ? node.label : "Pick a node"}</h3>
                    <div className="gr-sim" role="radiogroup" aria-label="Alike by">
                        {([["tags", "Shared tags"], ["graph", "Surroundings"], ["meaning", "Meaning"]] as const).map(([k, l]) => (
                            <button key={k} role="radio" aria-checked={sim === k} className={sim === k ? "on" : ""} onClick={() => setSim(k)}>{l}</button>
                        ))}
                    </div>
                    {!node && <p>Click a node in the graph, or a lead, to see what looks like it.</p>}
                    {node && sim === "tags" && !selProblem && <p>Shared tags compare problems. Pick a problem, or use Surroundings for a {KIND_NAME[node.kind]}.</p>}
                    {node && sim === "meaning" && <p className="gr-soon">Meaning vectors (a small model on the server, local only) are planned and not built yet. Shared tags and surroundings work now.</p>}
                    {alike.length > 0 && (
                        <div className="ly-cards">
                            {alike.map((a, i) => (
                                <ListCard key={a.key} i={i} title={a.label} note={a.noteTop} w={a.w * 100} text={a.note} to={a.slug ? { to: "/d/$slug", params: { slug: a.slug } } : undefined} onClick={a.slug ? undefined : () => { const m = g.nodes.find((x) => x.key === a.key); if (m) setSelected(m.id); }} />
                            ))}
                        </div>
                    )}
                    {node && sim !== "meaning" && (
                        <HowFound text={sim === "tags" ? "Problems whose LeetCode topic tags overlap most with this one. Exact: every problem is compared." : "Nodes whose place in the graph looks most like this one's: the same techniques, companies and neighbours. FastRP graph vectors, computed from the links alone, no language model."} rows={sim === "tags" ? [["Compared", `${o.problems.length.toLocaleString()} problems`], ["Ranked by", "shared tags"]] : [["Compared", `${nodesN} nodes`], ["Method", "FastRP, 64 dims"]]} />
                    )}
                </Panel>
            )}
            {mode === "path" && (
                <Panel title="How to get there">
                    <h3 className="gr-h">{path && path.length ? `${g.nodes[path[0]!]!.label} → ${g.nodes[path[path.length - 1]!]!.label}` : "Pick two nodes"}</h3>
                    <button className="ui-btn" onClick={() => { setPath(null); setPicking("first"); }}>{picking === "first" ? "Click the first node…" : typeof picking === "number" ? "Now click the second node…" : "Pick two nodes"}</button>
                    {path && path.length > 0 && (
                        <ReadingPath steps={path.map((id, i) => { const n = g.nodes[id]!; return { key: n.key, title: n.label, text: KIND_NAME[n.kind] + (n.state ? ` · ${{ sol: "solved", due: "due", att: "tried", new: "not started" }[n.state]}` : ""), state: i === path.length - 1 ? "cur" : n.state === "sol" ? "done" : "", onClick: () => setSelected(id) }; })} />
                    )}
                    {path && path.length === 0 && <p>No path joins those two.</p>}
                    <HowFound text="The fewest links between the two, through techniques, patterns, shared tags and companies." />
                </Panel>
            )}
        </>
    );

    const center = (
        <>
            <div className="gr-bar">
                <label className="cm-find gr-find">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" /></svg>
                    <input ref={find} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find a problem, technique, company…" aria-label="Find in the graph" />
                    <kbd>/</kbd>
                </label>
                <span className="ly-sp" />
                <div className="ui-seg" role="radiogroup" aria-label="Scope">
                    <button role="radio" aria-checked={!scopeAll} className={!scopeAll ? "on" : ""} onClick={() => setScopeAll(false)}>Goal list</button>
                    <button role="radio" aria-checked={scopeAll} className={scopeAll ? "on" : ""} onClick={() => setScopeAll(true)}>All</button>
                </div>
                <div className="ui-seg" role="radiogroup" aria-label="Colour by">
                    <button role="radio" aria-checked={colourBy === "pattern"} className={colourBy === "pattern" ? "on" : ""} onClick={() => setColourBy("pattern")}>Pattern</button>
                    <button role="radio" aria-checked={colourBy === "state"} className={colourBy === "state" ? "on" : ""} onClick={() => setColourBy("state")}>My progress</button>
                </div>
                <button className="ui-btn" onClick={() => engine.current?.exportPng()}>Export</button>
            </div>
            <div className="gr-wrap">
                <canvas ref={canvas} aria-label="The DSA graph" />
                <div className="gr-legend">
                    <div className="gr-lh">Patterns</div>
                    {o.patterns.map((p, i) => (
                        <button key={p.code} className="gr-li" aria-pressed={!hiddenPats.has(i)} onClick={() => togglePat(i)}>
                            <span className="gr-sw" style={{ background: colOf(i) }} />{p.name}
                        </button>
                    ))}
                    <div className="gr-lh">Links</div>
                    {EDGE_NAME.map(([k, l]) => (
                        <button key={k} className="gr-li" aria-pressed={!hiddenEdges.has(k)} onClick={() => toggleEdge(k)}>
                            <span className={`gr-esw is-${k}`} />{l}<em>{g.edges.filter((e) => e[2] === k).length}</em>
                        </button>
                    ))}
                    {picking !== null && <p className="gr-hint">{picking === "first" ? "Click the first node" : "Now click the second node"}</p>}
                </div>
                <div className="gr-stat"><b>{nodesN}</b> nodes · <b>{g.edges.length}</b> links</div>
                <div className="gr-ctl">
                    <button aria-label="Zoom in" onClick={() => engine.current?.zoom(1.2)}>+</button>
                    <button aria-label="Zoom out" onClick={() => engine.current?.zoom(1 / 1.2)}>−</button>
                    <i />
                    <button onClick={() => engine.current?.reheat(0.6)}>Arrange</button>
                    <button onClick={() => engine.current?.shake()}>Shake</button>
                    <button onClick={() => engine.current?.fit()}>Fit view</button>
                </div>
            </div>
            <p className="gr-keys">click a node to inspect · drag to pan or move · scroll to zoom · <kbd>/</kbd> find · <kbd>L</kbd> start · <kbd>S</kbd> looks like · <kbd>P</kbd> path · <kbd>Esc</kbd> clear</p>
        </>
    );

    const nb = node ? [...g.adj.get(node.id)!].map((id) => g.nodes[id]!) : [];
    const mix = new Map<number, number>();
    nb.forEach((m) => m.pat >= 0 && mix.set(m.pat, (mix.get(m.pat) ?? 0) + 1));
    const right = (
        <>
            <Panel title="Selected" aside={node ? <button onClick={() => setSelected(null)}>clear</button> : undefined}>
                {!node ? (
                    <p>Click a node in the graph to see its links, the patterns around it and what to do next.</p>
                ) : (
                    <div className="gr-insp">
                        <span className="ly-lab">{KIND_NAME[node.kind]}</span>
                        <h3 className="gr-h">{node.label}</h3>
                        <div className="gr-ird">
                            <div><span>Links</span><b>{nb.length}</b></div>
                            {node.state && <div><span>State</span><b>{{ sol: "solved", due: "due", att: "tried", new: "new" }[node.state]}</b></div>}
                            {selProblem && <div><span>Tags</span><b>{selProblem.tags.length}</b></div>}
                        </div>
                        {mix.size > 0 && (
                            <>
                                <span className="ly-lab">Patterns around it</span>
                                <div className="gr-mix">{[...mix.entries()].map(([k, v]) => <i key={k} style={{ flex: v, background: colOf(k) }} title={`${o.patterns[k]?.name}: ${v}`} />)}</div>
                            </>
                        )}
                        <span className="ly-lab">Neighbours</span>
                        <ul className="gr-nb">
                            {nb.slice(0, 8).map((m) => (
                                <li key={m.key}><button onClick={() => { if (typeof picking === "number") { setPath(shortestPath(g, picking, m.id)); setPicking(null); } else setSelected(m.id); }}><i style={{ background: m.pat >= 0 ? colOf(m.pat) : "var(--fg)" }} />{m.label}<em>{KIND_NAME[m.kind]}</em></button></li>
                            ))}
                        </ul>
                        <div className="gr-acts">
                            {node.kind !== "co" && <button className="ui-btn" onClick={() => open(node)}>Open</button>}
                            <button className="ui-btn" onClick={() => { setMode("like"); setSim(node.kind === "pro" ? "tags" : "graph"); }}>Looks like</button>
                            <button className="ui-btn" onClick={() => { setPath(null); setPicking(node.id); setMode("path"); }}>Path from here</button>
                        </div>
                    </div>
                )}
            </Panel>
            <HowFound title="What the graph holds" text="Every pattern, the techniques and problems of your goal list (or every problem), and the companies that ask most of them, with how they connect. Nothing is stored for the graph: it is drawn from the catalog and your progress." rows={[["Nodes", nodesN], ["Links", g.edges.length], ["Solved", solved], ["Due", due]]} />
        </>
    );

    return <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Graph"]} title="Graph" sub="Everything anneal knows, and how it connects. Start where the graph points." tiles={tiles} left={left} center={center} right={right} label="Graph modes" />;
}

interface View {
    selected: number | null;
    path: number[] | null;
    query: string;
    colourBy: "pattern" | "state";
    hiddenPats: Set<number>;
    hiddenEdges: Set<EdgeKind>;
}

/** The canvas: a force layout that settles, drag and pan, zoom on the wheel, and labels placed most important first. */
class Engine {
    private ctx: CanvasRenderingContext2D;
    private W = 0;
    private H = 0;
    private dpr = 1;
    private view = { x: 0, y: 0, k: 1 };
    private alpha = 1;
    private running = false;
    private stopped = false;
    private drag: GNode | null = null;
    private pan: [number, number, number, number] | null = null;
    private moved = false;
    private hover: GNode | null = null;
    private v: View = { selected: null, path: null, query: "", colourBy: "pattern", hiddenPats: new Set(), hiddenEdges: new Set() };
    private deg: Map<number, number>;
    private ro: ResizeObserver;
    private started = false;
    private rand = rng(11);
    private cleanup: (() => void)[] = [];

    constructor(private cv: HTMLCanvasElement, private g: Graph, private onPick: (id: number | null) => void) {
        this.ctx = cv.getContext("2d")!;
        this.deg = new Map(g.nodes.map((n) => [n.id, g.adj.get(n.id)!.size]));
        this.ro = new ResizeObserver(() => {
            if (!this.started) this.start();
            else if (this.size()) this.draw();
        });
        this.ro.observe(cv.parentElement!);
        const on = <K extends keyof HTMLElementEventMap>(t: K, f: (e: HTMLElementEventMap[K]) => void, o?: AddEventListenerOptions) => {
            cv.addEventListener(t, f as EventListener, o);
            this.cleanup.push(() => cv.removeEventListener(t, f as EventListener));
        };
        on("pointerdown", (e) => this.down(e));
        on("pointermove", (e) => this.move(e));
        on("pointerup", (e) => this.up(e));
        on("pointerleave", () => { this.hover = null; this.draw(); });
        on("wheel", (e) => this.wheel(e), { passive: false });
    }

    stop() {
        this.stopped = true;
        this.ro.disconnect();
        this.cleanup.forEach((f) => f());
    }

    set(v: View) {
        this.v = v;
        this.draw();
    }

    private css(n: string) {
        return getComputedStyle(document.documentElement).getPropertyValue(n).trim() || "#888";
    }

    private size() {
        const r = this.cv.parentElement!.getBoundingClientRect();
        if (r.width < 200 || r.height < 200) return false;
        this.dpr = window.devicePixelRatio || 1;
        this.W = r.width;
        this.H = r.height;
        this.cv.width = this.W * this.dpr;
        this.cv.height = this.H * this.dpr;
        this.cv.style.width = `${this.W}px`;
        this.cv.style.height = `${this.H}px`;
        return true;
    }

    private start() {
        if (this.started || !this.size()) return;
        this.started = true;
        const pats = Math.max(1, Math.max(...this.g.nodes.map((n) => n.pat)) + 1);
        for (const n of this.g.nodes) {
            const p = n.pat >= 0 ? n.pat : Math.floor(this.rand() * pats);
            const a = (p / pats) * Math.PI * 2 - Math.PI / 2;
            const R = Math.min(this.W, this.H) * 0.36 * (n.kind === "co" ? 0.3 : 1);
            const j = n.kind === "pat" || n.kind === "co" ? 0 : n.kind === "tec" ? 50 : 90;
            n.x = this.W / 2 + Math.cos(a) * R + (this.rand() - 0.5) * j;
            n.y = this.H / 2 + Math.sin(a) * R + (this.rand() - 0.5) * j;
        }
        this.alpha = 1;
        this.loop();
        window.setTimeout(() => !this.stopped && this.fit(), 2600);
    }

    private tick() {
        const N = this.g.nodes;
        const rep = Math.min(this.W, this.H) * 1.6;
        for (let i = 0; i < N.length; i++) {
            for (let j = i + 1; j < N.length; j++) {
                const a = N[i]!, b = N[j]!;
                const dx = a.x - b.x || 0.1, dy = a.y - b.y || 0.1;
                const d2 = Math.max(dx * dx + dy * dy, 144);
                if (d2 > 90000) continue;
                const d = Math.sqrt(d2);
                const f = (rep * (a.r + b.r)) / 14 / d2;
                a.vx += (dx / d) * f; a.vy += (dy / d) * f; b.vx -= (dx / d) * f; b.vy -= (dy / d) * f;
            }
        }
        for (const [u, w, k] of this.g.edges) {
            if (this.v.hiddenEdges.has(k)) continue;
            const a = N[u]!, b = N[w]!;
            const dx = b.x - a.x, dy = b.y - a.y, d = Math.hypot(dx, dy) || 0.01;
            const L = k === "tag" ? 170 : k === "asks" ? 220 : a.kind === "pat" ? 62 : 34;
            const s = k === "tag" ? 0.004 : k === "asks" ? 0.003 : 0.03;
            const f = (d - L) * s;
            a.vx += (dx / d) * f; a.vy += (dy / d) * f; b.vx -= (dx / d) * f; b.vy -= (dy / d) * f;
        }
        for (const n of N) {
            n.vx += (this.W / 2 - n.x) * 0.0016;
            n.vy += (this.H / 2 - n.y) * 0.0016;
            if (n === this.drag) { n.vx = n.vy = 0; continue; }
            n.vx = Math.max(-14, Math.min(14, n.vx * 0.86));
            n.vy = Math.max(-14, Math.min(14, n.vy * 0.86));
            n.x += n.vx * this.alpha;
            n.y += n.vy * this.alpha;
        }
        this.alpha *= 0.985;
    }

    private loop = () => {
        if (this.stopped) return;
        this.running = true;
        if (this.alpha > 0.02 || this.drag) {
            this.tick();
            this.draw();
            requestAnimationFrame(this.loop);
        } else {
            this.draw();
            this.running = false;
        }
    };

    reheat(a: number) {
        this.alpha = Math.max(this.alpha, a);
        if (!this.running) this.loop();
    }

    shake() {
        for (const n of this.g.nodes) { n.vx += (this.rand() - 0.5) * 40; n.vy += (this.rand() - 0.5) * 40; }
        this.reheat(0.9);
    }

    zoom(k: number) {
        this.view.x = this.W / 2 - (this.W / 2 - this.view.x) * k;
        this.view.y = this.H / 2 - (this.H / 2 - this.view.y) * k;
        this.view.k *= k;
        this.draw();
    }

    fit() {
        const N = this.g.nodes;
        if (!N.length || !this.W) return;
        const xs = N.map((n) => n.x), ys = N.map((n) => n.y);
        const x0 = Math.min(...xs) - 40, x1 = Math.max(...xs) + 40, y0 = Math.min(...ys) - 50, y1 = Math.max(...ys) + 30;
        const k = Math.min(this.W / (x1 - x0), this.H / (y1 - y0), 1.6);
        this.view = { k, x: (this.W - (x0 + x1) * k) / 2, y: (this.H - (y0 + y1) * k) / 2 };
        this.draw();
    }

    exportPng() {
        this.cv.toBlob((b) => {
            if (!b) return;
            const a = document.createElement("a");
            a.href = URL.createObjectURL(b);
            a.download = "anneal-graph.png";
            a.click();
            window.setTimeout(() => URL.revokeObjectURL(a.href), 1000);
        });
    }

    private dimmed(n: GNode) {
        const v = this.v;
        if (v.path && v.path.length) return !v.path.includes(n.id);
        if (n.pat >= 0 && v.hiddenPats.has(n.pat)) return true;
        if (v.query && !n.label.toLowerCase().includes(v.query)) return true;
        if (v.selected !== null && v.selected !== n.id && !this.g.adj.get(v.selected)!.has(n.id)) return true;
        return false;
    }

    private fill(n: GNode) {
        if (n.kind === "co") return this.css("--fg");
        if (this.v.colourBy === "state" && n.kind === "pro") return ({ sol: this.css("--grn"), due: this.css("--vio"), att: this.css("--warn"), new: this.css("--line") })[n.state ?? "new"];
        return colOf(n.pat);
    }

    draw() {
        if (!this.W || this.stopped) return;
        const { ctx, view, g, v } = this;
        ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
        ctx.clearRect(0, 0, this.W, this.H);
        ctx.translate(view.x, view.y);
        ctx.scale(view.k, view.k);
        const line = this.css("--line"), ink = this.css("--fg"), acc = this.css("--acc"), mut = this.css("--mut"), bg = this.css("--bg");
        for (const [u, w, k] of g.edges) {
            if (v.hiddenEdges.has(k)) continue;
            const a = g.nodes[u]!, b = g.nodes[w]!;
            const near = v.selected !== null && (u === v.selected || w === v.selected);
            const dim = this.dimmed(a) || this.dimmed(b) || (v.selected !== null && !near);
            ctx.globalAlpha = dim ? 0.07 : k === "tag" ? 0.32 : k === "asks" ? 0.18 : 0.5;
            ctx.strokeStyle = dim ? line : near ? acc : k === "asks" ? ink : mut;
            ctx.setLineDash(k === "tag" ? [3, 4] : []);
            ctx.lineWidth = (near ? 1.6 : 1) / view.k;
            ctx.beginPath();
            ctx.moveTo(a.x, a.y);
            ctx.lineTo(b.x, b.y);
            ctx.stroke();
        }
        ctx.setLineDash([]);
        if (v.path && v.path.length > 1) {
            ctx.globalAlpha = 1;
            ctx.strokeStyle = acc;
            ctx.lineWidth = 3 / view.k;
            for (let i = 0; i < v.path.length - 1; i++) {
                const a = g.nodes[v.path[i]!]!, b = g.nodes[v.path[i + 1]!]!;
                ctx.beginPath();
                ctx.moveTo(a.x, a.y);
                ctx.lineTo(b.x, b.y);
                ctx.stroke();
            }
        }
        for (const n of g.nodes) {
            const dim = this.dimmed(n);
            ctx.globalAlpha = dim ? 0.15 : 1;
            ctx.fillStyle = this.fill(n);
            ctx.beginPath();
            if (n.kind === "tec") {
                ctx.moveTo(n.x, n.y - n.r); ctx.lineTo(n.x + n.r, n.y); ctx.lineTo(n.x, n.y + n.r); ctx.lineTo(n.x - n.r, n.y); ctx.closePath();
            } else if (n.kind === "co") {
                ctx.moveTo(n.x, n.y - n.r); ctx.lineTo(n.x + n.r * 0.95, n.y + n.r * 0.7); ctx.lineTo(n.x - n.r * 0.95, n.y + n.r * 0.7); ctx.closePath();
            } else ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2);
            ctx.fill();
            const lit = n === this.hover || n.id === v.selected || (v.path?.includes(n.id) ?? false);
            if (lit) {
                ctx.lineWidth = 2.4 / view.k;
                ctx.strokeStyle = n.id === v.selected || v.path?.includes(n.id) ? acc : ink;
                ctx.stroke();
            }
        }
        ctx.globalAlpha = 1;
        // labels above the nodes, most important first, never on one another
        const prio = (n: GNode) => (n === this.hover || n.id === v.selected ? 1e9 : (n.kind === "pat" ? 1e6 : n.kind === "co" ? 5e5 : 0) + (this.deg.get(n.id) ?? 0));
        const placed: number[][] = [];
        ctx.textAlign = "center";
        for (const n of g.nodes.filter((x) => !this.dimmed(x)).sort((a, b) => prio(b) - prio(a))) {
            const always = n === this.hover || n.id === v.selected || (v.path?.includes(n.id) ?? false);
            if (!always && n.kind === "pro" && (this.deg.get(n.id) ?? 0) < 2 && view.k < 1.4) continue;
            const fs = (n.kind === "pat" ? 15 : n.kind === "co" ? 13.5 : 12.5) / view.k;
            ctx.font = `${n.kind === "pat" || n.kind === "co" ? 600 : 500} ${fs}px Inter, system-ui, sans-serif`;
            const t = n.label.length <= 26 ? n.label : `${n.label.slice(0, 14).trimEnd()}…${n.label.slice(-9).trimStart()}`;
            const w = ctx.measureText(t).width;
            const by = n.y - n.r - 8 / view.k;
            const box = [n.x - w / 2 - 3 / view.k, n.x + w / 2 + 3 / view.k, by - fs, by + 3 / view.k];
            if (!always && placed.some((o) => box[0]! < o[1]! && box[1]! > o[0]! && box[2]! < o[3]! && box[3]! > o[2]!)) continue;
            placed.push(box);
            ctx.lineWidth = 4 / view.k;
            ctx.strokeStyle = bg;
            ctx.strokeText(t, n.x, by);
            ctx.fillStyle = n.kind === "pat" || n.kind === "co" || always ? ink : mut;
            ctx.fillText(t, n.x, by);
        }
    }

    private pt(e: PointerEvent | WheelEvent): [number, number] {
        const r = this.cv.getBoundingClientRect();
        return [e.clientX - r.left, e.clientY - r.top];
    }

    private toWorld(x: number, y: number) {
        return { x: (x - this.view.x) / this.view.k, y: (y - this.view.y) / this.view.k };
    }

    private nodeAt(x: number, y: number) {
        const p = this.toWorld(x, y);
        let best: GNode | null = null, bd = 1e9;
        for (const n of this.g.nodes) {
            const d = Math.hypot(n.x - p.x, n.y - p.y);
            if (d < n.r + 6 / this.view.k && d < bd) { best = n; bd = d; }
        }
        return best;
    }

    private down(e: PointerEvent) {
        const [x, y] = this.pt(e);
        const hit = this.nodeAt(x, y);
        this.moved = false;
        if (hit) { this.drag = hit; this.reheat(0.3); } else this.pan = [x, y, this.view.x, this.view.y];
        this.cv.setPointerCapture(e.pointerId);
    }

    private move(e: PointerEvent) {
        const [x, y] = this.pt(e);
        if (this.drag) {
            const p = this.toWorld(x, y);
            this.drag.x = p.x;
            this.drag.y = p.y;
            this.moved = true;
            return;
        }
        if (this.pan) {
            this.view.x = this.pan[2] + x - this.pan[0];
            this.view.y = this.pan[3] + y - this.pan[1];
            this.moved = true;
            this.draw();
            return;
        }
        const h = this.nodeAt(x, y);
        if (h !== this.hover) {
            this.hover = h;
            this.cv.style.cursor = h ? "pointer" : "grab";
            this.cv.title = h ? `${h.label} · ${KIND_NAME[h.kind]} · ${this.deg.get(h.id)} links` : "";
            this.draw();
        }
    }

    private up(e: PointerEvent) {
        const [x, y] = this.pt(e);
        this.drag = null;
        this.pan = null;
        if (this.moved) return;
        const hit = this.nodeAt(x, y);
        this.onPick(hit ? hit.id : null);
    }

    private wheel(e: WheelEvent) {
        e.preventDefault();
        const [x, y] = this.pt(e);
        const k = e.deltaY > 0 ? 0.9 : 1.1;
        this.view.x = x - (x - this.view.x) * k;
        this.view.y = y - (y - this.view.y) * k;
        this.view.k *= k;
        this.draw();
    }
}

