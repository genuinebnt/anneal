import { Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api, type Grade, type MockConfig, type MockData } from "../api";
import { Header } from "../components/Header";
import { Skeleton } from "../components/Skeleton";
import { DIFF, leetcode, niceDate } from "../dsa";
import {
    BANDS,
    FAVOUR,
    MOCK_LISTS,
    MOCK_STATUS,
    PRESETS,
    clock,
    countBy,
    currentIndex,
    defaultConfig,
    draw,
    elapsed,
    isLogged,
    limitOf,
    mmss,
    newRound,
    onProblem,
    paceMinutes,
    poolOf,
    shortfall,
    slotsOf,
    suggested,
    summary,
    warning,
    type Round,
} from "../mock";

const ROUND_KEY = "anneal-mock-round";

function loadRound(): Round | null {
    try {
        const raw = localStorage.getItem(ROUND_KEY);
        return raw ? (JSON.parse(raw) as Round) : null;
    } catch {
        return null;
    }
}

function storeRound(r: Round | null) {
    try {
        if (r) localStorage.setItem(ROUND_KEY, JSON.stringify(r));
        else localStorage.removeItem(ROUND_KEY);
    } catch {
        // Private windows can refuse storage; the round still runs for this visit.
    }
}

/** A timed round of random LeetCode problems, set up the way you choose (docs/DSA.md, decision 25). */
export function MockPage() {
    const q = useQuery({ queryKey: ["dsa-mock"], queryFn: api.mock });
    return (
        <>
            <Header area="dsa" />
            <main className="page" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                {q.data ? (
                    <MockApp data={q.data} />
                ) : (
                    <div className="wrap" style={{ paddingBlock: 28 }}>
                        {q.isError ? <p className="rempty">Couldn't reach the API.</p> : <Skeleton lines={5} />}
                    </div>
                )}
            </main>
        </>
    );
}

function MockApp({ data }: { data: MockData }) {
    const qc = useQueryClient();
    const [round, setRound] = useState<Round | null>(loadRound);
    const [config, setConfig] = useState<MockConfig>(() => ({ ...defaultConfig(), ...data.saved?.last }));
    const [drawError, setDrawError] = useState<string | null>(null);
    const finishing = useRef(false);
    const remember = useMutation({ mutationFn: api.saveMock });

    const update = useCallback((fn: (r: Round) => Round) => {
        setRound((cur) => {
            if (!cur) return cur;
            const next = fn(cur);
            storeRound(next);
            return next;
        });
    }, []);

    const begin = (c: MockConfig) => {
        const pool = poolOf(data.problems, c, data.today);
        const picked = draw(pool, c, data.today);
        if (!picked) {
            setDrawError(shortfall(pool, c) ?? "Couldn't draw problems for this setup.");
            return false;
        }
        setDrawError(null);
        setConfig(c);
        finishing.current = false;
        const r = newRound(c, picked.map((p) => p.id), Date.now());
        storeRound(r);
        setRound(r);
        remember.mutate({ ...data.saved, last: c });
        return true;
    };

    const finish = (r: Round) => {
        if (r.ended != null || finishing.current) return;
        finishing.current = true;
        const now = Date.now();
        const ended = { ...r, ended: elapsed(r, now), pausedAt: null };
        const body = summary(ended, now);
        storeRound({ ...ended, saved: true });
        setRound({ ...ended, saved: true });
        api
            .finishMock({ config: r.config, ...body })
            .catch(() => undefined)
            .finally(() => {
                void qc.invalidateQueries({ queryKey: ["dsa-mock"] });
                void qc.invalidateQueries({ queryKey: ["dsa"] });
                void qc.invalidateQueries({ queryKey: ["tracks"] });
            });
    };

    const leave = (keepConfig: boolean) => {
        if (keepConfig && round) setConfig(round.config);
        storeRound(null);
        setRound(null);
        finishing.current = false;
    };

    if (round && round.ended == null) return <Live data={data} round={round} update={update} onFinish={finish} />;
    if (round) {
        return (
            <Result
                data={data}
                round={round}
                onAgain={() => {
                    if (!begin(round.config)) leave(true);
                }}
                onChange={() => leave(true)}
            />
        );
    }
    return <Setup data={data} initial={config} onStart={begin} error={drawError} />;
}

// ---------------------------------------------------------------- setup

const toggle = (list: string[], v: string) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
type Tab = "pool" | "topics" | "companies" | "clock";

function Switch({ on, onClick, title, hint, disabled }: { on: boolean; onClick: () => void; title: string; hint?: string; disabled?: boolean }) {
    return (
        <button className={`mi-tog${on ? " on" : ""}`} onClick={onClick} disabled={disabled} role="switch" aria-checked={on}>
            <i className="sw" />
            <span>
                <b>{title}</b>
                {hint && <small>{hint}</small>}
            </span>
        </button>
    );
}

function Num({ value, onChange, min, max, step = 1, label, unit }: { value: number; onChange: (n: number) => void; min: number; max: number; step?: number; label: string; unit?: string }) {
    return (
        <span className="mi-num" role="group" aria-label={label}>
            <button aria-label={`less ${label}`} disabled={value <= min} onClick={() => onChange(Math.max(min, value - step))}>−</button>
            <span aria-live="polite">{value}{unit && <small>{unit}</small>}</span>
            <button aria-label={`more ${label}`} disabled={value >= max} onClick={() => onChange(Math.min(max, value + step))}>+</button>
        </span>
    );
}

function Seg<T extends string>({ value, options, onPick, label }: { value: T; options: [T, string][]; onPick: (v: T) => void; label: string }) {
    return (
        <div className="mi-seg" role="group" aria-label={label}>
            {options.map(([v, l]) => <button key={v} className={v === value ? "on" : ""} aria-pressed={v === value} onClick={() => onPick(v)}>{l}</button>)}
        </div>
    );
}

/** The shape of a round as small bars, one per problem: green easy, amber medium, red hard. */
function Shape({ c }: { c: MockConfig }) {
    const bars = c.anyDiff ? Array.from({ length: c.anyCount }, () => "a") : c.mix.flatMap((k, i) => Array.from({ length: k }, () => "ewh"[i]!));
    return <span className="mi-shape">{bars.map((b, i) => <i key={i} className={b} />)}</span>;
}

function Setup({ data, initial, onStart, error }: { data: MockData; initial: MockConfig; onStart: (c: MockConfig) => void; error: string | null }) {
    const qc = useQueryClient();
    const [c, setC] = useState<MockConfig>(initial);
    const [preset, setPreset] = useState("custom");
    const [tab, setTab] = useState<Tab>("pool");
    const [coQuery, setCoQuery] = useState("");
    const [allCo, setAllCo] = useState(false);
    const [naming, setNaming] = useState(false);
    const [name, setName] = useState("");
    const presets = data.saved?.presets ?? [];
    const savePresets = useMutation({
        mutationFn: (next: typeof presets) => api.saveMock({ ...data.saved, presets: next }),
        onSuccess: () => void qc.invalidateQueries({ queryKey: ["dsa-mock"] }),
    });

    const edit = (fn: (c: MockConfig) => MockConfig) => {
        setC(fn);
        setPreset("custom");
    };

    const pool = useMemo(() => poolOf(data.problems, c, data.today), [data, c]);
    const have = countBy(pool);
    const problem = shortfall(pool, c);
    const need = slotsOf(c);

    // Counts shown beside each choice: what the other settings leave.
    const byTopic = useMemo(() => {
        const m = new Map<string, number>();
        for (const p of poolOf(data.problems, { ...c, topics: {} }, data.today)) m.set(p.pattern, (m.get(p.pattern) ?? 0) + 1);
        return m;
    }, [data, c]);
    const byCompany = useMemo(() => {
        const m = new Map<string, number>();
        const g = new Map<string, number>();
        for (const p of poolOf(data.problems, { ...c, groups: [], companies: [], recent: false }, data.today)) {
            const seenGroups = new Set<string>();
            for (const co of p.companies) {
                m.set(co.name, (m.get(co.name) ?? 0) + 1);
                seenGroups.add(co.group);
            }
            for (const n of seenGroups) g.set(n, (g.get(n) ?? 0) + 1);
        }
        return { companies: m, groups: g };
    }, [data, c]);
    const listCount = (key: string) => data.problems.filter((p) => !p.premium && p.lists.includes(key as never)).length;

    const allCompanies = useMemo(
        () =>
            [...new Set(data.company_groups.flatMap((g) => g.companies))]
                .map((n) => ({ name: n, n: byCompany.companies.get(n) ?? 0 }))
                .filter((x) => x.n > 0 || c.companies.includes(x.name))
                .sort((a, b) => b.n - a.n || a.name.localeCompare(b.name)),
        [data.company_groups, byCompany, c.companies],
    );
    const q = coQuery.trim().toLowerCase();
    const matching = q ? allCompanies.filter((x) => x.name.toLowerCase().includes(q)) : allCompanies;
    const shownCo = q || allCo ? matching : matching.slice(0, 8);

    const included = Object.values(c.topics).filter((v) => v === 1).length;
    const excluded = Object.values(c.topics).filter((v) => v === -1).length;
    const cycleTopic = (code: string) =>
        edit((cur) => {
            const topics = { ...cur.topics };
            const v = topics[code];
            if (v === undefined) topics[code] = 1;
            else if (v === 1) topics[code] = -1;
            else delete topics[code];
            return { ...cur, topics };
        });

    const slots = c.anyDiff ? Array.from({ length: c.anyCount }, () => 1) : c.mix.flatMap((k, i) => Array.from({ length: k }, () => i));
    const perTotal = paceMinutes(c);
    const warnings: { cls: string; text: string }[] = [];
    if (problem) warnings.push({ cls: "mi-bad", text: problem });
    if (!problem && error) warnings.push({ cls: "mi-bad", text: error });
    if (c.format === "total" && perTotal > c.total * 1.25) warnings.push({ cls: "mi-warn", text: `At the suggested pace the round takes ${perTotal} min, but the clock is ${c.total}. Lengthen the round or ask for fewer problems.` });
    if (c.favour === "weak" && !pool.some((p) => p.state.last_grade === "again" || p.state.last_grade === "hard" || (p.state.due && p.state.due <= data.today))) {
        warnings.push({ cls: "mi-warn", text: "Nothing here is marked weak yet, so this will draw uniformly." });
    }
    const timerName = c.format === "total" ? `Countdown, ${c.total}:00` : c.format === "per" ? "Countdown per problem" : "Stopwatch";

    // What the pool is drawn from, each removable.
    const listName = (k: string) => MOCK_LISTS.find(([key]) => key === k)?.[1] ?? k;
    const filters: { key: string; label: string; exc?: boolean; remove: () => void }[] = [
        ...c.lists.map((l) => ({ key: `list:${l}`, label: listName(l), remove: () => edit((cur) => (cur.lists.length > 1 ? { ...cur, lists: toggle(cur.lists, l) } : cur)) })),
        ...(c.status !== "all" ? [{ key: "status", label: MOCK_STATUS.find(([k]) => k === c.status)?.[1] ?? c.status, remove: () => edit((cur) => ({ ...cur, status: "all" as const })) }] : []),
        ...Object.entries(c.topics).map(([code, v]) => ({ key: `topic:${code}`, label: data.patterns.find((p) => p.code === code)?.name ?? code, exc: v === -1, remove: () => cycleTopic(code) })),
        ...c.groups.map((g) => ({ key: `group:${g}`, label: g, remove: () => edit((cur) => ({ ...cur, groups: toggle(cur.groups, g) })) })),
        ...c.companies.map((n) => ({ key: `co:${n}`, label: n, remove: () => edit((cur) => ({ ...cur, companies: toggle(cur.companies, n) })) })),
        ...(c.recent ? [{ key: "recent", label: "last 6 months", remove: () => edit((cur) => ({ ...cur, recent: false })) }] : []),
    ];
    const removeSlot = (band: number) => {
        if (c.anyDiff) edit((cur) => ({ ...cur, anyCount: Math.max(1, cur.anyCount - 1) }));
        else edit((cur) => ({ ...cur, mix: cur.mix.map((x, j) => (j === band ? Math.max(0, x - 1) : x)) as MockConfig["mix"] }));
    };
    const addSlot = (band: number) => edit((cur) => ({ ...cur, mix: cur.mix.map((x, j) => (j === band ? x + 1 : x)) as MockConfig["mix"] }));
    const tabs: [Tab, string, string | number][] = [
        ["pool", "Pool", pool.length],
        ["topics", "Topics", included || excluded ? `${included + excluded}` : "all"],
        ["companies", "Companies", c.groups.length + c.companies.length || "any"],
        ["clock", "Clock", c.format === "up" ? "∞" : c.format === "per" ? "each" : `${c.total}m`],
    ];

    return (
        <div className="mi-wrap m-setup">
            <div className="eyebrow">
                <Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link>
                <span>/</span>
                <span style={{ color: "var(--ca)" }}>MOCK INTERVIEW</span>
                <span>/</span>
                <span>{data.rounds.length} ROUND{data.rounds.length === 1 ? "" : "S"} TAKEN</span>
            </div>
            <h1 className="mi-h1">Build your interview</h1>
            <p className="mi-sub">Pick a shape, narrow the pool, set the clock. Problems are drawn at start.</p>

            <div className="mi-presets" role="group" aria-label="Presets">
                {PRESETS.map((p) => (
                    <button key={p.key} className={`mi-pc${preset === p.key ? " on" : ""}`} aria-pressed={preset === p.key} onClick={() => { setC(p.apply(c)); setPreset(p.key); }}>
                        <b>{p.name}</b>
                        <Shape c={p.apply(c)} />
                        <small>{p.detail}</small>
                    </button>
                ))}
                {presets.map((p) => (
                    <span key={p.name} className={`mi-pc${preset === `saved:${p.name}` ? " on" : ""}`}>
                        <button className="body" aria-pressed={preset === `saved:${p.name}`} onClick={() => { setC(p.config); setPreset(`saved:${p.name}`); }}>
                            <b>{p.name}</b>
                            <Shape c={p.config} />
                            <small>{describe(p.config, data)}</small>
                        </button>
                        <button className="x" aria-label={`Delete preset ${p.name}`} onClick={() => savePresets.mutate(presets.filter((x) => x.name !== p.name))}>×</button>
                    </span>
                ))}
            </div>

            <div className="mi-cols">
                <div className="mi-stack">
                    <div className="mi-sum">
                        <span className="lab">DRAWING FROM</span>
                        {filters.map((f) => (
                            <span key={f.key} className={`mi-fc${f.exc ? " exc" : ""}`}>
                                {f.exc ? "not " : ""}{f.label}
                                <button aria-label={`Remove ${f.label}`} onClick={f.remove}>✕</button>
                            </span>
                        ))}
                        <span className="meter"><b>{pool.length}</b> can be drawn</span>
                    </div>

                    <section className="mi-card">
                        <div className="mi-tabs" role="tablist">
                            {tabs.map(([k, label, n]) => (
                                <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>{label}<small>{n}</small></button>
                            ))}
                        </div>

                        {tab === "pool" && (
                            <>
                                <div className="mi-sec">
                                    <span className="t">WHERE PROBLEMS COME FROM <em>{c.lists.length} selected</em></span>
                                    <div className="mi-chips">
                                        {MOCK_LISTS.map(([key, label]) => (
                                            <button key={key} className={`mi-chip${c.lists.includes(key) ? " on" : ""}`} aria-pressed={c.lists.includes(key)} onClick={() => edit((cur) => ({ ...cur, lists: cur.lists.includes(key) ? (cur.lists.length > 1 ? toggle(cur.lists, key) : cur.lists) : [...cur.lists, key] }))}>
                                                {label}<small>{listCount(key)}</small>
                                            </button>
                                        ))}
                                    </div>
                                    <p className="mi-hint">Practice = problems outside the lists. Premium is always left out.</p>
                                </div>
                                <div className="mi-sec">
                                    <span className="t">YOUR HISTORY <em>which of those can be drawn</em></span>
                                    <Seg label="History" value={c.status} options={MOCK_STATUS.map(([k, l]) => [k, l])} onPick={(status) => edit((cur) => ({ ...cur, status }))} />
                                </div>
                                <div className="mi-sec">
                                    <span className="t">HOW TO DRAW</span>
                                    <Seg label="How to draw" value={c.favour} options={FAVOUR.map(([k, l]) => [k, l])} onPick={(favour) => edit((cur) => ({ ...cur, favour }))} />
                                </div>
                            </>
                        )}

                        {tab === "topics" && (
                            <div className="mi-sec">
                                <span className="t">TOPICS <em>{included || excluded ? `${included} included · ${excluded} excluded` : "click: include · exclude · clear"}</em></span>
                                <div className="mi-chips">
                                    {data.patterns.map((pt) => {
                                        const v = c.topics[pt.code];
                                        return (
                                            <button key={pt.code} className={`mi-chip${v === 1 ? " on" : v === -1 ? " exc" : ""}`} onClick={() => cycleTopic(pt.code)} aria-label={`${pt.name}: ${v === 1 ? "included" : v === -1 ? "excluded" : "allowed"}`}>
                                                {pt.name}<small>{byTopic.get(pt.code) ?? 0}</small>
                                            </button>
                                        );
                                    })}
                                </div>
                                <p className="mi-hint">No topic included = all topics except the excluded.</p>
                            </div>
                        )}

                        {tab === "companies" && (
                            <>
                                <div className="mi-sec">
                                    <span className="t">GROUPS</span>
                                    <div className="mi-chips">
                                        {data.company_groups.map((g) => (
                                            <button key={g.name} className={`mi-chip${c.groups.includes(g.name) ? " on" : ""}`} aria-pressed={c.groups.includes(g.name)} onClick={() => edit((cur) => ({ ...cur, groups: toggle(cur.groups, g.name) }))}>
                                                {g.name}<small>{byCompany.groups.get(g.name) ?? 0}</small>
                                            </button>
                                        ))}
                                    </div>
                                </div>
                                <div className="mi-sec">
                                    <span className="t">COMPANIES <em>{c.groups.length + c.companies.length ? [...c.groups, ...c.companies].slice(0, 3).join(", ") + (c.groups.length + c.companies.length > 3 ? ` +${c.groups.length + c.companies.length - 3}` : "") : "any company"}</em></span>
                                    <div className="mi-chips">
                                        {shownCo.map((x) => (
                                            <button key={x.name} className={`mi-chip${c.companies.includes(x.name) ? " on" : ""}`} aria-pressed={c.companies.includes(x.name)} onClick={() => edit((cur) => ({ ...cur, companies: toggle(cur.companies, x.name) }))}>
                                                {x.name}<small>{x.n}</small>
                                            </button>
                                        ))}
                                        {!q && matching.length > 8 && <button className="mi-chip more" onClick={() => setAllCo(!allCo)}>{allCo ? "show fewer" : `+${matching.length - 8} more`}</button>}
                                    </div>
                                    <input className="mi-search" value={coQuery} onChange={(e) => setCoQuery(e.target.value)} placeholder="Find a company" aria-label="Find a company" />
                                </div>
                                <Switch on={c.recent} onClick={() => edit((cur) => ({ ...cur, recent: !cur.recent }))} title="Asked in the last 6 months" hint="A problem can be drawn if any selected company asked it recently." />
                            </>
                        )}

                        {tab === "clock" && (
                            <>
                                <div className="mi-sec">
                                    <span className="t">FORMAT</span>
                                    <Seg label="Timer format" value={c.format} options={[["total", "Countdown for the round"], ["per", "Countdown per problem"], ["up", "Stopwatch only"]]} onPick={(format) => edit((cur) => ({ ...cur, format }))} />
                                </div>
                                {c.format === "up" ? (
                                    <p className="mi-hint">No limit. The clock counts up ({c.per.join(" / ")} min suggested).</p>
                                ) : (
                                    <div className="mi-fields">
                                        {c.format === "total" && (
                                            <div><label>ROUND LENGTH</label><Num label="minutes" unit="min" value={c.total} min={10} max={180} step={5} onChange={(total) => edit((cur) => ({ ...cur, total }))} /></div>
                                        )}
                                        {BANDS.map((band, i) => (
                                            <div key={band}><label>{c.format === "total" ? "SUGGESTED, " : ""}{band.toUpperCase()}</label><Num label={`${band} minutes`} unit="min" value={c.per[i] ?? 0} min={3} max={90} onChange={(n) => edit((cur) => ({ ...cur, per: cur.per.map((x, j) => (j === i ? n : x)) as MockConfig["per"] }))} /></div>
                                        ))}
                                    </div>
                                )}
                                <div className="mi-togs">
                                    <Switch on={c.strict} onClick={() => edit((cur) => ({ ...cur, strict: !cur.strict }))} title="Strict: no pause" hint="The clock keeps running, like a real call. Turn off to allow Pause." />
                                    <Switch on={c.blind} onClick={() => edit((cur) => ({ ...cur, blind: !cur.blind }))} title="Blind mode" hint="Hide topic, tags and companies until you log the problem." />
                                    <Switch on={c.auto} disabled={c.format === "up"} onClick={() => edit((cur) => ({ ...cur, auto: !cur.auto }))} title={c.format === "per" ? "Move on at 0:00" : "End the round at 0:00"} hint={c.format === "per" ? "Off: the problem's clock turns red and counts overtime." : "Off: the clock turns red and counts overtime, and you decide when to stop."} />
                                    <Switch on={c.warns} disabled={c.format === "up"} onClick={() => edit((cur) => ({ ...cur, warns: !cur.warns }))} title="Time warnings" hint="A banner at 10, 5 and 1 minute left." />
                                </div>
                            </>
                        )}
                    </section>

                    {data.rounds.length > 0 && (
                        <section className="mi-card">
                            <h3 className="mi-h3"><span>PAST ROUNDS</span><em>repeat any of them</em></h3>
                            <div className="mi-tbl">
                                <div className="mi-tr th"><span>DATE</span><span>ROUND</span><span>RESULT</span><span>TIME</span><span /></div>
                                {data.rounds.map((r) => (
                                    <div key={r.id} className="mi-tr">
                                        <span>{new Date(r.finished_at).toLocaleDateString(undefined, { day: "numeric", month: "short" })}</span>
                                        <span>{describe(r.config, data)}</span>
                                        <span className="res">{r.items.map((i, k) => <i key={k} className={`dot ${i.grade === "good" || i.grade === "easy" ? "g" : i.grade === "hard" ? "h" : i.grade === "again" ? "f" : "s"}`}>{glyphOf(i.grade)}</i>)}</span>
                                        <span>{mmss(r.seconds)}{r.config.format === "total" ? ` / ${mmss(r.config.total * 60)}` : ""}</span>
                                        <button className="mi-ghost" onClick={() => { setC(r.config); setPreset("custom"); window.scrollTo({ top: 0, behavior: "smooth" }); }}>Repeat</button>
                                    </div>
                                ))}
                            </div>
                        </section>
                    )}
                </div>

                <aside className="mi-ticket">
                    <div className="top">
                        <div className="big"><b>{need}</b><span>{need === 1 ? "problem" : "problems"} · {c.format === "total" ? `${c.total} min` : c.format === "per" ? "per problem" : "no limit"}</span></div>
                        <div className="bands">
                            {BANDS.map((band, i) => <span key={band} className={`band ${band[0]}`}>{DIFF[band][0]} {have[i]}</span>)}
                            <span className="note">can be drawn</span>
                        </div>
                    </div>
                    <div className="slots">
                        {slots.length === 0 && <p className="mi-hint" style={{ textAlign: "center", padding: "10px 0" }}>Add at least one problem.</p>}
                        {slots.map((band, j) => (
                            <div key={j} className="slot">
                                <i>{j + 1}</i>
                                <span className="ti">{c.anyDiff ? <b className="a">Any</b> : <b className={BANDS[band]![0]}>{DIFF[BANDS[band]!][0]}</b>} · hidden until you start</span>
                                <span className="tm">~{c.per[c.anyDiff ? 1 : band]}m</span>
                                <button aria-label={`Remove problem ${j + 1}`} disabled={c.anyDiff && c.anyCount <= 1} onClick={() => removeSlot(band)}>✕</button>
                            </div>
                        ))}
                    </div>
                    <div className="adds">
                        {c.anyDiff ? (
                            <button style={{ gridColumn: "1 / -1" }} disabled={c.anyCount >= 6} onClick={() => edit((cur) => ({ ...cur, anyCount: cur.anyCount + 1 }))}>+ Any difficulty<small>{pool.length} can be drawn</small></button>
                        ) : (
                            BANDS.map((band, i) => (
                                <button key={band} disabled={(c.mix[i] ?? 0) >= have[i]! || (c.mix[i] ?? 0) >= 4} onClick={() => addSlot(i)}>+ {DIFF[band][0]}<small>{have[i]} can be drawn</small></button>
                            ))
                        )}
                    </div>
                    <div className="anyrow"><Switch on={c.anyDiff} onClick={() => edit((cur) => ({ ...cur, anyDiff: !cur.anyDiff }))} title="Any difficulty" hint="Draw the number of problems without caring about easy, medium or hard." /></div>
                    <dl className="kv">
                        <div><dt>Time at suggested pace</dt><dd>{perTotal} min</dd></div>
                        <div><dt>Timer</dt><dd>{timerName}</dd></div>
                        <div><dt>Pause</dt><dd>{c.strict ? "not allowed" : "allowed"}</dd></div>
                        <div><dt>Topic and tags</dt><dd>{c.blind ? "hidden" : "shown"}</dd></div>
                    </dl>
                    {warnings.map((w) => <div key={w.text} className={w.cls}>{w.text}</div>)}
                    <button className="mi-go" disabled={!!problem} onClick={() => onStart(c)}>Draw and start the clock →</button>
                    <div className="foot">
                        {naming ? (
                            <form
                                onSubmit={(e) => {
                                    e.preventDefault();
                                    const n = name.trim();
                                    if (!n) return;
                                    savePresets.mutate([...presets.filter((p) => p.name !== n), { name: n, config: c }]);
                                    setPreset(`saved:${n}`);
                                    setNaming(false);
                                    setName("");
                                }}
                            >
                                <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name this preset" aria-label="Preset name" maxLength={40} autoFocus />
                                <div className="row"><button className="mi-ghost" type="submit" disabled={!name.trim()}>Save</button><button className="mi-ghost" type="button" onClick={() => setNaming(false)}>Cancel</button></div>
                            </form>
                        ) : (
                            <button className="mi-ghost" onClick={() => setNaming(true)}>Save as a preset</button>
                        )}
                        <p className="mi-note">The settings of your last round are remembered the next time you open this page.</p>
                    </div>
                </aside>
            </div>
            <div className="m-dock">
                <span><b>{pool.length}</b> can be drawn · {need} problem{need === 1 ? "" : "s"}</span>
                <button className="mi-go" disabled={!!problem} onClick={() => onStart(c)}>Start →</button>
            </div>
        </div>
    );
}

/** One line for a round's settings: "2 medium · Trees, Graphs · NeetCode 150". */
function describe(c: MockConfig, data: MockData): string {
    const parts: string[] = [];
    parts.push(c.anyDiff ? `${c.anyCount} problem${c.anyCount === 1 ? "" : "s"}` : BANDS.flatMap((b, i) => ((c.mix[i] ?? 0) > 0 ? [`${c.mix[i]} ${b}`] : [])).join(", ") || "no problems");
    const topics = Object.entries(c.topics).filter(([, v]) => v === 1).map(([code]) => data.patterns.find((p) => p.code === code)?.name ?? code);
    parts.push(topics.length ? topics.join(", ") : "any topic");
    parts.push(c.lists.map((l) => MOCK_LISTS.find(([k]) => k === l)?.[1] ?? l).join(" + "));
    if (c.groups.length + c.companies.length) parts.push([...c.groups, ...c.companies].join(", "));
    return parts.join(" · ");
}

const glyphOf = (g: Grade | null) => (g === "good" || g === "easy" ? "✓" : g === "hard" ? "½" : g === "again" ? "✗" : "–");

// ---------------------------------------------------------------- live

function useNow(running: boolean): number {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        setNow(Date.now());
        if (!running) return;
        const t = window.setInterval(() => setNow(Date.now()), 250);
        return () => window.clearInterval(t);
    }, [running]);
    return now;
}

