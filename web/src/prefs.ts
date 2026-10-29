// Small choices (panel sizes, timer lengths, what is hidden). They live in this browser and are copied to the server, so they follow
// the owner to another laptop: `hydratePrefs` pulls the server's copy before the app draws, `setPref` writes through. Storage or the
// network can fail: every read and write is guarded and the app works without either.

import { api } from "./api";

const PREFIX = "anneal:";
const DIRTY = `${PREFIX}__dirty`;
const pending = new Map<string, number>();

/** Keys changed here whose copy has not reached the server yet. They survive a reload, so the server's older copy can never win over them. */
function dirty(): Set<string> {
    try {
        return new Set(JSON.parse(localStorage.getItem(DIRTY) ?? "[]") as string[]);
    } catch {
        return new Set();
    }
}
function markDirty(key: string, on: boolean) {
    try {
        const d = dirty();
        if (on) d.add(key);
        else d.delete(key);
        localStorage.setItem(DIRTY, JSON.stringify([...d]));
    } catch {
        // nothing to do
    }
}

export function getPref<T>(key: string, fallback: T): T {
    try {
        const v = localStorage.getItem(PREFIX + key);
        return v === null ? fallback : (JSON.parse(v) as T);
    } catch {
        return fallback;
    }
}

export function setPref(key: string, value: unknown) {
    try {
        localStorage.setItem(PREFIX + key, JSON.stringify(value));
    } catch {
        // nothing to do: the choice is simply not remembered here
    }
    window.dispatchEvent(new CustomEvent("anneal:pref", { detail: key }));
    markDirty(key, true);
    window.clearTimeout(pending.get(key));
    pending.set(
        key,
        window.setTimeout(() => {
            pending.delete(key);
            void api.savePref(key, value).then(() => markDirty(key, false), () => undefined);
        }, 400),
    );
}

/** Takes the server's preferences into this browser (the server wins, except for keys changed here and not yet sent), and sends up
 *  the ones only this browser has. Never throws. A browser driven by automation (the end-to-end tests) keeps its own, so tests
 *  that share one server do not see each other's choices. */
export async function hydratePrefs(): Promise<void> {
    if (navigator.webdriver) return;
    try {
        const server = await Promise.race([api.prefs(), new Promise<never>((_, no) => window.setTimeout(() => no(new Error("slow")), 1500))]);
        const mine = dirty();
        for (const [key, value] of Object.entries(server)) if (!mine.has(key)) localStorage.setItem(PREFIX + key, JSON.stringify(value));
        for (let i = 0; i < localStorage.length; i++) {
            const full = localStorage.key(i);
            if (!full?.startsWith(PREFIX) || full === DIRTY) continue;
            const key = full.slice(PREFIX.length);
            if (!(key in server) || mine.has(key)) void api.savePref(key, getPref(key, null)).then(() => markDirty(key, false), () => undefined);
        }
    } catch {
        // offline, signed out or slow: this browser's own choices stand
    }
}
