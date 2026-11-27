// Layout rules from the 2026-11 UI review: what must stay readable, reachable and aligned at 1500, 1100 and a phone.
import { expect, test, type Page } from "@playwright/test";

const open = async (page: Page, path: string, width: number, height = 844) => {
    await page.setViewportSize({ width, height });
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready);
};

for (const width of [1500, 1100, 901]) test(`at ${width} with motion reduced the bottom bar is one still row that shows only whole numbers, the first ones first`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await open(page, "/dsa", width);
    const items = page.locator(".sb-run").first().locator(".sb-i");
    await expect(items.first()).toBeVisible();
    expect((await page.locator(".sb").boundingBox())!.height).toBeLessThanOrEqual(31);
    const shown = await items.evaluateAll((all) => {
        const bar = document.querySelector(".sb")!.getBoundingClientRect();
        // each number is either whole inside the bar or wholly below it; none is cut
        return all.map((e) => { const r = e.getBoundingClientRect(); return r.bottom <= bar.bottom + 0.5 && r.right <= innerWidth + 0.5 ? "in" : r.top >= bar.bottom - 1.5 ? "out" : "cut"; });
    });
    expect(shown).not.toContain("cut");
    expect(shown[0]).toBe("in");
    // once one is left out, so is every one after it: the least important go first
    if (shown.includes("out")) expect(shown.slice(shown.indexOf("out"))).not.toContain("in");
    expect(await page.evaluate(() => parseFloat(getComputedStyle(document.body).paddingBottom))).toBe(30);
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

for (const scheme of ["dark", "light"] as const) test(`in ${scheme} an empty progress segment can be seen on its card, softly`, async ({ page }) => {
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
    expect(ratio).toBeGreaterThanOrEqual(1.4);
    expect(ratio).toBeLessThan(3);
    // the readiness tracks on the Rust home are the same empty bar
    const empty = await page.locator(".sx-segs>:not([data-state])").first().evaluate((e) => getComputedStyle(e).backgroundColor);
    await open(page, "/rust", 390);
    expect(await page.locator(".rs-ready .s-meter").first().evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(empty);
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
    const kinds = { home: ["/dsa", "/rust", "/cookbook", "/courses", "/sd"], page: ["/dsa/plan", "/dsa/read", "/dsa/company", "/rust/series", "/rust/series/web-server", "/sd/review", "/progress", "/settings", "/cookbook/p/cow"] };
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
    await expect(page.locator(".hd-dock a")).toHaveText(["DSA", "Rust", "Courses", "Design", "Cookbook", "Progress"]);
    await expect(page.locator(".hd-dock").getByRole("link", { name: "System design" })).toHaveCount(1);
    // every label is one line
    expect(await page.locator(".hd-dock a").evaluateAll((all) => new Set(all.map((a) => { const r = document.createRange(); r.selectNode([...a.childNodes].find((n) => n.nodeType === 3)!); return r.getClientRects().length; })).size)).toBe(1);
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

/** Every point across a control answers to that control: nothing lies over any part of it. */
const covered = (page: Page, selector: string) => page.locator(selector).evaluate((el) => {
    const r = el.getBoundingClientRect();
    let missed = 0;
    // the rounded corners are not part of the target, so the points stay 3px inside
    for (let x = r.left + 3; x <= r.right - 3; x += 2)
        for (const y of [r.top + 5, r.top + r.height / 2, r.bottom - 5]) {
            const hit = document.elementFromPoint(x, y);
            if (!hit || !el.contains(hit)) missed++;
        }
    return missed;
});

for (const width of [1181, 1280, 1500]) test(`at ${width} the folded focus timer fits its pane and nothing lies over Start`, async ({ page }) => {
    await open(page, "/p/l1-fix-use-after-move", width, 800);
    const card = page.locator(".pane.r .ft.ft-fold");
    await expect(card).toBeVisible();
    for (const resumed of [false, true]) {
        if (resumed) {
            // a timer stopped part way says Resume, the longest the folded button gets with this label
            await card.getByRole("button", { name: "Start", exact: true }).click();
            await page.waitForTimeout(1200);
            await page.getByRole("button", { name: "Pause", exact: true }).click();
            await page.getByRole("button", { name: "Fold the focus timer" }).click();
            await expect(card.getByRole("button", { name: "Resume", exact: true })).toBeVisible();
        }
        expect(await covered(page, ".pane.r .ft-fold .ft-go")).toBe(0);
        expect(await covered(page, ".pane.r .ft-fold .ft-unfold")).toBe(0);
        const m = await card.evaluate((c) => {
            const box = c.getBoundingClientRect();
            const parts = [".ft-ring b", ".ft-ring small", ".ft-go", ".ft-unfold"].map((s) => c.querySelector<HTMLElement>(s)!);
            return {
                outside: parts.filter((e) => { const r = e.getBoundingClientRect(); return r.left < box.left - 0.5 || r.right > box.right + 0.5; }).length,
                cut: parts.slice(0, 3).filter((e) => e.scrollWidth > e.clientWidth + 1).length,
                gap: parts[3]!.getBoundingClientRect().left - parts[2]!.getBoundingClientRect().right,
                text: parts[2]!.getBoundingClientRect().left - Math.max(parts[0]!.getBoundingClientRect().right, parts[1]!.getBoundingClientRect().right),
                height: box.height,
            };
        });
        expect(m.outside).toBe(0);
        expect(m.cut).toBe(0);
        expect(m.gap).toBeGreaterThanOrEqual(0);
        expect(m.text).toBeGreaterThanOrEqual(0);
        expect(m.height).toBeLessThanOrEqual(56);
    }
    await card.getByRole("button", { name: "Open the focus timer" }).click();
    await page.locator(".pane.r .ft").getByRole("button", { name: "Reset", exact: true }).click();
});

test("on a phone the focus timer's Start comes before the tests", async ({ page }) => {
    await open(page, "/p/l1-fix-small-struct-copy", 390);
    await page.getByRole("tab", { name: "Tests", exact: true }).click();
    await page.evaluate(() => scrollTo(0, 0));
    const start = (await page.locator(".pane.r .ft").getByRole("button", { name: "Start", exact: true }).boundingBox())!;
    expect(start.y).toBeLessThan((await page.locator(".pane.r .tcase").first().boundingBox())!.y);
    expect(start.y + start.height).toBeLessThanOrEqual((await page.locator(".hd-dock").boundingBox())!.y);
    expect(await covered(page, ".pane.r .ft-go")).toBe(0);
});

// where the panes began before the 2026-11 round two (commit 92ebbb75), in pixels from the top: a track problem, then a series part
const CONTENT_TOP = [
    { width: 390, height: 844, track: 237, series: 204 },
    { width: 320, height: 700, track: 276, series: 261 },
    { width: 844, height: 390, track: 237, series: 204 },
];
for (const at of CONTENT_TOP) test(`at ${at.width}x${at.height} the problem header is a one-line breadcrumb and one row of controls, and no taller than before`, async ({ page }) => {
    for (const [path, top] of [["/p/l1-fix-small-struct-copy", at.track], ["/p/c2-thread-pool", at.series]] as const) {
        await open(page, path, at.width, at.height);
        await expect(page.locator(".subbar .clock")).toBeVisible();
        expect((await page.locator(".rx-ws .ws>.pane").filter({ visible: true }).first().boundingBox())!.y, `${path}: content top`).toBeLessThanOrEqual(top);
        const m = await page.locator(".rx-ws .subbar").evaluate((bar) => {
            const mid = (e: Element) => { const r = e.getBoundingClientRect(); return r.top + r.height / 2; };
            const crumb = [...bar.querySelectorAll(".ly-crumb>span")];
            const row = [...bar.querySelector(".sbr")!.children].filter((e) => e.getBoundingClientRect().width > 0);
            return {
                crumbLines: Math.max(...crumb.map(mid)) - Math.min(...crumb.map(mid)),
                crumbHeight: Math.max(...crumb.map((e) => e.getBoundingClientRect().height)),
                cutWithEllipsis: crumb.filter((e) => e.scrollWidth > e.clientWidth + 1).every((e) => getComputedStyle(e).textOverflow === "ellipsis"),
                rowSpread: Math.max(...row.map(mid)) - Math.min(...row.map(mid)),
                outside: [...crumb, ...row].filter((e) => e.getBoundingClientRect().right > innerWidth + 0.5 || e.getBoundingClientRect().left < -0.5).length,
                controls: row.length,
            };
        });
        expect(m.crumbLines, `${path}: crumb`).toBeLessThan(3);
        expect(m.crumbHeight).toBeLessThanOrEqual(28);
        expect(m.cutWithEllipsis).toBe(true);
        expect(m.controls).toBeGreaterThanOrEqual(2);
        expect(m.rowSpread, `${path}: pager, timer and layout buttons on one row`).toBeLessThan(3);
        expect(m.outside).toBe(0);
        // the part number is never what gets cut
        if (path.includes("c2-")) expect(await page.locator(".sx-crumb>span").last().evaluate((e) => e.scrollWidth - e.clientWidth)).toBeLessThanOrEqual(1);
        for (const name of ["Previous problem", "Next problem"]) {
            const link = page.getByRole("link", { name });
            if (!(await link.count())) continue;
            const b = (await link.boundingBox())!;
            expect(b.height, name).toBeGreaterThanOrEqual(44);
            expect(b.width, name).toBeGreaterThanOrEqual(at.width < 361 ? 36 : 44);
        }
    }
});

test("on a short screen Submit does not cover the tests, and is still reachable at their end", async ({ page }) => {
    await open(page, "/p/l1-fix-small-struct-copy", 844, 390);
    await page.getByRole("tab", { name: "Tests", exact: true }).click();
    const submit = page.locator(".pane.r").getByRole("button", { name: "Submit", exact: true });
    expect(await submit.evaluate((e) => getComputedStyle(e.parentElement!).position)).toBe("static");
    await submit.evaluate((e) => e.scrollIntoView({ block: "center", behavior: "instant" }));
    await expect.poll(() => covered(page, ".pane.r .acts .go")).toBe(0);
});

for (const width of [901, 1000, 1100]) test(`at ${width} the calendar has no sideways scroll and every streak count is inside its tile`, async ({ page }) => {
    await open(page, "/dsa/calendar", width, 800);
    await expect(page.locator(".c-stk")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(0);
    expect(await page.locator(".c-stk").evaluate((tile) => [...tile.querySelectorAll("*")].filter((e) => e.getBoundingClientRect().right > tile.getBoundingClientRect().right + 0.5).length)).toBe(0);
});

test("on a phone the next-problem pill, Reset and the arrows share a row", async ({ page }) => {
    for (const path of ["/t/l1-ownership-moves", "/dsa/practice/D1"]) {
        await open(page, path, 390);
        const mids = await page.locator(".ps-acts>*").evaluateAll((all) => all.map((e) => { const r = e.getBoundingClientRect(); return Math.round(r.top + r.height / 2); }));
        expect(Math.max(...mids) - Math.min(...mids), path).toBeLessThan(3);
        expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth), path).toBeLessThanOrEqual(0);
    }
});
