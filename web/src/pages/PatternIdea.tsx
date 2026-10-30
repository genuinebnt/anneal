import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import type { CSSProperties, ReactNode } from "react";
import { api, type PatternIdea as Idea } from "../api";
import { IdeaDemo } from "../components/IdeaDemo";
import { IdeaGlyph } from "../components/IdeaGlyph";
import { PatternShell } from "../components/PatternShell";
import { rich } from "../components/rich";

/** How a cost reads at a glance: constant and logarithmic are good, a product of the input sizes is poor, the rest sits between. */
const tone = (c: string) => (/^O\(1\)$|log/.test(c) ? "good" : /n\s*[·*]\s*k|n²|n\^2|2\^|!/.test(c) ? "bad" : "mid");

const anchor = (id: string) => `t-${id.split(":")[1] ?? id}`;

function Section({ title, i, children, wide }: { title: ReactNode; i: number; children: ReactNode; wide?: boolean }) {
    return (
        <section className={`id-card${wide ? " wide" : ""}`} style={{ "--i": i } as CSSProperties}>
            <h2>{title}</h2>
            {children}
        </section>
    );
}

/** A pattern's Idea tab (docs/DSA_IDEA_TAB.md): what the pattern is, a step-through, where it applies, how to tell it is this one and which
 *  variation, what it costs, how it meets the other patterns, and how to apply it. All the words come from `content/dsa/ideas`. */
export function PatternIdea({ code }: { code: string }) {
    const lessons = useQuery({ queryKey: ["patterns", code], queryFn: () => api.patternLessons(code) });
    const d = lessons.data;
    const idea = d?.idea;
    return (
        <PatternShell code={code} tab="idea">
            {lessons.isError && <p className="notice bad">Couldn't load this page: {(lessons.error as Error).message}</p>}
            {d && !idea && (
                <p className="notice">
                    There is no Idea tab for {d.pattern} yet. <Link to="/dsa/patterns/$code" params={{ code: d.code }}>Open the techniques</Link>
                </p>
            )}
            {d && idea && <Body idea={idea} code={d.code} />}
        </PatternShell>
    );
}

function Body({ idea, code }: { idea: Idea; code: string }) {
    return (
        <div className="id-wrap">
            <Section title="WHAT IT IS" i={0}>
                {idea.what.map((t, n) => <p key={n}>{rich(t)}</p>)}
            </Section>

            <Section title="WATCH IT RUN" i={1} wide>
                <IdeaDemo names={idea.demos} />
            </Section>

            <div className="id-cols">
                <Section title="WHERE IT APPLIES" i={2}>
                    <ul>{idea.applies.map((t, n) => <li key={n}>{rich(t)}</li>)}</ul>
                </Section>
                <Section title={`IS IT ${idea.pattern.toUpperCase()}?`} i={3}>
                    <ul className="id-yn">
                        {idea.yes.map((t, n) => <li key={`y${n}`}><span className="id-yes">YES</span><span>{rich(t)}</span></li>)}
                        {idea.no.map((t, n) => <li key={`n${n}`}><span className="id-no">NO</span><span>{rich(t)}</span></li>)}
                    </ul>
                </Section>
            </div>

            <Section title={`WHICH ${idea.pattern.toUpperCase()}?`} i={4}>
                <div className="id-tbl">
                    <table>
                        <thead><tr><th>THE QUESTION SAYS</th><th /><th>KIND</th><th>THE MOVE</th><th /></tr></thead>
                        <tbody>
                            {idea.which.map((w) => (
                                <tr key={w.technique}>
                                    <td>{rich(w.says)}</td>
                                    <td className="id-gl"><IdeaGlyph kind={w.glyph} /></td>
                                    <td><b>{w.kind}</b></td>
                                    <td>{rich(w.move)}</td>
                                    <td><Link className="id-go" to="/dsa/patterns/$code" params={{ code }} hash={anchor(w.technique)}>See the technique →</Link></td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            </Section>

            <Section title="TIME AND SPACE" i={5}>
                <div className="id-tbl">
                    <table className="id-cost">
                        <thead><tr><th>VARIATION</th><th>TIME</th><th>SPACE</th><th>WHY</th></tr></thead>
                        <tbody>
                            {idea.cost.map((c) => (
                                <tr key={c.name}>
                                    <td><b>{c.name}</b></td>
                                    <td><code className={`id-cx id-${tone(c.time)}`}>{c.time}</code></td>
                                    <td><code className={`id-cx id-${tone(c.space)}`}>{c.space}</code></td>
                                    <td>{rich(c.why)}</td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            </Section>

            {idea.with.length > 0 && (
                <Section title="WITH OTHER PATTERNS" i={6}>
                    <div className="id-with">
                        {idea.with.map((w) => (
                            <div key={w.pattern} className="id-w">
                                {w.code ? (
                                    <Link className="id-who" to={w.has_idea ? "/dsa/patterns/$code/idea" : "/dsa/patterns/$code"} params={{ code: w.code }}>{w.pattern} →</Link>
                                ) : <span className="id-who">{w.pattern}</span>}
                                <p>{rich(w.how)}</p>
                                <Link className="id-ex" to="/d/$slug" params={{ slug: w.example.slug }}>
                                    {w.example.number != null && <small>{w.example.number}</small>}{w.example.title ?? w.example.slug}
                                </Link>
                            </div>
                        ))}
                    </div>
                </Section>
            )}

            <Section title="HOW TO APPLY IT" i={7}>
                <ol className="id-steps">{idea.apply.map((t, n) => <li key={n}><span>{rich(t)}</span></li>)}</ol>
            </Section>

            <div className="id-next" style={{ "--i": 8 } as CSSProperties}>
                <Link to="/dsa/patterns/$code" params={{ code }}>Open the techniques <span>→</span></Link>
                <Link to="/dsa/patterns/$code/problems" params={{ code }}>Go to the problems <span>→</span></Link>
            </div>
        </div>
    );
}
