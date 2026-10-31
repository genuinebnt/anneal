// The page frame of the 2026 redesign (styles/layout.css): a title row with summary tiles, then a left panel, the centre and a right panel.
// On a phone the left panel is a drawer, opened from a bar under the header, and the right panel stacks below the centre.
import { Link, useRouterState } from "@tanstack/react-router";
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";

export function Page3({ crumb, title, sub, tiles, left, center, right, label, className = "" }: { crumb: ReactNode[]; title: ReactNode; sub?: ReactNode; tiles?: ReactNode; left?: ReactNode; center: ReactNode; right?: ReactNode; label?: string; className?: string }) {
    const [open, setOpen] = useState(false);
    const path = useRouterState({ select: (s) => s.location.pathname });
    useEffect(() => setOpen(false), [path]);
    useEffect(() => {
        if (!open) return;
        const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
        addEventListener("keydown", esc);
        return () => removeEventListener("keydown", esc);
    }, [open]);
    const cls = left && right ? "" : left ? " two" : right ? " tworight" : " one";
    return (
        <main className={`page ly-page${className ? ` ${className}` : ""}`}>
            {left && (
                <div className="ly-pbar">
                    <button type="button" onClick={() => setOpen(true)} aria-expanded={open}>
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
                            <rect x="1.5" y="2.5" width="13" height="11" rx="2" />
                            <path d="M6 2.5v11" />
                        </svg>
                        Panel
                    </button>
                    {label && <span>{label}</span>}
                </div>
            )}
            <div className="ly-head">
                <div className="ly-ttl">
                    <div className="ly-crumb">
                        {crumb.map((c, i) => (
                            <span key={i} style={{ display: "contents" }}>
                                {i > 0 && <i>/</i>}
                                <span>{c}</span>
                            </span>
                        ))}
                    </div>
                    <h1 className="ly-h1">{title}</h1>
                    {sub && <p className="ly-sub">{sub}</p>}
                </div>
                {tiles && <div className="ly-tiles">{tiles}</div>}
            </div>
            <div className={`ly-lay${cls}${open ? " open" : ""}`}>
                {left && <aside className="ly-lp" onClick={(e) => (e.target as HTMLElement).closest("a") && setOpen(false)}>{left}</aside>}
                <div className="ly-cp">{center}</div>
                {right && <aside className="ly-rp">{right}</aside>}
                {left && <div className="ly-scrim" onClick={() => setOpen(false)} />}
            </div>
        </main>
    );
}

type TileLink = { to: string; params?: Record<string, string>; search?: Record<string, unknown>; hash?: string };

/** A summary tile: a small label, a big value, a line under it. With `to` it is a link; `hot` gives it a colour. */
export function Tile({ em, strong, small, hot, color, to, title, onClick, hint }: { em: ReactNode; strong: ReactNode; small?: ReactNode; hot?: boolean; color?: string; to?: TileLink; title?: string; onClick?: () => void; hint?: string }) {
    const cls = `ly-tl${hot ? " hot" : ""}`;
    const style = color ? ({ "--c": color } as CSSProperties) : undefined;
    const body = (
        <>
            {hint ? <em className="hinted" tabIndex={0} data-tip={hint}>{em}</em> : <em>{em}</em>}
            <strong>{strong}</strong>
            {small !== undefined && <small>{small}</small>}
        </>
    );
    if (to)
        return (
            <Link className={cls} style={style} title={title} {...(to as any)}>
                {body}
            </Link>
        );
    if (onClick)
        return (
            <button type="button" className={cls} style={style} title={title} onClick={onClick}>
                {body}
            </button>
        );
    return (
        <div className={cls} style={style} title={title}>
            {body}
        </div>
    );
}

