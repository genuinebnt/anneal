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
    await expect(page.getByRole("heading", { name: "This page tripped. Your progress is safe." })).toBeVisible();
    await expect(page.getByRole("button", { name: "Back to where I was" })).toBeVisible();
    await page.getByText("What happened").click();
    await expect(page.getByRole("button", { name: "Copy the details" })).toBeVisible();
});

test("an unknown address says so and suggests what was probably meant", async ({ page }) => {
    await page.goto("/d/permutation-in-strng");
    await expect(page.getByRole("heading", { name: "Nothing lives at this address." })).toBeVisible();
    await expect(page.locator(".es-path")).toHaveText("/d/permutation-in-strng");
    await page.locator(".es-guess a", { hasText: "Permutation in String" }).click();
    await expect(page).toHaveURL(/\/d\/permutation-in-string$/);
});

test("a page whose data fails says what failed, retries by itself, and recovers", async ({ page }) => {
    let fail = true;
    await page.route("**/api/dsa", (route) => (fail ? route.fulfill({ status: 500, json: { error: "internal", message: "boom" } }) : route.continue()));
    await page.goto("/dsa");
    await expect(page.getByRole("heading", { name: "Your problems did not load." })).toBeVisible({ timeout: 15_000 });
    await expect(page.locator(".lf code")).toContainText("500");
    fail = false;
    await page.getByRole("button", { name: "Try now" }).click();
    await expect(page.locator(".lf")).toHaveCount(0);
});

test("while the server restarts for a deploy the app says it is updating", async ({ page }) => {
    await page.goto("/dsa");
    await expect(page.locator(".ly-h1")).toBeVisible();
    await page.route("**/api/**", (route) => route.fulfill({ status: 502, body: "Bad Gateway" }));
    await page.locator(".hd-tabs a", { hasText: "Review" }).click();
    await expect(page.getByRole("heading", { name: "Annealing a new version." })).toBeVisible();
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
