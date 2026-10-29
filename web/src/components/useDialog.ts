import { useEffect, type RefObject } from "react";

const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select,textarea,[tabindex]:not([tabindex="-1"])';

/** A dialog or popover that behaves like one: focus moves into it when it opens, Tab stays inside, Escape closes it, and focus goes back to
 *  what had it before. `ref` is the element that holds it. */
export function useDialog(ref: RefObject<HTMLElement | null>, open: boolean, onClose: () => void) {
    useEffect(() => {
        const el = ref.current;
        if (!open || !el) return;
        const before = document.activeElement as HTMLElement | null;
        const items = () => [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((e) => e.offsetParent !== null);
        if (!el.contains(document.activeElement)) items()[0]?.focus();
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                e.stopPropagation();
                onClose();
            } else if (e.key === "Tab") {
                const f = items();
                if (!f.length) return;
                const at = f.indexOf(document.activeElement as HTMLElement);
                if (e.shiftKey && at <= 0) {
                    e.preventDefault();
                    f[f.length - 1]!.focus();
                } else if (!e.shiftKey && at === f.length - 1) {
                    e.preventDefault();
                    f[0]!.focus();
                }
            }
        };
        document.addEventListener("keydown", key, true);
        return () => {
            document.removeEventListener("keydown", key, true);
            before?.focus?.();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open]);
}
