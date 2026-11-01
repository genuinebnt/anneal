import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState, type CSSProperties } from "react";
import { api } from "../api";
import { highlightLines } from "../highlight";
import { fmt, hottest, opInfo, type Plan, type PlanNode } from "../plan";

type Metric = "time" | "pages" | "rows";

const ENGINE: Record<Plan["engine"], string> = { postgres: "Postgres", mysql: "MySQL", sqlite: "SQLite" };

/** The order the database runs nodes in: children before their parent, so rows appear bottom-up, as data flows. */
function runOrder(root: PlanNode): PlanNode[] {
    const out: PlanNode[] = [];
    const walk = (n: PlanNode) => {
        n.children.forEach(walk);
        out.push(n);
    };
    walk(root);
    return out;
}

function value(n: PlanNode, m: Metric): number | undefined {
    return m === "time" ? n.selfMs : m === "pages" ? n.selfPages : n.rows;
}

function label(n: PlanNode, m: Metric): string {
    const v = value(n, m);
    if (v === undefined) return "";
    return m === "time" ? `${v < 1 ? v.toFixed(3) : v.toFixed(1)} ms` : m === "pages" ? `${fmt(v)} pages` : `${fmt(v)} rows`;
}

function Row({ n, order, metric, max, hot, open, toggle, pages }: { n: PlanNode; pages: Set<string>; order: number; metric: Metric; max: number; hot: boolean; open: boolean; toggle: () => void }) {
    const v = value(n, metric) ?? 0;
    const info = opInfo(n.op);
    const tone = n.flags.find((f) => f.tone === "bad") ? "bad" : n.flags.find((f) => f.tone === "warn") ? "warn" : "";
    return (
        <li className={`pv-node${hot ? " hot" : ""}${open ? " open" : ""}`} style={{ "--d": n.depth, "--o": order } as CSSProperties}>
            <button className="pv-row" onClick={toggle} aria-expanded={open}>
                <span className="pv-n" title="the order this step runs in" aria-hidden="true">{order + 1}</span>
                <span className="pv-op">
                    <b>{n.op}</b>
                    {(n.on || n.index) && <em>{n.index ? `${n.index}${n.on ? ` on ${n.on}` : ""}` : n.on}</em>}
                </span>
                <span className="pv-bar" aria-hidden="true"><i style={{ width: `${max ? Math.max(2, (v / max) * 100) : 0}%` }} /></span>
                <span className="pv-val">{label(n, metric)}</span>
                <span className="pv-tags">
                    {hot && <span className="pv-hot">hottest</span>}
                    {tone && <span className={`pv-flag ${tone}`} title={n.flags.map((f) => f.text).join("\n")}>{n.flags.length}</span>}
                </span>
            </button>
            {open && (
                <div className="pv-more">
                    <div className="pv-facts">
                        {n.estRows !== undefined && <span>planned <b>{fmt(n.estRows)}</b> rows</span>}
                        {n.rows !== undefined && <span>got <b>{fmt(n.rows)}</b></span>}
                        {n.loops !== undefined && n.loops > 1 && <span>loops <b>{n.loops}</b></span>}
                        {n.pages !== undefined && <span>pages <b>{fmt(n.pages)}</b>{n.pagesRead ? ` (${fmt(n.pagesRead)} from disk)` : ""}</span>}
                        {n.totalMs !== undefined && <span>time with children <b>{n.totalMs < 1 ? n.totalMs.toFixed(3) : n.totalMs.toFixed(1)} ms</b></span>}
                        {n.removed !== undefined && <span>removed by filter <b>{fmt(n.removed)}</b></span>}
                    </div>
                    {n.details.length > 0 && (
                        <dl className="pv-details">
                            {n.details.map(([k, v], j) => (
                                <div key={j}><dt>{k}</dt><dd><code>{v}</code></dd></div>
                            ))}
                        </dl>
                    )}
                    {n.flags.map((f, j) => <p key={j} className={`pv-note ${f.tone}`}>{f.text}</p>)}
                    {info && (
                        <div className="pv-info">
                            <p><b>What it does.</b> {info.what}</p>
                            <p><b>Good when.</b> {info.good}</p>
                            {info.watch && <p><b>Watch for.</b> {info.watch}</p>}
                            {info.page && pages.has(info.page) && <Link to="/sd/learn/$id" params={{ id: info.page }}>Read more in the course →</Link>}
                        </div>
                    )}
                </div>
            )}
        </li>
    );
}

/** A query plan drawn as a tree: each node's own time, pages or rows as a bar, the hottest node marked, problems flagged, and
 *  what every operation means on click. Rows appear in the order the database runs them, deepest first. */
