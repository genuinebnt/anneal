import { expect, test } from "@playwright/test";

// The section page shared by the Rust area (docs/mockups/section-page.html); the DSA home is its own page with the same parts.
test.describe("/rust", () => {
    test.beforeEach(async ({ page }) => {
        await page.goto("/rust");
        await expect(page.locator(".s-trk").first()).toBeVisible();
    });

    test("the numbers count up to the real ones", async ({ page }) => {
        const cards = await page.locator(".s-trk").count();
        await expect(page.locator(".ly-tl strong").first()).toHaveText(String(cards), { timeout: 5000 });
        await expect(page.locator(".ly-tl strong").nth(1)).not.toHaveText("0");
        await expect(page.locator(".ly-tl strong").nth(2)).toHaveText(/^~\d/);
    });

    test("the cards and the page rise in and end fully visible", async ({ page }) => {
        const first = page.locator(".s-trk").first();
        expect(await first.evaluate((e) => getComputedStyle(e).animationName)).toBe("s-rise");
        await expect.poll(() => first.evaluate((e) => getComputedStyle(e).opacity), { timeout: 5000 }).toBe("1");
        await expect.poll(() => page.locator(".s-trk").last().evaluate((e) => getComputedStyle(e).opacity), { timeout: 8000 }).toBe("1");
    });

    test("the view toggle has a thumb that slides to list and back", async ({ page }) => {
        const seg = page.locator(".s-seg");
        const x0 = await seg.locator(".s-thumb").evaluate((e) => (e as HTMLElement).style.transform);
        await seg.getByRole("button", { name: /list/ }).click();
        await expect(page.locator(".s-grid.s-list").first()).toBeVisible();
        await expect.poll(() => seg.locator(".s-thumb").evaluate((e) => (e as HTMLElement).style.transform)).not.toBe(x0);
        await seg.getByRole("button", { name: /grid/ }).click();
        await expect(page.locator(".s-grid.s-list")).toHaveCount(0);
    });

    test("a card gets a light that follows the pointer", async ({ page }) => {
        const card = page.locator("a.s-trk").first();
        await card.scrollIntoViewIfNeeded();
        const box = (await card.boundingBox())!;
        await page.mouse.move(box.x + 40, box.y + 30);
        await page.mouse.move(box.x + 120, box.y + 60);
        await expect.poll(() => card.evaluate((e) => (e as HTMLElement).style.getPropertyValue("--mx"))).toMatch(/^\d+(\.\d+)?px$/);
    });

    test("no planned or placeholder track is shown", async ({ page }) => {
        await expect(page.locator(".s-trk.s-plan")).toHaveCount(0);
    });

    test("search narrows the cards, the empty state offers a way back, and / focuses the box", async ({ page }) => {
        await page.keyboard.press("/");
        await expect(page.getByLabel("Search tracks")).toBeFocused();
        const all = await page.locator(".s-trk:not(.s-gone)").count();
        await page.getByLabel("Search tracks").fill("zzzz-nothing-here");
        await expect(page.getByText("No track matches that.")).toBeVisible();
        await expect(page.locator(".s-trk:not(.s-gone)")).toHaveCount(0);
        await page.getByRole("button", { name: "Clear filters" }).click();
        await expect(page.locator(".s-trk:not(.s-gone)")).toHaveCount(all);
    });

    test("a filter chip shows only its tracks and the all chip restores them", async ({ page }) => {
        const all = await page.locator(".s-trk:not(.s-gone)").count();
        const chips = page.locator(".ly-lp .d-opt");
        expect(await chips.count()).toBeGreaterThan(2);
        await chips.nth(1).click();
        await expect(chips.nth(1)).toHaveAttribute("aria-pressed", "true");
        expect(await page.locator(".s-trk:not(.s-gone)").count()).toBeLessThanOrEqual(all);
        await chips.first().click();
        await expect(page.locator(".s-trk:not(.s-gone)")).toHaveCount(all);
    });
});

test.describe("reduced motion", () => {
    test.use({ contextOptions: { reducedMotion: "reduce" } });
    test("nothing animates and the numbers are final at once", async ({ page }) => {
        await page.goto("/rust");
        const card = page.locator(".s-trk").first();
        await expect(card).toBeVisible();
        expect(await card.evaluate((e) => getComputedStyle(e).animationName)).toBe("none");
        const cards = await page.locator(".s-trk").count();
        await expect(page.locator(".ly-tl strong").first()).toHaveText(String(cards), { timeout: 1000 });
    });
});
