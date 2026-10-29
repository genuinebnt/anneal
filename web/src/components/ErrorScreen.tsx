import { Link } from "@tanstack/react-router";
import { useState } from "react";

/** Shown instead of a blank page when something in a page throws. It says what to do, and keeps the details to copy. */
export function ErrorScreen({ error, reset }: { error: unknown; reset?: () => void }) {
    const detail = `${(error as Error)?.name ?? "Error"}: ${(error as Error)?.message ?? String(error)}\n${location.href}\n${((error as Error)?.stack ?? "").split("\n").slice(1, 6).join("\n")}`;
    const [copied, setCopied] = useState(false);
    return (
        <main className="page" role="alert">
            <div className="es-wrap">
                <h1>This page ran into a problem.</h1>
                <p>Nothing you logged is lost. Reload the page, or go back to the problems.</p>
                <div className="es-acts">
                    <button className="ui-btn primary" onClick={() => (reset ? reset() : location.reload())}>Try again</button>
                    <button className="ui-btn" onClick={() => location.reload()}>Reload</button>
                    <Link className="ui-btn" to="/dsa">Back to the problems</Link>
                    <button className="ui-btn quiet" onClick={() => void navigator.clipboard?.writeText(detail).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1500); }, () => undefined)}>{copied ? "Copied" : "Copy details"}</button>
                </div>
                <pre className="es-detail">{detail}</pre>
            </div>
        </main>
    );
}
