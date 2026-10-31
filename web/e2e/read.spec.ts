import { expect, test } from "@playwright/test";

test("Read: the problem first, the idea on reveal, and marking it schedules a reread without touching the coding review", async ({ page, request }) => {
    await request.post("/api/dsa/problems/lc-two-sum/log", { data: { grade: "good" } });
    try {
        const before = await (await request.get("/api/dsa")).json();
        const due = before.problems.find((p: { id: string }) => p.id === "lc-two-sum").state.due;
        await page.goto("/dsa/read");
        await expect(page.locator(".hd-tabs a.on")).toHaveText("Read");
        await expect(page.locator(".rd-card h2")).toBeVisible();
        await expect(page.locator(".rd-ins")).toHaveCount(0);
        await page.keyboard.press("Space");
        await expect(page.locator(".rd-ins")).toBeVisible();
        await page.getByRole("button", { name: /Remembered/ }).click();
        await expect(page.locator(".ly-tl", { hasText: "This visit" })).toContainText("1");
        const after = await (await request.get("/api/dsa")).json();
        expect(after.problems.find((p: { id: string }) => p.id === "lc-two-sum").state.due).toBe(due);
    } finally {
        await request.delete("/api/dsa/problems/lc-two-sum/log");
    }
});

test("Read fits a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/dsa/read");
    await expect(page.locator(".ly-h1")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
});