const WARN_TEXT: Record<number, string> = { 600: "10 minutes left.", 300: "5 minutes left.", 60: "1 minute left." };

function Live({ data, round, update, onFinish }: { data: MockData; round: Round; update: (fn: (r: Round) => Round) => void; onFinish: (r: Round) => void }) {
    const now = useNow(round.pausedAt == null);
    const byId = useMemo(() => new Map(data.problems.map((p) => [p.id, p])), [data.problems]);
    const c = round.config;
    const index = currentIndex(round);
    const problem = byId.get(round.ids[index] ?? "");
    const logged = isLogged(round);
    const band = problem?.difficulty ?? null;
    const time = clock(round, band, now);
    const limit = limitOf(c, band);
    const [banner, setBanner] = useState<string | null>(null);
    const [confirmEnd, setConfirmEnd] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const last = useRef(time.value);
    const lastKey = useRef(`${index}`);

    const advance = useCallback(
        (fn: (r: Round) => Round) => {
            update(fn);
        },
        [update],
    );

    // Warnings as the clock crosses 10, 5 and 1 minute, and the moves at 0:00.
    useEffect(() => {
        const key = `${index}`;
        if (lastKey.current !== key) {
            lastKey.current = key;
            last.current = time.value;
            setBanner(null);
            return;
        }
        if (c.format !== "up" && c.warns) {
            const w = warning(last.current, time.value, limit);
            if (w != null) {
                setBanner(WARN_TEXT[w] ?? null);
                const t = window.setTimeout(() => setBanner(null), 9000);
                last.current = time.value;
                return () => window.clearTimeout(t);
            }
        }
        last.current = time.value;
    }, [time.value, index, c.format, c.warns, limit]);

    useEffect(() => {
        if (!c.auto || c.format === "up" || time.value > 0 || round.pausedAt != null) return;
        if (c.format === "total") {
            onFinish(round);
        } else if (!logged) {
            const to = elapsed(round, Date.now());
            if (index + 1 < round.ids.length) advance((r) => ({ ...r, marks: [...r.marks, { grade: null, to, due: null }], starts: [...r.starts, to] }));
            else onFinish({ ...round, marks: [...round.marks, { grade: null, to, due: null }] });
        }
    }, [time.value, c.auto, c.format, round, logged, index, advance, onFinish]);

    useEffect(() => {
        const before = document.title;
        document.title = `${mmss(time.value)} · Mock round`;
        return () => {
            document.title = before;
        };
    }, [time.value]);

    if (!problem) {
        return (
            <div className="wrap" style={{ paddingBlock: 28 }}>
                <p className="rempty">A problem in this round is no longer in the catalog.</p>
                <button className="mi-ghost" onClick={() => onFinish(round)}>End the round</button>
            </div>
        );
    }

    const log = async (grade: Grade) => {
        if (logged || busy) return;
        setBusy(true);
        setError(null);
        const to = elapsed(round, Date.now());
        try {
            const out = await api.logDsa(problem.id, grade);
            advance((r) => ({ ...r, marks: [...r.marks, { grade, to, due: out.due }] }));
        } catch (e) {
            setError((e as Error).message);
        } finally {
            setBusy(false);
        }
    };
    const skip = () => {
        const to = elapsed(round, Date.now());
        if (index + 1 < round.ids.length) advance((r) => ({ ...r, marks: [...r.marks, { grade: null, to, due: null }], starts: [...r.starts, to] }));
        else onFinish({ ...round, marks: [...round.marks, { grade: null, to, due: null }] });
    };
    const next = () => {
        const to = elapsed(round, Date.now());
        if (index + 1 < round.ids.length) advance((r) => ({ ...r, starts: [...r.starts, to] }));
        else onFinish(round);
    };
    const togglePause = () =>
        update((r) => {
            const t = Date.now();
            return r.pausedAt == null ? { ...r, pausedAt: t } : { ...r, pausedMs: r.pausedMs + (t - r.pausedAt), pausedAt: null };
        });

    const total = elapsed(round, now);
    const sug = suggested(c, problem.difficulty);
    const spent = onProblem(round, now);
    const mark = round.marks[index];
    const overall = time.value < 0 && time.counting === "down";
    const showMeta = logged || !c.blind;
    const patternName = data.patterns.find((p) => p.code === problem.pattern)?.name ?? "";
    const lastOne = index + 1 >= round.ids.length;
    const clockLabel = c.format === "per" ? `PROBLEM ${index + 1} · ${time.value < 0 ? "OVERTIME" : "LEFT"}` : c.format === "up" ? "ELAPSED" : overall ? "OVERTIME" : "MOCK ROUND · LIVE";


    const C = 2 * Math.PI * 46;
    const frac = c.format === "total" ? time.value / (c.total * 60) : c.format === "per" ? time.value / Math.max(1, sug) : spent / Math.max(1, sug);
    const ringCls = time.counting === "down" && time.value < 0 ? " over" : time.counting === "down" && time.value <= 300 && limit > 300 ? " low" : "";
    const paceAll = round.ids.reduce((n, id) => n + suggested(c, byId.get(id)?.difficulty ?? "medium"), 0);
    const clockText = mmss(time.value);

    return (
        <div className="mi-wrap mi-room">
            <div className="mi-rbar">
                <div className={`mi-ring${ringCls}`} aria-live="off">
                    <svg viewBox="0 0 108 108" aria-hidden="true"><circle className="bg" cx="54" cy="54" r="46" /><circle className="fg" cx="54" cy="54" r="46" strokeDasharray={C} strokeDashoffset={C * (1 - Math.min(1, Math.max(0, frac)))} /></svg>
                    <div className="c"><b className={clockText.length > 5 ? "long" : ""}>{clockText}</b><small>{clockLabel.replace("MOCK ROUND · LIVE", "LEFT")}</small></div>
                </div>
                <div className="mi-segbar">
                    <div className="lab"><span>MOCK ROUND · LIVE</span><span>{mmss(total)} used · {Math.round(paceAll / 60)} min suggested</span></div>
                    <div className="segs" aria-label="Problems">
                        {round.ids.map((id, i) => {
                            const m = round.marks[i];
                            const b = byId.get(id)?.difficulty ?? "medium";
                            const tone = !m ? "" : m.grade === "again" ? " no" : m.grade === "hard" ? " help" : m.grade ? " ok" : " skip";
                            const here = i === index && !m;
                            const w = here ? Math.min(100, (spent / Math.max(1, suggested(c, b))) * 100) : 0;
                            return (
                                <div key={id} className={`sg${here ? " cur" : ""}${m ? " done" : ""}${tone}`} style={{ flex: suggested(c, b) }}>
                                    <span className="fill" style={{ width: `${w}%` }} />
                                    <b className={`m-${b[0]}`}>{i + 1} · {DIFF[b][0]}</b>
                                    <small>{m ? glyphOf(m.grade) : `~${Math.round(suggested(c, b) / 60)}m`}</small>
                                </div>
                            );
                        })}
                    </div>
                </div>
                <div className="mi-rctl">
                    {!c.strict && <button className="mi-ghost" onClick={togglePause}>{round.pausedAt == null ? "Pause" : "Resume"}</button>}
                    {confirmEnd ? (
                        <>
                            <button className="mi-ghost" onClick={() => onFinish(round)}>End now</button>
                            <button className="mi-ghost" onClick={() => setConfirmEnd(false)}>Keep going</button>
                        </>
                    ) : (
                        <button className="mi-ghost" onClick={() => setConfirmEnd(true)}>End round</button>
                    )}
                </div>
            </div>
            {banner && <div className="mi-banner" role="status">{banner}</div>}
            {round.pausedAt != null && <div className="mi-banner" role="status">Paused. The clock is stopped until you resume.</div>}

            <div className="mi-stage">
                <section className="mi-card mi-prob">
                    <span className="lab">PROBLEM {index + 1} OF {round.ids.length} · {DIFF[problem.difficulty][0].toUpperCase()} · SUGGESTED {Math.round(sug / 60)} MIN</span>
                    <h2 className="pt">{showMeta ? `#${problem.number} ` : ""}{problem.title}</h2>
                    <a className="open" href={leetcode(problem.slug)} target="_blank" rel="noopener noreferrer">Open on LeetCode ↗</a>
                    <div className="ptime">
                        <div className="row"><span><b>{mmss(spent)}</b> on this problem</span><span>{mmss(sug)} suggested</span></div>
                        <div className="track"><i className={spent > sug ? "over" : ""} style={{ width: `${Math.min(100, (spent / Math.max(1, sug)) * 100)}%` }} /></div>
                    </div>
                    {showMeta ? (
                        <div className="meta">
                            <span>{patternName}</span>
                            {problem.tags.slice(0, 5).map((t) => <span key={t}>{t}</span>)}
                            {problem.companies.length > 0 && <span className="co">Asked by {problem.companies.slice(0, 4).map((x) => x.name).join(", ")}{problem.companies.length > 4 ? ` +${problem.companies.length - 4}` : ""}</span>}
                        </div>
                    ) : (
                        <div className="blind"><span className="lock" aria-hidden="true">🔒</span><span><b>Blind mode.</b> Number, topic and companies stay hidden until you log it.</span></div>
                    )}
                </section>

                <div className="mi-side">
                    <section className="mi-card">
                        <h3 className="mi-h3"><span>LOG PROBLEM {index + 1}</span><em>same as a normal attempt</em></h3>
                        <div className="mi-log3">
                            {([["good", "✓", "on my own", "ok"], ["hard", "½", "with help", "half"], ["again", "✗", "not yet", "no"]] as const).map(([g, glyph, label, cls]) => (
                                <button key={g} className={`lg ${cls}${mark?.grade === g && logged ? " on" : ""}`} disabled={logged || busy} onClick={() => void log(g)}>
                                    <b>{glyph}</b><span>{label}</span>
                                </button>
                            ))}
                        </div>
                        {error && <div className="mi-bad">{error}</div>}
                        {logged && mark && mark.grade && (
                            <div className="mi-logged">Logged {glyphOf(mark.grade)}. {mark.due ? `Review ${niceDate(mark.due, data.today)}.` : "Practice problems schedule no reviews."}</div>
                        )}
                        <button className="mi-go" disabled={!logged} onClick={next}>{lastOne ? "Finish the round →" : "Next problem →"}</button>
                        {!logged && <button className="mi-ghost" onClick={skip}>Skip this problem</button>}
                        <p className="mi-note">Logging stops the clock and shows the topic. Skipping logs nothing.</p>
                    </section>
                    <section className="mi-card">
                        <h3 className="mi-h3"><span>THIS ROUND</span><em>{round.marks.filter((m) => m.grade).length} of {round.ids.length} logged</em></h3>
                        <ol className="mi-rr">
                            {round.ids.map((id, i) => {
                                const p = byId.get(id);
                                const m = round.marks[i];
                                const here = i === index;
                                const upcoming = i > index;
                                const took = m ? m.to - (round.starts[i] ?? 0) : null;
                                const tone = !m ? "" : m.grade === "again" ? "no" : m.grade === "hard" ? "half" : m.grade ? "ok" : "skip";
                                return (
                                    <li key={id} className={`rr${here ? " cur" : ""}${upcoming ? " later" : ""}`}>
                                        <i>{i + 1}</i>
                                        <span>
                                            {upcoming || !p ? (
                                                <><b className={`m-${p?.difficulty[0] ?? "m"}`}>{p ? DIFF[p.difficulty][0] : "Problem"}</b> <span className="dim">· hidden until you get there</span></>
                                            ) : here && !m && c.blind ? "Working on it now" : p.title}
                                        </span>
                                        <span className={`m ${tone}`}>{m ? (m.grade ? `${glyphOf(m.grade)} ${mmss(took ?? 0)}` : "skipped") : here ? "now" : `~${p ? Math.round(suggested(c, p.difficulty) / 60) : "?"}m`}</span>
                                    </li>
                                );
                            })}
                        </ol>
                    </section>
                </div>
            </div>
        </div>
    );
}

