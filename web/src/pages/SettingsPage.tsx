import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState, type CSSProperties } from "react";
import { api } from "../api";
import { Header } from "../components/Header";
import { Segmented, Switch } from "../components/ui";
import { Skeleton } from "../components/Skeleton";
import { getPref, setPref } from "../prefs";
import { ACCENTS, useAppearance, usePageWidth } from "../settings";
import { THEMES, applyTheme, storedTheme, type Theme } from "../theme";
import { readUi, setUi } from "../uiprefs";
import { COUNT_KEYS, usePremiumSwitch } from "../premium";
import { useShowIdeas } from "../spoilers";



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
            <p>Anyone with the link can see problem titles and dates.</p>
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

    const [ui, setUiState] = useState(readUi);
    const premiumSwitch = usePremiumSwitch();
    const [showIdeas, setShowIdeas] = useShowIdeas();
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
    const countPremium = !premiumSwitch.hidden;
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
                    {!srs ? (
                        <Skeleton lines={6} />
                    ) : (
                        <>
                            <section className="st-card" aria-labelledby="st-count">
                                <h2 id="st-count">COUNTING</h2>
                                <div className="st-row">
                                    <b>Show and count premium problems</b>
                                    <p>When off, premium problems are hidden and not counted. What you logged on them is kept.</p>
                                    <Switch className="st-ctl" on={countPremium} label="Show and count premium problems" onChange={(v) => premiumSwitch.set(!v)} />
                                </div>
                                {preview && (
                                    <div className="st-note">
                                        The goal list has <b>{preview.total}</b> problems: <b>{preview.free}</b> free and <b>{preview.premium}</b> premium
                                        {preview.solvedPremium ? <> ({preview.solvedPremium} already solved)</> : null}. {countPremium ? "All of them are shown and count now." : `Turning this on shows them and makes the goal ${preview.total} instead of ${preview.free}.`}
                                    </div>
                                )}
                                <p className="rempty" style={{ margin: 0 }}>
                                    Reviews, capacity and the target date are on <Link to="/dsa/plan" style={{ color: "var(--ca)" }}>the plan page</Link>; it has the same premium switch.
                                </p>
                                {premiumSwitch.error && <span className="st-bad">Couldn't save: {premiumSwitch.error}</span>}
                            </section>

                            <section className="st-card" aria-labelledby="st-look">
                                <h2 id="st-look">APPEARANCE</h2>
                                <div className="st-row">
                                    <b>Theme</b>
                                    
                                    <Segmented className="st-ctl" label="Theme" value={theme} options={THEMES.map((t) => [t, t[0]!.toUpperCase() + t.slice(1)] as const)} onChange={(t) => { applyTheme(t); setTheme(t); }} />
                                </div>
                                <div className="st-row">
                                    <b>Accent colour</b>
                                    
                                    <Segmented className="st-ctl" label="Accent colour" value={accent} options={ACCENTS.map((x) => [x.id, x.label] as const)} onChange={setAccent} />
                                </div>
                                <div className="st-row">
                                    <b>Page width</b>
                                    
                                    <Segmented className="st-ctl" label="Page width" value={width} options={[["narrow", "Centred"], ["wide", "Wide"]] as const} onChange={setWidth} />
                                </div>
                                <div className="st-row">
                                    <b>Text size</b>
                                    
                                    <Segmented className="st-ctl" label="Text size" value={ui.zoom} options={[["100", "100%"], ["110", "110%"], ["125", "125%"]] as const} onChange={(v) => { setUi("ui.zoom", v); setUiState(readUi()); }} />
                                </div>
                                <div className="st-row">
                                    <b>High contrast</b>
                                    
                                    <Switch className="st-ctl" on={ui.contrast === "high"} label="High contrast" onChange={(v) => { setUi("ui.contrast", v ? "high" : "normal"); setUiState(readUi()); }} />
                                </div>
                                <div className="st-row">
                                    <b>Reduce motion</b>
                                    
                                    <Switch className="st-ctl" on={ui.motion === "reduce"} label="Reduce motion" onChange={(v) => { setUi("ui.motion", v ? "reduce" : "system"); setUiState(readUi()); }} />
                                </div>
                                <div className="st-row">
                                    <b>Show the key idea</b>
                                    
                                    <Switch className="st-ctl" on={showIdeas} label="Show the key idea" onChange={setShowIdeas} />
                                </div>
                                <div className="st-row">
                                    <b>Numbers bar at the bottom</b>
                                    
                                    <Switch className="st-ctl" on={!barHidden} label="Show the numbers bar" onChange={(v) => { setBarHidden(!v); setPref("statusbar.hidden", !v); }} />
                                </div>
                                <div className="st-row">
                                    <b>Timer chime</b>
                                    
                                    <Switch className="st-ctl" on={chime} label="Timer chime" onChange={(v) => { setChime(v); setPref("focus.cfg", { ...getPref<Record<string, unknown>>("focus.cfg", {}), chime: v }); }} />
                                </div>
                            </section>

                            <section className="st-card" aria-labelledby="st-data">
                                <h2 id="st-data">YOUR DATA</h2>
                                <div className="st-row">
                                    <b>Back up everything</b>
                                    
                                    <a className="st-btn st-ctl" href="/api/export" download>Download backup</a>
                                </div>
                                <div className="st-row">
                                    <b>Restore from a backup</b>
                                    
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
