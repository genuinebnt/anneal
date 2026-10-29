import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { markDown, markUp, useConnectionDown } from "../connection";

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
