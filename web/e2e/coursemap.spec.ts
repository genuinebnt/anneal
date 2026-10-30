import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
    await page.goto("/courses/bustub");
});

test("the course map is a card per project, each with a moving drawing, a ring, its modules and a bar", async ({ page }) => {
    const cards = page.locator(".k-map .kp-card");
    await expect(cards.first()).toBeVisible();
    expect(await cards.count()).toBeGreaterThanOrEqual(5);
    for (const card of await cards.all()) {
        await expect(card.locator(".kp-stage svg.lp")).toBeVisible();
        await expect(card.locator(".kp-ring")).toBeVisible();
        await expect(card.locator(".kp-foot")).toContainText("passed");
        expect(await card.locator(".k-node").count()).toBeGreaterThan(0);
    }
    // the modules keep their place: every one is still a button on the map
    expect(await page.locator(".k-map .k-node").count()).toBeGreaterThanOrEqual(18);
    // a project's drawing is its own idea
    await expect(cards.filter({ hasText: "Buffer pool" }).locator(".lp-fr").first()).toBeVisible();
    await expect(cards.filter({ hasText: "Indexes" }).locator(".lp-la0")).toBeVisible();
});

test("the drawings move, and stand still under reduced motion", async ({ page, browser }) => {
    const name = await page.locator(".kp-card .lp-fr").first().evaluate((e) => getComputedStyle(e).animationName);
    expect(name).toContain("lp-hit");
    const calm = await browser.newContext({ reducedMotion: "reduce" });
    const q = await calm.newPage();
    await q.goto("/courses/bustub");
    expect(await q.locator(".kp-card .lp-fr").first().evaluate((e) => getComputedStyle(e).animationName)).toBe("none");
    await calm.close();
});

test("a module on a card still jumps to it, and the card does not scroll sideways on a phone", async ({ page }) => {
    await page.locator(".k-map .k-node", { hasText: "4A" }).click();
    await expect(page.locator("#mod-4a .k-mh")).toHaveAttribute("aria-expanded", "true");
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/courses/bustub");
    await expect(page.locator(".kp-card").first()).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
});
