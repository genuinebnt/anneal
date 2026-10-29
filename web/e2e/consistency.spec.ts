import { expect, test, type Page } from "@playwright/test";

// A consistency check over the key pages: both themes, every accent, phone and laptop. It measures instead of comparing pictures, so it holds on any
// machine: nothing wider than the screen, no console errors, no control without a name, and readable text. A change that breaks one of these fails here.
const PAGES: [string, string][] = [
    ["dsa-home", "/dsa"], ["problem", "/d/two-sum"], ["review", "/dsa/review"], ["calendar", "/dsa/calendar"], ["plan", "/dsa/plan"], ["company", "/dsa/company"],
    ["settings", "/settings"], ["log", "/log"], ["progress", "/progress"], ["mock-setup", "/dsa/mock"], ["pattern-learn", "/dsa/patterns/D1"],
    ["rust-home", "/rust"], ["rust-track", "/t/l1-ownership-moves"], ["rust-workspace", "/p/l1-fix-use-after-move"],
    ["course-home", "/courses"], ["course-stage", "/courses/bustub/1a-01"], ["course-concept", "/courses/bustub/concept/positional-io-and-short-reads"],
];
/** Text this check cannot judge: it cannot see a pill's moving background, gradient headline text, or a blur made on purpose; the dim items are meant to be dim. */
const NOT_JUDGED = /^(a\.on\.active|a\.on|em|span\.k-spoil|span\.hd-later|button\.si\.off)$/;
const MIN_CONTRAST = 4.5;

const probe = () => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    const rgba = (c: string): [number, number, number, number] => {
        ctx.clearRect(0, 0, 1, 1);
        ctx.fillStyle = "#000";
        ctx.fillStyle = c;
        ctx.fillRect(0, 0, 1, 1);
        const d = ctx.getImageData(0, 0, 1, 1).data;
        return [d[0]!, d[1]!, d[2]!, d[3]! / 255];
    };
    const lum = ([r, g, b]: number[]) => { const f = (v: number) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r!) + 0.7152 * f(g!) + 0.0722 * f(b!); };
    const bgOf = (el: Element): [number, number, number] => {
        const stack: [number, number, number, number][] = [];
        for (let e: Element | null = el; e; e = e.parentElement) {
            const c = rgba(getComputedStyle(e).backgroundColor);
            if (c[3] > 0) stack.push(c);
            if (c[3] >= 1) break;
        }
        let base: [number, number, number] = rgba(getComputedStyle(document.body).backgroundColor).slice(0, 3) as [number, number, number];
        if (rgba(getComputedStyle(document.body).backgroundColor)[3] === 0) base = [255, 255, 255];
        for (const c of stack.reverse()) base = [0, 1, 2].map((i) => c[i]! * c[3] + base[i]! * (1 - c[3])) as [number, number, number];
        return base;
    };
    const visible = (el: Element) => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && Number(s.opacity) > 0.05; };
    const sel = (el: Element) => el.tagName.toLowerCase() + (el.className && typeof el.className === "string" ? "." + el.className.trim().split(/\s+/).slice(0, 2).join(".") : "");
    const out: Record<string, unknown> = {};
    out.overflowX = document.documentElement.scrollWidth - innerWidth;
    const named = (el: Element) => !!((el.getAttribute("aria-label") || el.getAttribute("title") || el.textContent || "").trim() || (el as HTMLInputElement).labels?.length || el.getAttribute("aria-labelledby") || el.querySelector("img[alt]"));
    out.unnamed = [...document.querySelectorAll("button,a[href],input:not([type=hidden]),select,textarea,[role=button],[role=switch]")].filter((e) => visible(e) && !named(e)).slice(0, 8).map(sel);
    const small = [...document.querySelectorAll("button,a[href],select,input[type=checkbox],input[type=radio],[role=button],[role=switch]")].filter(visible).filter((e) => { const r = e.getBoundingClientRect(); return r.height < 32 || r.width < 32; });
    out.smallTargets = small.length;
    out.smallExamples = [...new Set(small.map(sel))].slice(0, 6);
    const bad = new Map<string, { ratio: number; text: string; n: number }>();
    for (const el of document.querySelectorAll("body *")) {
        if (!visible(el)) continue;
        const own = [...el.childNodes].some((n) => n.nodeType === 3 && (n.textContent || "").trim().length > 1);
        if (!own) continue;
        const s = getComputedStyle(el);
        const fg = rgba(s.color);
        const bg = bgOf(el);
        const f: [number, number, number] = [0, 1, 2].map((i) => fg[i]! * fg[3] + bg[i]! * (1 - fg[3])) as [number, number, number];
        const [a, b] = [lum(f), lum(bg)];
        const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
        const size = parseFloat(s.fontSize);
        const large = size >= 24 || (size >= 18.66 && Number(s.fontWeight) >= 700);
        if (ratio < (large ? 3 : 4.5)) {
            const k = sel(el);
            const cur = bad.get(k);
            if (!cur) bad.set(k, { ratio: Math.round(ratio * 100) / 100, text: (el.textContent || "").trim().slice(0, 28), n: 1 });
            else { cur.n++; cur.ratio = Math.min(cur.ratio, Math.round(ratio * 100) / 100); }
        }
    }
    out.lowContrast = [...bad.entries()].sort((x, y) => x[1].ratio - y[1].ratio).slice(0, 10).map(([k, v]) => ({ el: k, ...v }));
    out.radii = [...new Set([...document.querySelectorAll("body *")].filter(visible).map((e) => getComputedStyle(e).borderRadius).filter((r) => r && r !== "0px"))].length;
    out.fontSizes = [...new Set([...document.querySelectorAll("body *")].filter(visible).map((e) => getComputedStyle(e).fontSize))].length;
    return out;
};


