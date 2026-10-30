import type { CSSProperties, ReactNode } from "react";
import { Header } from "./Header";
import { MockRoot } from "./mock";

/** `hint` explains the number in plain words, on hover or keyboard focus. */
type Stat = { value: ReactNode; label: string; hint?: string };

/** The Problems page's header, for the other pages: a small eyebrow, a large title with one accent word, and a strip of the numbers that matter. */
export function PageHero({ eyebrow, title, accent, stats = [] }: { eyebrow: ReactNode; title: string; accent?: string; stats?: Stat[] }) {
    return (
        <section className="s-hero">
            <div>
                <div className="s-eye s-rv" style={{ "--i": 0 } as CSSProperties}>{eyebrow}</div>
                <h1 className="s-rv" style={{ "--i": 1 } as CSSProperties}>{title}{accent ? <> <em>{accent}</em></> : null}</h1>
            </div>
            {stats.length > 0 && (
                <div className="s-stats s-rv" style={{ "--i": 3 } as CSSProperties}>
                    {stats.map((s) => (
                        <div key={s.label}><b>{s.value}</b>{s.hint ? <span className="hinted" tabIndex={0} data-tip={s.hint} aria-label={`${s.label}: ${s.hint}`}>{s.label}</span> : <span>{s.label}</span>}</div>
                    ))}
                </div>
            )}
        </section>
    );
}

/** The frame those pages share with the Problems page: the DSA header, the page width and the accent. */
export function PageFrame({ children, area = "dsa" }: { children: ReactNode; area?: "dsa" | "rust" | "courses" }) {
    return (
        <>
            <Header area={area} />
            <MockRoot prefix="s-" className="s-dsa s-sub" style={{ "--s-ac": "var(--acc)", "--s-acb": "var(--acc-bg)", "--ca": "var(--acc)", "--cab": "var(--acc-bg)" } as CSSProperties}>
                <main className="s-wrap">{children}</main>
            </MockRoot>
        </>
    );
}
