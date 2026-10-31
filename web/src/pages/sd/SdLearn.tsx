import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type CSSProperties } from "react";
import { api, type SdMark } from "../../api";
import { HowFound, ListCard, Page3, Panel, Tile, PageLoading, LoadFailed } from "../../components/layout";
import { SdMd } from "../../components/SdMd";
import { SD_GROUPS, SdFrame } from "./SdHome";
import { ConceptReader } from "./SdRead";

/** Every concept page, by group. */
export function SdLearn() {
    const q = useQuery({ queryKey: ["sd"], queryFn: api.sd });
    const o = q.data;
    if (!o) return <SdFrame><PageLoading crumb={["SYSTEM DESIGN", "LEARN"]} title="Learn" /></SdFrame>;
    const groups = SD_GROUPS.map((g) => ({ ...g, items: o.concepts.filter((c) => c.group === g.id).sort((a, b) => a.order - b.order) }));
    const minutes = o.concepts.reduce((n, c) => n + c.minutes, 0);
    const left = (
        <Panel title="Groups">
            <div className="sd-list">
                {groups.map((g) => (
                    <a key={g.id} href={`#${g.id}`} className={g.items.length ? "" : "soon"}>
                        <i style={{ background: g.color }} />
                        {g.name}
                        <em>{g.items.length || "soon"}</em>
                    </a>
                ))}
            </div>
        </Panel>
    );
    const center = (
        <>
            {groups.filter((g) => g.items.length).map((g) => (
                <section key={g.id} id={g.id} className="sd-group">
                    <h2 className="sd-gh" style={{ "--c": g.color } as CSSProperties}>{g.name}<span>{g.blurb}</span></h2>
                    <div className="sd-cards">
                        {g.items.map((c, i) => (
                            <ListCard key={c.id} i={i} title={c.title} note={`${c.minutes} min`} w={c.checks ? (Math.min(c.checked, c.checks) / c.checks) * 100 : 0} color={g.color} text={c.lede} to={{ to: "/sd/learn/$id", params: { id: c.id } }} />
                        ))}
                    </div>
                </section>
            ))}
        </>
    );
    const right = <HowFound title="How the pages work" text="Each page says what the thing is, when to reach for it, how it works inside, what it costs, and one sentence to say in an interview; animated drawings show it moving. It ends with open checks: write your answer, compare, and mark yourself. Every mark becomes a review card." rows={[["Pages", o.concepts.length], ["Reading", `${Math.round(minutes / 6) / 10} h`]]} />;
    return (
        <SdFrame>
            <Page3 crumb={["SYSTEM DESIGN", "LEARN"]} title="Learn" sub="Concepts, patterns, technologies and how they work inside, each with open checks." tiles={<Tile em="Pages" strong={o.concepts.length} small={`about ${Math.round(minutes / 60)} hours of reading`} />} left={left} center={center} right={right} label="Groups" />
        </SdFrame>
    );
}

const MARKS: [SdMark, string][] = [["got", "Got it"], ["part", "Partly"], ["miss", "Missed"]];

/** One open check: write, compare with the answer, mark. */
function Check({ concept, index, prompt, answer, marked }: { concept: string; index: number; prompt: string; answer: string; marked: boolean }) {
    const qc = useQueryClient();
    const [text, setText] = useState("");
    const [open, setOpen] = useState(false);
    const [done, setDone] = useState<SdMark | null>(null);
    const mark = useMutation({
        mutationFn: (m: SdMark) => api.sdCheck(concept, index, m),
        onSuccess: (_r, m) => {
            setDone(m);
            void qc.invalidateQueries({ queryKey: ["sd"] });
            void qc.invalidateQueries({ queryKey: ["sd-concept", concept] });
            void qc.invalidateQueries({ queryKey: ["sd-review"] });
        },
    });
    return (
        <div className="sd-q">
            <b>{prompt}</b>
            <textarea value={text} onChange={(e) => setText(e.target.value)} placeholder="Write it as you would say it…" rows={3} />
            {!open ? (
                <div className="sd-row"><button className="sd-ghost" onClick={() => setOpen(true)}>Compare</button>{marked && <span className="sd-dim">marked before; it is in Review</span>}</div>
            ) : (
                <>
                    <div className="sd-ref"><b>A strong answer</b><SdMd text={answer} /></div>
                    <div className="sd-row">
                        <div className="sd-seg" role="group" aria-label="How did you do">
                            {MARKS.map(([m, l]) => <button key={m} className={done === m ? "on" : ""} data-m={m} disabled={mark.isPending} onClick={() => mark.mutate(m)}>{l}</button>)}
                        </div>
                        <span className="sd-dim">{done ? (done === "miss" ? "Back in Review today." : done === "part" ? "Back in two days." : "Back in a week.") : "mark yourself honestly"}</span>
                    </div>
                </>
            )}
        </div>
    );
}

/** A concept page: the reading view (SdRead.tsx) with the checks under the article. */
export function SdConceptPage({ id }: { id: string }) {
    const q = useQuery({ queryKey: ["sd-concept", id], queryFn: () => api.sdConcept(id) });
    const all = useQuery({ queryKey: ["sd"], queryFn: api.sd });
    const v = q.data;
    if (!v) return <SdFrame><PageLoading crumb={["SYSTEM DESIGN", "LEARN"]} title="Learn" error={q.isError ? <LoadFailed what="this page" missing={`There is no page called ${id}.`} back={{ to: "/sd/learn", label: "All system design pages" }} error={q.error} retry={() => void q.refetch()} /> : undefined} /></SdFrame>;
    const c = v.concept;
    const checks = (
        <section className="ly-pn sd-checks">
            <h5 className="ly-pnh"><span>Check yourself</span><span className="sd-dim">open answers, then compare</span></h5>
            {c.checks.map((k, i) => <Check key={i} concept={id} index={i} prompt={k.prompt} answer={k.answer} marked={v.marked.includes(`check:${id}:${i}`)} />)}
        </section>
    );
    return (
        <SdFrame>
            <ConceptReader key={id} id={id} v={v} all={all.data} checks={checks} />
        </SdFrame>
    );
}
