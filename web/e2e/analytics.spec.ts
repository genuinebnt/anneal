import { expect, test } from "@playwright/test";

test("Analytics shows the server, the database and the live log, and the header links to it", async ({ page, request }) => {
    const s = await (await request.get("/api/system")).json();
    expect(s.db.size).toBeGreaterThan(0);
    await page.goto("/dsa");
    await page.getByRole("link", { name: "Analytics" }).first().click();
    await expect(page).toHaveURL(/\/analytics$/);
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Analytics");
    await expect(page.locator(".ly-tl", { hasText: "VPS uptime" })).toBeVisible();
    await expect(page.getByText("Containers on the VPS")).toBeVisible();
    await expect(page.locator(".ly-pn", { hasText: "Database" }).getByText("Size")).toBeVisible();
    // the page's own request to /api/system shows in the live log
    await expect(page.locator(".an-log")).toContainText("/system");
    await page.getByRole("radio", { name: "1h" }).click();
    await expect(page.getByRole("radio", { name: "1h" })).toHaveAttribute("aria-checked", "true");
});

test("Analytics fits a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/analytics");
    await expect(page.locator(".ly-tl", { hasText: "VPS uptime" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
});
