// The colour theme: dark and light are explicit choices, System follows the device and clears the stored choice.

export type Theme = "dark" | "light" | "system";
export const THEMES: Theme[] = ["dark", "light", "system"];

export function storedTheme(): Theme {
    try {
        const t = localStorage.getItem("anneal-theme");
        return t === "light" || t === "dark" ? t : "system";
    } catch {
        return "system";
    }
}

export function applyTheme(theme: Theme) {
    const root = document.documentElement;
    try {
        if (theme === "system") {
            delete root.dataset.theme;
            localStorage.removeItem("anneal-theme");
        } else {
            root.dataset.theme = theme;
            localStorage.setItem("anneal-theme", theme);
        }
    } catch {
        // Private windows can refuse storage; the choice still applies for this visit.
    }
}
