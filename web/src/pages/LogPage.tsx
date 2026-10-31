import { useQuery } from "@tanstack/react-query";
import { useState, type CSSProperties } from "react";
import { Link } from "@tanstack/react-router";
import { api, type Grade } from "../api";
import { Header } from "../components/Header";
import { Page3, Panel, Tile } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { useLogger } from "../components/dsaBits";
import { DIFF } from "../dsa";

const OUTCOMES: { grade: Grade; label: string }[] = [
    { grade: "good", label: "On my own" },
    { grade: "hard", label: "With help" },
    { grade: "again", label: "Attempted" },
    { grade: "easy", label: "Instant" },
];

/** A one-screen way to log a problem from a phone: type part of the name, tap how it went. Install it to the home screen from the browser menu. */
export function LogPage() {
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const o = dsa.data;
    const { log, toast, busy } = useLogger(o?.today);
    const [q, setQ] = useState("");
    const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const next = (o?.plan.next_up ?? []).slice(0, 3).map((id) => o?.problems.find((p) => p.id === id)).filter((p) => !!p);
    const found = words.length ? (o?.problems ?? []).filter((p) => words.every((w) => `${p.title} ${p.number}`.toLowerCase().includes(w))).slice(0, 8) : next;
    const today = o?.today ?? "";
    const loggedToday = (o?.problems ?? []).filter((p) => !!today && p.state.last_review?.slice(0, 10) === today);
    const target = o?.plan.solve_day ? o.settings.new_per_day : 0;
    const tiles = o && (
        <>
            <Tile em="Today" strong={<>{loggedToday.length}{target ? <span>/{target}</span> : null}</>} small={target ? "new problems planned" : "no new problems planned"} />
            <Tile em="Due" strong={o.plan.due} hot={o.plan.due > 0} color="var(--vio)" small="reviews" to={{ to: "/dsa/review" }} />
        </>
    );
    const center = (
        <section className="ly-pn lg-wrap">
            <span className="ly-lab">Quick log</span>
            <input id="lg-q" className="lg-q" type="search" placeholder="Name or number, e.g. two sum" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Find a problem" autoComplete="off" autoFocus />
            {!o ? (
                <Skeleton lines={4} />
            ) : (
                <>
                    <span className="lg-lab">{words.length ? `${found.length} match${found.length === 1 ? "" : "es"}` : "UP NEXT"}</span>
                    {found.map((p) => p && (
                        <div key={p.id} className="lg-row">
                            <div className="lg-t">
                                <Link to="/d/$slug" params={{ slug: p.slug }}>{p.title}</Link>
                                <small>#{p.number} · {DIFF[p.difficulty][0]}{p.premium ? " · premium" : ""}{p.state.last_grade ? " · logged" : ""}</small>
                            </div>
                            <div className="lg-btns">
                                {OUTCOMES.map((g) => (
                                    <button key={g.grade} className={`lg-b ${g.grade}`} disabled={busy} onClick={() => log(p, g.grade)}>{g.label}</button>
                                ))}
                            </div>
                        </div>
                    ))}
                    {words.length > 0 && found.length === 0 && <p className="rempty">No problem matches. Try the number.</p>}
                </>
            )}
            <p className="lg-tip">Add this page to your phone's home screen from the browser menu; it opens straight here.</p>
        </section>
    );
    const right = (
        <Panel title="Logged today" aside={<span className="mk-cnt">{loggedToday.length}</span>}>
            {loggedToday.length === 0 ? <p>Nothing yet today.</p> : (
                <div className="lg-today">
                    {loggedToday.map((p) => (
                        <Link key={p.id} to="/d/$slug" params={{ slug: p.slug }}>
                            <i className={p.state.last_grade ?? ""} />
                            <span>{p.title}</span>
                            <small>{{ good: "on my own", hard: "with help", again: "attempted", easy: "instant" }[p.state.last_grade ?? "good"]}</small>
                        </Link>
                    ))}
                </div>
            )}
        </Panel>
    );
    return (
        <>
            <Header area="dsa" />
            <div className="lg-3" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <Page3 crumb={["Quick log"]} title="Log a problem" sub="Type part of the name, tap how it went." tiles={tiles} center={center} right={right} />
            </div>
            {toast}
        </>
    );
}
