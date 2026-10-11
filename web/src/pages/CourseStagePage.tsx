// A stage of the course, built from the mockup's own components (docs/mockups/course-motion.html, screen 2): the course tree on the left, the
// reading column with its tabs in the middle, the page panel and focus timer on the right, a run strip along the bottom, a reading-progress
// bar across the top, a condensed header and a back-to-top ring that appear as you scroll.

import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api, type CourseStagePage as Page, type CourseRun } from "../api";
import { Header } from "../components/Header";
import { Celebration, celebrateOff } from "../components/kit";
import { FocusTimer } from "../components/FocusTimer";
import { MockCopy, MockRoot, reducedMotion } from "../components/mock";
import { toast } from "../components/toasts";
import { getPref, setPref } from "../prefs";
import { Resizer, usePanels } from "./stagePanels";
import { ConceptsTab, HintsTab, Prose, RunTab, SolutionTab } from "./stage/Tabs";
import { Tree } from "./stage/Tree";
import { DIFFICULTY_LABEL, ago } from "./stage/shared";

const LIMIT = "Time limit exceeded";

/** A run the CLI stopped for taking too long: the whole run, or each test that never finished, says "Time limit exceeded". */
const hitLimit = (r: CourseRun | null | undefined): boolean => !!r && !r.ok && ((r.problem?.includes(LIMIT) ?? false) || r.tests.some((t) => !t.ok && t.detail.startsWith(LIMIT)));

type Tab = "instructions" | "hints" | "solution" | "concepts" | "run";
const TAB_IDS: Tab[] = ["instructions", "hints", "solution", "concepts", "run"];

/** Sections that go beyond what passing the stage needs; they can be hidden. */
const OPTIONAL_SECTIONS = new Set(["performance", "learn-more", "experiment", "other-designs"]);

/** Splits markdown at its `### ` headings (not inside code fences): [title | null, body]. */
function blocks(text: string): [string | null, string][] {
    const out: [string | null, string][] = [[null, ""]];
    const add = (line: string) => {
        const last = out[out.length - 1];
        if (last) last[1] += line + "\n";
    };
    let fence = false;
    for (const line of text.split("\n")) {
        if (line.trimStart().startsWith("```")) fence = !fence;
        if (!fence && line.startsWith("### ")) {
            out.push([line.slice(4).trim(), ""]);
            continue;
        }
        add(line);
    }
    return out.filter(([t, b]) => t !== null || b.trim());
}

function TaskBox({ stageId, text }: { stageId: string; text: string }) {
    return (
        <div className="k-task">
            <div className="k-th">
                YOUR TURN<span>run  anneal course test {stageId}</span>
            </div>
            <div className="k-tb">
                <Prose text={text} />
            </div>
        </div>
    );
}

/** A stage part or section: prose, with "The task" lifted into the highlighted Your turn box. */
function Body({ text, stageId }: { text: string; stageId: string }) {
    return (
        <>
            {blocks(text).map(([title, body], i) =>
                title === "The task" ? (
                    <TaskBox key={i} stageId={stageId} text={body} />
                ) : (
                    <Fragment key={i}>
                        {title && <h3 className="k-h3">{title}</h3>}
                        <Prose text={body} />
                    </Fragment>
                ),
            )}
        </>
    );
}

