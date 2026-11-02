import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { HowFound, Page3, Panel, Tile } from "../../components/layout";
import { PlanView } from "../../components/PlanView";
import { fmt, parsePlan, type Plan } from "../../plan";
import { SdFrame } from "./SdHome";

/** The key a plan block on a course page leaves its plan under when you open it in the explorer. */
export const EXPLORER_HANDOFF = "sd.plans.handoff";

type Parsed = { plan: Plan } | { error: string } | null;

function parse(text: string): Parsed {
    if (!text.trim()) return null;
    try {
        const plan = parsePlan(text);
        if (!plan.nodes.length) return { error: "No plan steps found in this text." };
        return { plan };
    } catch (e) {
        return { error: e instanceof Error ? e.message : "This text could not be read as a plan." };
    }
}

/** What a plan costs in the three measures the course uses, for the side-by-side summary. */
function totals(p: Plan) {
    return {
        ms: p.executionMs,
        pages: p.root.pages,
        rows: p.root.rows,
        steps: p.nodes.length,
        flags: p.nodes.reduce((n, x) => n + x.flags.length, 0),
    };
}

function change(a: number | undefined, b: number | undefined): { text: string; tone: "good" | "bad" | "" } | null {
    if (a === undefined || b === undefined) return null;
    if (a === b) return { text: "same", tone: "" };
    const pct = a === 0 ? null : Math.round(((b - a) / a) * 100);
    const word = b < a ? "less" : "more";
    return { text: `${fmt(Math.abs(b - a))} ${word}${pct === null ? "" : ` (${pct > 0 ? "+" : ""}${pct}%)`}`, tone: b < a ? "good" : "bad" };
}

function Compare({ a, b }: { a: Plan; b: Plan }) {
    const ta = totals(a);
    const tb = totals(b);
    const rows: [string, string, number | undefined, number | undefined][] = [
        ["Execution", "ms", ta.ms, tb.ms],
        ["Pages touched", "", ta.pages, tb.pages],
        ["Rows returned", "", ta.rows, tb.rows],
        ["Steps", "", ta.steps, tb.steps],
        ["Flags raised", "", ta.flags, tb.flags],
    ];
    const show = (v: number | undefined, unit: string) => (v === undefined ? "n/a" : `${fmt(v)}${unit ? ` ${unit}` : ""}`);
    return (
        <table className="pe-cmp" aria-label="Plan A against plan B">
            <thead>
                <tr><th /><th>A</th><th>B</th><th>B against A</th></tr>
            </thead>
            <tbody>
                {rows.map(([k, unit, x, y]) => {
                    const c = change(x, y);
                    return (
                        <tr key={k}>
                            <th scope="row">{k}</th>
                            <td>{show(x, unit)}</td>
                            <td>{show(y, unit)}</td>
                            <td className={c?.tone ?? ""}>{c?.text ?? ""}</td>
                        </tr>
                    );
                })}
            </tbody>
        </table>
    );
}

function Slot({ label, text, set, parsed, query }: { label: string; text: string; set: (t: string) => void; parsed: Parsed; query?: string }) {
    return (
        <section className="pe-slot">
            <label className="pe-lab" htmlFor={`pe-${label}`}>{label}</label>
            <textarea
                id={`pe-${label}`}
                className="pe-in"
                value={text}
                onChange={(e) => set(e.target.value)}
                spellCheck={false}
                placeholder={"Paste the output of EXPLAIN here:\nPostgres (text or FORMAT JSON), MySQL (EXPLAIN ANALYZE or FORMAT=TREE) or SQLite (EXPLAIN QUERY PLAN)."}
                rows={text ? Math.min(14, Math.max(6, text.split("\n").length + 1)) : 6}
            />
            {parsed && "error" in parsed && <p className="pe-err" role="alert">{parsed.error}</p>}
            {parsed && "plan" in parsed && <PlanView plan={parsed.plan} query={query} text={text.trim().startsWith("[") || text.trim().startsWith("{") ? undefined : text} />}
        </section>
    );
}

/** Paste any plan and read it: the parsers pick the engine, the plan view draws it, and a second plan can be set beside it to
 *  see what an index or a rewrite changed. Course pages open here through the handoff in sessionStorage. */
