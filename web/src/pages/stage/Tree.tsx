// The stage page's left panel, in the mockup's markup (docs/mockups/course-motion.html, screen 2): the course ring, a filter, modules that
// open one by one with a progress bar, stage rows with a tick, number, length and difficulty, a switch for passed stages, and the next stage.

import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { api, coreStages, type CourseStagePage as Page } from "../../api";
import { getPref, setPref } from "../../prefs";
import { Resizer, type PanelsApi } from "../stagePanels";
import { Bars, STAGE_MINUTES } from "./shared";
import { useShowPlanned, withoutPlanned } from "../plannedModules";

export function Tree({ course, page, panels }: { course: string; page: Page; panels: PanelsApi }) {
    const overview = useQuery({ queryKey: ["course", course], queryFn: () => api.course(course) });
    const [showPlanned] = useShowPlanned();
    const o = withoutPlanned(overview.data, showPlanned);
    const here = page.module.code;
    const [open, setOpen] = useState<Set<string>>(() => new Set([here]));
    useEffect(() => {
        setOpen((prev) => (prev.has(here) ? prev : new Set(prev).add(here)));
    }, [here]);
    const [find, setFind] = useState("");
    const [showPassed, setShowPassed] = useState<boolean>(() => getPref("tree.passed", true));
    const needle = find.trim().toLowerCase();
    const pct = o && o.total ? Math.round((100 * o.done) / o.total) : 0;
    const { p: pp, update, drawer, setDrawer } = panels;
    const mods = (o?.projects ?? []).flatMap((p) => p.modules);
    const mine = mods.find((m) => m.code === here);
    // Bring the current stage into view in the tree (the tree scrolls, not the page).
    const list = useRef<HTMLDivElement>(null);
    useEffect(() => {
        const box = list.current;
        const leaf = box?.querySelector<HTMLElement>(".k-tl.k-cur");
        if (!box || !leaf) return;
        const top = leaf.getBoundingClientRect().top - box.getBoundingClientRect().top + box.scrollTop;
        box.scrollTop = Math.max(0, top - box.clientHeight / 2);
    }, [page.stage.id, o]);
    const toggle = (code: string) =>
        setOpen((prev) => {
            const next = new Set(prev);
            if (!next.delete(code)) next.add(code);
            return next;
        });
    const shown = mods
        .map((m) => ({ m, rows: m.stages.filter((s) => (showPassed || s.state === "todo" || s.id === page.stage.id) && (!needle || `${s.title} ${s.id}`.toLowerCase().includes(needle))) }))
        .filter((x) => x.rows.length > 0);
    return (
        <nav className={`k-tree${drawer === "l" ? " k-open" : ""}`} id="treeN" aria-label="Course">
            <Resizer side="l" panels={panels} />
            <div className="k-chead">
                <div className="k-cring" style={{ "--p": pct } as React.CSSProperties}>
                    <span>{pct}%</span>
                </div>
                <div style={{ flex: 1, minWidth: 0 }}>
                    <b>{page.course.title}</b>
                    <small>{o ? `${o.done} of ${o.total} stages passed` : ""}</small>
                </div>
                <button className="k-sbtn k-lt" id="lt" aria-label={pp.lc ? "Expand the course panel" : "Collapse the course panel"} title="Collapse (⌘B)" onClick={() => update({ ...pp, lc: !pp.lc })}>
                    «
                </button>
                <button className="k-drawer-x" aria-label="Close" onClick={() => setDrawer(null)}>
                    ×
                </button>
            </div>
            <label className="k-tsrch">
                <span>⌕</span>
                <input value={find} onChange={(e) => setFind(e.target.value)} placeholder="filter stages…" aria-label="Filter stages" autoComplete="off" spellCheck={false} onKeyDown={(e) => e.key === "Escape" && e.currentTarget.blur()} />
            </label>
            <div className="k-tlist" ref={list}>
                {shown.map(({ m, rows }) => {
                    const core = coreStages(m.stages);
                    const done = core.filter((s) => s.state !== "todo").length;
                    const isOpen = needle ? true : open.has(m.code);
                    return (
                        <section key={m.code} className={`k-tm${isOpen ? " k-open" : ""}`}>
                            <button className="k-tmh" aria-expanded={isOpen} onClick={() => !needle && toggle(m.code)}>
                                <span className="k-chev">›</span>
                                <span>
                                    <b>
                                        {m.code.toUpperCase()} · {m.title}
                                    </b>
                                    <div className="k-mini" style={{ width: "100%", marginTop: 5 }}>
                                        <i style={{ width: `${core.length ? Math.round((100 * done) / core.length) : 0}%` }} />
                                    </div>
                                </span>
                                <small>
                                    {done}/{core.length}
                                </small>
                            </button>
                            <div className="k-tmb">
                                <div inert={!isOpen}>
                                    {rows.map((s) => (
                                        <Link
                                            key={s.id}
                                            className={`k-tl${s.id === page.stage.id ? " k-cur" : ""}`}
                                            to="/courses/$course/$stage"
                                            params={{ course, stage: s.id }}
                                            title={s.title}
                                            aria-current={s.id === page.stage.id ? "step" : undefined}
                                            onClick={() => setDrawer(null)}
                                        >
                                            <span className={`k-ck${s.state !== "todo" ? " k-ok" : ""}`} title={s.state === "assisted" ? "passed with help" : undefined}>
                                                {s.state !== "todo" ? "✓" : ""}
                                            </span>
                                            <span className="k-no">{s.id.split("-")[1]}</span>
                                            <span>{s.title}</span>
                                            <span className="k-dm">
                                                {s.kind === "boss" ? "test" : `${STAGE_MINUTES[s.difficulty]}m`}
                                                <Bars d={s.difficulty} />
                                            </span>
                                        </Link>
                                    ))}
                                </div>
                            </div>
                        </section>
                    );
                })}
                {needle && shown.length === 0 && <p style={{ padding: "8px 16px", color: "var(--dim)", fontSize: 13 }}>No stage matches that.</p>}
            </div>
            <div className="k-rail" aria-label="This module">
                {mine?.stages.map((s) => (
                    <Link key={s.id} className={`k-tl${s.id === page.stage.id ? " k-cur" : ""}`} to="/courses/$course/$stage" params={{ course, stage: s.id }} title={`${s.id.split("-")[1]} · ${s.title}`} aria-label={s.title}>
                        <span className={`k-ck${s.state !== "todo" ? " k-ok" : ""}`}>{s.state !== "todo" ? "✓" : ""}</span>
                    </Link>
                ))}
            </div>
            <div className="k-tfoot">
                <div className="k-tswitch">
                    <button
                        className={`k-sw${showPassed ? " k-on" : ""}`}
                        role="switch"
                        aria-checked={showPassed}
                        aria-label="Show passed"
                        onClick={() => {
                            setShowPassed(!showPassed);
                            setPref("tree.passed", !showPassed);
                        }}
                    />
                    show passed stages
                </div>
                {page.next ? (
                    <Link className="k-cta k-sm k-fx" to="/courses/$course/$stage" params={{ course, stage: page.next.id }} onClick={() => setDrawer(null)}>
                        <span className="k-lbl">Next: {page.next.title}</span> <span className="k-ar">→</span>
                    </Link>
                ) : (
                    <Link className="k-cta k-sm k-fx" to="/courses/$course" params={{ course }}>
                        <span className="k-lbl">Back to the course</span>
                    </Link>
                )}
            </div>
        </nav>
    );
}
