import { expect, test } from "@playwright/test";

test("progress shows readiness by pattern and by data structure, weakest first", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "good" } });
    try {
        await page.goto("/progress");
        const pat = page.locator(".pbox", { hasText: "Readiness by pattern" });
        const ds = page.locator(".pbox", { hasText: "Readiness by data structure" });
        await expect(pat.locator("tbody tr")).toHaveCount(18);
        await expect(ds.locator("tbody tr").first()).toBeVisible();
        for (const box of [pat, ds]) {
            const ready = (await box.locator("tbody tr td:last-child").allInnerTexts()).map((t) => parseInt(t, 10));
            expect(ready).toEqual([...ready].sort((a, b) => a - b));
            expect(ready[ready.length - 1]).toBeGreaterThan(0);
        }
        await expect(ds.locator("tbody tr", { hasText: "Array" }).first()).toContainText("1 /");
        await page.screenshot({ path: "test-results/readiness.png", fullPage: true });
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});
