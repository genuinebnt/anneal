import { expect, test } from "@playwright/test";

// The DSA log: the four outcomes, the reset, and the Next up card. Run against a database you do not mind logging problems in.
test("the whole Next up card opens the problem page, the LeetCode button still goes to LeetCode", async ({ page }) => {
    await page.goto("/dsa");
    const card = page.locator(".s-next");
    await expect(card.locator("h3")).toBeVisible();
    const href = await card.locator(".dn-solve").getAttribute("href");
    expect(href).toContain("leetcode.com/problems/");
    await card.locator("h3").click();
    await expect(page).toHaveURL(/\/d\/[a-z0-9-]+$/);
});

test("a problem is logged four ways, Attempted is shown as attempted, and Reset takes it back to not started", async ({ page }) => {
    await page.goto("/d/two-sum");
    const log = page.locator(".pp-log");
    await expect(log.locator(".lg")).toHaveText([/On my own/, /With help/, /Attempted/, /Instant/]);
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

test("the toast after a log offers Undo, which takes the log back", async ({ page }) => {
    await page.goto("/d/valid-anagram");
    await page.locator(".pp-log").getByRole("button", { name: /With help/ }).click();
    await expect(page.locator(".pp-status")).toContainText("Solved with help");
    await page.locator(".k-tstack").getByRole("button", { name: "Undo" }).click();
    await expect(page.locator(".pp-status")).toContainText("Not started");
    await expect(page.locator(".k-tstack")).toContainText("log undone");
});

test("Reset archives the log first and the toast's Restore brings it back", async ({ page }) => {
    await page.goto("/d/contains-duplicate");
    await page.locator(".pp-log").getByRole("button", { name: /On my own/ }).click();
    await expect(page.locator(".pp-status")).toContainText("Solved on my own");
    await page.getByRole("button", { name: "Reset progress" }).click();
    await page.getByRole("button", { name: "Yes, reset" }).click();
    await expect(page.locator(".pp-status")).toContainText("Not started");
    await page.locator(".k-tstack").getByRole("button", { name: "Restore" }).click();
    await expect(page.locator(".pp-status")).toContainText("Solved on my own");
    // leave the database as it was, for the tests that follow
    await page.request.delete("/api/dsa/problems/lc-contains-duplicate/log");
});
