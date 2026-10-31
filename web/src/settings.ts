// Settings stored on the server: editor fonts (applied as CSS variables the editor theme reads) and the
// app's accent colour (applied as data-accent on <html>; the palettes live in app.css).
import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type Accent, type EditorSettings, type Settings } from "./api";

export const DEFAULT_EDITOR: EditorSettings = { font_size: 13, font_family: "Fira Code", vim: false, autocomplete: true, rust_analyzer: true, borrow_lanes: false, live_clippy: true, format_on_pause: false, ligatures: false };

const STACK: Record<string, string> = {
  system: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
};
const stack = (family: string) => STACK[family] ?? `'${family}', ui-monospace, monospace`;

/** Google Fonts families loaded on demand; JetBrains Mono and Fira Code are already in index.html. */
const GOOGLE: Record<string, string> = {
  "Fira Code": "Fira+Code:wght@400;500",
  "IBM Plex Mono": "IBM+Plex+Mono:wght@400;500",
  "Source Code Pro": "Source+Code+Pro:wght@400;500",
  "Roboto Mono": "Roboto+Mono:wght@400;500",
};

export const fontLabel = (family: string) => (family === "system" ? "System monospace" : family);

export function loadFont(family: string) {
  const spec = GOOGLE[family];
  const id = `font-${spec}`;
  if (!spec || document.getElementById(id)) return;
  const link = document.createElement("link");
  link.id = id;
  link.rel = "stylesheet";
  link.href = `https://fonts.googleapis.com/css2?family=${spec}&display=swap`;
  document.head.appendChild(link);
}

export function applyEditor(e: EditorSettings) {
  loadFont(e.font_family);
  const root = document.documentElement.style;
  root.setProperty("--editor-font", stack(e.font_family));
  root.setProperty("--editor-size", `${e.font_size}px`);
  root.setProperty("--editor-line", `${Math.round(e.font_size * 1.55)}px`);
  // Ligatures come from the fonts' contextual alternates; both properties are set so every browser agrees.
  root.setProperty("--editor-ligatures", e.ligatures ? "normal" : "none");
  root.setProperty("--editor-features", e.ligatures ? "normal" : '"liga" 0, "calt" 0');
  window.dispatchEvent(new Event("anneal:editor-font"));
  // A newly loaded web font changes glyph widths once it arrives.
  document.fonts?.ready.then(() => window.dispatchEvent(new Event("anneal:editor-font")));
}

/** The saved editor settings, applied to the page, and a setter that saves them. */
export function useEditorSettings() {
  const qc = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings, staleTime: Infinity });
  const editor = settings.data?.editor ?? DEFAULT_EDITOR;
  const save = useMutation({
    mutationFn: api.saveEditor,
    onMutate: (next) => {
      const prev = qc.getQueryData<Settings>(["settings"]);
      if (prev) qc.setQueryData<Settings>(["settings"], { ...prev, editor: next });
      return prev;
    },
    onError: (_e, _next, prev) => {
      if (prev) qc.setQueryData(["settings"], prev);
    },
  });
  useEffect(() => {
    applyEditor(editor);
  }, [editor.font_family, editor.font_size, editor.ligatures]); // eslint-disable-line react-hooks/exhaustive-deps
  return {
    editor,
    families: settings.data?.font_families ?? [DEFAULT_EDITOR.font_family],
    sizes: settings.data?.font_sizes ?? [10, 24],
    set: (next: EditorSettings) => save.mutate(next),
  };
}

/** The accent colours: six deep ones, three soft ones, and `custom` (any colour, picked in the header). */
export const ACCENTS: { id: Accent; label: string; group: "deep" | "soft" }[] = [
  { id: "red", label: "Red", group: "deep" },
  { id: "teal", label: "Teal", group: "deep" },
  { id: "orange", label: "Orange", group: "deep" },
  { id: "blue", label: "Blue", group: "deep" },
  { id: "purple", label: "Purple", group: "deep" },
  { id: "yellow", label: "Yellow", group: "deep" },
  { id: "copper", label: "Copper", group: "soft" },
  { id: "rose", label: "Rose", group: "soft" },
  { id: "sky", label: "Sky", group: "soft" },
];

const ACCENT_KEY = "anneal-accent";
const CUSTOM_KEY = "anneal-accent-custom";

/** Sets the accent on the page and caches it, so index.html can apply it before first paint. A custom colour goes in --acc-custom;
 *  the stylesheet keeps its lightness readable in each theme. */
export function applyAccent(accent: Accent, custom?: string) {
  const root = document.documentElement;
  if (accent === "copper") delete root.dataset.accent;
  else root.dataset.accent = accent;
  if (accent === "custom" && custom) root.style.setProperty("--acc-custom", custom);
  else root.style.removeProperty("--acc-custom");
  try {
    localStorage.setItem(ACCENT_KEY, accent);
    if (custom) localStorage.setItem(CUSTOM_KEY, custom);
  } catch {
    // Private windows can refuse storage; the server copy still applies after load.
  }
}

/** The saved accent, applied to the page, and setters that save it. */
export function useAppearance() {
  const qc = useQueryClient();
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings, staleTime: Infinity });
  const appearance = settings.data?.appearance;
  const save = useMutation({
    mutationFn: api.saveAppearance,
    onMutate: (next) => {
      const prev = qc.getQueryData<Settings>(["settings"]);
      if (prev) qc.setQueryData<Settings>(["settings"], { ...prev, appearance: next });
      applyAccent(next.accent, next.custom);
      return prev;
    },
    onError: (_e, _next, prev) => {
      if (prev) {
        qc.setQueryData(["settings"], prev);
        applyAccent(prev.appearance.accent, prev.appearance.custom);
      }
    },
  });
  useEffect(() => {
    if (appearance) applyAccent(appearance.accent, appearance.custom);
  }, [appearance?.accent, appearance?.custom]); // eslint-disable-line react-hooks/exhaustive-deps
  return {
    accent: appearance?.accent ?? "copper",
    custom: appearance?.custom,
    set: (next: Accent) => save.mutate({ accent: next, ...(appearance?.custom ? { custom: appearance.custom } : {}) }),
    setCustom: (colour: string) => save.mutate({ accent: "custom", custom: colour }),
  };
}

// Pages are full width (the 2026 redesign removed the centred option). An older choice kept in this browser is cleared once.
if (typeof document !== "undefined") {
  delete document.documentElement.dataset.width;
  try {
    localStorage.removeItem("anneal-width");
  } catch {
    // nothing kept, nothing to clear
  }
}