// ---------------------------------------------------------------- result

function Result({ data, round, onAgain, onChange }: { data: MockData; round: Round; onAgain: () => void; onChange: () => void }) {
    const byId = useMemo(() => new Map(data.problems.map((p) => [p.id, p])), [data.problems]);
    const c = round.config;
    const { items, seconds } = summary(round, 0);
    const own = items.filter((i) => i.grade === "good" || i.grade === "easy").length;
    const help = items.filter((i) => i.grade === "hard").length;
    const failed = items.filter((i) => i.grade === "again").length;
    const skipped = items.filter((i) => !i.grade).length;
    const pace = items.reduce((n, it) => n + suggested(c, byId.get(it.id)?.difficulty ?? "medium"), 0);
    const diff = seconds - pace;
    const spare = c.format === "total" ? c.total * 60 - seconds : null;
    const parts = [own ? `${own} solved on your own` : "", help ? `${help} with help` : "", failed ? `${failed} not yet` : "", skipped ? `${skipped} skipped` : ""].filter(Boolean);
    const C = 2 * Math.PI * 62;
    return (
        <div className="mi-wrap">
            <div className="eyebrow">
                <Link to="/dsa" style={{ color: "var(--ca)" }}>DSA</Link>
                <span>/</span>
                <span style={{ color: "var(--ca)" }}>MOCK INTERVIEW</span>
                <span>/</span>
                <span>ROUND FINISHED</span>
            </div>
            <div className="mi-hero">
                <div className="mi-score">
                    <svg viewBox="0 0 140 140" aria-hidden="true"><circle className="bg" cx="70" cy="70" r="62" /><circle className="fg" cx="70" cy="70" r="62" strokeDasharray={C} strokeDashoffset={C * (1 - (items.length ? own / items.length : 0))} /></svg>
                    <div className="c"><b>{own}/{items.length}</b><small>ON MY OWN</small></div>
                </div>
                <div>
                    <h1 className="mi-h2">{parts.length ? `${parts.join(", ")}.` : "Round ended."}</h1>
                    <p className="mi-sub" style={{ marginTop: 8 }}>
                        {spare != null && <b style={{ color: spare >= 0 ? "var(--grn)" : "var(--warn)" }}>{spare >= 0 ? `Finished with ${mmss(spare)} to spare. ` : `${mmss(-spare)} over the time. `}</b>}
                        {describe(c, data)}
                    </p>
                </div>
            </div>

            <div className="mi-stats">
                <div className="stat"><small>TIME USED</small><b>{mmss(seconds)}</b><span>{c.format === "total" ? `of ${mmss(c.total * 60)}` : c.format === "per" ? "per-problem countdowns" : "stopwatch"}</span></div>
                <div className="stat"><small>SOLVED ON MY OWN</small><b>{own} / {items.length}</b><span>{help} with help, {failed} not yet{skipped ? `, ${skipped} skipped` : ""}</span></div>
                <div className="stat"><small>AGAINST SUGGESTED</small><b style={{ color: diff <= 0 ? "var(--grn)" : "var(--warn)" }}>{diff <= 0 ? "−" : "+"}{mmss(Math.abs(diff))}</b><span>{mmss(pace)} suggested in all</span></div>
                <div className="stat"><small>SETTINGS</small><b style={{ fontSize: 16, paddingTop: 6 }}>{c.strict ? "strict" : "pausable"} · {c.blind ? "blind" : "open"}</b><span>{c.favour === "random" ? "uniform draw" : c.favour === "weak" ? "favoured weak ones" : "favoured often-asked"}</span></div>
            </div>

            <section className="mi-card">
                <h3 className="mi-h3"><span>THE PROBLEMS</span><em>open any to read it on LeetCode</em></h3>
                {items.map((it, i) => {
                    const p = byId.get(it.id);
                    if (!p) return null;
                    const sug = suggested(c, p.difficulty);
                    const cls = it.grade === "good" || it.grade === "easy" ? "ok" : it.grade === "hard" ? "half" : it.grade === "again" ? "no" : "skip";
                    const mark = round.marks[i];
                    const practice = p.lists.every((l) => l === "practice");
                    return (
                        <div key={it.id} className="mi-prow">
                            <span className={`g ${cls}`}>{glyphOf(it.grade)}</span>
                            <div>
                                <a className="t1" href={leetcode(p.slug)} target="_blank" rel="noopener noreferrer">#{p.number} {p.title} ↗</a>
                                <small>{data.patterns.find((x) => x.code === p.pattern)?.name} · <span className={`m-${p.difficulty[0]}`}>{DIFF[p.difficulty][0]}</span>{p.companies.length ? ` · ${p.companies.slice(0, 3).map((x) => x.name).join(", ")}` : ""}</small>
                            </div>
                            <div className="tm">
                                <div className="row"><span><b>{it.seconds ? mmss(it.seconds) : "–"}</b> {it.seconds ? "used" : "not opened"}</span><span>{mmss(sug)} suggested</span></div>
                                <div className="track"><i className={it.seconds > sug ? "over" : ""} style={{ width: `${Math.min(100, (it.seconds / Math.max(1, sug)) * 100)}%` }} /></div>
                            </div>
                            <span className="next">{!it.grade ? "Not logged" : practice ? "No review (practice)" : mark?.due ? `Review ${niceDate(mark.due, data.today)}` : "Logged"}</span>
                        </div>
                    );
                })}
            </section>

            <div className="mi-acts">
                <button className="mi-go wide" onClick={onAgain}>Another round, same settings →</button>
                <button className="mi-ghost tall" onClick={onChange}>Change settings</button>
            </div>
            <p className="mi-note">Problems you logged ½ or ✗ become more likely when you choose "Favour my weak ones".</p>
        </div>
    );
}
