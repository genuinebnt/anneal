// Whether the server can be reached. A failed request (the server stopped, the network dropped) turns it off; the next request that works turns it back on.
import { useSyncExternalStore } from "react";

let down = false;
const listeners = new Set<() => void>();
const set = (v: boolean) => {
    if (down === v) return;
    down = v;
    listeners.forEach((l) => l());
};

/** A fetch that never got an answer (as opposed to an answer that was an error). */
export const isNetworkError = (e: unknown) => e instanceof TypeError || /failed to fetch|networkerror|load failed/i.test(String((e as Error)?.message ?? ""));

export const markDown = () => set(true);
export const markUp = () => {
    set(false);
    setUpdating(false);
};
export const useConnectionDown = () => useSyncExternalStore((l) => (listeners.add(l), () => listeners.delete(l)), () => down);

// The gateway answers 502, 503 or 504 while the app restarts for a deploy: the app shows "updating" and reloads when it is back.
let updating = false;
const setUpdating = (v: boolean) => {
    if (updating === v) return;
    updating = v;
    listeners.forEach((l) => l());
};
export const markUpdating = () => setUpdating(true);
export const useUpdating = () => useSyncExternalStore((l) => (listeners.add(l), () => listeners.delete(l)), () => updating);
