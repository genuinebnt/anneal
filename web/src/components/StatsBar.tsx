import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { isRustSection, PLANNED, sectionOf } from "../curriculum";
import { niceDate } from "../dsa";
import { getPref } from "../prefs";
import { useShowIdeas } from "../spoilers";

type AreaId = "dsa" | "rust" | "courses" | "sd";
type Item = { key: string; label: string; value: string; of?: string; note?: string; bar?: number; tone?: "ok" | "warn" | "bad" | "hot"; spark?: number[] };

const pct = (a: number, b: number) => (b > 0 ? Math.max(0, Math.min(1, a / b)) : 0);
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The thin bar along the bottom: a slow ticker of what matters in the area you are in. It pauses under the pointer. */
export function StatsBar({ area }: { area: AreaId }) {
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const progress = useQuery({ queryKey: ["progress"], queryFn: api.progress });
    const tracks = useQuery({ queryKey: ["tracks"], queryFn: api.tracks });
    const course = useQuery({ queryKey: ["course", "bustub"], queryFn: () => api.course("bustub") });
    const sd = useQuery({ queryKey: ["sd"], queryFn: api.sd, staleTime: 60_000, enabled: area === "sd" });

    // Hidden from Settings; the choice is a preference, so it follows the owner to another laptop.
    const [hidden, setHidden] = useState(() => getPref("statusbar.hidden", false));
    useEffect(() => {
        const f = (e: Event) => (e as CustomEvent<string>).detail === "statusbar.hidden" && setHidden(getPref("statusbar.hidden", false));
        window.addEventListener("anneal:pref", f);
        return () => window.removeEventListener("anneal:pref", f);
    }, []);
    useEffect(() => {
        if (hidden) return;
        document.documentElement.classList.add("has-sb");
        return () => document.documentElement.classList.remove("has-sb");
    }, [hidden]);

    const [showIdeas] = useShowIdeas();
    const items = useMemo<Item[]>(() => {
        const out: Item[] = [];
        const o = dsa.data;
        const p = progress.data;
        const spark = p ? p.heat.slice(-14) : undefined;
        if (area === "dsa" && o) {
            const plan = o.plan;
            out.push({ key: "goal", label: "NEETCODE 150", value: String(plan.goal_done), of: `/${plan.goal_total}`, bar: pct(plan.goal_done, plan.goal_total), tone: "ok" });
            // the streak is not repeated here: the header above already shows it
            out.push({ key: "due", label: "REVIEWS DUE", value: String(plan.due), note: plan.overdue ? `${plan.overdue} overdue` : "none overdue", tone: plan.overdue ? "bad" : plan.due ? "warn" : "ok" });
            out.push({ key: "today", label: "TODAY", value: plan.solve_day ? "solve day" : "practice day", note: `room for ${plan.capacity} reviews` });
            const next = o.problems.find((x) => x.id === plan.next_up[0]);
            // the pattern's name would give the approach away, so it is shown only when the key idea is
            if (next) out.push({ key: "next", label: "NEXT UP", value: next.title, note: showIdeas ? (o.patterns.find((x) => x.code === next.pattern)?.name ?? next.pattern) : undefined });
            const pace = plan.pace;
            if (pace.per_week != null) out.push({ key: "pace", label: "PACE NEEDED", value: `${pace.per_week.toFixed(1)}/week`, note: `${pace.solve_days_left} solve days left` });
            if (pace.finish_at_current) {
                const d = pace.days_vs_target;
                out.push({ key: "finish", label: "FINISH AT THIS PACE", value: niceDate(pace.finish_at_current, o.today), note: d == null ? undefined : d >= 0 ? `${d}d before target` : `${-d}d after target`, tone: d != null && d < 0 ? "warn" : "ok" });
            }
            if (p) out.push({ key: "year", label: "THIS YEAR", value: String(p.solved_year), note: `${plural(p.active_days_year, "active day")}` });
            const listed = o.problems.filter((x) => !x.lists.every((l) => l === "practice"));
            out.push({ key: "bank", label: "BANK", value: String(o.problems.length), note: `${listed.length} in the lists · ${o.patterns.length} patterns` });
        } else if (area === "rust" && tracks.data) {
            const t = tracks.data.filter((x) => isRustSection(x.section));
            const total = t.reduce((n, x) => n + x.total, 0);
            const solved = t.reduce((n, x) => n + x.solved, 0);
            out.push({ key: "rs", label: "RUST PROBLEMS", value: String(solved), of: `/${total}`, bar: pct(solved, total), tone: "ok" });
            out.push({ key: "tr", label: "TRACKS", value: String(PLANNED.filter((x) => isRustSection(sectionOf(x.code))).length) });
            const open = t.find((x) => x.solved < x.total);
            if (open) out.push({ key: "op", label: "IN PROGRESS", value: open.name, note: `${open.solved}/${open.total}` });
            if (p) out.push({ key: "st", label: "STREAK", value: plural(p.streak, "day"), tone: p.streak > 0 ? "hot" : undefined, spark });
        } else if (area === "courses" && course.data) {
            const c = course.data;
            out.push({ key: "stages", label: "STAGES PASSED", value: String(c.done), of: `/${c.total}`, bar: pct(c.done, c.total), tone: "ok" });
            if (c.challenges != null) out.push({ key: "ch", label: "CHALLENGES", value: String(c.challenges_done ?? 0), of: `/${c.challenges}`, bar: pct(c.challenges_done ?? 0, c.challenges) });
            if (c.current) out.push({ key: "cur", label: "CONTINUE", value: c.current });
            if (c.last_run) out.push({ key: "run", label: "LAST RUN", value: c.last_run.stage_id, note: `${c.last_run.passed}/${c.last_run.total} passing`, tone: c.last_run.ok ? "ok" : "bad" });
        } else if (area === "sd" && sd.data) {
            const v = sd.data;
            const checks = v.concepts.reduce((n, c) => n + c.checks, 0);
            const done = v.concepts.reduce((n, c) => n + Math.min(c.checked, c.checks), 0);
            const drilled = v.problems.filter((x) => x.attempts > 0).length;
            out.push({ key: "chk", label: "CHECKS ANSWERED", value: String(done), of: `/${checks}`, bar: pct(done, checks), tone: "ok" });
            out.push({ key: "dr", label: "PROBLEMS DRILLED", value: String(drilled), of: `/${v.problems.length}`, bar: pct(drilled, v.problems.length) });
            out.push({ key: "due", label: "REVIEWS DUE", value: String(v.due), tone: v.due > 0 ? "hot" : undefined, note: `${v.week} this week` });
            out.push({ key: "pages", label: "PAGES", value: String(v.concepts.length), note: "to learn" });
        }
        return out;
    }, [area, dsa.data, progress.data, tracks.data, course.data, sd.data, showIdeas]);

    if (hidden || items.length === 0) return null;
    const render = (copy: number) =>
        items.map((it) => (
            <span className={`sb-i${it.tone ? ` ${it.tone}` : ""}`} key={`${copy}-${it.key}`} aria-hidden={copy > 0}>
                <i>{it.label}</i>
                <b>{it.value}</b>
                {it.of && <small>{it.of}</small>}
                {it.bar != null && (
                    <span className="sb-bar" aria-hidden="true">
                        <u style={{ width: `${Math.round(it.bar * 100)}%` }} />
                    </span>
                )}
                {it.spark && (
                    <svg className="sb-spark" viewBox="0 0 28 10" aria-hidden="true">
                        {it.spark.map((v, k) => (
                            <rect key={k} x={k * 2} y={10 - Math.min(10, 2 + v * 2)} width="1.4" height={Math.min(10, 2 + v * 2)} rx=".4" />
                        ))}
                    </svg>
                )}
                {it.note && <em>{it.note}</em>}
                <s aria-hidden="true">◆</s>
            </span>
        ));
    return (
        <div className="sb" role="region" aria-label="Your numbers" style={{ "--sb-n": items.length } as React.CSSProperties}>
            <div className="sb-track">
                {[0, 1, 2, 3, 4, 5, 6, 7].map((k) => (
                    <div className="sb-run" key={k} aria-hidden={k > 0}>
                        {render(k)}
                    </div>
                ))}
            </div>
        </div>
    );
}
