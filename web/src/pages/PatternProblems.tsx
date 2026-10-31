import { Link } from "@tanstack/react-router";
import { GradeGlyph } from "../components/icons";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { api, type Band, type DsaProblem } from "../api";
import { Companies, Insight, Mark, ProblemTags, useLogger } from "../components/dsaBits";
import { PatternShell } from "../components/PatternShell";
import { DIFF, MINUTES, REVIEW_GRADES, counted, statusOf } from "../dsa";
import { Segmented } from "../components/ui";


/** A pattern's NeetCode problems, grouped by the technique they use, with the same log buttons as the home list. */
export function PatternProblems({ code }: { code: string }) {
    const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const o = overview.data;
    const { log, toast } = useLogger(o?.today);
    const [list, setList] = useState<"neetcode150" | "neetcode250" | "all">("neetcode150");
    const [show, setShow] = useState<"all" | "todo" | "due" | "solved">("all");
    const [diff, setDiff] = useState<Band[]>([]);
    const pattern = o?.patterns.find((p) => p.code.toLowerCase() === code.toLowerCase() || p.slug === code);
    const inList = o && pattern ? o.problems.filter((p) => p.pattern === pattern.code && counted(o, p) && (list === "all" || p.lists.includes(list as never))).sort((a, b) => a.order - b.order) : [];
    const keep = (p: DsaProblem) => (diff.length === 0 || diff.includes(p.difficulty)) && (show === "all" || (show === "solved" ? p.state.solved : show === "todo" ? !p.state.solved : !!o && statusOf(p, o.today) === "due"));
    const problems = inList.filter(keep);
    const groups = o ? o.techniques.filter((t) => t.pattern === pattern?.name).map((t) => ({ t, rows: problems.filter((p) => p.technique === t.id) })).filter((g) => g.rows.length) : [];
    const loose = problems.filter((p) => !groups.some((g) => g.rows.includes(p)));
    // premium problems are in the rows and the totals together, or in neither (Settings, or the filter on the DSA home)
    const totals = inList;
    const solvedN = totals.filter((p) => p.state.solved).length;
    const dueN = o ? totals.filter((p) => statusOf(p, o.today) === "due").length : 0;
    // one bar above the list: how far you are, then which problems are shown
    const filters = o && pattern && (
        <div className="ps-filt">
            <div className="ps-fsum"><b>{solvedN}</b> of {totals.length} solved<span className="ps-fbar"><i style={{ width: `${totals.length ? (solvedN / totals.length) * 100 : 0}%` }} /></span></div>
            <Segmented label="List" value={list} options={[["neetcode150", "NeetCode 150"], ["neetcode250", "NeetCode 250"], ["all", "All NeetCode"]] as const} onChange={setList} />
            <Segmented label="Status" value={show} options={[["all", "All"], ["todo", "To do"], ["due", `Due${dueN ? ` ${dueN}` : ""}`], ["solved", "Solved"]] as const} onChange={setShow} />
            <div className="ps-fchips" role="group" aria-label="Difficulty">
                {(["easy", "medium", "hard"] as Band[]).map((b) => (
                    <button key={b} className={`fchip${diff.includes(b) ? " on" : ""}`} aria-pressed={diff.includes(b)} onClick={() => setDiff((d) => (d.includes(b) ? d.filter((x) => x !== b) : [...d, b]))}>{DIFF[b][0]}</button>
                ))}
            </div>
            {(show !== "all" || diff.length > 0) && <span className="ps-fshown">{problems.length} of {totals.length} shown · <button className="ly-lk" onClick={() => { setShow("all"); setDiff([]); }}>show all</button></span>}
        </div>
    );
    return (
        <PatternShell code={code} tab="problems">
            {overview.isError && <p className="notice bad">Couldn't load the problems.</p>}
            {o && pattern && (
                <>
                    {filters}
                    {[...groups.map((g) => ({ key: g.t.id, name: g.t.name, rows: g.rows })), ...(loose.length ? [{ key: "other", name: "Other", rows: loose }] : [])].map((g) => (
                        <section key={g.key} className="d-tech">
                            <div className="d-th">
                                <h2>{g.name}</h2>
                                <span className="d-tcount">{g.rows.filter((p) => p.state.solved).length} / {g.rows.length}</span>
                            </div>
                            <div className="d-plist">
                                {g.rows.map((p) => <Row key={p.id} p={p} today={o.today} log={log} />)}
                            </div>
                        </section>
                    ))}
                    {problems.length === 0 && <p className="rempty">Nothing matches. <button className="ly-lk" onClick={() => { setShow("all"); setDiff([]); }}>Show all</button></p>}
                </>
            )}
            {toast}
        </PatternShell>
    );
}

function Row({ p, today, log }: { p: DsaProblem; today: string; log: ReturnType<typeof useLogger>["log"] }) {
    return (
        <article className="d-pcard d-prow">
            <Mark p={p} today={today} />
            <div className="d-pbody">
                <div className="d-ph">
                    <span className="d-pnum">#{p.number}</span>
                    <Link className="d-ptitle" to="/d/$slug" params={{ slug: p.slug }}>{p.title}</Link>
                </div>
                <Insight text={p.insight} />
                <div className="d-pmeta">
                    <ProblemTags p={p} />
                    <span>~{MINUTES[p.difficulty]}m</span>
                    <span>{p.tags.slice(0, 3).join(" · ")}</span>
                </div>
                <Companies companies={p.companies} limit={4} />
            </div>
            <div className="d-pside">
                <div className="d-acts">
                    {REVIEW_GRADES.map((g) => (
                        <button key={g.grade} className={g.cls} title={g.label} onClick={() => log(p, g.grade)}><GradeGlyph grade={g.grade} /></button>
                    ))}
                </div>
            </div>
        </article>
    );
}
