// Comfort settings that change the whole page: less motion, a larger interface, stronger contrast. They are set as attributes on <html> (the stylesheets
// read them), kept as preferences so they follow the owner to another laptop, and applied before the first paint by a line in index.html.
import { getPref, setPref } from "./prefs";

export type Zoom = "100" | "110" | "125";

export const readUi = () => ({
    motion: getPref<"system" | "reduce">("ui.motion", "system"),
    zoom: getPref<Zoom>("ui.zoom", "100"),
    contrast: getPref<"normal" | "high">("ui.contrast", "normal"),
    density: getPref<"comfortable" | "compact">("ui.density", "comfortable"),
    companies: getPref<boolean>("ui.companies", true),
    topics: getPref<boolean>("ui.topics", true),
});

export function applyUi() {
    const u = readUi();
    const root = document.documentElement;
    if (u.motion === "reduce") root.dataset.motion = "reduce";
    else delete root.dataset.motion;
    if (u.zoom !== "100") root.dataset.zoom = u.zoom;
    else delete root.dataset.zoom;
    if (u.contrast === "high") root.dataset.contrast = "high";
    else delete root.dataset.contrast;
    if (u.density === "compact") root.dataset.density = "compact";
    else delete root.dataset.density;
    if (!u.companies) root.dataset.hideCompanies = "";
    else delete root.dataset.hideCompanies;
    if (!u.topics) root.dataset.hideTopics = "";
    else delete root.dataset.hideTopics;
}

export const setUi = (key: "ui.motion" | "ui.zoom" | "ui.contrast" | "ui.density" | "ui.companies" | "ui.topics", value: string | boolean) => {
    setPref(key, value);
    applyUi();
};
