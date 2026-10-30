import { expect, test } from "@playwright/test";

for (const [name, width, height] of [["desktop", 1440, 900], ["phone", 390, 800]] as const) {
    test(`the bell's panel stays inside the window on a ${name}`, async ({ page }) => {
        await page.setViewportSize({ width, height });
        await page.goto("/dsa");
        await page.getByRole("button", { name: /^Notifications/ }).click();
        const pop = page.getByRole("dialog", { name: "Notifications" });
        await expect(pop.locator(".nt-lab").first()).toContainText("NEEDS YOU");
        await page.waitForTimeout(600);
        const b = (await pop.boundingBox())!;
        expect(b.x).toBeGreaterThanOrEqual(11);
        expect(b.x + b.width).toBeLessThanOrEqual(width - 11);
        expect(b.y + b.height).toBeLessThanOrEqual(height);
    });
}

test("a dismissed item folds away and can be shown again", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "again" } });
    try {
        await page.goto("/dsa");
        await page.getByRole("button", { name: /^Notifications/ }).click();
        const pop = page.getByRole("dialog", { name: "Notifications" });
        const items = pop.locator(".nt-item");
        await expect(items.first()).toBeVisible();
        const before = await items.count();
        await items.first().hover();
        await items.first().getByRole("button", { name: /Dismiss/ }).click();
        await expect(items).toHaveCount(before - 1);
        await pop.getByRole("button", { name: /dismissed/ }).click();
        await expect(items).toHaveCount(before);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});
