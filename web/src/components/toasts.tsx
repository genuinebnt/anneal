// The mockup's toast stack (docs/mockups/course-motion.html, ".tstack" / ".tt"): a card slides in at the top right, a line runs down along its
// bottom edge for how long it stays, and it can be closed. `toast()` can be called from anywhere.

import { useEffect, useSyncExternalStore } from "react";

export type ToastKind = "ok" | "in" | "er";
export interface ToastItem {
    id: number;
    kind: ToastKind;
    title: string;
    text?: string;
    action?: { label: string; run: () => void };
    leaving?: boolean;
}

/** What was said, newest first, kept in this browser so the notification panel can show it after the toast is gone. */
export interface Note {
    id: number;
    kind: ToastKind;
    title: string;
    text?: string;
    at: number;
}
const NOTES = "anneal:notes";
const readNotes = (): Note[] => {
    try {
        return JSON.parse(localStorage.getItem(NOTES) ?? "[]") as Note[];
    } catch {
        return [];
    }
};
let notes: Note[] = readNotes();
const noteListeners = new Set<() => void>();
const saveNotes = () => {
    try {
        localStorage.setItem(NOTES, JSON.stringify(notes));
    } catch {
        // not remembered; the panel still shows this visit's
    }
    noteListeners.forEach((l) => l());
};
/** Adds a line to the notification panel without showing a toast. */
export function record(kind: ToastKind, title: string, text?: string) {
    notes = [{ id: Date.now() + Math.random(), kind, title, text, at: Date.now() }, ...notes].slice(0, 30);
    saveNotes();
}
export function clearNotes() {
    notes = [];
    saveNotes();
}
export function useNotes(): Note[] {
    return useSyncExternalStore(
        (l) => {
            noteListeners.add(l);
            return () => noteListeners.delete(l);
        },
        () => notes,
    );
}

let items: ToastItem[] = [];
let next = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const ICON: Record<ToastKind, string> = { ok: "✓", in: "i", er: "×" };

function remove(id: number) {
    items = items.map((t) => (t.id === id ? { ...t, leaving: true } : t));
    emit();
    setTimeout(() => {
        items = items.filter((t) => t.id !== id);
        emit();
    }, 300);
}

/** Shows a toast. It stays four seconds (longer with an action), unless it is closed first. */
export function toast(kind: ToastKind, title: string, text?: string, action?: ToastItem["action"]) {
    const id = next++;
    record(kind, title, text);
    items = [...items.slice(-3), { id, kind, title, text, action }];
    emit();
    setTimeout(() => remove(id), action ? 9000 : 4000);
    return id;
}

/** Mounted once, near the root of the app. */
export function ToastHost() {
    const list = useSyncExternalStore(
        (cb) => (listeners.add(cb), () => listeners.delete(cb)),
        () => items,
    );
    useEffect(() => () => void (items = []), []);
    return (
        <div className="k-root">
            <div className="k-tstack" role="region" aria-label="Notifications" aria-live="polite">
                {list.map((t) => (
                    <div key={t.id} className={`k-tt k-${t.kind}${t.leaving ? " k-out" : ""}`} role="status">
                        <span className="k-i" aria-hidden="true">{ICON[t.kind]}</span>
                        <div>
                            <b>{t.title}</b>
                            {t.text && <span>{t.text}</span>}
                            {t.action && (
                                <button className="k-tact" onClick={() => (t.action?.run(), remove(t.id))}>
                                    {t.action.label}
                                </button>
                            )}
                        </div>
                        <button className="k-q" aria-label="Dismiss" onClick={() => remove(t.id)}>
                            ×
                        </button>
                    </div>
                ))}
            </div>
        </div>
    );
}