/** A tile led by a progress ring. */
export function RingTile({ p, center, em, strong, small, color, title }: { p: number; center: ReactNode; em: ReactNode; strong: ReactNode; small?: ReactNode; color?: string; title?: string }) {
    return (
        <div className="ly-tl ring" title={title}>
            <span className="ly-bigring" style={{ "--p": Math.round(p), "--c": color ?? "var(--acc)" } as CSSProperties}>
                <b>{center}</b>
            </span>
            <span>
                <em>{em}</em>
                <strong>{strong}</strong>
                {small !== undefined && <small>{small}</small>}
            </span>
        </div>
    );
}

export function Panel({ title, aside, children, className = "", id }: { title?: ReactNode; aside?: ReactNode; children: ReactNode; className?: string; id?: string }) {
    return (
        <section className={`ly-pn${className ? ` ${className}` : ""}`} id={id}>
            {(title || aside) && (
                <h5 className="ly-pnh">
                    <span>{title}</span>
                    {aside}
                </h5>
            )}
            {children}
        </section>
    );
}

/** "How these were found": says plainly where a list came from, with a few facts. */
export function HowFound({ title = "How these were found", text, rows }: { title?: string; text: ReactNode; rows?: [ReactNode, ReactNode][] }) {
    return (
        <section className="ly-pn ly-how">
            <h5>{title}</h5>
            <p>{text}</p>
            {rows && rows.length > 0 && (
                <dl className="ly-dl">
                    {rows.map(([k, v], i) => (
                        <div key={i}>
                            <dt>{k}</dt>
                            <dd>{v}</dd>
                        </div>
                    ))}
                </dl>
            )}
        </section>
    );
}

export function Kv({ rows }: { rows: [ReactNode, ReactNode][] }) {
    return (
        <div className="ly-kv">
            {rows.map(([k, v], i) => (
                <div key={i}>
                    <span>{k}</span>
                    <b>{v}</b>
                </div>
            ))}
        </div>
    );
}

/** A list card: a title with a short note, a bar (how strong or how far), a line, and chips. */
export function ListCard({ i = 0, title, note, text, w, color, chips, on, to, onClick, role }: { i?: number; title: ReactNode; note?: ReactNode; text?: ReactNode; w?: number; color?: string; chips?: ReactNode; on?: boolean; to?: TileLink; onClick?: () => void; role?: "option" }) {
    const style = { "--i": i, "--w": `${Math.max(0, Math.min(100, w ?? 0))}%`, "--c": color ?? "var(--acc)" } as CSSProperties;
    const body = (
        <>
            <div className="ly-itt">
                <h5>{title}</h5>
                {note !== undefined && <span>{note}</span>}
            </div>
            {w !== undefined && <i className="ly-itb" />}
            {text && <p>{text}</p>}
            {chips && <div className="ly-itm">{chips}</div>}
        </>
    );
    const cls = `ly-it${on ? " on" : ""}`;
    if (to)
        return (
            <Link className={cls} style={style} {...(to as any)}>
                {body}
            </Link>
        );
    return (
        <button type="button" className={cls} style={style} onClick={onClick} role={role} aria-selected={role ? !!on : undefined}>
            {body}
        </button>
    );
}

export type PathStep = { key: string; title: ReactNode; text?: ReactNode; state: "done" | "half" | "cur" | ""; to?: TileLink; onClick?: () => void };

/** A reading path: steps joined by a line; done, half done, here, and to come. */
export function ReadingPath({ steps }: { steps: PathStep[] }) {
    return (
        <ol className="ly-path">
            {steps.map((s) => {
                const body = (
                    <>
                        <i />
                        <div>
                            <b>{s.title}</b>
                            {s.text && <span>{s.text}</span>}
                        </div>
                        {s.state === "cur" && <em>here</em>}
                    </>
                );
                const cls = `ly-ps${s.state ? ` ${s.state}` : ""}`;
                return (
                    <li key={s.key}>
                        {s.to ? (
                                            <Link className={cls} {...(s.to as any)}>
                                {body}
                            </Link>
                        ) : s.onClick ? (
                            <button type="button" className={cls} onClick={s.onClick}>
                                {body}
                            </button>
                        ) : (
                            <div className={cls}>{body}</div>
                        )}
                    </li>
                );
            })}
        </ol>
    );
}