export function SdPlans() {
    const [a, setA] = useState("");
    const [b, setB] = useState("");
    const [compare, setCompare] = useState(false);
    const [pageId, setPageId] = useState("");
    const overview = useQuery({ queryKey: ["sd"], queryFn: api.sd, staleTime: 60_000 });
    const sqlPages = useMemo(() => (overview.data?.concepts ?? []).filter((c) => c.group.startsWith("sql-")).sort((x, y) => x.order - y.order), [overview.data]);
    const examples = useQuery({ queryKey: ["sd-concept", pageId], queryFn: () => api.sdConcept(pageId), enabled: !!pageId, staleTime: 60_000 });
    const plans = useMemo(() => Object.values(examples.data?.plans ?? {}), [examples.data]);
    const [query, setQuery] = useState<string | undefined>();

    useEffect(() => {
        try {
            const raw = sessionStorage.getItem(EXPLORER_HANDOFF);
            if (!raw) return;
            sessionStorage.removeItem(EXPLORER_HANDOFF);
            const h = JSON.parse(raw) as { text?: string; query?: string };
            if (h.text) setA(h.text);
            setQuery(h.query);
        } catch {
            /* storage can be unavailable; the explorer simply starts empty */
        }
    }, []);

    const pa = useMemo(() => parse(a), [a]);
    const pb = useMemo(() => parse(b), [b]);
    const plan = pa && "plan" in pa ? pa.plan : null;
    const planB = pb && "plan" in pb ? pb.plan : null;
    const flags = plan ? plan.nodes.reduce((n, x) => n + x.flags.length, 0) : 0;

    const load = (text: string, q: string, into: "a" | "b") => {
        if (into === "b") setB(text);
        else {
            setA(text);
            setQuery(q);
        }
    };

    const left = (
        <>
            <Panel title="Mode">
                <div className="ui-seg sd-kinds" role="radiogroup" aria-label="Mode">
                    <button role="radio" aria-checked={!compare} className={!compare ? "on" : ""} onClick={() => setCompare(false)}>One plan</button>
                    <button role="radio" aria-checked={compare} className={compare ? "on" : ""} onClick={() => setCompare(true)}>Compare two</button>
                </div>
            </Panel>
            <Panel title="Captured plans">
                <label className="pe-lab" htmlFor="pe-page">Course page</label>
                <select id="pe-page" className="pe-sel" value={pageId} onChange={(e) => setPageId(e.target.value)}>
                    <option value="">Choose a page</option>
                    {sqlPages.map((c) => <option key={c.id} value={c.id}>{c.order}. {c.title}</option>)}
                </select>
                {pageId && examples.isLoading && <p className="sd-dim">Loading…</p>}
                <ul className="pe-list">
                    {plans.map((f) => (
                        <li key={f.id}>
                            <span title={f.query}>{f.id}</span>
                            <button onClick={() => load(f.text, f.query, "a")}>{compare ? "A" : "Open"}</button>
                            {compare && <button onClick={() => load(f.text, f.query, "b")}>B</button>}
                        </li>
                    ))}
                </ul>
                {pageId && !examples.isLoading && plans.length === 0 && <p className="sd-dim">This page shows no plans.</p>}
            </Panel>
        </>
    );

    const center = (
        <div className="pe-main">
            <Slot label={compare ? "Plan A" : "Plan"} text={a} set={(t) => { setA(t); setQuery(undefined); }} parsed={pa} query={query} />
            {compare && <Slot label="Plan B" text={b} set={setB} parsed={pb} />}
            {compare && plan && planB && <Compare a={plan} b={planB} />}
            {compare && !(plan && planB) && <p className="sd-dim">Put a plan in each box to see them side by side.</p>}
        </div>
    );

    const right = (
        <HowFound
            title="How a plan is read"
            text="The text is parsed here in your browser; nothing is sent anywhere. Each step's own time and pages are its figures minus its children's, the bars grow in the order the database runs the steps, and a flag is raised for a misestimate of 10 times or more, rows thrown away by a filter, a sort that spilled, a hash that ran in batches, or heap fetches in an index-only scan."
            rows={plan ? [["Engine", plan.engine], ["Steps", plan.nodes.length], ["Flags", flags]] : [["Engine", "none yet"]]}
        />
    );

    return (
        <SdFrame>
            <Page3
                crumb={["SYSTEM DESIGN", "SQL", "PLAN EXPLORER"]}
                title="Plan explorer"
                sub="Paste any query plan to see where the work went. Compare two to see what an index or a rewrite changed."
                tiles={
                    <>
                        <Tile em="Engine" strong={plan ? plan.engine : "none"} small="found from the text" />
                        <Tile em="Pages" strong={plan?.root.pages !== undefined ? fmt(plan.root.pages) : "n/a"} small="8 KB blocks touched" />
                        <Tile em="Flags" strong={String(flags)} small="things worth a look" hot={flags > 0} />
                    </>
                }
                left={left}
                center={center}
                right={right}
                label="Plans"
            />
        </SdFrame>
    );
}
