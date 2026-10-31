import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import type { CSSProperties } from "react";
import { api, type SdOverview } from "../../api";
import { Header } from "../../components/Header";
import { HowFound, ListCard, Page3, Panel, Tile, PageLoading } from "../../components/layout";

/** The groups of concept pages, in order, with what each holds. */
export const SD_GROUPS: { id: string; name: string; blurb: string; color: string }[] = [
    { id: "fundamentals", name: "Fundamentals", blurb: "the framework, numbers, estimation", color: "var(--acc)" },
    { id: "core", name: "Core concepts", blurb: "data, caching, sharding, replication, APIs", color: "var(--warn)" },
    { id: "pattern", name: "Patterns", blurb: "scaling reads and writes, contention, realtime", color: "var(--grn)" },
    { id: "technology", name: "Key technologies", blurb: "Kafka, Redis, Postgres, and how they work inside", color: "var(--vio)" },
    { id: "advanced", name: "Advanced topics", blurb: "proximity, vectors, CDC, CRDTs, sketches", color: "var(--bad)" },
    { id: "distributed", name: "Distributed systems", blurb: "clocks, consensus, quorums, isolation", color: "var(--acc)" },
    { id: "lld", name: "Low-level design", blurb: "SOLID, patterns, concurrency", color: "var(--vio)" },
    { id: "rust", name: "Rust", blurb: "patterns, async architecture, Rust design questions", color: "var(--warn)" },
];

export const groupOf = (id: string) => SD_GROUPS.find((g) => g.id === id) ?? SD_GROUPS[0]!;

/** The page frame for system design: the header in the area, the accent in its colour. */
export function SdFrame({ children }: { children: React.ReactNode }) {
    return (
        <>
            <Header area="sd" />
            <div className="sd-3" style={{ "--ca": "var(--warn)", "--cab": "color-mix(in oklab, var(--warn) 14%, transparent)" } as CSSProperties}>{children}</div>
        </>
    );
}

/** What to do next: an unread concept a problem leans on, else the next problem not drilled, else what is due. */
function nextUp(o: SdOverview) {
    const problem = [...o.problems].sort((a, b) => a.order - b.order).find((p) => p.attempts === 0);
    const unread = problem ? o.concepts.find((c) => problem.uses.includes(c.id) && c.checked === 0) : o.concepts.find((c) => c.checked === 0);
    if (unread) return { kind: "Learn", title: unread.title, text: problem ? `${problem.title} leans on it; read it and answer its checks first.` : unread.lede, to: { to: "/sd/learn/$id" as const, params: { id: unread.id } } };
    if (problem) return { kind: "Practice", title: problem.title, text: `${problem.minutes} minutes through the delivery framework, then compare with a ${problem.points}-point rubric.`, to: { to: "/sd/practice/$id" as const, params: { id: problem.id } } };
    return null;
}

