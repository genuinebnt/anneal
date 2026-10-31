import { Link } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api } from "../api";
import { Panel } from "./layout";

const today = () => new Date().toISOString().slice(0, 10);

/** Your plans: switch to one, or save the current rules and days as a new one. The same list as the Calendar's. */
export function PlanSwitcher({ link }: { link?: boolean }) {
    const qc = useQueryClient();
    const q = useQuery({ queryKey: ["plan", "first"], queryFn: () => api.plan(today(), today()), staleTime: 30_000 });
    const [naming, setNaming] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const refresh = async () => {
        await Promise.all([qc.invalidateQueries({ queryKey: ["plan"] }), qc.invalidateQueries({ queryKey: ["dsa"] }), qc.invalidateQueries({ queryKey: ["reviews"] })]);
    };
    const p = q.data;
    if (!p) return null;
    const short = (d: string) => new Date(`${d}T00:00`).toLocaleDateString("en-GB", { day: "numeric", month: "short" });
    return (
        <Panel title="Plans" aside={link ? <Link to="/dsa/calendar">Calendar →</Link> : undefined}>
            <div className="c-plans">
                {p.plans.map((x) => (
                    <button key={x.id} className={`c-pl${x.id === p.active ? " on" : ""}`} aria-pressed={x.id === p.active} onClick={() => x.id !== p.active && void api.setActivePlan(x.id).then(refresh)}>
                        <b>{x.name}</b>
                        <small>{x.problems} problems · {x.finish ? short(x.finish) : "—"}{x.late ? <em> · ⚠ {x.late} late</em> : null}</small>
                    </button>
                ))}
                {naming === null ? (
                    <button className="c-pl add" onClick={() => setNaming("")}><b>+ Save as new plan</b><small>keeps these rules and days</small></button>
                ) : (
                    <form className="c-pl add on" onSubmit={(e) => { e.preventDefault(); if (naming.trim()) void api.createPlan(naming.trim()).then(refresh).then(() => setNaming(null)).catch((er) => setError((er as Error).message)); }}>
                        <input autoFocus placeholder="Name, e.g. Amazon loop" value={naming} maxLength={60} onChange={(e) => setNaming(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setNaming(null)} />
                        <small>Enter to save · Esc to cancel</small>
                    </form>
                )}
                {error && <small className="c-err">{error}</small>}
            </div>
        </Panel>
    );
}
