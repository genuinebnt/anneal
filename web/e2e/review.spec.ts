import { expect, test } from "@playwright/test";

// Nothing is due in a fresh database, so these start from "review ahead": an upcoming review picked early.
test("pick an upcoming review, grade it, and undo the grade", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-group-anagrams/log", { data: { grade: "good" } });
    try {
        await page.goto("/dsa/review");
        await expect(page.locator(".rv-ahead summary")).toBeVisible();
        await page.locator(".rv-ahead").getByRole("button", { name: /Group Anagrams/ }).click();
        await expect(page.locator(".rv-title")).toHaveText("Group Anagrams");

        // the timer waits to be started
        await page.waitForTimeout(1500);
        await expect(page.locator(".rv-t .c")).toHaveText("00:00");
        await page.getByRole("button", { name: "Start timer" }).click();
        await expect(page.locator(".rv-t .c")).not.toHaveText("00:00");

        await page.keyboard.press("Space");
        await page.getByRole("button", { name: /^Good/ }).click();
        await expect(page.getByText("SESSION DONE")).toBeVisible();
        await page.getByRole("button", { name: /Undo the last grade/ }).click();
        await expect(page.locator(".rv-title")).toHaveText("Group Anagrams");
        await expect(page.locator(".rv-t .c")).toHaveText("00:00");
        await expect(page.locator(".rv-sg")).toContainText("0 done");
    } finally {
        await page.request.delete("/api/dsa/problems/lc-group-anagrams/log");
    }
});

test("the review queue sort choice is remembered", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-group-anagrams/log", { data: { grade: "good" } });
    try {
        await page.goto("/dsa/review");
        await page.locator(".rv-ahead").getByRole("button", { name: /Group Anagrams/ }).click();
        await page.getByRole("button", { name: "Overdue" }).click();
        await expect(page.getByRole("button", { name: "Overdue" })).toHaveAttribute("aria-pressed", "true");
        await page.reload();
        await page.goto("/dsa/review");
        await page.locator(".rv-ahead").getByRole("button", { name: /Group Anagrams/ }).click();
        await expect(page.getByRole("button", { name: "Overdue" })).toHaveAttribute("aria-pressed", "true");
    } finally {
        await page.request.delete("/api/dsa/problems/lc-group-anagrams/log");
    }
});
