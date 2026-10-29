import { Link } from "@tanstack/react-router";
import { Skeleton } from "../components/Skeleton";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import { Header } from "../components/Header";
import { MockRoot, reducedMotion } from "../components/mock";
import { Prose } from "./stage/Tabs";

/** A concept: a short article that teaches one idea a stage needs, in the mockup's reading column and page panel. */
export function CourseConceptPage({ course, id }: { course: string; id: string }) {
    const q = useQuery({ queryKey: ["course-concept", course, id], queryFn: () => api.courseConcept(course, id) });
    const [active, setActive] = useState("");
    const p = q.data;
    const barRef = useRef<HTMLDivElement>(null);
    const outline = useRef<HTMLDivElement>(null);
    useEffect(() => {
        window.scrollTo({ top: 0 });
    }, [id]);
    useEffect(() => {
        if (!p) return;
        const ids = p.concept.sections.map((s) => `c-${s.id}`);
        const onScroll = () => {
            let cur = ids[0] ?? "";
            for (const i of ids) {
                const el = document.getElementById(i);
                if (el && el.getBoundingClientRect().top < 140) cur = i;
            }
            setActive(cur);
            const h = document.documentElement;
            if (barRef.current) barRef.current.style.width = `${(100 * Math.min(1, h.scrollTop / Math.max(1, h.scrollHeight - h.clientHeight)))}%`;
        };
        window.addEventListener("scroll", onScroll, { passive: true });
        onScroll();
        return () => window.removeEventListener("scroll", onScroll);
    }, [p]);
    // The marker in the outline slides to the section you are in.
    useLayoutEffect(() => {
        const mk = outline.current?.querySelector<HTMLElement>(".k-mk");
        const on = outline.current?.querySelector<HTMLElement>("a.k-on");
        if (!mk || !on) return;
        mk.style.transform = `translateY(${on.offsetTop}px)`;
        mk.style.height = `${on.offsetHeight}px`;
    });
    if (q.isError || !p) {
        return (
            <>
                <Header area="courses" stats={false} />
                <main className="page">
                    <div className="wrap">
                        {q.isError ? <p className="notice bad">{`Couldn't load ${id}: ${(q.error as Error).message}`}</p> : <Skeleton lines={6} />}
                    </div>
                </main>
            </>
        );
    }
    const first = p.used_in[0];
    return (
        <>
            <Header area="courses" stats={false} />
            <MockRoot>
                <div className="k-read" ref={barRef} aria-hidden="true" />
                <div className="k-concept">
                    <main className="k-read2">
                        <div className="k-eye k-rv" style={{ "--i": 0 } as React.CSSProperties}>
                            <Link to="/courses/$course" params={{ course }} style={{ color: "var(--grn)" }}>
                                <b>COURSE</b>
                            </Link>{" "}
                            / CONCEPT / ~{p.concept.minutes} MIN READ{first && <> / USED IN {first.id.toUpperCase()}</>}
                        </div>
                        <h2 className="k-ttl k-rv" style={{ "--i": 1 } as React.CSSProperties}>
                            {p.concept.title}
                        </h2>
                        <p className="k-lead k-rv" style={{ "--i": 2 } as React.CSSProperties}>
                            {p.concept.summary}
                        </p>
                        {p.concept.sections.map((s, i) => (
                            <section key={s.id} id={`c-${s.id}`}>
                                <div className="k-sh">
                                    <b>{String(i + 1).padStart(2, "0")}</b>
                                    {s.title.toUpperCase()}
                                </div>
                                <Prose text={s.md} />
                            </section>
                        ))}
                        {p.used_in.length > 0 && (
                            <>
                                <div className="k-sh" style={{ marginTop: 56 }}>
                                    <b>BACK TO WORK</b>
                                </div>
                                <nav className="k-pn" style={{ marginTop: 12, gridTemplateColumns: "repeat(auto-fill,minmax(260px,1fr))" }}>
                                    {p.used_in.map((s) => (
                                        <Link key={s.id} to="/courses/$course/$stage" params={{ course, stage: s.id }}>
                                            <small>
                                                STAGE {s.rank} · {s.module.toUpperCase()}
                                            </small>
                                            {s.title}
                                        </Link>
                                    ))}
                                </nav>
                            </>
                        )}
                    </main>
                    <aside className="k-toc k-concept-toc" aria-label="This concept">
                        <div className="k-tscroll">
                            <div className="k-rcard">
                                <h6>IN THIS CONCEPT</h6>
                                <div ref={outline} style={{ position: "relative", borderLeft: "1px solid var(--line2)" }}>
                                    <span className="k-mk" />
                                    {p.concept.sections.map((s) => (
                                        <a
                                            key={s.id}
                                            href={`#c-${s.id}`}
                                            className={active === `c-${s.id}` ? "k-on" : ""}
                                            onClick={(e) => {
                                                e.preventDefault();
                                                document.getElementById(`c-${s.id}`)?.scrollIntoView({ behavior: reducedMotion() ? "auto" : "smooth", block: "start" });
                                            }}
                                        >
                                            {s.title}
                                        </a>
                                    ))}
                                </div>
                            </div>
                        </div>
                    </aside>
                </div>
            </MockRoot>
        </>
    );
}
