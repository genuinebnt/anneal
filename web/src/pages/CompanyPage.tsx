import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState, type CSSProperties } from "react";
import { api, type DsaProblem } from "../api";
import { Header } from "../components/Header";
import { Skeleton } from "../components/Skeleton";
import { DIFF, counted, niceDate } from "../dsa";
import { COUNT_KEYS } from "../premium";

const WEEKDAY: Record<string, number> = { sun: 0, mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6 };
const isoDay = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** How strongly a company asks a problem: its frequency, the last six months counting double (the plan's own "most asked" order). */
const score = (p: DsaProblem, company: string) => {
    const c = p.companies.find((x) => x.name === company);
    return c ? c.frequency * (c.recent ? 2 : 1) : 0;
};

/** Interview mode: pick the company and the day, see how much it asks and the pace that needs, and make it your plan. The main plan is
 *  not changed: a plan of its own is made beside it, and "Back to my main plan" switches back. */
export function CompanyPage() {
    const qc = useQueryClient();
    const nav = useNavigate();
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const plan = useQuery({ queryKey: ["plan", "company"], queryFn: () => api.plan(isoDay(new Date()), isoDay(new Date())) });
    const o = dsa.data;
    const [q, setQ] = useState("");
    const [company, setCompany] = useState<string | null>(null);
    const [recent, setRecent] = useState(true);
    const [date, setDate] = useState("");
    const [made, setMade] = useState<string | null>(null);

    const companies = useMemo(() => {
        if (!o) return [];
        const by = new Map<string, { name: string; group: string; asked: number; recent: number; solved: number }>();
        for (const p of o.problems.filter((x) => counted(o, x))) {
            for (const c of p.companies) {
                const e = by.get(c.name) ?? { name: c.name, group: c.group, asked: 0, recent: 0, solved: 0 };
                e.asked++;
                if (c.recent) e.recent++;
                if (p.state.solved) e.solved++;
                by.set(c.name, e);
            }
        }
        return [...by.values()].sort((a, b) => b.asked - a.asked || a.name.localeCompare(b.name));
    }, [o]);
    const shown = companies.filter((c) => c.name.toLowerCase().includes(q.trim().toLowerCase()));
    const chosen = companies.find((c) => c.name === company);

    const scope = useMemo(() => (o && company ? o.problems.filter((p) => counted(o, p) && p.companies.some((c) => c.name === company && (!recent || c.recent))) : []), [o, company, recent]);
    const left = scope.filter((p) => !p.state.solved);
    const next = [...left].sort((a, b) => score(b, company ?? "") - score(a, company ?? "") || a.order - b.order).slice(0, 8);
    // the solve days between now and the interview, from the weekly routine
    const solveDays = useMemo(() => {
        if (!o || !date) return null;
        const days = new Set((o.settings.new_days as string[]).map((d) => WEEKDAY[d]));
        let n = 0;
        for (let d = new Date(); isoDay(d) < date; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) if (days.has(d.getDay() === 0 ? 0 : d.getDay()) && isoDay(d) !== isoDay(new Date())) n++;
        return n + ((o.settings.new_days as string[]).some((d) => WEEKDAY[d] === new Date().getDay()) ? 1 : 0);
    }, [o, date]);
    const perDay = solveDays ? left.length / solveDays : null;
    const planned = o?.settings.new_per_day ?? 1;

    const make = useMutation({
        mutationFn: () => api.companyPlan({ company: company!, interview: date || null, recent_only: recent }),
        onSuccess: () => {
            setMade(company);
            COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] }));
        },
    });
    const back = useMutation({
        mutationFn: () => api.setActivePlan("default"),
        onSuccess: () => {
            setMade(null);
            COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] }));
        },
    });
    const active = plan.data?.plans.find((p) => p.id === plan.data?.active);
    const onCompanyPlan = !!active && active.rules.companies.length > 0;
    const tomorrow = isoDay(new Date(Date.now() + 864e5));

    return (
        <>
            <Header area="dsa" />
            <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <div className="cm-wrap">
                    <h1>Interview mode</h1>

                    {onCompanyPlan && (
                        <div className="cm-now" role="status">
                            <span>Your plan is <b>{active.name}</b>{active.finish ? <>, finishing {niceDate(active.finish, plan.data!.today)}</> : null}{active.late ? <>, <b>{active.late}</b> late</> : null}. Next up follows it.</span>
                            <span className="cm-acts">
                                <Link to="/dsa/calendar" className="cm-btn">See the calendar</Link>
                                <button className="cm-btn" disabled={back.isPending} onClick={() => back.mutate()}>Back to my main plan</button>
                            </span>
                        </div>
                    )}

                    {!o ? (
                        <Skeleton lines={6} />
                    ) : (
                        <div className="cm-cols">
                            <section className="cm-card" aria-label="Companies">
                                <label className="cm-lab" htmlFor="cm-q">COMPANY</label>
                                <input id="cm-q" className="cm-q" type="search" placeholder="Search, e.g. Google" value={q} onChange={(e) => setQ(e.target.value)} autoComplete="off" />
                                <div className="cm-list" role="listbox" aria-label="Companies">
                                    {shown.slice(0, 60).map((c) => (
                                        <button key={c.name} role="option" aria-selected={company === c.name} className={`cm-co${company === c.name ? " on" : ""}`} onClick={() => { setCompany(c.name); setMade(null); setRecent(c.recent >= 8); }}>
                                            <b>{c.name}</b>
                                            <small>{c.asked} asked · {c.recent} recent · {c.solved} solved</small>
                                        </button>
                                    ))}
                                    {shown.length === 0 && <p className="rempty">No company matches.</p>}
                                </div>
                            </section>

                            <section className="cm-card" aria-label="The plan">
                                {!chosen ? (
                                    <p className="rempty" style={{ margin: 0 }}>Choose a company to see what it asks.</p>
                                ) : (
                                    <>
                                        <h2 className="cm-h">{chosen.name}</h2>
                                        <label className="cm-chk"><input type="checkbox" checked={recent} onChange={(e) => setRecent(e.target.checked)} /> Only what it asked in the last six months <small>({chosen.recent} of {chosen.asked})</small></label>
                                        <div className="cm-stats">
                                            <div><b>{scope.length}</b><span>in the plan</span></div>
                                            <div><b>{scope.length - left.length}</b><span>already solved</span></div>
                                            <div><b>{left.length}</b><span>to go</span></div>
                                        </div>
                                        <label className="cm-lab" htmlFor="cm-date">INTERVIEW DAY (OPTIONAL)</label>
                                        <input id="cm-date" className="cm-date" type="date" min={tomorrow} value={date} onChange={(e) => setDate(e.target.value)} />
                                        {date && solveDays != null && (
                                            <p className="cm-pace" role="status">
                                                {solveDays === 0 ? "There is no solve day before that day in your routine." : (
                                                    <>
                                                        {solveDays} solve day{solveDays === 1 ? "" : "s"} until then. <b>{left.length}</b> to go is about <b>{perDay!.toFixed(1)}</b> a solve day; your routine does {planned}.{" "}
                                                        {perDay! <= planned ? "That fits." : perDay! <= planned * 1.5 ? "Tight: add a solve day or two on the calendar." : "More than the routine can do: narrow it to the last six months, or move the day."}
                                                    </>
                                                )}
                                            </p>
                                        )}
                                        <span className="cm-lab">FIRST UP</span>
                                        <ol className="cm-next">
                                            {next.map((p) => (
                                                <li key={p.id}><Link to="/d/$slug" params={{ slug: p.slug }}>{p.title}</Link><small>{DIFF[p.difficulty][0]} · {p.companies.find((c) => c.name === company)?.recent ? "recent" : "older"}</small></li>
                                            ))}
                                            {next.length === 0 && <li className="rempty">You have done every problem in this plan.</li>}
                                        </ol>
                                        <div className="cm-acts">
                                            <button className="cm-btn pri" disabled={make.isPending || scope.length === 0} onClick={() => make.mutate()}>{make.isPending ? "Making it…" : `Make this my plan`}</button>
                                            {made === company && <span className="cm-ok" role="status">Done. Your plan is now Interview: {company}. <Link to="/dsa" style={{ color: "var(--ca)" }}>Next up</Link> · <button className="pp-link" onClick={() => void nav({ to: "/dsa/calendar" })}>Calendar</button></span>}
                                            {make.isError && <span className="st-bad">{(make.error as Error).message}</span>}
                                        </div>
                                    </>
                                )}
                            </section>
                        </div>
                    )}
                </div>
            </main>
        </>
    );
}
