// Layout rules from the 2026-11 UI review: what must stay readable, reachable and aligned at 1500, 1100 and a phone.
import { expect, test, type Page } from "@playwright/test";

const open = async (page: Page, path: string, width: number, height = 844) => {
    await page.setViewportSize({ width, height });
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready);
};

for (const width of [1500, 1100]) test(`at ${width} with motion reduced every number of the bottom bar can be read`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await open(page, "/dsa", width);
    const items = page.locator(".sb-run").first().locator(".sb-i");
    await expect(items.first()).toBeVisible();
    expect(await items.evaluateAll((all) => all.filter((e) => e.getBoundingClientRect().right > innerWidth + 0.5 || e.getBoundingClientRect().bottom > innerHeight + 0.5).length)).toBe(0);
    // the page keeps as much room under it as the bar takes
    await expect.poll(() => page.evaluate(() => parseFloat(getComputedStyle(document.body).paddingBottom) - document.querySelector<HTMLElement>(".sb")!.offsetHeight)).toBe(0);
});

test("on a phone the reading button clears the bottom bar", async ({ page }) => {
    await open(page, "/sd/learn/sql-vectors", 390);
    const fab = (await page.locator(".rd-fab").boundingBox())!;
    expect(fab.y + fab.height).toBeLessThanOrEqual((await page.locator(".hd-dock").boundingBox())!.y);
});

for (const width of [901, 1100]) test(`at ${width} every DSA tab is on screen`, async ({ page }) => {
    await open(page, "/dsa", width);
    expect(await page.locator(".hd-tabs").evaluate((t) => [...t.querySelectorAll("a")].filter((a) => a.getBoundingClientRect().right > t.getBoundingClientRect().right + 0.5).map((a) => a.textContent))).toEqual([]);
});

for (const width of [1100, 390]) test(`at ${width} no stat tile is left alone on a part of a row`, async ({ page }) => {
    for (const path of ["/rust", "/dsa", "/sd", "/progress", "/analytics", "/dsa/calendar"]) {
        await open(page, path, width);
        const holes = await page.locator(".ly-tiles").first().evaluate((box) => {
            const rows = new Map<number, number>();
            for (const t of box.children) {
                const r = t.getBoundingClientRect();
                const y = Math.round(r.top / 8);
                rows.set(y, Math.max(rows.get(y) ?? 0, r.right));
            }
            const right = box.getBoundingClientRect().right;
            return [...rows.values()].filter((end) => right - end > 4).length + (box.parentElement!.getBoundingClientRect().width - box.getBoundingClientRect().width > 60 ? 1 : 0);
        });
        expect(holes, path).toBe(0);
    }
});

test("at 1500 the calendar's title and tiles share a row", async ({ page }) => {
    await open(page, "/dsa/calendar", 1500, 900);
    const title = (await page.locator(".ly-ttl").boundingBox())!;
    const tiles = (await page.locator(".ly-tiles").boundingBox())!;
    expect(tiles.x).toBeGreaterThanOrEqual(title.x + title.width - 1);
    expect(tiles.y).toBeLessThan(title.y + title.height);
});

test("on a phone nothing in a row or a card is dropped or cut", async ({ page }) => {
    await open(page, "/t/l1-ownership-moves?tab=problems", 390);
    const row = page.locator(".rt-row").first();
    for (const part of [".num", ".rt-t", ".lvl", ".best"]) await expect(row.locator(part)).toBeVisible();
    await expect(row.locator(":scope>*").nth(3)).toBeVisible();
    // the next-problem pill ends in an ellipsis inside the window
    const pill = await page.locator(".ps-next").evaluate((e) => ({ right: e.getBoundingClientRect().right, over: e.scrollWidth - e.clientWidth, cut: getComputedStyle(e.querySelector("span")!).textOverflow }));
    expect(pill.right).toBeLessThanOrEqual(390);
    expect(pill.over).toBeLessThanOrEqual(0);
    expect(pill.cut).toBe("ellipsis");
    await open(page, "/cookbook/p/mini-kv-store", 390);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(0);
    await open(page, "/rust", 390);
    expect(await page.locator(".s-trk").first().evaluate((e) => getComputedStyle(e).minHeight)).toBe("0px");
    await open(page, "/dsa", 390);
    for (const h of await page.locator(".s-tools .s-seg button").evaluateAll((all) => all.map((b) => { const r = document.createRange(); r.selectNodeContents(b); return new Set([...r.getClientRects()].map((x) => Math.round((x.top + x.height / 2) / 8))).size; }))) expect(h).toBe(1);
});

