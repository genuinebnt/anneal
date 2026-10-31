import { useQuery } from "@tanstack/react-query";
import { useState, type CSSProperties } from "react";
import { api, type SystemSample, type SystemView } from "../api";
import { Header } from "../components/Header";
import { HowFound, Kv, Page3, Panel, Tile } from "../components/layout";
import { Skeleton } from "../components/Skeleton";
import { Segmented } from "../components/ui";

/** The VPS and the app on it (GET /api/system): the host now and over the day, the containers, the database and the API's requests.
 *  It asks again every 15 seconds. The day's samples live in the API's memory, so a deploy starts the charts again. */
export function AnalyticsPage() {
    const q = useQuery({ queryKey: ["system"], queryFn: api.system, refetchInterval: 15_000 });
    const [range, setRange] = useState<"1h" | "6h" | "24h">("6h");
    const s = q.data;
    return (
        <>
            <Header />
            <div className="an-3" style={{ "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                {!s ? (
                    <Page3 crumb={["Analytics"]} title="Analytics" sub="The VPS, its containers and the app on it." center={q.isError ? <p className="notice bad">Couldn't read the server's numbers: {(q.error as Error).message}</p> : <Skeleton lines={8} />} />
                ) : (
                    <Body s={s} range={range} setRange={setRange} />
                )}
            </div>
        </>
    );
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const bytes = (n: number | null | undefined) => (n == null ? "–" : n >= GB ? `${(n / GB).toFixed(1)} GB` : `${Math.round(n / MB)} MB`);
const pct = (a: number, b: number) => (b > 0 ? Math.round((a / b) * 100) : 0);
const span = (secs: number | null | undefined) => {
    if (secs == null) return "–";
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
};
const clock = (t: number) => new Date(t * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

function Body({ s, range, setRange }: { s: SystemView; range: "1h" | "6h" | "24h"; setRange: (r: "1h" | "6h" | "24h") => void }) {
    const since = Date.now() / 1000 - { "1h": 3600, "6h": 6 * 3600, "24h": 86400 }[range];
    const samples = s.samples.filter((x) => x.t >= since);
    const minutes = s.app.minutes.filter((x) => x.t >= since);
    const mem = s.now.mem;
    const disk = s.now.disk;
    const reqs = minutes.reduce((n, m) => n + m.n, 0);
    const errs = minutes.reduce((n, m) => n + m.errors, 0);
    const avgMs = reqs ? Math.round(minutes.reduce((n, m) => n + m.ms_sum, 0) / reqs) : null;
    const series = (key: keyof SystemSample) => samples.map((x) => x[key] as number | null);

    const tiles = (
        <>
            <Tile em="VPS uptime" strong={span(s.host.uptime)} small="since the last reboot" />
            <Tile em="CPU" strong={s.now.cpu == null ? "–" : <>{Math.round(s.now.cpu)}<span>%</span></>} small={s.now.load ? `load ${s.now.load.map((l) => l.toFixed(2)).join(" ")}` : "load –"} />
            <Tile em="Memory" strong={mem ? <>{pct(mem.used, mem.total)}<span>%</span></> : "–"} small={mem ? `${bytes(mem.used)} of ${bytes(mem.total)}` : "not readable here"} />
            <Tile em="Disk" strong={disk ? <>{pct(disk.used, disk.total)}<span>%</span></> : "–"} small={disk ? `${bytes(disk.free)} free` : "not readable here"} />
        </>
    );

    const left = (
        <>
            <Panel title="Range">
                <Segmented className="an-seg" label="Range" value={range} options={[["1h", "1h"], ["6h", "6h"], ["24h", "24h"]] as const} onChange={setRange} />
                <p>Samples are taken every minute and kept for a day. A deploy starts them again.</p>
            </Panel>
            <Panel title="Health">
                <Kv
                    rows={[
                        ["Host", s.host.hostname ?? "–"],
                        ["OS", s.host.os ?? "–"],
                        ["Kernel", s.host.kernel ?? "–"],
                        ["CPUs", s.host.cpus ?? "–"],
                        ["App", `v${s.app.version} · up ${span(s.app.up)}`],
                    ]}
                />
                <div className="an-ok">
                    <span className="ok">API ok</span>
                    <span className="ok">DB ok · {s.db.connections} conn</span>
                    <span className={s.containers ? "ok" : "off"}>{s.containers ? "Docker ok" : "Docker not reachable"}</span>
                </div>
            </Panel>
        </>
    );

    const center = (
        <>
            <div className="an-kpis">
                <Kpi label="VPS CPU" value={s.now.cpu == null ? "–" : `${Math.round(s.now.cpu)}%`} values={series("cpu")} color="var(--grn)" note={s.host.cpus ? `${s.host.cpus} CPUs` : ""} />
                <Kpi label="Memory" value={mem ? `${pct(mem.used, mem.total)}%` : "–"} values={series("mem_used")} color="var(--vio)" note={mem ? `${bytes(mem.used)} of ${bytes(mem.total)}` : ""} />
                <Kpi label="Load" value={s.now.load ? s.now.load[0].toFixed(2) : "–"} values={series("load1")} color="var(--warn)" note="1 minute" />
                <Kpi label="Requests" value={String(reqs)} values={minutes.map((m) => m.n)} color="var(--acc)" note={`${errs} errors · ${avgMs ?? "–"} ms avg`} />
            </div>
            <div className="an-grid">
                <Panel title="CPU and load" aside={<span className="an-leg"><i style={{ background: "var(--grn)" }} />cpu % <i style={{ background: "var(--warn)" }} />load ×10</span>}>
                    <Lines times={samples.map((x) => x.t)} series={[{ v: series("cpu"), c: "var(--grn)" }, { v: series("load1").map((v) => (v == null ? null : v * 10)), c: "var(--warn)" }]} max={100} />
                </Panel>
                <Panel title="Memory and swap" aside={<span className="an-leg"><i style={{ background: "var(--vio)" }} />used <i style={{ background: "var(--bad)" }} />swap</span>}>
                    <Lines times={samples.map((x) => x.t)} series={[{ v: series("mem_used"), c: "var(--vio)" }, { v: series("swap_used"), c: "var(--bad)" }]} max={mem?.total ?? undefined} fill />
                </Panel>
            </div>
            <Panel title="Containers on the VPS" aside={<span>docker</span>}>
                {s.containers ? (
                    <div className="an-tbl" role="table" aria-label="Containers">
                        <div className="an-tr th" role="row"><span>Container</span><span>State</span><span>CPU</span><span>Memory</span><span>Network</span><span>Up</span></div>
                        {s.containers.map((c) => (
                            <div key={c.name} className="an-tr" role="row" title={c.image}>
                                <b>{c.name}</b>
                                <span><em className={`an-st ${c.state}`}>{c.state}</em></span>
                                <span className="an-bar"><i style={{ width: `${Math.min(100, parseFloat(c.cpu ?? "0"))}%`, background: "var(--grn)" }} /><small>{c.cpu ?? "–"}</small></span>
                                <span className="an-bar"><i style={{ width: `${Math.min(100, parseFloat(c.mem_pct ?? "0"))}%`, background: "var(--vio)" }} /><small>{c.mem?.split(" / ")[0] ?? "–"}</small></span>
                                <span className="an-mono">{c.net ?? "–"}</span>
                                <span className="an-mono an-dim">{c.status}</span>
                            </div>
                        ))}
                    </div>
                ) : (
                    <p>Docker isn't reachable from here (a laptop without the socket). On the VPS the app reads it through the mounted socket.</p>
                )}
            </Panel>
            <div className="an-grid">
                <Panel title="Requests per minute" aside={<span>{reqs} in {range}</span>}>
                    <Bars values={minutes.map((m) => m.n)} errors={minutes.map((m) => m.errors)} times={minutes.map((m) => m.t)} />
                </Panel>
                <Panel title="Database" aside={<span>postgres</span>}>
                    <Kv rows={[["Size", bytes(s.db.size)], ["Connections", s.db.connections], ...s.db.tables.slice(0, 5).map(([t, n]) => [t, n.toLocaleString()] as [string, string])]} />
                </Panel>
            </div>
            {disk && (
                <Panel title="Disk" aside={<span>{disk.path}</span>}>
                    <div className="an-disk"><i style={{ width: `${pct(disk.used, disk.total)}%` }} /></div>
                    <p>{bytes(disk.used)} used of {bytes(disk.total)} · {bytes(disk.free)} free</p>
                </Panel>
            )}
        </>
    );

    const right = (
        <>
            <Panel title="Live log" aside={<span className="an-live"><i />every 15 s</span>}>
                <div className="an-log">
                    {s.app.recent.length === 0 && <p>No requests yet.</p>}
                    {s.app.recent.map((h, i) => (
                        <div key={`${h.t}-${i}`} title={`${clock(h.t)} · ${h.method} ${h.path}`}>
                            <em className={h.status >= 500 ? "e5" : h.status >= 400 ? "e4" : h.status >= 300 ? "e3" : ""}>{h.status}</em>
                            <span>{h.method} {h.path}</span>
                            <small>{h.ms} ms</small>
                        </div>
                    ))}
                </div>
            </Panel>
            {s.app.image && (
                <Panel title="Running build">
                    <Kv rows={[["Image", s.app.image.split(":").pop() ?? s.app.image], ["Container", s.app.status ?? "–"]]} />
                </Panel>
            )}
            <HowFound
                title="Where these numbers come from"
                text="CPU, load, memory, swap and uptime are read from the host's /proc every minute; the disk from df on the work directory; containers from Docker through the mounted socket; the database from Postgres; requests are counted by the API itself. Nothing leaves the server."
                rows={[["Samples", `${s.samples.length} (1 a minute)`], ["Kept", "a day, in memory"], ["Requests", s.app.requests.toLocaleString()], ["Errors (5xx)", s.app.errors]]}
            />
        </>
    );

    return <Page3 crumb={["Analytics"]} title="Analytics" sub="The VPS, its containers and the app on it." tiles={tiles} left={left} center={center} right={right} label="Analytics" />;
}

/** A small line of the last values. */
function Spark({ values, color }: { values: (number | null)[]; color: string }) {
    const v = values.filter((x): x is number => x != null);
    if (v.length < 2) return <svg className="an-spark" viewBox="0 0 100 28" aria-hidden="true"><line x1="0" y1="26" x2="100" y2="26" stroke="var(--line2)" /></svg>;
    const max = Math.max(...v) || 1;
    const min = Math.min(...v);
    const pts = v.map((x, i) => `${(i / (v.length - 1)) * 100},${26 - ((x - min) / (max - min || 1)) * 22}`).join(" ");
    return (
        <svg className="an-spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden="true">
            <polyline points={pts} fill="none" stroke={color} strokeWidth="1.6" vectorEffect="non-scaling-stroke" />
        </svg>
    );
}

function Kpi({ label, value, values, color, note }: { label: string; value: string; values: (number | null)[]; color: string; note: string }) {
    return (
        <div className="an-kpi">
            <em>{label}</em>
            <strong>{value}</strong>
            <Spark values={values} color={color} />
            <small>{note}</small>
        </div>
    );
}

/** Lines over time, sharing one scale: `max` when given, else the largest value. */
function Lines({ times, series, max, fill }: { times: number[]; series: { v: (number | null)[]; c: string }[]; max?: number; fill?: boolean }) {
    const all = series.flatMap((s) => s.v).filter((x): x is number => x != null);
    if (times.length < 2 || all.length < 2) return <p className="an-empty">Waiting for samples: one a minute.</p>;
    const top = max ?? (Math.max(...all) || 1);
    const W = 600;
    const H = 150;
    const x = (i: number) => (i / (times.length - 1)) * W;
    const y = (v: number) => H - 6 - (Math.min(v, top) / top) * (H - 16);
    const ticks = [0, Math.floor((times.length - 1) / 2), times.length - 1];
    return (
        <div className="an-lines">
            <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
                {[0.25, 0.5, 0.75].map((f) => <line key={f} x1="0" x2={W} y1={H - 6 - f * (H - 16)} y2={H - 6 - f * (H - 16)} className="g" />)}
                {series.map((s, k) => {
                    const pts = s.v.map((v, i) => (v == null ? null : `${x(i)},${y(v)}`)).filter(Boolean).join(" ");
                    return (
                        <g key={k}>
                            {fill && k === 0 && <polygon points={`0,${H - 6} ${pts} ${W},${H - 6}`} fill={s.c} opacity=".12" />}
                            <polyline points={pts} fill="none" stroke={s.c} strokeWidth="1.8" vectorEffect="non-scaling-stroke" />
                        </g>
                    );
                })}
            </svg>
            <div className="an-ax">{ticks.map((i) => <span key={i}>{i === times.length - 1 ? "now" : clock(times[i]!)}</span>)}</div>
        </div>
    );
}

function Bars({ values, errors, times }: { values: number[]; errors: number[]; times: number[] }) {
    if (values.length === 0) return <p className="an-empty">No requests in this range.</p>;
    const top = Math.max(1, ...values);
    return (
        <div className="an-bars" aria-label="Requests per minute">
            {values.map((v, i) => (
                <i key={times[i]} title={`${clock(times[i]!)}: ${v} request${v === 1 ? "" : "s"}${errors[i] ? `, ${errors[i]} errors` : ""}`} className={errors[i] ? "err" : ""} style={{ height: `${Math.max(4, (v / top) * 100)}%` }} />
            ))}
        </div>
    );
}