export function SdHome() {
    const q = useQuery({ queryKey: ["sd"], queryFn: api.sd });
    const o = q.data;
    if (!o) return <SdFrame><PageLoading crumb={["SYSTEM DESIGN"]} title="System design" error={q.isError ? <p className="notice bad">Couldn't reach the API.</p> : undefined} /></SdFrame>;
    const checks = o.concepts.reduce((n, c) => n + c.checks, 0);
    const answered = o.concepts.reduce((n, c) => n + Math.min(c.checked, c.checks), 0);
    const drilled = o.problems.filter((p) => p.attempts > 0);
    const best = drilled.length ? drilled.reduce((n, p) => n + (p.best ?? 0), 0) / drilled.length : null;
    const next = nextUp(o);
    const groups = SD_GROUPS.map((g) => {
        const cs = o.concepts.filter((c) => c.group === g.id);
        const total = cs.reduce((n, c) => n + c.checks, 0);
        const done = cs.reduce((n, c) => n + Math.min(c.checked, c.checks), 0);
        return { ...g, pages: cs.length, total, done };
    });

    const tiles = (
        <>
            <Tile em="Checks answered" strong={`${answered}/${checks}`} small="open questions on the concept pages" to={{ to: "/sd/learn" }} />
            <Tile em="Drilled" strong={`${drilled.length}/${o.problems.length}`} small={best != null ? `best scores average ${Math.round(best * 100)}%` : "problems through the framework"} to={{ to: "/sd/problems" }} />
            <Tile em="Reviews due" strong={o.due} hot={o.due > 0} color="var(--vio)" small={`${o.week} more this week`} to={{ to: "/sd/review" }} />
        </>
    );
    const left = (
        <Panel title="Sections">
            <div className="ly-cards">
                {groups.filter((g) => g.pages > 0).map((g, i) => (
                    <ListCard key={g.id} i={i} title={g.name} note={`${g.done}/${g.total}`} w={g.total ? (g.done / g.total) * 100 : 0} color={g.color} text={g.blurb} to={{ to: "/sd/learn", hash: g.id }} />
                ))}
            </div>
        </Panel>
    );
    const center = (
        <>
            {next && (
                <section className="ly-pn sd-next">
                    <span className="sd-k">NEXT UP · {next.kind.toUpperCase()}</span>
                    <h2>{next.title}</h2>
                    <p>{next.text}</p>
                    <div className="sd-row"><Link className="sd-go" {...next.to}>{next.kind === "Learn" ? "Read it" : "Start the drill"}</Link><Link className="sd-ghost" to="/sd/problems">All problems</Link></div>
                </section>
            )}
            <div className="sd-grid">
                {groups.map((g) => (
                    <Link key={g.id} to="/sd/learn" hash={g.id} className={`ly-pn sd-sec${g.pages ? "" : " soon"}`} style={{ "--c": g.color } as CSSProperties}>
                        <span className="ly-bigring" style={{ "--p": g.total ? Math.round((g.done / g.total) * 100) : 0, "--c": g.color } as CSSProperties}><b>{g.pages}</b></span>
                        <div><h3>{g.name}</h3><p>{g.blurb}</p><small>{g.pages ? `${g.pages} page${g.pages === 1 ? "" : "s"} · ${g.done}/${g.total} checks` : "being written"}</small></div>
                    </Link>
                ))}
            </div>
            <Panel title="Problems">
                <div className="ly-cards">
                    {[...o.problems].sort((a, b) => a.order - b.order).map((p, i) => (
                        <ListCard key={p.id} i={i} title={p.title} note={p.best != null ? `${Math.round(p.best * 100)}%` : `${p.minutes} min`} w={(p.best ?? 0) * 100} text={p.attempts ? `${p.attempts} attempt${p.attempts === 1 ? "" : "s"}${p.unmarked ? " · last one not marked" : ""}` : `uses ${p.uses.length} concepts`} to={{ to: "/sd/practice/$id", params: { id: p.id } }} />
                    ))}
                </div>
            </Panel>
        </>
    );
    const right = (
        <>
            <Panel title="The loop">
                <ol className="sd-loop">
                    <li><b>Learn</b> a concept, then answer its checks in your own words</li>
                    <li><b>Practise</b> a problem against the clock, step by step</li>
                    <li><b>Compare</b> with the rubric: each point got it, partly or missed</li>
                    <li><b>Review</b>: every marked point comes back on a schedule</li>
                </ol>
            </Panel>
            <HowFound title="How this works" text="Nothing is graded by a model: rubrics and references are written for anneal, you mark yourself, and FSRS brings each point back when you are about to forget it. A miss comes back today, a partly in two days, a got it in a week." rows={[["Pages", o.concepts.length], ["Problems", o.problems.length], ["Cards", o.cards]]} />
        </>
    );
    return (
        <SdFrame>
            <Page3 crumb={["SYSTEM DESIGN", "HOME"]} title={<>System design, <span className="sd-acc">practised.</span></>} sub="Learn it, design it against the clock, compare with a rubric, and review what you missed." tiles={tiles} left={left} center={center} right={right} label="Sections" />
        </SdFrame>
    );
}
