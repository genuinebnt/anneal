import { expect, test } from "@playwright/test";

test("Read: the problem first, the idea on reveal, and marking it schedules a reread without touching the coding reviews", async ({ page, request }) => {
    const dues = async () => JSON.stringify((await (await request.get("/api/dsa")).json()).problems.map((p: { id: string; state?: { due?: string } }) => [p.id, p.state?.due ?? null]));
    const before = await dues();
    await page.goto("/dsa/read");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Read");
    // "Everything" always has cards nobody has read yet, whatever an earlier run left behind
    await page.getByRole("radio", { name: "Everything" }).click();
    const title = page.locator(".rd-card h2");
    await expect(title).toBeVisible();
    const first = await title.textContent();
    await expect(page.locator(".rd-ins")).toHaveCount(0);
    await page.keyboard.press("Space");
    await expect(page.locator(".rd-ins")).toBeVisible();
    await page.getByRole("button", { name: /Remembered/ }).click();
    await expect(page.locator(".ly-tl", { hasText: "This visit" })).toContainText("1");
    await expect(title).not.toHaveText(first!);
    expect(await dues()).toBe(before);
});

test("Read fits a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/dsa/read");
    await expect(page.locator(".ly-h1")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
});
