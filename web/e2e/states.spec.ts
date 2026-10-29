import { expect, test } from "@playwright/test";

test("when the server cannot be reached a bar says so, and Try again brings it back", async ({ page }) => {
    await page.route("**/api/dsa", (route) => route.abort());
    await page.goto("/dsa");
    await expect(page.getByRole("alert").filter({ hasText: "Can't reach anneal" })).toBeVisible();
    await page.unroute("**/api/dsa");
    await page.getByRole("button", { name: "Try again" }).click();
    await expect(page.getByRole("alert").filter({ hasText: "Can't reach anneal" })).toHaveCount(0);
    await expect(page.locator(".s-next")).toBeVisible();
});

test("a page that throws shows a way out, not a blank screen", async ({ page }) => {
    await page.goto("/kit");
    await page.getByRole("button", { name: "Show the error screen" }).click();
    await expect(page.getByRole("heading", { name: "This page ran into a problem." })).toBeVisible();
    await expect(page.getByRole("link", { name: "Back to the problems" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Copy details" })).toBeVisible();
});

test("the home's filters are in the address bar and come back after a reload", async ({ page }) => {
    await page.goto("/dsa");
    await page.locator(".d-goal").nth(1).click();
    await expect(page).toHaveURL(/list=new250/);
    await page.reload();
    await expect(page.locator(".d-goal").nth(1)).toHaveClass(/on/);
    await page.goto("/dsa?q=sudoku&recent=1");
    await expect(page.getByPlaceholder(/search title/)).toHaveValue("sudoku");
});

test("the calendar's month is in the address bar", async ({ page }) => {
    await page.goto("/dsa/calendar");
    const heading = page.locator(".c-top h2");
    await page.getByRole("button", { name: "Next month" }).click();
    const next = await heading.textContent();
    await expect(page).toHaveURL(/#\d{4}-\d{2}$/);
    await page.reload();
    await expect(heading).toHaveText(next!);
});

test("the review that is open is in the address bar", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-group-anagrams/log", { data: { grade: "good" } });
    try {
        await page.goto("/dsa/review");
        await page.locator(".rv-ahead").getByRole("button", { name: /Group Anagrams/ }).click();
        await expect(page).toHaveURL(/\?p=group-anagrams/);
        await page.reload();
        await expect(page.locator(".rv-title")).toHaveText("Group Anagrams");
    } finally {
        await page.request.delete("/api/dsa/problems/lc-group-anagrams/log");
    }
});

test("pages are preloaded on hover and the scroll position is kept on Back", async ({ page }) => {
    await page.goto("/dsa/practice/D1");
    await page.waitForTimeout(800);
    await page.evaluate(() => window.scrollTo(0, 1500));
    await page.waitForTimeout(400);
    const y = await page.evaluate(() => window.scrollY);
    expect(y).toBeGreaterThan(300);
    await page.getByRole("link", { name: "Calendar", exact: true }).click();
    await expect(page).toHaveURL(/\/dsa\/calendar/);
    await page.goBack();
    await page.waitForTimeout(800);
    expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(y / 2);
});
