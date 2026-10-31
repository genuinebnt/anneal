import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { api, type DsaProblem } from "../api";
import { Header } from "../components/Header";
import { HowFound, ListCard, Page3, Panel, Tile } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { DateField, Switch } from "../components/ui";
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
    // never an empty page: the company asked about most is chosen until you pick another
    useEffect(() => {
        if (!company && companies[0]) {
            setCompany(companies[0].name);
            setRecent(companies[0].recent >= 8);
        }
    }, [companies, company]);
    const shown = companies.filter((c) => c.name.toLowerCase().includes(q.trim().toLowerCase()));
    const chosen = companies.find((c) => c.name === company);

    const scope = useMemo(() => (o && company ? o.problems.filter((p) => counted(o, p) && p.companies.some((c) => c.name === company && (!recent || c.recent))) : []), [o, company, recent]);
    const left = scope.filter((p) => !p.state.solved);
    // the solve days between now and the interview, from the weekly routine
    const solveDays = useMemo(() => {
        if (!o || !date) return null;
        const days = new Set((o.settings.new_days as string[]).map((d) => WEEKDAY[d]));
        let n = 0;
        for (let d = new Date(); isoDay(d) < date; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) if (days.has(d.getDay() === 0 ? 0 : d.getDay()) && isoDay(d) !== isoDay(new Date())) n++;
        return n + ((o.settings.new_days as string[]).some((d) => WEEKDAY[d] === new Date().getDay()) ? 1 : 0);
    }, [o, date]);
    const perDay = solveDays ? left.length / solveDays : null;
    const bands = [0, 1, 2].map((i) => left.filter((p) => ["easy", "medium", "hard"][i] === p.difficulty).length);
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

    const [all, setAll] = useState(false);
    const firstUp = [...left].sort((a, b) => score(b, company ?? "") - score(a, company ?? "") || a.order - b.order);
    const top = Math.max(1, ...firstUp.map((p) => score(p, company ?? "")));
    // companies that ask the same problems: how many of this plan's problems each one asks too
    const alike = useMemo(() => {
        if (!company) return [];
        const n = new Map<string, number>();
        for (const p of scope) for (const c of p.companies) if (c.name !== company) n.set(c.name, (n.get(c.name) ?? 0) + 1);
        return [...n.entries()].sort((a, b) => b[1] - a[1]).slice(0, 4);
    }, [scope, company]);
    const counted_ = o ? o.problems.filter((p) => counted(o, p)).length : 0;

    const tiles = (
        <>
            <Tile em="Companies" hint="Companies that have asked at least one problem of the site." strong={companies.length} small="that asked a problem of the site" />
            <Tile em={chosen ? `${chosen.name} in the plan` : "In the plan"} strong={chosen ? scope.length : "–"} small={recent ? "last six months" : "any time"} />
            <Tile em="To go" hint="Problems in this plan that you have not solved yet." strong={chosen ? left.length : "–"} hot color="var(--acc)" small={perDay != null ? `${perDay.toFixed(1)} a solve day` : "pick an interview day"} />
        </>
    );

    const leftPanel = (
        <>
            <label className="cm-find">
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5 14 14" /></svg>
                <input id="cm-q" type="search" placeholder="Find a company…" value={q} onChange={(e) => setQ(e.target.value)} autoComplete="off" aria-label="Find a company" />
            </label>
            <div className="ly-rh"><span className="ly-lab">Companies · {companies.length}</span></div>
            <div className="ly-cards" role="listbox" aria-label="Companies">
                {shown.slice(0, 60).map((c, i) => (
                    <ListCard
                        key={c.name}
                        i={Math.min(i, 12)}
                        on={company === c.name}
                        title={c.name}
                        note={`${c.asked} asked`}
                        w={c.asked ? (c.solved / c.asked) * 100 : 0}
                        color="var(--grn)"
                        chips={<><span className="ly-pat" style={{ "--c": "var(--vio)" } as CSSProperties}>{c.group}</span><span className="cm-tp">{c.recent} recent · {c.solved} solved</span></>}
                        role="option"
                        onClick={() => { setCompany(c.name); setMade(null); setRecent(c.recent >= 8); setAll(false); }}
                    />
                ))}
                {shown.length === 0 && <p className="rempty">No company matches.</p>}
            </div>
        </>
    );

    const center = !o ? (
        <Skeleton lines={6} />
    ) : !chosen ? (
        <p className="rempty">Choose a company to see what it asks.</p>
    ) : (
        <>
            <section className="ly-pn cm-pan">
                <div className="cm-top">
                    <h2 className="cm-h">{chosen.name}</h2>
                    <span className="cm-sub">{chosen.asked} asked · {chosen.recent} in the last six months</span>
                    <span className="ly-sp" />
                    <button className="cm-btn pri" disabled={make.isPending || scope.length === 0} onClick={() => make.mutate()}>{make.isPending ? "Making it…" : "Make this my plan"}</button>
                </div>
                {made === company && <p className="cm-ok" role="status">Done. Your plan is now Interview: {company}. <Link to="/dsa" style={{ color: "var(--ca)" }}>Next up</Link> · <button className="pp-link" onClick={() => void nav({ to: "/dsa/calendar" })}>Calendar</button></p>}
                {make.isError && <p className="st-bad">{(make.error as Error).message}</p>}
                <div className="cm-row">
                    <span className="cm-opt">
                        <Switch on={recent} onChange={setRecent} label="Only what it asked in the last six months" />
                        <span>Only the last six months <small>{chosen.recent} of {chosen.asked}</small></span>
                    </span>
                    <span className="ly-sp" />
                    <span className="cm-day"><span>Interview day</span><DateField label="Interview day" min={tomorrow} value={date} onChange={setDate} /></span>
                </div>
                {date && solveDays != null && (
                    <div className="cm-chips" role="status">
                        {solveDays === 0 ? <span className="cm-chip bad">No solve day before then</span> : (
                            <>
                                <span className="cm-chip"><b>{solveDays}</b> solve day{solveDays === 1 ? "" : "s"}</span>
                                <span className="cm-chip"><b>{perDay!.toFixed(1)}</b> a day · routine {planned}</span>
                                <span className={`cm-chip ${perDay! <= planned ? "ok" : perDay! <= planned * 1.5 ? "warn" : "bad"}`}>{perDay! <= planned ? "Fits" : perDay! <= planned * 1.5 ? "Tight" : "Too much"}</span>
                            </>
                        )}
                    </div>
                )}
                <div className="cm-stats">
                    {([[scope.length, "in the plan", 100, "var(--fg)"], [scope.length - left.length, "solved", scope.length ? ((scope.length - left.length) / scope.length) * 100 : 0, "var(--grn)"], [left.length, "to go", scope.length ? (left.length / scope.length) * 100 : 0, "var(--warn)"], [perDay != null ? perDay.toFixed(1) : "–", solveDays ? `per solve day · ${solveDays} days` : "per solve day", perDay != null ? Math.min(100, (perDay / Math.max(planned, 1)) * 50) : 0, "var(--acc)"]] as [string | number, string, number, string][]).map(([v, l, w, c]) => (
                        <div key={l}><b>{v}</b><span>{l}</span><i style={{ "--w": `${w}%`, "--c": c } as CSSProperties} /></div>
                    ))}
                </div>
                <div className="cm-split" role="img" aria-label={`${bands[0]} easy, ${bands[1]} medium, ${bands[2]} hard to go`}>
                    <div className="split">{bands.map((n, i) => (n ? <i key={i} className={["e", "m", "h"][i]} style={{ flex: n }} /> : null))}</div>
                    <span><i className="e" />Easy <b>{bands[0]}</b></span>
                    <span><i className="m" />Medium <b>{bands[1]}</b></span>
                    <span><i className="h" />Hard <b>{bands[2]}</b></span>
                    <span className="r"><b>{left.filter((p) => p.companies.find((c) => c.name === company)?.recent).length}</b> recent</span>
                </div>
                <div className="cm-row"><span className="ly-lab">First up</span><span className="ly-sp" /><span className="cm-tp">most asked first · recent counts double</span></div>
                <ol className="cm-fus">
                    {(all ? firstUp : firstUp.slice(0, 10)).map((p, i) => (
                        <li key={p.id} style={{ "--i": Math.min(i, 14) } as CSSProperties}>
                            <span className="rk">{i + 1}</span>
                            <Link to="/d/$slug" params={{ slug: p.slug }}>{p.title}</Link>
                            <span className="fq" title="How often it is asked"><i style={{ width: `${(score(p, company ?? "") / top) * 100}%` }} /></span>
                            <span className={`cm-d ${p.difficulty}`}>{DIFF[p.difficulty][0]}</span>
                            <small>{p.companies.find((c) => c.name === company)?.recent ? "recent" : ""}</small>
                        </li>
                    ))}
                    {firstUp.length === 0 && <li className="rempty">You have done every problem in this plan.</li>}
                </ol>
                {firstUp.length > 10 && <button className="cm-btn" style={{ justifySelf: "start" }} onClick={() => setAll(!all)}>{all ? "Show the first 10" : `Show all ${firstUp.length}`}</button>}
            </section>
            {alike.length > 0 && (
                <Panel title="Companies that ask the same problems">
                    <div className="cm-alike">
                        {alike.map(([n, k], i) => (
                            <ListCard key={n} i={i} title={n} note={`${k} shared`} w={(k / Math.max(1, scope.length)) * 100} color="var(--vio)" onClick={() => { setCompany(n); setMade(null); const c = companies.find((x) => x.name === n); setRecent((c?.recent ?? 0) >= 8); setAll(false); }} />
                        ))}
                    </div>
                </Panel>
            )}
        </>
    );

    const right = (
        <>
            <Panel title="Your plan">
                {onCompanyPlan ? (
                    <p className="cm-now" role="status">Your plan is <b>{active.name}</b>{active.finish ? <>, finishing {niceDate(active.finish, plan.data!.today)}</> : null}{active.late ? <>, <b>{active.late}</b> late</> : null}. Next up follows it.</p>
                ) : (
                    <p>You are on your main plan. Making a company plan puts one beside it; your main plan is kept.</p>
                )}
                <div className="cm-acts">
                    <Link to="/dsa/calendar" className="cm-btn">See the calendar</Link>
                    {onCompanyPlan && <button className="cm-btn" disabled={back.isPending} onClick={() => back.mutate()}>Back to my main plan</button>}
                </div>
            </Panel>
            {chosen && (
                <HowFound
                    text={`Every catalog problem ${chosen.name} has asked${recent ? " in the last six months" : ""}, most asked first; the last six months count double. Premium problems follow your setting.`}
                    rows={[["Scanned", `${counted_} problems`], ["Ranked by", "frequency, recent ×2"], ["In the plan", scope.length], ["Already solved", scope.length - left.length]]}
                />
            )}
        </>
    );

    return (
        <>
            <Header area="dsa" />
            <div className="cm-3" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <Page3 crumb={[<Link to="/dsa">DSA</Link>, "Interview mode"]} title={<>Interview <span style={{ color: "var(--acc)" }}>mode.</span></>} sub="A plan of what one company asks, paced to your interview day, beside your main plan." tiles={tiles} left={leftPanel} center={center} right={right} label="Companies" />
            </div>
        </>
    );
}
