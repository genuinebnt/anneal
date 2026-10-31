import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "../../api";
import { HowFound, ListCard, Page3, Panel, Tile } from "../../components/layout";
import { Skeleton } from "../../components/Skeleton";
import { SdFrame } from "./SdHome";

const KINDS: [string, string][] = [["all", "All"], ["hld", "High-level"], ["api", "API"], ["lld", "Low-level"]];

/** Every practice problem: its kind, time, the concepts it leans on, and how you have done. */
export function SdProblems() {
    const q = useQuery({ queryKey: ["sd"], queryFn: api.sd });
    const [kind, setKind] = useState("all");
    const o = q.data;
    if (!o) return <SdFrame><Page3 crumb={["SYSTEM DESIGN", "PROBLEMS"]} title="Problems" center={<Skeleton lines={6} />} /></SdFrame>;
    const title = (id: string) => o.concepts.find((c) => c.id === id)?.title ?? id;
    const shown = [...o.problems].filter((p) => kind === "all" || p.kind === kind).sort((a, b) => a.order - b.order);
    const drilled = o.problems.filter((p) => p.attempts > 0).length;
    const left = (
        <Panel title="Kind">
            <div className="ui-seg sd-kinds" role="radiogroup" aria-label="Kind">
                {KINDS.map(([k, l]) => <button key={k} role="radio" aria-checked={kind === k} className={kind === k ? "on" : ""} onClick={() => setKind(k)}>{l}<em>{k === "all" ? o.problems.length : o.problems.filter((p) => p.kind === k).length}</em></button>)}
            </div>
        </Panel>
    );
    const center = (
        <div className="sd-cards">
            {shown.map((p, i) => (
                <ListCard
                    key={p.id}
                    i={i}
                    title={p.title}
                    note={p.best != null ? `best ${Math.round(p.best * 100)}%` : `${p.minutes} min`}
                    w={(p.best ?? 0) * 100}
                    text={`${p.kind.toUpperCase()} · ${p.points} rubric points · uses ${p.uses.map(title).join(", ")}`}
                    chips={<>{p.companies.slice(0, 4).map((c) => <span key={c}>{c}</span>)}{p.attempts > 0 && <span className="on">{p.attempts} attempt{p.attempts === 1 ? "" : "s"}</span>}{p.unmarked && <span className="hot">not marked</span>}</>}
                    to={{ to: "/sd/practice/$id", params: { id: p.id } }}
                />
            ))}
            {shown.length === 0 && <p className="sd-dim">No {kind.toUpperCase()} problems yet: they are being written.</p>}
        </div>
    );
    const right = <HowFound title="How a drill works" text="One problem through the delivery framework, each step on its own clock. The rubric and the reference stay hidden until you finish; then you compare step by step and mark each point. Every marked point becomes a review card." rows={[["Problems", o.problems.length], ["Drilled", drilled]]} />;
    return (
        <SdFrame>
            <Page3 crumb={["SYSTEM DESIGN", "PROBLEMS"]} title="Problems" sub="Design them against the clock, then compare with a rubric written for each step." tiles={<Tile em="Drilled" strong={`${drilled}/${o.problems.length}`} small="at least once" />} left={left} center={center} right={right} label="Kind" />
        </SdFrame>
    );
}