async function check(page: Page, path: string, label: string, errors: string[]) {
    await page.goto(path);
    await page.waitForTimeout(900);
    const r = (await page.evaluate(probe)) as { overflowX: number; unnamed: string[]; lowContrast: { el: string; ratio: number; text: string }[] };
    expect.soft(r.overflowX, `${label}: wider than the screen`).toBeLessThanOrEqual(1);
    expect.soft(r.unnamed, `${label}: controls with no name`).toEqual([]);
    expect.soft(r.lowContrast.filter((c) => !NOT_JUDGED.test(c.el) && c.ratio < MIN_CONTRAST), `${label}: text under ${MIN_CONTRAST}:1`).toEqual([]);
    expect.soft(errors, `${label}: console errors`).toEqual([]);
}

function watch(page: Page): string[] {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(m.text().slice(0, 120)));
    page.on("pageerror", (e) => errors.push(e.message.slice(0, 120)));
    return errors;
}

for (const theme of ["dark", "light"]) {
    for (const [view, width, height] of [["laptop", 1440, 900], ["phone", 390, 844]] as const) {
        test(`pages are consistent in ${theme} on a ${view}`, async ({ page }) => {
            test.setTimeout(240_000);
            const errors = watch(page);
            await page.addInitScript((t) => localStorage.setItem("anneal-theme", t), theme);
            await page.setViewportSize({ width, height });
            for (const [name, path] of PAGES) {
                errors.length = 0;
                await check(page, path, `${theme} ${view} ${name}`, errors);
            }
        });
    }
    for (const accent of ["rose", "sky", "teal"]) {
        test(`the ${accent} accent reads well in ${theme}`, async ({ page }) => {
            test.setTimeout(120_000);
            const errors = watch(page);
            await page.addInitScript(([t, a]) => { localStorage.setItem("anneal-theme", t!); localStorage.setItem("anneal-accent", a!); }, [theme, accent]);
            await page.setViewportSize({ width: 1440, height: 900 });
            for (const [name, path] of PAGES.filter(([n]) => ["dsa-home", "problem", "review", "settings", "calendar"].includes(n))) {
                errors.length = 0;
                await check(page, path, `${accent} ${theme} ${name}`, errors);
            }
        });
    }
}
