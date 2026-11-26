import { expect, test } from "@playwright/test";

test("the header shows the area you are in and the pages of that area", async ({ page }) => {
    await page.goto("/courses");
    const areas = page.getByRole("navigation", { name: "Areas" }).first();
    await expect(areas.getByRole("link", { name: /Courses/ })).toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("BusTub");
    await expect(page.locator(".hd-crumb")).toHaveCount(0); // the tab already says where you are
    await areas.getByRole("link", { name: /DSA/ }).click();
    await expect(page).toHaveURL(/\/dsa$/);
    await expect(page.locator(".hd-tabs a", { hasText: "Mock interview" })).toBeVisible();
    await expect(page.locator(".hd-tabs a.on")).toHaveText(/Problems/);
});

test("a stage page adds a crumb to the area", async ({ page }) => {
    await page.goto("/courses/bustub/1a-01");
    await expect(page.locator(".hd-crumb")).toContainText("1a-01");
});

test("search opens with Ctrl+K, filters by name, opens a result with Enter and closes with Escape", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const box = page.getByRole("dialog", { name: "Search" });
    await expect(box).toBeVisible();
    await box.getByRole("textbox").fill("3i-04");
    await expect(box.getByRole("option").first()).toContainText("3i-04");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/courses\/bustub\/3i-04$/);
    await page.keyboard.press("Control+k");
    await expect(box).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(box).toHaveCount(0);
});

test("main screens are full width with no width option; reading screens keep their measure", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 900 });
    await page.goto("/courses");
    await expect.poll(async () => (await page.locator(".k-wrap").first().boundingBox())!.width).toBeGreaterThan(1700);
    await page.locator(".acct .av").click();
    await expect(page.getByRole("radio", { name: "Wide" })).toHaveCount(0);
    await page.goto("/courses/bustub/1a-01");
    const read = page.locator(".k-read2").first();
    await expect(read).toBeVisible();
    expect((await read.boundingBox())!.width).toBeLessThanOrEqual(1040);
});

test("Progress has its own place beside the areas, with Settings and Quick log as its pages", async ({ page }) => {
    await page.goto("/dsa");
    const areas = page.getByRole("navigation", { name: "Areas" }).first();
    await expect(page.locator(".hd-tabs a", { hasText: "Progress" })).toHaveCount(0);
    await areas.getByRole("link", { name: "Progress" }).click();
    await expect(page).toHaveURL(/\/progress$/);
    await expect(areas.getByRole("link", { name: "Progress" })).toHaveAttribute("aria-current", "page");
    await expect(areas.getByRole("link", { name: /DSA/ })).not.toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Progress");
    await expect(page.locator(".hd-tabs a", { hasText: "Settings" })).toBeVisible();
});

test("on a phone the areas are a bar at the bottom", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/courses");
    const dock = page.locator(".hd-dock");
    await expect(dock).toBeVisible();
    await expect(dock.getByRole("link", { name: /Courses/ })).toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-areas")).toBeHidden();
});

// The bottom bar starts at 900, so the top switcher has to be there at every width above it, and still fit beside the search.
for (const width of [901, 960, 1024, 1100, 1101, 1180, 1181]) {
    test(`at ${width} the header has all six area links and nothing overflows`, async ({ page }) => {
        await page.setViewportSize({ width, height: 800 });
        // a page with the second header row, and a workspace with the compact one
        for (const path of ["/dsa", "/p/l1-fix-use-after-move"]) {
            await page.goto(path);
            await expect(page.locator(".hd-dock")).toBeHidden();
            const areas = page.getByRole("navigation", { name: "Areas" });
            for (const name of ["DSA", "Rust", "Courses", "System design", "Cookbook", "Progress"]) await expect(areas.getByRole("link", { name, exact: true })).toBeVisible();
            await page.evaluate(() => document.fonts.ready);
            const r = await page.evaluate(() => {
                const box = (s: string) => document.querySelector(s)!.getBoundingClientRect();
                const [l, search, right] = [box(".hd-l"), box(".hd-srch"), box(".hdr-r")];
                const links = [...document.querySelectorAll(".hd-areas a, .hd-t1 .hd-srch, .hdr-r > *")].filter((e) => e.getBoundingClientRect().width > 0);
                const covered = links.filter((e) => {
                    const b = e.getBoundingClientRect();
                    if (!e.matches("a, button")) return false;
                    return !e.contains(document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2));
                });
                return { left: search.left - l.right, rightGap: right.left - search.right, end: right.right - document.documentElement.clientWidth, search: search.width, covered: covered.map((e) => e.outerHTML.slice(0, 80)), page: document.documentElement.scrollWidth - document.documentElement.clientWidth };
            });
            expect(r.left, "areas run into the search").toBeGreaterThanOrEqual(0);
            expect(r.rightGap, "the search runs into the actions").toBeGreaterThanOrEqual(0);
            expect(r.end, "the actions leave the window").toBeLessThanOrEqual(0);
            expect(r.search, "the search is squeezed away").toBeGreaterThanOrEqual(120);
            expect(r.covered).toEqual([]);
            expect(r.page).toBeLessThanOrEqual(0);
        }
    });
}
