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
export const markUp = () => set(false);
export const useConnectionDown = () => useSyncExternalStore((l) => (listeners.add(l), () => listeners.delete(l)), () => down);
