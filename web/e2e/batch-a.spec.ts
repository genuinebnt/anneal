import { expect, test } from "@playwright/test";

test("an unknown pattern says so and links back", async ({ page }) => {
    await page.goto("/dsa/patterns/no-such-pattern");
    await expect(page.getByText('There is no pattern called "no-such-pattern"')).toBeVisible();
    await expect(page.getByRole("link", { name: "Back to the problems" })).toBeVisible();
});

test("a pattern page offers the next unsolved problem", async ({ page }) => {
    const o = await (await page.request.get("/api/dsa")).json();
    await page.goto(`/dsa/patterns/${o.patterns[0].code}`);
    await expect(page.locator(".ps-next")).toBeVisible();
    await page.locator(".ps-next").click();
    await expect(page).toHaveURL(/\/d\//);
});

test("the theme button cycles dark, light and system", async ({ page }) => {
    await page.goto("/dsa");
    const theme = () => page.evaluate(() => document.documentElement.dataset.theme ?? "system");
    const start = await theme();
    const seen = [start];
    for (let i = 0; i < 3; i++) {
        await page.locator(".thm").click();
        seen.push(await theme());
    }
    expect(new Set(seen.slice(0, 3))).toEqual(new Set(["dark", "light", "system"]));
    expect(seen[3]).toBe(start);
});

test("the calendar moves by month with the arrow keys and T returns to today", async ({ page }) => {
    await page.goto("/dsa/calendar");
    const heading = page.locator(".c-top h2");
    const now = await heading.textContent();
    await page.keyboard.press("ArrowRight");
    await expect(heading).not.toHaveText(now!);
    await page.keyboard.press("t");
    await expect(heading).toHaveText(now!);
    await expect(page.locator(".c-msum")).toContainText("problem");
});

test("the Mock interview's Start button is visible without scrolling", async ({ page }) => {
    await page.goto("/dsa/mock");
    await expect(page.getByRole("button", { name: /Draw and start the clock/ })).toBeInViewport();
});

test("a page that is still loading shows a placeholder, not bare text", async ({ page }) => {
    await page.route("**/api/dsa/review", async (route) => {
        await new Promise((r) => setTimeout(r, 800));
        await route.continue();
    });
    await page.goto("/dsa/review");
    await expect(page.locator(".sk")).toBeVisible();
});

test("? opens the list of shortcuts and Escape closes it", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("?");
    await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
    await expect(page.getByText("Log On my own")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
});
