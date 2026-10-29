import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type CSSProperties } from "react";
import { api, type SrsSettings } from "../api";
import { Header } from "../components/Header";
import { Skeleton } from "../components/Skeleton";
import { getPref, setPref } from "../prefs";
import { ACCENTS, useAppearance, usePageWidth } from "../settings";
import { THEMES, applyTheme, storedTheme, type Theme } from "../theme";
import { WHATS_NEW } from "../whatsnew";

/** The keys of every query that shows a count, so a change to what counts is seen everywhere at once. */
const COUNT_KEYS = ["settings", "dsa", "dsa-row", "tracks", "progress", "activity", "stats", "reviews", "dsa-review", "practice"];

function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label: string }) {
    return <button role="switch" aria-checked={on} aria-label={label} className="st-sw st-ctl" onClick={() => onChange(!on)} />;
}

/** A calendar address for a phone: the reviews that fall due and the weekly solve days. The address holds a random token; making a new one stops the old. */
function FeedLink() {
    const qc = useQueryClient();
    const feed = useQuery({ queryKey: ["feed"], queryFn: api.feed });
    const make = useMutation({ mutationFn: api.makeFeed, onSuccess: () => void qc.invalidateQueries({ queryKey: ["feed"] }) });
    const [copied, setCopied] = useState(false);
    const url = feed.data?.path ? `${location.origin}${feed.data.path}` : null;
    return (
        <div className="st-row">
            <b>Calendar link</b>
            <p>Subscribe in your phone's calendar app to see which days reviews are due and which are solve days. Anyone with the link can see problem titles and dates, so keep it private; making a new link stops the old one.</p>
            <div className="st-ctl st-actions">
                {url && (
                    <>
                        <input className="st-date" style={{ width: 220 }} readOnly value={url} aria-label="Calendar link" onFocus={(e) => e.currentTarget.select()} />
                        <button className="st-btn" onClick={() => void navigator.clipboard?.writeText(url).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1500); }, () => undefined)}>{copied ? "Copied" : "Copy"}</button>
                    </>
                )}
                <button className="st-btn" disabled={make.isPending} onClick={() => make.mutate()}>{url ? "New link" : "Make a link"}</button>
            </div>
        </div>
    );
}

/** Problems solved before anneal: paste their LeetCode numbers, see what would be marked, then mark them. They count as solved on a past day (so
 *  today's activity and the streak are untouched) and start no reviews; the whole batch can be taken back. */
function BulkSolved() {
    const qc = useQueryClient();
    const [text, setText] = useState("");
    const [date, setDate] = useState("");
    const [result, setResult] = useState<Awaited<ReturnType<typeof api.bulkSolved>> | null>(null);
    const [done, setDone] = useState<{ attempts: number[]; n: number } | null>(null);
    const [message, setMessage] = useState<string | null>(null);
    const numbers = [...new Set((text.match(/\d+/g) ?? []).map(Number))];
    const refresh = () => COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] }));
    const check = useMutation({ mutationFn: () => api.bulkSolved({ numbers, date: date || null, dry_run: true }), onSuccess: (r) => { setResult(r); setMessage(null); }, onError: (e: Error) => setMessage(e.message) });
    const mark = useMutation({
        mutationFn: () => api.bulkSolved({ numbers, date: date || null }),
        onSuccess: (r) => {
            setDone({ attempts: r.attempts, n: r.attempts.length });
            setResult(null);
            setMessage(`Marked ${r.attempts.length} problem${r.attempts.length === 1 ? "" : "s"} as solved on ${r.date}.`);
            refresh();
        },
        onError: (e: Error) => setMessage(e.message),
    });
    const undo = useMutation({
        mutationFn: () => api.bulkSolvedUndo(done?.attempts ?? []),
        onSuccess: (r) => { setDone(null); setMessage(`Took back ${r.removed}.`); refresh(); },
        onError: (e: Error) => setMessage(e.message),
    });
    return (
        <section className="st-card" aria-labelledby="st-bulk">
            <h2 id="st-bulk">ALREADY SOLVED BEFORE ANNEAL</h2>
            <p className="rempty" style={{ margin: 0 }}>Paste LeetCode problem numbers (commas, spaces or lines). They are marked solved on a past day, so today's activity and your streak do not change, and no review is started. A problem you already logged here is left alone.</p>
            <textarea id="st-bulk-numbers" className="st-ta" rows={3} placeholder="1, 49, 128, 217 …" value={text} onChange={(e) => { setText(e.target.value); setResult(null); }} aria-label="LeetCode problem numbers" />
            <div className="st-row">
                <b>Solved on</b>
                <p>Leave empty for "before anneal" (1 January 2020).</p>
                <input id="st-bulk-date" className="st-date st-ctl" type="date" value={date} max={new Date().toISOString().slice(0, 10)} onChange={(e) => setDate(e.target.value)} aria-label="Day they were solved" />
            </div>
            <div className="st-actions">
                <button className="st-btn" disabled={!numbers.length || check.isPending} onClick={() => check.mutate()}>Check {numbers.length || ""} number{numbers.length === 1 ? "" : "s"}</button>
                {result && result.matched.length > 0 && <button className="st-btn" disabled={mark.isPending} onClick={() => mark.mutate()}>Mark {result.matched.length} as solved</button>}
                {done && <button className="st-btn" disabled={undo.isPending} onClick={() => undo.mutate()}>Undo this batch ({done.n})</button>}
            </div>
            {result && (
                <div className="st-note">
                    <b>{result.matched.length}</b> would be marked{result.matched.length ? `: ${result.matched.slice(0, 8).map((m) => m.title).join(", ")}${result.matched.length > 8 ? ` and ${result.matched.length - 8} more` : ""}` : ""}.
                    {result.already_logged.length ? <> {result.already_logged.length} already logged, left alone.</> : null}
                    {result.unknown.length ? <> Not in anneal: {result.unknown.slice(0, 10).join(", ")}{result.unknown.length > 10 ? "…" : ""}.</> : null}
                </div>
            )}
            {message && <span className={message.startsWith("Marked") || message.startsWith("Took") ? "st-ok" : "st-bad"} role="status">{message}</span>}
        </section>
    );
}

