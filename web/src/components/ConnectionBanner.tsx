import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { markDown, markUp, useConnectionDown, useUpdating } from "../connection";

/** Full screen while the server restarts for a deploy: the gateway answers 502, 503 or 504 meanwhile. It asks the server every few
 *  seconds and reloads the page once it answers, so the new version is what comes back. */
function Updating() {
    const [left, setLeft] = useState(3);
    useEffect(() => {
        const id = window.setInterval(() => {
            setLeft((n) => {
                if (n > 1) return n - 1;
                void fetch("/api/health", { cache: "no-store" }).then(
                    (r) => r.ok && location.reload(),
                    () => undefined,
                );
                return 3;
            });
        }, 1000);
        return () => window.clearInterval(id);
    }, []);
    return (
        <div className="up-screen" role="alert" aria-live="polite">
            <span className="up-logo">anneal</span>
            <div className="up-heat" aria-hidden="true" />
            <h1>Annealing a new version.</h1>
            <p>anneal is restarting with an update. This page checks every few seconds and reloads itself when it is back, usually within a minute.</p>
            <span className="up-check"><i aria-hidden="true" />checking again in {left} s</span>
            <small>Your data was backed up before the update started.</small>
        </div>
    );
}

/** A bar across the top when the server cannot be reached, with a way to try again. It goes by itself when a request works. */
export function ConnectionBanner() {
    const qc = useQueryClient();
    const down = useConnectionDown();
    useEffect(() => {
        const off = () => markDown();
        const on = () => void qc.refetchQueries({ type: "active" });
        window.addEventListener("offline", off);
        window.addEventListener("online", on);
        return () => {
            window.removeEventListener("offline", off);
            window.removeEventListener("online", on);
        };
    }, [qc]);
    const updating = useUpdating();
    if (updating) return <Updating />;
    if (!down) return null;
    return (
        <div className="cn-bar" role="alert">
            <span>Can't reach anneal. Your changes are not lost; what you see may be out of date.</span>
            <button
                className="ui-btn"
                onClick={() => {
                    markUp();
                    void qc.refetchQueries({ type: "active" });
                }}
            >
                Try again
            </button>
        </div>
    );
}