export function CourseStagePage({ course, stage }: { course: string; stage: string }) {
    const qc = useQueryClient();
    const nav = useNavigate();
    const panels = usePanels();
    const key = ["course-stage", course, stage];
    // Poll while the page is open and visible: a push in the terminal shows up here within a few seconds.
    const q = useQuery({ queryKey: key, queryFn: () => api.courseStage(course, stage), refetchInterval: 3000 });
    const set = (p: Page) => {
        qc.setQueryData(key, p);
        qc.invalidateQueries({ queryKey: ["course", course] });
    };
    const hint = useMutation({ mutationFn: () => api.revealCourseHint(course, stage), onSuccess: set });
    const sol = useMutation({
        mutationFn: () => api.revealCourseSolution(course, stage),
        onSuccess: (p) => {
            set(p);
            toast("ok", "Solution opened", "This stage will count as assisted.");
        },
    });
    const p = q.data;
    const [active, setActive] = useState("s-top");
    // The tests-passed popup (unless "don't show again" is on, then a toast says it).
    const [win, setWin] = useState(false);
    // Sidenotes written in the text (`^[...]`): shown in the page panel, numbered in the order they appear.
    const [notes, setNotes] = useState<{ n: number; html: string; code: boolean }[]>([]);
    const [selNote, setSelNote] = useState<number | null>(null);
    const [curNotes, setCurNotes] = useState<number[]>([]);
    // Optional sections: hidden or shown for all stages (remembered); each one opens on its own.
    const [optHidden, setOptHidden] = useState<boolean>(() => getPref("optional.hidden", false));
    const [flipped, setFlipped] = useState<Set<string>>(() => new Set());
    const [openOpt, setOpenOpt] = useState<Set<string>>(() => new Set());
    const [strip, setStrip] = useState(false);
    const [time, setTime] = useState("25:00");
    const seenRun = useRef<number | null>(null);
    const tabFromHash = (): Tab => TAB_IDS.find((x) => x === location.hash.slice(1)) ?? "instructions";
    const [tab, setTab] = useState<Tab>(tabFromHash);
    // Each tab keeps its own scroll position, so going to Hints and back returns to where you were reading.
    const scrolls = useRef<Partial<Record<Tab, number>>>({});
    const restore = useRef<number | null>(null);
    const pick = (k: Tab) => {
        scrolls.current[tab] = window.scrollY;
        restore.current = scrolls.current[k] ?? 0;
        setTab(k);
        // A hash on every tab: dropping it makes the router treat the change as a new page and scroll to the top.
        history.replaceState(null, "", `#${k}`);
    };
    // After the new tab is in the page: scrolling earlier would be clamped to the height of the tab being left.
    useLayoutEffect(() => {
        if (restore.current === null) return;
        window.scrollTo({ top: restore.current });
        restore.current = null;
    }, [tab]);
    useEffect(() => {
        window.scrollTo({ top: 0 });
    }, [stage]);

    // The sliding underline under the active tab.
    const tabsRef = useRef<HTMLDivElement>(null);
    const placeUnderline = () => {
        const box = tabsRef.current;
        const on = box?.querySelector<HTMLElement>("button.k-on");
        const ul = box?.querySelector<HTMLElement>(".k-ul");
        if (!on || !ul || !on.offsetWidth) return;
        ul.style.width = `${on.offsetWidth}px`;
        ul.style.transform = `translateX(${on.offsetLeft}px)`;
    };
    useLayoutEffect(placeUnderline);
    useEffect(() => {
        addEventListener("resize", placeUnderline);
        // The panels slide for a moment when they open or close and the column changes width with them.
        const t = setTimeout(placeUnderline, 450);
        document.fonts?.ready.then(placeUnderline);
        return () => {
            removeEventListener("resize", placeUnderline);
            clearTimeout(t);
        };
    }, [panels.p.lc, panels.p.rc, !!p]); // eslint-disable-line react-hooks/exhaustive-deps

    // A run that arrives while the page is open (or one from the last half minute) pops a result: "Tests passed, proceed" or the count.
    const lastRun = p?.last_run ?? null;
    useEffect(() => {
        if (!lastRun || !p) return;
        const first = seenRun.current === null;
        if (seenRun.current === lastRun.id) return;
        seenRun.current = lastRun.id;
        const fresh = Date.now() - new Date(lastRun.at).getTime() < 30_000;
        if (first && !fresh) return;
        qc.invalidateQueries({ queryKey: ["course", course] });
        if (lastRun.ok && !celebrateOff()) {
            setWin(true);
            return;
        }
        if (lastRun.ok) {
            toast("ok", "Tests passed", p.next ? "Moving on is one click away." : "That was the last stage.", p.next ? { label: `PROCEED TO ${p.next.title.toUpperCase()} →`, run: () => nav({ to: "/courses/$course/$stage", params: { course, stage: p.next!.id } }) } : undefined);
        } else {
            toast("er", hitLimit(lastRun) ? "Time limit exceeded" : lastRun.problem ? "The tests did not run" : `${lastRun.passed} of ${lastRun.total} passing`, undefined, { label: "SEE DETAILS →", run: () => pick("run") });
        }
    }, [lastRun?.id]); // eslint-disable-line react-hooks/exhaustive-deps
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement | null)?.closest("input, textarea, [contenteditable]")) return;
            const to = e.key === "[" ? p?.prev : e.key === "]" ? p?.next : null;
            if (to) nav({ to: "/courses/$course/$stage", params: { course, stage: to.id } });
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [p, course, nav]);

    // What the page lists, in order: the stage's sections.
    const entries = useMemo(() => {
        if (!p) return [];
        return p.stage.sections.map((s) => ({ id: `sec-${s.id}`, title: s.title, optional: OPTIONAL_SECTIONS.has(s.id), md: s.md }));
    }, [p]);
    const ids = ["s-top", ...entries.map((e) => e.id)];
    const idsKey = ids.join();

    // Which section the outline marks, and the marker that slides to it.
    useEffect(() => {
        const onScroll = () => {
            let cur = ids[0] ?? "s-top";
            for (const id of ids) {
                const el = document.getElementById(id);
                if (el && el.getBoundingClientRect().top < 140) cur = id;
            }
            setActive(cur);
        };
        window.addEventListener("scroll", onScroll, { passive: true });
        onScroll();
        return () => window.removeEventListener("scroll", onScroll);
    }, [idsKey, tab]); // eslint-disable-line react-hooks/exhaustive-deps
    const outline = useRef<HTMLDivElement>(null);
    useLayoutEffect(() => {
        const box = outline.current;
        const mk = box?.querySelector<HTMLElement>(".k-mk");
        const on = box?.querySelector<HTMLElement>("a.k-on");
        if (!mk || !on) return;
        mk.style.transform = `translateY(${on.offsetTop}px)`;
        mk.style.height = `${on.offsetHeight}px`;
    });

    // The reading-progress bar, the condensed header and the back-to-top ring follow the scroll (written straight to the elements).
    const barRef = useRef<HTMLDivElement>(null);
    const pheadRef = useRef<HTMLDivElement>(null);
    const pctRef = useRef<HTMLSpanElement>(null);
    const topRef = useRef<HTMLButtonElement>(null);
    const ringRef = useRef<SVGCircleElement>(null);
    useEffect(() => {
        const onScroll = () => {
            const h = document.documentElement;
            const f = Math.min(1, h.scrollTop / Math.max(1, h.scrollHeight - h.clientHeight));
            if (barRef.current) barRef.current.style.width = `${100 * f}%`;
            if (pctRef.current) pctRef.current.textContent = `${Math.round(100 * f)}% read`;
            pheadRef.current?.classList.toggle("k-show", h.scrollTop > 220 && h.scrollHeight - h.clientHeight > 300);
            topRef.current?.classList.toggle("k-show", h.scrollTop > 500);
            if (ringRef.current) ringRef.current.style.strokeDashoffset = String(126 * (1 - f));
        };
        window.addEventListener("scroll", onScroll, { passive: true });
        onScroll();
        return () => window.removeEventListener("scroll", onScroll);
    }, [!!p]); // eslint-disable-line react-hooks/exhaustive-deps

    // Number the sidenote markers on the page and collect their text for the panel.
    useEffect(() => {
        const found: { n: number; html: string; code: boolean }[] = [];
        let n = 0;
        document.querySelectorAll<HTMLElement>(".k-read2 .k-snr").forEach((row) => {
            const marks = [...row.querySelectorAll<HTMLElement>(".k-snm")];
            const asides = [...row.querySelectorAll<HTMLElement>(".k-sn")];
            marks.forEach((m, i) => {
                n++;
                m.textContent = String(n);
                m.dataset.sn = String(n);
                const a = asides[i];
                if (!a) return;
                a.dataset.sn = String(n);
                const b = a.querySelector("b");
                if (b) b.textContent = String(n);
                const body = a.cloneNode(true) as HTMLElement;
                body.querySelector("b")?.remove();
                found.push({ n, html: body.innerHTML, code: !!body.querySelector("pre") });
            });
        });
        setNotes((prev) => (JSON.stringify(prev) === JSON.stringify(found) ? prev : found));
    });
    const panelOn = () => !panels.p.rc && innerWidth > 1100;
    // A marker opens its note in the panel, or under the paragraph when the panel is not on screen.
    useEffect(() => {
        const open = (m: HTMLElement) => {
            const num = Number(m.dataset.sn);
            if (panelOn()) {
                setSelNote((cur) => (cur === num ? null : num));
                document.querySelector(`.k-pnote[data-sn="${num}"]`)?.scrollIntoView({ block: "nearest", behavior: reducedMotion() ? "auto" : "smooth" });
            } else {
                const on = m.closest(".k-snr")?.querySelector(`.k-sn[data-sn="${num}"]`)?.classList.toggle("k-open");
                m.setAttribute("aria-expanded", String(!!on));
            }
        };
        const onClick = (e: MouseEvent) => {
            const m = (e.target as HTMLElement).closest<HTMLElement>(".k-snm");
            if (m) open(m);
        };
        const onKey = (e: KeyboardEvent) => {
            const m = (e.target as HTMLElement).closest<HTMLElement>(".k-snm");
            if (m && (e.key === "Enter" || e.key === " ")) (e.preventDefault(), open(m));
        };
        const over = (e: MouseEvent) => {
            const m = (e.target as HTMLElement).closest<HTMLElement>(".k-snm");
            if (m) document.querySelector(`.k-pnote[data-sn="${m.dataset.sn}"]`)?.classList.add("k-sel");
        };
        const out = (e: MouseEvent) => {
            const m = (e.target as HTMLElement).closest<HTMLElement>(".k-snm");
            if (m) document.querySelector(`.k-pnote[data-sn="${m.dataset.sn}"]`)?.classList.remove("k-sel");
        };
        document.addEventListener("click", onClick);
        document.addEventListener("keydown", onKey);
        document.addEventListener("mouseover", over);
        document.addEventListener("mouseout", out);
        return () => {
            document.removeEventListener("click", onClick);
            document.removeEventListener("keydown", onKey);
            document.removeEventListener("mouseover", over);
            document.removeEventListener("mouseout", out);
        };
    }, [panels.p.rc]); // eslint-disable-line react-hooks/exhaustive-deps
    // The notes of the paragraph you are reading stand out in the panel.
    useEffect(() => {
        if (notes.length === 0) return;
        let last = "";
        const onScroll = () => {
            const rows = [...document.querySelectorAll<HTMLElement>(".k-read2 .k-snr")];
            let cur: HTMLElement | undefined;
            for (const r of rows) {
                const b = r.getBoundingClientRect();
                if (b.top < 230 && b.bottom > 110) cur = r;
            }
            cur ??= rows.find((r) => r.getBoundingClientRect().top >= 230);
            const nums = cur ? [...cur.querySelectorAll<HTMLElement>(".k-snm")].map((m) => Number(m.dataset.sn)) : [];
            if (nums.join() === last) return;
            last = nums.join();
            setCurNotes(nums);
            const f = nums[0];
            if (f && panelOn()) document.querySelector(`.k-pnote[data-sn="${f}"]`)?.scrollIntoView({ block: "nearest", behavior: reducedMotion() ? "auto" : "smooth" });
        };
        window.addEventListener("scroll", onScroll, { passive: true });
        const t = setTimeout(onScroll, 100);
        return () => {
            window.removeEventListener("scroll", onScroll);
            clearTimeout(t);
        };
    }, [notes.length, tab]); // eslint-disable-line react-hooks/exhaustive-deps

    if (q.isError) {
        return (
            <>
                <Header area="courses" stats={false} />
                <main className="page">
                    <div className="wrap">
                        <p className="notice bad">Couldn't load {stage}: {(q.error as Error).message}</p>
                    </div>
                </main>
            </>
        );
    }
    if (!p) {
        return (
            <>
                <Header area="courses" stats={false} />
                <main className="page">
                    <div className="wrap">
                        <p className="notice">Loading…</p>
                    </div>
                </main>
            </>
        );
    }
    const inModule = p.module.stages.findIndex((s) => s.id === p.stage.id);
    const run = p.last_run;
    // a run the CLI has started and not reported yet (it ends with the report)
    const running = p.running ?? null;
    const cmd = `anneal course test ${p.stage.id}`;
    const reqConcepts = p.concepts.filter((k) => k.required);
    const reqDone = reqConcepts.filter((k) => k.read).length;
    const limit = hitLimit(run);
    // a test that hit the time limit comes first: it is the one to look at, and the others may only have been waiting behind it
    const failedFirst = run?.tests.find((t) => !t.ok && t.detail.startsWith(LIMIT)) ?? run?.tests.find((t) => !t.ok);
    const outlineEntries: [string, string][] = [["s-top", "Overview"], ...entries.map((e): [string, string] => [e.id, e.title])];
    const tabs: [Tab, string, string][] = [
        ["instructions", "Instructions", ""],
        ["hints", "Hints", p.hints.total ? `${p.hints.revealed.length}/${p.hints.total}` : ""],
        ["solution", "Solution", ""],
        ["concepts", "Concepts", reqConcepts.length ? `${reqDone}/${reqConcepts.length}` : p.concepts.length ? String(p.concepts.length) : ""],
        ["run", "Last run", run ? (run.problem ? "—" : `${run.passed}/${run.total}`) : ""],
    ];
    const hidden = (id: string) => optHidden && !flipped.has(id);
    const flip = (id: string) =>
        setFlipped((f) => {
            const n = new Set(f);
            if (!n.delete(id)) n.add(id);
            return n;
        });
    const copyCmd = () => {
        try {
            navigator.clipboard.writeText(cmd).then(() => toast("ok", "Command copied: run it in your repo", "This page is listening and shows the run as it happens"), () => toast("in", "Run it in your repo", cmd));
        } catch {
            toast("in", "Run it in your repo", cmd);
        }
    };
    let number = 0;
    const part = (title: string) => /^Part (\d+) · (.*)$/.exec(title);
    return (
        <>
            <Header area="courses" stats={false} />
            <MockRoot>
                <div className="k-read" ref={barRef} aria-hidden="true" />
                <div className="k-phead" ref={pheadRef}>
                    <b>{p.stage.title}</b>
                    <span>
                        {p.module.code.toUpperCase()} · stage {inModule + 1} of {p.module.stages.length}
                    </span>
                    <span className="k-sp" />
                    <span className="k-pct" ref={pctRef}>0% read</span>
                    <button className="k-cta k-sm k-fx" onClick={copyCmd} title="Copies the command; run it in your repo and this page shows the result">
                        <span className="k-lbl">Copy test command</span>
                    </button>
                </div>
                <div className="k-mbar" id="mbar">
                    <button onClick={() => panels.setDrawer(panels.drawer === "l" ? null : "l")} aria-expanded={panels.drawer === "l"}>
                        ☰ Stages <b>{inModule + 1}/{p.module.stages.length}</b>
                    </button>
                    <button onClick={() => panels.setDrawer(panels.drawer === "r" ? null : "r")} aria-expanded={panels.drawer === "r"}>
                        ▤ Page &amp; notes <b>{notes.length}</b>
                    </button>
                    <span className="k-sp" />
                    <button
                        className="k-tm2"
                        onClick={() => {
                            panels.setDrawer("r");
                            setTimeout(() => document.getElementById("focus")?.scrollIntoView({ block: "end", behavior: reducedMotion() ? "auto" : "smooth" }), 420);
                        }}
                    >
                        ⏱ <span>{time}</span>
                    </button>
                </div>
                <div className={`k-dbk${panels.drawer ? " k-on" : ""}`} onClick={() => panels.setDrawer(null)} aria-hidden="true" />
                <div
                    className={`k-stage${panels.dragging ? " k-dragging" : ""}${panels.p.lc ? " k-lc" : ""}${panels.p.rc ? " k-rc" : ""}`}
                    style={{ "--lw": `${panels.p.l}px`, "--rw": `${panels.p.r}px` } as React.CSSProperties}
                >
                    <Tree course={course} page={p} panels={panels} />
                    <main className="k-read2" key={p.stage.id}>
                        <div className="k-eye k-rv" style={{ "--i": 0 } as React.CSSProperties} id="s-top">
                            <b>
                                {p.module.code.toUpperCase()} · STAGE {inModule + 1} OF {p.module.stages.length}
                            </b>{" "}
                            / {DIFFICULTY_LABEL[p.stage.difficulty]} / #{p.stage.id}
                            {p.state !== "todo" && <> / {p.state === "assisted" ? "PASSED · ASSISTED" : "PASSED"}</>}
                        </div>
                        <h2 className="k-ttl k-rv" style={{ "--i": 1 } as React.CSSProperties}>
                            {p.stage.title}
                        </h2>
                        <div className="k-cli k-rv" style={{ "--i": 2 } as React.CSSProperties}>
                            <span>test this stage</span>
                            <code>{cmd}</code>
                            <MockCopy text={cmd} />
                        </div>
                        {p.stage.learn.length > 0 && (
                            <div className="k-learn k-rv" style={{ "--i": 3 } as React.CSSProperties}>
                                <span>YOU'LL LEARN</span>
                                {p.stage.learn.map((l) => (
                                    <span className="k-cp" key={l}>
                                        {l}
                                    </span>
                                ))}
                            </div>
                        )}
                        <div
                            className="k-tabs k-rv"
                            ref={tabsRef}
                            style={{ "--i": 4 } as React.CSSProperties}
                            role="tablist"
                            aria-label="Stage"
                            onKeyDown={(e) => {
                                const i = TAB_IDS.indexOf(tab);
                                const n = e.key === "ArrowRight" ? (i + 1) % 5 : e.key === "ArrowLeft" ? (i + 4) % 5 : e.key === "Home" ? 0 : e.key === "End" ? 4 : -1;
                                if (n < 0) return;
                                e.preventDefault();
                                pick(TAB_IDS[n] as Tab);
                                setTimeout(() => tabsRef.current?.querySelectorAll<HTMLElement>("[role=tab]")[n]?.focus(), 0);
                            }}
                        >
                            {tabs.map(([k, label, n]) => (
                                <button key={k} role="tab" aria-selected={tab === k} aria-controls={`pane-${k}`} tabIndex={tab === k ? 0 : -1} className={tab === k ? "k-on" : ""} onClick={() => pick(k)}>
                                    {label}
                                    {n !== "" && <> <small>{n}</small></>}
                                </button>
                            ))}
                            <span className="k-ul" />
                        </div>

                        <section className="k-pane k-on" key={tab} id={`pane-${tab}`} role="tabpanel">
                            {tab === "instructions" && (
                                <>
                                    {reqConcepts.length > 0 && (
                                        <a
                                            className="k-read-first"
                                            href="#concepts"
                                            onClick={(e) => {
                                                e.preventDefault();
                                                pick("concepts");
                                            }}
                                        >
                                            <b>READ FIRST</b>
                                            <span>
                                                {reqConcepts[0]?.title}
                                                {reqConcepts.length > 1 && <> · and {reqConcepts.length - 1} more</>}
                                            </span>
                                            <em>~{reqConcepts.reduce((n, k) => n + k.minutes, 0)} min</em>
                                        </a>
                                    )}
                                    {p.stage.intro && <Prose text={p.stage.intro} />}
                                    {entries.map((e, i) => {
                                        const nextIsOpt = e.optional && !entries[i + 1]?.optional;
                                        if (e.optional) {
                                            const id = e.id.replace(/^sec-/, "");
                                            const isOpen = openOpt.has(id);
                                            return (
                                                <Fragment key={e.id}>
                                                    <div className={`k-opt${isOpen ? " k-open" : ""}${hidden(id) ? " k-hid" : ""}`} id={e.id}>
                                                        <button
                                                            className="k-oh"
                                                            aria-expanded={isOpen}
                                                            onClick={() =>
                                                                setOpenOpt((s) => {
                                                                    const n = new Set(s);
                                                                    if (!n.delete(id)) n.add(id);
                                                                    return n;
                                                                })
                                                            }
                                                        >
                                                            <span className="k-badge2 k-op">OPTIONAL</span>
                                                            <b>{e.title}</b>
                                                            <span className="k-r">
                                                                <span className="k-chev" style={{ transform: isOpen ? "rotate(90deg)" : undefined }}>›</span>
                                                            </span>
                                                        </button>
                                                        <div className="k-ob">
                                                            <div inert={!isOpen}>
                                                                <div className="k-in2">
                                                                    <Body text={e.md} stageId={p.stage.id} />
                                                                </div>
                                                            </div>
                                                        </div>
                                                    </div>
                                                    {nextIsOpt && (
                                                        <div className="k-bar2">
                                                            <button
                                                                className="k-cta k-xs k-sec k-fx"
                                                                onClick={() => {
                                                                    setOptHidden(!optHidden);
                                                                    setPref("optional.hidden", !optHidden);
                                                                    setFlipped(new Set());
                                                                    toast("in", !optHidden ? "Optional sections hidden" : "Optional sections shown");
                                                                }}
                                                            >
                                                                <span className="k-lbl">{optHidden ? "Show optional sections" : "Hide optional sections"}</span>
                                                            </button>
                                                            <span style={{ font: "500 11px var(--mono)", color: "var(--dim)" }}>remembered for next stages</span>
                                                        </div>
                                                    )}
                                                </Fragment>
                                            );
                                        }
                                        number++;
                                        const pt = part(e.title);
                                        return (
                                            <Fragment key={e.id}>
                                                <div className="k-sh" id={e.id}>
                                                    <b>{pt ? `PART ${pt[1]}` : String(number).padStart(2, "0")}</b>
                                                    {(pt ? pt[2] ?? "" : e.title).toUpperCase()}
                                                </div>
                                                {e.title === "The task" ? <TaskBox stageId={p.stage.id} text={e.md} /> : <Body text={e.md} stageId={p.stage.id} />}
                                                {e.id === "sec-tests" && run && run.tests.length > 0 && (
                                                    <div className="k-tests" style={{ marginTop: 14 }}>
                                                        {run.tests.map((t) => (
                                                            <div key={t.name} className={`k-t ${t.ok ? "k-ok" : "k-bad"}`}>
                                                                <i />
                                                                <div>
                                                                    <div className="k-n">{t.name}</div>
                                                                    <div className="k-d">{t.ok ? "passes" : (t.detail.split("\n").find((l) => l.trim()) ?? "fails")}</div>
                                                                </div>
                                                            </div>
                                                        ))}
                                                    </div>
                                                )}
                                            </Fragment>
                                        );
                                    })}
                                </>
                            )}
                            {tab === "hints" && <HintsTab page={p} onOpen={() => hint.mutate()} pending={hint.isPending} />}
                            {tab === "solution" && <SolutionTab page={p} onOpen={() => sol.mutate()} pending={sol.isPending} />}
                            {tab === "concepts" && <ConceptsTab course={course} page={p} queryKey={key} />}
                            {tab === "run" &&
                                (run ? (
                                    <RunTab runs={p.runs.length ? p.runs : [run]} stageId={p.stage.id} sources={p.stage.test_sources ?? {}} />
                                ) : (
                                    <div className="k-empty">
                                        No run yet. In your repo, run <code>anneal course test</code>, or commit and push: each run is reported here.
                                    </div>
                                ))}
                            <nav className="k-pn">
                                {p.prev ? (
                                    <Link to="/courses/$course/$stage" params={{ course, stage: p.prev.id }}>
                                        <small>‹ PREVIOUS</small>
                                        {p.prev.title}
                                    </Link>
                                ) : (
                                    <span />
                                )}
                                {p.next ? (
                                    <Link to="/courses/$course/$stage" params={{ course, stage: p.next.id }}>
                                        <small>NEXT ›</small>
                                        {p.next.title}
                                    </Link>
                                ) : (
                                    <span />
                                )}
                            </nav>
                        </section>
                    </main>

                    <aside className={`k-toc${panels.drawer === "r" ? " k-open" : ""}`} id="tocN" aria-label="This page">
                        <Resizer side="r" panels={panels} />
                        <div className="k-tscroll">
                            <div className="k-rcard">
                                <h6>
                                    ON THIS PAGE
                                    <button className="k-sbtn" id="rt" aria-label="Hide sidebar" title="Hide (⌘.)" onClick={() => panels.update({ ...panels.p, rc: true })}>
                                        »
                                    </button>
                                    <button className="k-drawer-x" aria-label="Close" onClick={() => panels.setDrawer(null)}>
                                        ×
                                    </button>
                                </h6>
                                <div id="toc" ref={outline} style={{ position: "relative", borderLeft: "1px solid var(--line2)" }}>
                                    <span className="k-mk" id="mk" />
                                    {outlineEntries.map(([id, label]) => (
                                        <a
                                            key={id}
                                            href={`#${id}`}
                                            className={active === id ? "k-on" : ""}
                                            onClick={(e) => {
                                                e.preventDefault();
                                                panels.setDrawer(null);
                                                if (tab !== "instructions") pick("instructions");
                                                // a hidden optional section opens when you go to it
                                                const sid = id.replace(/^sec-/, "");
                                                if (OPTIONAL_SECTIONS.has(sid)) {
                                                    if (hidden(sid)) flip(sid);
                                                    setOpenOpt((s) => new Set(s).add(sid));
                                                }
                                                setTimeout(() => {
                                                    if (id === "s-top") window.scrollTo({ top: 0, behavior: reducedMotion() ? "auto" : "smooth" });
                                                    else document.getElementById(id)?.scrollIntoView({ behavior: reducedMotion() ? "auto" : "smooth", block: "start" });
                                                }, 30);
                                            }}
                                        >
                                            {label}
                                        </a>
                                    ))}
                                </div>
                            </div>
                            {notes.length > 0 && tab === "instructions" && (
                                <div className="k-rcard" id="notesCard" aria-label="Notes">
                                    <h6>
                                        NOTES <span className="k-pin">{notes.length}</span>
                                    </h6>
                                    <div id="nlist">
                                        {notes.map((n) => (
                                            <div
                                                key={n.n}
                                                role="button"
                                                tabIndex={0}
                                                data-sn={n.n}
                                                className={`k-pnote${curNotes.includes(n.n) ? " k-cur" : ""}${selNote === n.n ? " k-sel" : ""}`}
                                                onClick={(e) => {
                                                    const t = e.target as HTMLElement;
                                                    if (t.closest("a")) return;
                                                    const marker = document.querySelector<HTMLElement>(`.k-read2 .k-snm[data-sn="${n.n}"]`);
                                                    if (t.closest("[data-show]")) {
                                                        const o = marker?.closest(".k-snr")?.querySelector(`.k-sn[data-sn="${n.n}"]`);
                                                        o?.classList.toggle("k-open");
                                                        o?.scrollIntoView({ behavior: reducedMotion() ? "auto" : "smooth", block: "center" });
                                                        return;
                                                    }
                                                    if (t.closest("[data-cp]")) {
                                                        const text = (e.currentTarget as HTMLElement).querySelector("pre")?.textContent ?? "";
                                                        try {
                                                            navigator.clipboard.writeText(text);
                                                        } catch {
                                                            // no clipboard in this context
                                                        }
                                                        toast("ok", "Code copied");
                                                        return;
                                                    }
                                                    setSelNote((cur) => (cur === n.n ? null : n.n));
                                                    if (marker) {
                                                        marker.classList.remove("k-flash");
                                                        void marker.offsetWidth;
                                                        marker.classList.add("k-flash");
                                                        marker.scrollIntoView({ behavior: reducedMotion() ? "auto" : "smooth", block: "center" });
                                                    }
                                                }}
                                                onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && e.target === e.currentTarget && (e.preventDefault(), (e.currentTarget as HTMLElement).click())}
                                                onMouseEnter={() => document.querySelector(`.k-read2 .k-snm[data-sn="${n.n}"]`)?.classList.add("k-on")}
                                                onMouseLeave={() => document.querySelector(`.k-read2 .k-snm[data-sn="${n.n}"]`)?.classList.remove("k-on")}
                                            >
                                                <b>{n.n}</b>
                                                <div>
                                                    <div className="k-pb2" dangerouslySetInnerHTML={{ __html: n.html }} />
                                                    <div className="k-acts3">
                                                        <button className="k-sx2" data-show>SHOW IN TEXT ↗</button>
                                                        {n.code && <button className="k-sx2" data-cp>COPY CODE</button>}
                                                    </div>
                                                </div>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            )}
                            {p.concepts.length > 0 && (
                                <div className="k-rcard">
                                    <h6>
                                        {reqConcepts.length > 0 ? "CONCEPTS" : "RUST & SYSTEMS NOTES"}{" "}
                                        <span className={`k-pin${reqConcepts.length > 0 ? " k-req" : ""}`}>{reqConcepts.length > 0 ? `${reqDone} / ${reqConcepts.length}` : "OPTIONAL"}</span>
                                    </h6>
                                    <div id="pconc">
                                        {p.concepts.map((k, i) => (
                                            <Link key={k.id} className={`k-cc2${k.read ? "" : " k-todo"}`} to="/courses/$course/concept/$id" params={{ course, id: k.id }}>
                                                <span className="k-ci">{k.read ? "✓" : i + 1}</span>
                                                <div>
                                                    <b>{k.title}</b>
                                                    <small>{k.minutes} min read</small>
                                                </div>
                                                <span className="k-st4">{k.read ? "READ" : k.required ? "TO READ" : "OPTIONAL"}</span>
                                            </Link>
                                        ))}
                                    </div>
                                </div>
                            )}
                            <div className="k-rcard">
                                <h6>
                                    THIS STAGE <span className="k-pin">PINNED</span>
                                </h6>
                                <div className="k-pincmd">
                                    <span>{cmd}</span>
                                    <MockCopy text={cmd} className="k-copy k-fx" />
                                </div>
                                <div className="k-kv2">
                                    <span>Last run</span>
                                    <b>{run ? (limit ? `time limit exceeded · ${ago(run.at)}` : run.problem ? "did not run" : `${run.passed} / ${run.total} · ${ago(run.at)}`) : "none yet"}</b>
                                </div>
                                <div className="k-kv2">
                                    <span>Hints used</span>
                                    <b>
                                        {p.hints.revealed.length} / {p.hints.total}
                                    </b>
                                </div>
                                <div className="k-hl2">
                                    {Array.from({ length: p.hints.total }, (_, i) => (
                                        <i key={i} className={i < p.hints.revealed.length ? "k-on" : ""} />
                                    ))}
                                </div>
                            </div>
                        </div>
                        <FocusTimer onTime={setTime} />
                    </aside>
                </div>
                {panels.p.rc && (
                    <button className="k-sbtn" aria-label="Show the page panel" title="Show the page panel (⌘.)" onClick={() => panels.update({ ...panels.p, rc: false })} style={{ position: "fixed", right: 14, top: 66, zIndex: 40 }}>
                        «
                    </button>
                )}

                <div className={`k-strip${strip ? " k-open" : ""}`} id="strip">
                    <div className="k-in">
                        <span className={`k-sd${running ? " k-run" : run ? (run.ok ? " k-ok" : "") : " k-idle"}`} id="sd" />
                        <div className="k-sb" id="sb">
                            {run?.tests.map((t) => <i key={t.name} className={t.ok ? "k-p" : "k-f"} />)}
                        </div>
                        <span className="k-tx" id="stx" role="status" aria-live="polite">
                            {running ? (
                                <>
                                    <b>running tests…</b> · started {ago(running)}
                                </>
                            ) : run ? (
                                <>
                                    <b>{run.ok ? "stage passed" : limit ? (run.problem ? "time limit exceeded" : `time limit exceeded · ${run.passed} / ${run.total} passing`) : run.problem ? "did not run" : `${run.passed} / ${run.total} passing`}</b> · run {ago(run.at)}
                                    {run.commit_sha ? ` · ${run.commit_sha.slice(0, 7)}` : ""}
                                </>
                            ) : (
                                <>
                                    <b>listening for your next run</b> · run <code>anneal course test</code> in your repo
                                </>
                            )}
                        </span>
                        <button className={`k-cta${running ? " k-busy" : ""}`} id="runbtn" onClick={copyCmd} title="Copies the command; run it in your repo and this page shows the result">
                            <span className="k-spn" />
                            <span className="k-lbl">{running ? "Running…" : "Copy test command"}</span>
                            <span className="k-ar">▶</span>
                        </button>
                        <button className="k-copy" id="logb" onClick={() => setStrip(!strip)} aria-expanded={strip}>
                            {strip ? "HIDE LOGS" : "SHOW LOGS"}
                        </button>
                    </div>
                    <div className="k-logs">
                        <div inert={!strip}>
                            <pre id="logt">
                                {!run ? (
                                    "No run yet."
                                ) : run.problem ? (
                                    run.problem
                                ) : failedFirst ? (
                                    <>
                                        <span className="k-er">{failedFirst.name}</span> failed{"\n"}
                                        {failedFirst.detail}
                                    </>
                                ) : (
                                    "All tests passed."
                                )}
                            </pre>
                        </div>
                    </div>
                </div>
                <button
                    className="k-totop k-fx"
                    ref={topRef}
                    aria-label="Back to top"
                    onClick={() => window.scrollTo({ top: 0, behavior: reducedMotion() ? "auto" : "smooth" })}
                >
                    <svg viewBox="0 0 44 44" aria-hidden="true">
                        <circle className="k-a" ref={ringRef} cx="22" cy="22" r="20" />
                    </svg>
                    ↑
                </button>
            </MockRoot>
            {win && lastRun?.ok && (
                <Celebration
                    title="Stage passed"
                    message={`All ${lastRun.total} tests pass.${p.hints.revealed.length === 0 ? " Nicely done: no hints used." : ""}`}
                    stats={[
                        { value: `${lastRun.passed}/${lastRun.total}`, label: "TESTS" },
                        { value: `${(lastRun.duration_ms / 1000).toFixed(1)}s`, label: "RUN TIME" },
                        { value: String(p.hints.revealed.length), label: "HINTS" },
                    ]}
                    next={p.next ? { kicker: "NEXT STAGE", title: p.next.title } : undefined}
                    goLabel={p.next ? "Go to next stage" : "Back to the course"}
                    onGo={() => {
                        setWin(false);
                        if (p.next) nav({ to: "/courses/$course/$stage", params: { course, stage: p.next.id } });
                        else nav({ to: "/courses/$course", params: { course } });
                    }}
                    review={p.solution.available ? { label: "Review solution", onReview: () => (setWin(false), pick("solution")) } : undefined}
                    onClose={() => setWin(false)}
                />
            )}
        </>
    );
}
