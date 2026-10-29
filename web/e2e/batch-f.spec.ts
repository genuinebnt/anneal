import { expect, test } from "@playwright/test";

test("the phone quick log finds a problem by name and logs it", async ({ page }) => {
    await page.goto("/log");
    try {
        await page.getByLabel("Find a problem").fill("valid sudoku");
        const row = page.locator(".lg-row", { hasText: "Valid Sudoku" });
        await row.getByRole("button", { name: "On my own" }).click();
        await expect(page.locator(".k-tstack")).toContainText("Valid Sudoku");
        await expect.poll(async () => (await (await page.request.get("/api/dsa")).json()).problems.find((p: { id: string }) => p.id === "lc-valid-sudoku").state.solved).toBe(true);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});

test("a premium problem points to free problems with the same idea", async ({ page }) => {
    const o = await (await page.request.get("/api/dsa")).json();
    const prem = o.problems.find((p: { premium: boolean }) => p.premium);
    await page.goto(`/d/${prem.slug}`);
    await expect(page.getByText("FREE INSTEAD")).toBeVisible();
    await expect(page.locator(".pp-card", { hasText: "FREE INSTEAD" }).locator("a")).not.toHaveCount(0);
});

test("the calendar link can be made and replaced", async ({ page }) => {
    await page.goto("/settings");
    const input = page.getByLabel("Calendar link");
    await expect(page.getByRole("button", { name: /Make a link|New link/ })).toBeVisible();
    const before = (await input.count()) ? await input.inputValue() : "";
    await page.getByRole("button", { name: /Make a link|New link/ }).click();
    if (before) await expect(input).not.toHaveValue(before);
    else await expect(input).toBeVisible();
    const first = await input.inputValue();
    expect(first).toMatch(/\/api\/feed\/.+\.ics$/);
    expect((await page.request.get(new URL(first).pathname)).status()).toBe(200);
    await page.getByRole("button", { name: "New link" }).click();
    await expect(input).not.toHaveValue(first);
    expect((await page.request.get(new URL(first).pathname)).status()).toBe(404);
});

test("coming back from LeetCode asks how it went, and never logs by itself", async ({ page, context }) => {
    await context.route("https://leetcode.com/**", (route) => route.fulfill({ status: 200, contentType: "text/html", body: "<title>LeetCode</title>" }));
    await page.goto("/d/valid-sudoku");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        const popup = page.waitForEvent("popup");
        await page.locator(".pp-solve .pp-act.pri").click();
        await (await popup).close();
        await page.waitForTimeout(5500);
        await page.evaluate(() => window.dispatchEvent(new Event("focus")));
        const bar = page.getByRole("region", { name: "Back from LeetCode" });
        await expect(bar).toContainText("Done with Valid Sudoku?");
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await bar.getByRole("button", { name: "Not now" }).click();
        await expect(bar).toHaveCount(0);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});
