import { expect, test } from "@playwright/test";

// The DSA log: the four outcomes, the reset, and the Next up card. Run against a database you do not mind logging problems in.
test("the whole Next up card opens the problem page, the LeetCode button still goes to LeetCode", async ({ page }) => {
    await page.goto("/dsa");
    const card = page.locator(".s-next");
    await expect(card.locator("h3")).toBeVisible();
    const href = await card.locator(".s-cta").getAttribute("href");
    expect(href).toContain("leetcode.com/problems/");
    await card.locator("h3").click();
    await expect(page).toHaveURL(/\/d\/[a-z0-9-]+$/);
});

test("a problem is logged four ways, Attempted is shown as attempted, and Reset takes it back to not started", async ({ page }) => {
    await page.goto("/d/two-sum");
    const log = page.locator(".pp-log");
    await expect(log.locator(".lg")).toHaveText([/On my own/, /With hint/, /Attempted/, /Instant/]);
    await expect(page.locator(".pp-status")).toContainText("Not started");
    await expect(page.getByRole("button", { name: "Reset progress" })).toHaveCount(0);

    await log.getByRole("button", { name: /Attempted/ }).click();
    await expect(page.locator(".pp-status")).toContainText("Attempted, not solved");

    await page.getByRole("button", { name: "Reset progress" }).click();
    await page.getByRole("button", { name: "Keep" }).click();
    await expect(page.locator(".pp-status")).toContainText("Attempted, not solved");

    await page.getByRole("button", { name: "Reset progress" }).click();
    await page.getByRole("button", { name: "Yes, reset" }).click();
    await expect(page.locator(".pp-status")).toContainText("Not started");
    await expect(page.getByRole("button", { name: "Reset progress" })).toHaveCount(0);
});