/** One place for the switches that change what anneal counts and shows. Everything here is kept on the server. */
export function SettingsPage() {
    const qc = useQueryClient();
    const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
    const { accent, set: setAccent } = useAppearance();
    const { width, set: setWidth } = usePageWidth();
    const [theme, setTheme] = useState<Theme>(storedTheme);
    const [barHidden, setBarHidden] = useState(() => getPref("statusbar.hidden", false));
    const [chime, setChime] = useState(() => getPref<{ chime?: boolean }>("focus.cfg", {}).chime === true);
    const [imported, setImported] = useState<string | null>(null);
    const file = useRef<HTMLInputElement>(null);

    const saveSrs = useMutation({
        mutationFn: (srs: SrsSettings) => api.saveSrs(srs),
        onSuccess: () => COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] })),
    });
    const doImport = useMutation({
        mutationFn: async (f: File) => api.importBackup(JSON.parse(await f.text()) as unknown),
        onSuccess: (r) => {
            const n = Object.values(r.added).reduce((a, b) => a + b, 0);
            setImported(n ? `Added ${n} rows the backup had and this database lacked.` : "Nothing to add: everything in that backup is already here.");
            COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] }));
        },
        onError: (e: Error) => setImported(`Couldn't import that file: ${e.message}`),
    });

    const srs = settings.data?.srs;
    const countPremium = srs ? !srs.goal.free_only : false;
    // What flipping the switch would do to the goal the plan follows, shown before the owner flips it.
    const preview = (() => {
        const o = dsa.data;
        if (!o || !srs) return null;
        const inList = o.problems.filter((p) => p.lists.includes(srs.goal.list));
        const premium = inList.filter((p) => p.premium);
        const solvedPremium = premium.filter((p) => p.state.solved).length;
        return { total: inList.length, premium: premium.length, solvedPremium, free: inList.length - premium.length };
    })();

    return (
        <>
            <Header />
            <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <div className="st-wrap">
                    <h1>Settings</h1>
                    <p className="lead">What anneal counts and shows. Nothing here deletes anything you have logged.</p>
                    {!srs ? (
                        <Skeleton lines={6} />
                    ) : (
                        <>
                            <section className="st-card" aria-labelledby="st-count">
                                <h2 id="st-count">COUNTING</h2>
                                <div className="st-row">
                                    <b>Count premium problems</b>
                                    <p>Off by default. When off, LeetCode Premium problems are still listed and can be logged, but they are not in the totals, the progress bars or readiness. A premium problem you complete always counts toward your streak and activity, and it stays logged if you switch this on later.</p>
                                    <Switch on={countPremium} label="Count premium problems" onChange={(v) => saveSrs.mutate({ ...srs, goal: { ...srs.goal, free_only: !v } })} />
                                </div>
                                {preview && (
                                    <div className="st-note">
                                        The goal list has <b>{preview.total}</b> problems: <b>{preview.free}</b> free and <b>{preview.premium}</b> premium
                                        {preview.solvedPremium ? <> ({preview.solvedPremium} already solved)</> : null}. {countPremium ? "All of them count now." : `Turning this on makes the goal ${preview.total} instead of ${preview.free}.`}
                                    </div>
                                )}
                                <p className="rempty" style={{ margin: 0 }}>
                                    Reviews, capacity and the target date are on <Link to="/dsa/plan" style={{ color: "var(--ca)" }}>the plan page</Link>; it has the same premium switch.
                                </p>
                                {saveSrs.isError && <span className="st-bad">Couldn't save: {(saveSrs.error as Error).message}</span>}
                            </section>

                            <section className="st-card" aria-labelledby="st-look">
                                <h2 id="st-look">APPEARANCE</h2>
                                <div className="st-row">
                                    <b>Theme</b>
                                    <p>System follows your device.</p>
                                    <div className="st-seg st-ctl" role="radiogroup" aria-label="Theme">
                                        {THEMES.map((t) => (
                                            <button key={t} role="radio" aria-checked={theme === t} className={theme === t ? "on" : ""} onClick={() => { applyTheme(t); setTheme(t); }}>
                                                {t[0]!.toUpperCase() + t.slice(1)}
                                            </button>
                                        ))}
                                    </div>
                                </div>
                                <div className="st-row">
                                    <b>Accent colour</b>
                                    <p />
                                    <div className="st-seg st-ctl" role="radiogroup" aria-label="Accent colour">
                                        {ACCENTS.map((a) => (
                                            <button key={a.id} role="radio" aria-checked={accent === a.id} className={accent === a.id ? "on" : ""} onClick={() => setAccent(a.id)}>
                                                {a.label}
                                            </button>
                                        ))}
                                    </div>
                                </div>
                                <div className="st-row">
                                    <b>Page width</b>
                                    <p>Centred keeps lines readable; wide uses more of the window.</p>
                                    <div className="st-seg st-ctl" role="radiogroup" aria-label="Page width">
                                        {(["narrow", "wide"] as const).map((w) => (
                                            <button key={w} role="radio" aria-checked={width === w} className={width === w ? "on" : ""} onClick={() => setWidth(w)}>
                                                {w === "narrow" ? "Centred" : "Wide"}
                                            </button>
                                        ))}
                                    </div>
                                </div>
                                <div className="st-row">
                                    <b>Numbers bar at the bottom</b>
                                    <p>The slow ticker of your numbers. Hiding it gives the pages its 30 pixels back.</p>
                                    <Switch on={!barHidden} label="Show the numbers bar" onChange={(v) => { setBarHidden(!v); setPref("statusbar.hidden", !v); }} />
                                </div>
                                <div className="st-row">
                                    <b>Timer chime</b>
                                    <p>A short two-note sound when a focus session or the countdown ends. The timer itself never starts by itself.</p>
                                    <Switch on={chime} label="Timer chime" onChange={(v) => { setChime(v); setPref("focus.cfg", { ...getPref<Record<string, unknown>>("focus.cfg", {}), chime: v }); }} />
                                </div>
                            </section>

                            <section className="st-card" id="new" aria-labelledby="st-new">
                                <h2 id="st-new">WHAT'S NEW</h2>
                                {WHATS_NEW.map((e) => (
                                    <div key={e.id} className="st-new">
                                        <b>{e.title}</b> <small>{e.date}</small>
                                        <ul>
                                            {e.items.map((t) => (
                                                <li key={t}>{t}</li>
                                            ))}
                                        </ul>
                                    </div>
                                ))}
                            </section>

                            <BulkSolved />

                            <section className="st-card" aria-labelledby="st-data">
                                <h2 id="st-data">YOUR DATA</h2>
                                <div className="st-row">
                                    <b>Back up everything</b>
                                    <p>One file with your attempts, reviews, saved solutions, settings, plans and course runs, and the archive of anything you reset or deleted.</p>
                                    <a className="st-btn st-ctl" href="/api/export" download>Download backup</a>
                                </div>
                                <div className="st-row">
                                    <b>Restore from a backup</b>
                                    <p>Only adds what is missing. Nothing already here is changed or removed, so an old backup is safe to import.</p>
                                    <div className="st-ctl">
                                        <input ref={file} type="file" accept="application/json,.json" hidden onChange={(e) => { const f = e.target.files?.[0]; if (f) { setImported(null); doImport.mutate(f); } e.target.value = ""; }} />
                                        <button className="st-btn" disabled={doImport.isPending} onClick={() => file.current?.click()}>{doImport.isPending ? "Importing…" : "Choose a file"}</button>
                                    </div>
                                </div>
                                <FeedLink />
                                {imported && <span className={imported.startsWith("Couldn't") ? "st-bad" : "st-ok"} role="status">{imported}</span>}
                                <p className="rempty" style={{ margin: 0 }}>A reset or a deleted solution is archived first and kept forever, and the production database is backed up before every deploy.</p>
                            </section>
                        </>
                    )}
                </div>
            </main>
        </>
    );
}
