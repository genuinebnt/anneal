import { Link, useRouterState } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { Header } from "./Header";

/** A small crystal lattice, anneal's picture of order: for a missing page one node is gone (a dashed ring where it should be); for a
 *  crash a crack runs through it. */
function Lattice({ kind }: { kind: "missing" | "cracked" }) {
    const nodes: [number, number][] = [[30, 40], [100, 40], [170, 40], [30, 100], [170, 100], [30, 160], [100, 160], [170, 160]];
    return (
        <svg className={`es-lat es-${kind}`} viewBox="0 0 200 200" aria-hidden="true">
            <g className="es-bonds">
                <path d="M30 40H170M30 160H170M30 40V160M170 40V160" />
                {kind === "missing" ? <path d="M30 100H88M112 100H170M100 40V88M100 112V160" /> : <path d="M30 100H170M100 40V160" />}
            </g>
            {kind === "cracked" && <path className="es-crack" d="M118 22 104 70 126 92 96 132 112 178" />}
            {nodes.map(([x, y]) => <circle key={`${x}-${y}`} className="es-node" cx={x} cy={y} r="7" />)}
            {kind === "missing" ? <circle className="es-gone" cx="100" cy="100" r="11" /> : <circle className="es-hurt" cx="100" cy="100" r="7" />}
        </svg>
    );
}

/** Shown instead of a blank page when something in a page throws: progress is safe, what to do, and the details folded away to copy. */
export function ErrorScreen({ error, reset }: { error: unknown; reset?: () => void }) {
    const detail = `${(error as Error)?.name ?? "Error"}: ${(error as Error)?.message ?? String(error)}\n${location.href}\n${((error as Error)?.stack ?? "").split("\n").slice(1, 6).join("\n")}`;
    const [copied, setCopied] = useState(false);
    const [at] = useState(() => new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
    return (
        <main className="page es-page" role="alert">
            <div className="es-card">
                <Lattice kind="cracked" />
                <div className="es-body">
                    <span className="es-k es-bad">SOMETHING BROKE ON THIS PAGE</span>
                    <h1>This page tripped. Your progress is safe.</h1>
                    <p>Everything you logged is already saved on the server. Trying again usually fixes it.</p>
                    <div className="es-acts">
                        <button className="es-go" onClick={() => (reset ? reset() : location.reload())}>Try again</button>
                        <button onClick={() => location.reload()}>Reload</button>
                        <button onClick={() => (history.length > 1 ? history.back() : location.assign("/dsa"))}>Back to where I was</button>
                    </div>
                    <details className="es-what">
                        <summary><span>What happened</span><em>{at} · {location.pathname}</em></summary>
                        <pre>{detail}</pre>
                        <button onClick={() => void navigator.clipboard?.writeText(detail).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1500); }, () => undefined)}>{copied ? "Copied" : "Copy the details"}</button>
                    </details>
                    <small className="es-note">The details stay on this device.</small>
                </div>
            </div>
        </main>
    );
}

/** Bigram overlap of two strings (Dice's coefficient): 1 for equal, near 0 for unrelated. Good at typos and dropped letters. */
function likeness(a: string, b: string): number {
    const grams = (s: string) => {
        const t = s.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
        const out = new Map<string, number>();
        for (let i = 0; i < t.length - 1; i++) out.set(t.slice(i, i + 2), (out.get(t.slice(i, i + 2)) ?? 0) + 1);
        return out;
    };
    const x = grams(a);
    const y = grams(b);
    let both = 0;
    for (const [g, n] of x) both += Math.min(n, y.get(g) ?? 0);
    const total = [...x.values()].reduce((s, n) => s + n, 0) + [...y.values()].reduce((s, n) => s + n, 0);
    return total ? (2 * both) / total : 0;
}

const AREA = (path: string) => (path.startsWith("/sd") ? "sd" : path.startsWith("/courses") ? "courses" : path.startsWith("/t/") || path.startsWith("/rust") ? "rust" : "dsa");

/** An address the app does not know: the address itself, the pages it most likely meant, and ways out. */
export function NotFound() {
    const path = useRouterState({ select: (s) => s.location.pathname });
    const dsa = useQuery({ queryKey: ["dsa"], queryFn: api.dsa, staleTime: 60_000 });
    const sd = useQuery({ queryKey: ["sd"], queryFn: api.sd, staleTime: 60_000 });
    const asked = decodeURIComponent(path.split("/").filter(Boolean).pop() ?? "");
    const guesses = useMemo(() => {
        if (!asked) return [];
        const all: { label: string; kind: string; to: string; tone?: string }[] = [
            ...(dsa.data?.problems ?? []).map((p) => ({ label: p.title, kind: `problem · ${p.difficulty}`, to: `/d/${p.slug}`, tone: p.difficulty })),
            ...(dsa.data?.patterns ?? []).map((p) => ({ label: p.name, kind: "pattern", to: `/dsa/patterns/${p.code}` })),
            ...(sd.data?.concepts ?? []).map((c) => ({ label: c.title, kind: "system design page", to: `/sd/learn/${c.id}` })),
            ...(sd.data?.problems ?? []).map((p) => ({ label: p.title, kind: "system design drill", to: `/sd/practice/${p.id}` })),
        ];
        return all
            .map((g) => ({ g, s: Math.max(likeness(asked, g.label), likeness(asked, g.to.split("/").pop() ?? "")) }))
            .filter((x) => x.s > 0.45)
            .sort((a, b) => b.s - a.s)
            .slice(0, 3)
            .map((x) => x.g);
    }, [asked, dsa.data, sd.data]);
    useEffect(() => {
        document.title = "Not found · anneal";
    }, []);
    return (
        <>
            <Header area={AREA(path)} />
            <main className="page es-page">
                <div className="es-card">
                    <Lattice kind="missing" />
                    <div className="es-body">
                        <span className="es-k">404 · NOT FOUND</span>
                        <h1>Nothing lives at this address.</h1>
                        <code className="es-path">{path}</code>
                        {guesses.length > 0 && (
                            <div className="es-guess">
                                <span>DID YOU MEAN</span>
                                {guesses.map((g) => (
                                    <Link key={g.to} to={g.to as never}>
                                        <b>{g.label}</b>
                                        <em className={g.tone ? `es-${g.tone.toLowerCase()}` : ""}>{g.kind}</em>
                                    </Link>
                                ))}
                            </div>
                        )}
                        <div className="es-acts">
                            <Link className="es-go" to={(AREA(path) === "sd" ? "/sd" : "/dsa") as never}>{AREA(path) === "sd" ? "Go to System design" : "Go to the DSA home"}</Link>
                            <button onClick={() => window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, ctrlKey: true, bubbles: true }))}>Search <kbd>⌘K</kbd></button>
                            <button className="es-quiet" onClick={() => history.back()}>← Back</button>
                        </div>
                    </div>
                </div>
            </main>
        </>
    );
}
