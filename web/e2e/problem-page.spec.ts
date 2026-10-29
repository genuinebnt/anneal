import { expect, test } from "@playwright/test";

test("the four log tiles are above the fold with Solve on LeetCode in the same card", async ({ page }) => {
    await page.goto("/d/contains-duplicate");
    await expect(page.locator(".pp-card").first().locator(".pp-act.pri")).toBeVisible();
    for (const name of [/On my own/, /With help/, /Attempted/, /Instant/]) await expect(page.locator(".pp-log").getByRole("button", { name })).toBeInViewport();
});

test("keys 1 to 4 log and U takes the log back", async ({ page }) => {
    await page.goto("/d/product-of-array-except-self");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await page.keyboard.press("3");
        await expect(page.locator(".pp-status")).toContainText("Attempted, not solved");
        await page.keyboard.press("u");
        await expect(page.locator(".pp-status")).toContainText("Not started");
    } finally {
        await page.request.delete("/api/dsa/problems/lc-product-of-array-except-self/log");
    }
});

test("after a log the toast names the next problem and links to it", async ({ page }) => {
    await page.goto("/d/longest-consecutive-sequence");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await page.keyboard.press("1");
        const next = page.locator(".k-tstack .d-next");
        await expect(next).toContainText("Next up:");
        await next.click();
        await expect(page).toHaveURL(/\/d\/(?!longest-consecutive-sequence)/);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-longest-consecutive-sequence/log");
    }
});

test("the countdown starts at the problem's estimate, and the timer stays stopped", async ({ page }) => {
    await page.goto("/d/contains-duplicate");
    await page.locator(".ft .ft-modes").getByRole("button", { name: "TIMER" }).click();
    // easy problems are estimated at 12 minutes, offered as 10
    await expect(page.locator(".ft .ft-ring b")).toHaveText("10:00");
    await expect(page.locator(".ft").getByRole("button", { name: "Start", exact: true })).toBeVisible();
});