for (const scheme of ["dark", "light"] as const) test(`in ${scheme} an empty progress segment stands out from its card`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page, "/rust/series/web-server", 390);
    const ratio = await page.locator(".sx-progress").evaluate((card) => {
        const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
        const lum = (colour: string) => {
            ctx.fillStyle = colour;
            ctx.fillRect(0, 0, 1, 1);
            const [r, g, b] = [...ctx.getImageData(0, 0, 1, 1).data].map((v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
            return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
        };
        const [a, b] = [lum(getComputedStyle(card).backgroundColor), lum(getComputedStyle(card.querySelector(".sx-segs>:not([data-state]):not(.on)")!).backgroundColor)];
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    });
    expect(ratio).toBeGreaterThanOrEqual(3);
});

test("the readiness list uses the whole card and says Part for one part", async ({ page }) => {
    await open(page, "/rust/series/web-server", 1500, 900);
    await page.locator(".sx-need summary").first().click();
    const m = await page.locator(".sx-need").evaluate((list) => {
        const card = list.closest(".sx-box")!;
        const inner = card.clientWidth - parseFloat(getComputedStyle(card).paddingLeft) - parseFloat(getComputedStyle(card).paddingRight);
        return { share: list.querySelector("details[open] ul")!.getBoundingClientRect().width / inner, words: [...list.querySelectorAll("small")].map((e) => e.textContent ?? "") };
    });
    expect(m.share).toBeGreaterThan(0.97);
    for (const w of m.words) expect(w).not.toMatch(/\b(Parts|steps) \d+( ·|$)/);
    expect(m.words.some((w) => /^Part \d+$/.test(w))).toBe(true);
});

for (const width of [1500, 390]) test(`at ${width} pages share two title sizes, one gutter and one breadcrumb`, async ({ page }) => {
    const gutter = width > 900 ? 20 : 16;
    const sizes = { home: new Set<string>(), page: new Set<string>() };
    const kinds = { home: ["/dsa", "/rust", "/cookbook", "/courses"], page: ["/dsa/plan", "/dsa/read", "/dsa/company", "/rust/series", "/rust/series/web-server", "/sd", "/sd/review", "/progress", "/settings", "/cookbook/p/cow"] };
    for (const kind of ["home", "page"] as const)
        for (const path of kinds[kind]) {
            await open(page, path, width);
            const h1 = page.locator("h1").filter({ visible: true }).first();
            sizes[kind].add(await h1.evaluate((e) => getComputedStyle(e).fontSize));
            if (path !== "/cookbook/p/cow") expect(Math.round((await h1.boundingBox())!.x), path).toBe(gutter);
            await expect(page.locator("main"), path).toHaveCount(1);
        }
    expect([...sizes.home], "landing titles").toHaveLength(1);
    expect([...sizes.page], "page titles").toHaveLength(1);
    for (const path of ["/dsa/plan", "/rust/series/web-server", "/cookbook/p/cow", "/p/l1-fix-use-after-move", "/p/c2-thread-pool"]) {
        await open(page, path, width);
        await expect(page.getByRole("navigation", { name: "Breadcrumb" }), path).toHaveClass(/ly-crumb/);
    }
    await open(page, "/rust/series/web-server", width);
    await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toContainText("From threads to a web server");
});

test("on a phone the pattern breadcrumb's steps sit on one line", async ({ page }) => {
    // on this page one crumb is the link to the page itself's section, which the touch rules once made a box of its own
    await open(page, "/dsa/practice/D1", 390);
    const middles = await page.getByRole("navigation", { name: "Breadcrumb" }).locator(":scope>*").evaluateAll((all) => all.map((e) => { const r = e.getBoundingClientRect(); return r.top + r.height / 2; }));
    expect(Math.max(...middles) - Math.min(...middles)).toBeLessThan(2);
});