export function PlanView({ plan, query, setup, version, text, json }: { plan: Plan; query?: string; setup?: { id: string; sql: string }[]; version?: string; text?: string; json?: unknown }) {
    const hasTime = plan.nodes.some((n) => n.selfMs !== undefined);
    const hasPages = plan.nodes.some((n) => n.selfPages !== undefined);
    const [metric, setMetric] = useState<Metric>(hasPages ? "pages" : hasTime ? "time" : "rows");
    const [tab, setTab] = useState<"tree" | "text" | "json">("tree");
    const hot = useMemo(() => hottest(plan), [plan]);
    const [open, setOpen] = useState<Set<PlanNode>>(() => new Set(hot ? [hot] : []));
    const order = useMemo(() => new Map(runOrder(plan.root).map((n, i) => [n, i])), [plan]);
    const max = Math.max(0, ...plan.nodes.map((n) => value(n, metric) ?? 0));
    const totalPages = plan.root.pages;
    const totalRead = plan.nodes.reduce((a, n) => a + (n.depth === 0 ? n.pagesRead ?? 0 : 0), 0);
    // the course pages that exist, so a "read more" link never leads to a page that has not been written yet
    const overview = useQuery({ queryKey: ["sd"], queryFn: api.sd, staleTime: 60_000 });
    const pages = useMemo(() => new Set((overview.data?.concepts ?? []).map((c) => c.id)), [overview.data]);
    const sqlLines = useMemo(() => (query ? highlightLines("sql", query) : []), [query]);
    return (
        <figure className="pv" aria-label="Query plan">
            <div className="pv-head">
                <span className="pv-engine">{ENGINE[plan.engine]}{version ? ` ${version}` : ""}</span>
                <div className="pv-stats">
                    {plan.executionMs !== undefined && <span><small>EXECUTION</small><b>{plan.executionMs < 1 ? plan.executionMs.toFixed(3) : plan.executionMs.toFixed(1)} ms</b></span>}
                    {totalPages !== undefined && <span><small>PAGES</small><b>{fmt(totalPages)}</b>{totalRead > 0 && <em>{fmt(totalRead)} from disk</em>}</span>}
                    {plan.root.rows !== undefined && <span><small>ROWS</small><b>{fmt(plan.root.rows)}</b></span>}
                    {plan.planningMs !== undefined && <span><small>PLANNING</small><b>{plan.planningMs.toFixed(2)} ms</b></span>}
                </div>
            </div>
            {query && (
                <pre className="pv-sql">{sqlLines.map((h, i) => <span key={i} dangerouslySetInnerHTML={{ __html: (h || " ") + "\n" }} />)}</pre>
            )}
            {setup && (
                <div className="pv-setup">
                    {setup.length ? setup.map((s) => <code key={s.id} title={s.sql}>{s.sql.replace(/;\s*(VACUUM|ANALYZE).*$/i, "").replace(/^CREATE INDEX /, "").replace(/;$/, "")}</code>) : <span>no index beyond the primary keys</span>}
                </div>
            )}
            <div className="pv-bar-top">
                {(text || json !== undefined) && (
                    <div className="pv-seg" role="tablist" aria-label="View">
                        {(["tree", ...(text ? ["text"] : []), ...(json !== undefined ? ["json"] : [])] as const).map((t) => (
                            <button key={t} role="tab" aria-selected={tab === t} className={tab === t ? "on" : ""} onClick={() => setTab(t as typeof tab)}>{t === "tree" ? "Tree" : t === "text" ? "Text" : "JSON"}</button>
                        ))}
                    </div>
                )}
                {tab === "tree" && (
                    <div className="pv-seg" role="radiogroup" aria-label="Bars show">
                        {hasPages && <button role="radio" aria-checked={metric === "pages"} className={metric === "pages" ? "on" : ""} onClick={() => setMetric("pages")}>Pages</button>}
                        {hasTime && <button role="radio" aria-checked={metric === "time"} className={metric === "time" ? "on" : ""} onClick={() => setMetric("time")}>Time</button>}
                        <button role="radio" aria-checked={metric === "rows"} className={metric === "rows" ? "on" : ""} onClick={() => setMetric("rows")}>Rows</button>
                    </div>
                )}
            </div>
            {tab === "tree" && (
                <ol className="pv-tree">
                    {plan.nodes.map((n, i) => (
                        <Row
                            key={i}
                            n={n}
                            pages={pages}
                            order={order.get(n) ?? 0}
                            metric={metric}
                            max={max}
                            hot={n === hot}
                            open={open.has(n)}
                            toggle={() => setOpen((s) => { const next = new Set(s); if (next.has(n)) next.delete(n); else next.add(n); return next; })}
                        />
                    ))}
                </ol>
            )}
            {tab === "text" && text && <pre className="pv-raw">{text}</pre>}
            {tab === "json" && json !== undefined && <pre className="pv-raw">{JSON.stringify(json, null, 2)}</pre>}
            <figcaption>Bars show each step's own {metric === "time" ? "time" : metric === "pages" ? "pages, the 8 KB blocks it touched" : "rows"}; steps appear in the order they run, deepest first. Click a step for what it means.</figcaption>
        </figure>
    );
}
