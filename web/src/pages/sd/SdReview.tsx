import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, type Grade, type SdCard } from "../../api";
import { HowFound, ListCard, Page3, Panel, Tile } from "../../components/layout";
import { SdMd } from "../../components/SdMd";
import { Skeleton } from "../../components/Skeleton";
import { SdFrame } from "./SdHome";

const GRADES: [Grade, string, string][] = [["again", "Again", "var(--bad)"], ["hard", "Hard", "var(--warn)"], ["good", "Good", "var(--grn)"], ["easy", "Easy", "var(--vio)"]];
const KIND: Record<string, string> = { point: "RUBRIC POINT", check: "CONCEPT CHECK" };

/** One card: say or write the answer, reveal the reference, grade yourself. */
function Card({ card, onGraded }: { card: SdCard; onGraded: () => void }) {
    const [text, setText] = useState("");
    const [shown, setShown] = useState(false);
    const grade = useMutation({ mutationFn: (g: Grade) => api.sdGrade(card.card_id, g), onSuccess: onGraded });
    const from = card.source.type === "problem" ? `${card.source.title} · ${card.source.step}` : card.source.title;
    return (
        <section className="ly-pn sd-art" key={card.card_id}>
            <span className="sd-k">{KIND[card.kind]} · {from.toUpperCase()}{card.level ? ` · ${card.level.toUpperCase()}` : ""}</span>
            <h2>{card.kind === "point" ? "Explain this as you would in the interview" : card.prompt}</h2>
            {card.kind === "point" && <p className="sd-lede">{card.prompt}</p>}
            <textarea value={text} onChange={(e) => setText(e.target.value)} placeholder="Your answer…" rows={4} />
            {!shown ? (
                <div className="sd-row"><button className="sd-go" onClick={() => setShown(true)}>Show the reference</button><span className="sd-dim">{card.reps ? `reviewed ${card.reps}×${card.lapses ? `, forgotten ${card.lapses}×` : ""}` : "first review"}</span></div>
            ) : (
                <>
                    <div className="sd-ref"><b>Reference</b><SdMd text={card.answer} /></div>
                    <div className="sd-grades">
                        {GRADES.map(([g, l, c]) => <button key={g} style={{ "--c": c } as React.CSSProperties} disabled={grade.isPending} onClick={() => grade.mutate(g)}><b>{l}</b></button>)}
                    </div>
                </>
            )}
            <div className="sd-row"><Link className="sd-ghost" to={card.source.type === "problem" ? "/sd/practice/$id" : "/sd/learn/$id"} params={{ id: card.source.id }}>Open {card.source.type === "problem" ? "the problem" : "the page"}</Link></div>
        </section>
    );
}

export function SdReview() {
    const qc = useQueryClient();
    const q = useQuery({ queryKey: ["sd-review"], queryFn: api.sdReview });
    const [done, setDone] = useState(0);
    const v = q.data;
    if (!v) return <SdFrame><Page3 crumb={["SYSTEM DESIGN", "REVIEW"]} title="Review" center={<Skeleton lines={6} />} /></SdFrame>;
    const card = v.due[0];
    const graded = () => {
        setDone((n) => n + 1);
        void qc.invalidateQueries({ queryKey: ["sd-review"] });
        void qc.invalidateQueries({ queryKey: ["sd"] });
    };
    const week = v.week.reduce((n, d) => n + d.n, 0);
    const left = (
        <Panel title={`Due · ${v.due.length}`}>
            <div className="ly-cards">
                {v.due.slice(0, 12).map((c, i) => <ListCard key={c.card_id} i={i} title={c.kind === "point" ? c.prompt : c.prompt} note={c.kind === "point" ? "point" : "check"} on={i === 0} text={c.source.title} color={c.kind === "point" ? "var(--warn)" : "var(--acc)"} />)}
                {v.due.length === 0 && <p className="sd-dim">Nothing due.</p>}
            </div>
        </Panel>
    );
    const center = card ? (
        <Card key={card.card_id} card={card} onGraded={graded} />
    ) : (
        <section className="ly-pn sd-art">
            <span className="sd-k">NOTHING DUE</span>
            <h2>{done ? `${done} reviewed. That's everything for today.` : "You're caught up."}</h2>
            <p className="sd-lede">{week ? `${week} card${week === 1 ? "" : "s"} come back in the next seven days.` : "Cards appear here when you mark a drill or a concept check."}</p>
            <div className="sd-row"><Link className="sd-go" to="/sd/problems">Practise a problem</Link><Link className="sd-ghost" to="/sd/learn">Read a page</Link></div>
        </section>
    );
    const right = (
        <>
            <Panel title="Next 7 days">
                <div className="sd-week7">{v.week.length ? v.week.map((d) => <span key={d.day}><b>{d.n}</b><em>{new Date(`${d.day}T00:00`).toLocaleDateString([], { weekday: "short" })}</em></span>) : <p className="sd-dim">None scheduled.</p>}</div>
            </Panel>
            <HowFound title="How review works" text="Every rubric point and concept check you mark becomes a card. Say or write the answer, compare with the reference, and grade yourself; FSRS schedules the next time, aiming for 90% recall." rows={[["Due", v.due.length], ["Done today", done]]} />
        </>
    );
    return (
        <SdFrame>
            <Page3 crumb={["SYSTEM DESIGN", "REVIEW"]} title="Review" sub="Missed rubric points and concept checks, coming back before you forget them." tiles={<><Tile em="Due" strong={v.due.length} hot={v.due.length > 0} color="var(--vio)" small={`${week} more this week`} /><Tile em="Reviewed" strong={done} small="this visit" /></>} left={left} center={center} right={right} label="Due" />
        </SdFrame>
    );
}