test("on a phone the small controls are big enough to press, and the bottom bar draws the area icons", async ({ page }) => {
    await open(page, "/p/c2-thread-pool", 390);
    for (const name of ["Previous problem", "Next problem"]) {
        const b = (await page.getByRole("link", { name }).boundingBox())!;
        expect(Math.min(b.width, b.height), name).toBeGreaterThanOrEqual(44);
    }
    await expect(page.locator("h1")).toHaveCount(1);
    await open(page, "/courses/bustub/4a-04", 390);
    expect((await page.locator('[data-jump="task"]').boundingBox())!.height).toBeGreaterThanOrEqual(36);
    await expect(page.locator("h1")).toHaveCount(1);
    await expect(page.locator(".hd-dock a .hd-aicon")).toHaveCount(5);
    await expect(page.locator(".hd-dock a")).toHaveText(["DSA", "Rust", "Courses", "System design", "Cookbook", "Progress"]);
    // each area that has a goal says how far along it is, without a tap: a bar under its name, all on one line
    const bars = page.locator(".hd-dock .hd-dbar");
    await expect(bars).toHaveCount(4);
    const m = await bars.evaluateAll((all) => all.map((e) => ({ y: Math.round(e.getBoundingClientRect().bottom), w: e.getBoundingClientRect().width, fill: e.firstElementChild!.getBoundingClientRect().width })));
    expect(new Set(m.map((x) => x.y)).size).toBe(1);
    for (const x of m) expect(x.w).toBeGreaterThanOrEqual(24);
    const course = await (await page.request.get("/api/courses/bustub")).json();
    await expect(page.locator(".hd-dock a", { hasText: "Courses" })).toHaveAttribute("title", `Courses · ${course.done} of ${course.total}`);
    if (course.done > 0) expect(m[2]!.fill).toBeGreaterThanOrEqual(3);
    await open(page, "/settings", 390);
    // the swatch answers a press just outside its 22 pixels
    expect(await page.locator(".st-sws .hd-sw").first().evaluate((e) => { e.scrollIntoView({ block: "center" }); const r = e.getBoundingClientRect(); return document.elementFromPoint(r.x + r.width / 2, r.y - 2) === e; })).toBe(true);
    await open(page, "/rust", 390);
    for (const h of await page.locator(".rs-line").evaluateAll((all) => all.map((e) => e.getBoundingClientRect().height))) expect(h).toBeGreaterThanOrEqual(36);
});

test("the focus timer is one line until it is opened or running, with every control still there", async ({ page }) => {
    await open(page, "/p/l1-fix-use-after-move", 1500, 900);
    const timer = page.locator(".pane.r .ft");
    expect((await timer.boundingBox())!.height).toBeLessThanOrEqual(56);
    await expect(timer.getByRole("button", { name: "Start", exact: true })).toBeVisible();
    const toggle = timer.getByRole("button", { name: "Open the focus timer" });
    await toggle.focus();
    await page.keyboard.press("Enter");
    for (const name of ["Start", "Reset", "Skip to the next phase", "Settings", "POMODORO", "TIMER", "STOPWATCH"]) await expect(timer.getByRole("button", { name, exact: true })).toBeVisible();
    await timer.getByRole("button", { name: "Fold the focus timer" }).click();
    expect((await timer.boundingBox())!.height).toBeLessThanOrEqual(56);
    await timer.getByRole("button", { name: "Start", exact: true }).click();
    await expect(timer.getByRole("button", { name: "Reset", exact: true })).toBeVisible();
    await timer.getByRole("button", { name: "Pause", exact: true }).click();
    await timer.getByRole("button", { name: "Reset", exact: true }).click();
});

test("on a phone Submit stays in reach while the tests scroll", async ({ page }) => {
    // one run, so the pane has a long list of cases
    await open(page, "/p/l1-fix-use-after-move", 390);
    await page.getByRole("tab", { name: "Tests", exact: true }).click();
    await page.evaluate(() => scrollTo(0, 0));
    const submit = page.locator(".pane.r").getByRole("button", { name: "Submit", exact: true });
    const bar = (await page.locator(".hd-dock").boundingBox())!;
    await expect.poll(async () => { const b = (await submit.boundingBox())!; return b.y > 0 && b.y + b.height <= bar.y + 1; }).toBe(true);
});

test("empty header counters take no room, the Cookbook's tooltip has no empty total, and unbuilt tabs say so", async ({ page }) => {
    await open(page, "/sd", 1500);
    await expect(page.locator(".hd-chips")).toBeHidden();
    await expect(page.getByRole("navigation", { name: "Areas" }).first().getByRole("link", { name: "Cookbook" })).toHaveAttribute("title", /^Cookbook · [\d,]+ pages$/);
    const later = page.locator(".hd-later").first();
    expect(await later.evaluate((e) => getComputedStyle(e, "::after").content)).toBe('"soon"');
});
