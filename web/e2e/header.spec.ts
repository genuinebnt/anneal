import { expect, test } from "@playwright/test";

test("the header shows the area you are in and the pages of that area", async ({ page }) => {
    await page.goto("/courses");
    const areas = page.getByRole("navigation", { name: "Areas" }).first();
    await expect(areas.getByRole("link", { name: /Courses/ })).toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("BusTub");
    await expect(page.locator(".hd-crumb")).toHaveCount(0); // the tab already says where you are
    await areas.getByRole("link", { name: /DSA/ }).click();
    await expect(page).toHaveURL(/\/dsa$/);
    await expect(page.locator(".hd-tabs a", { hasText: "Mock interview" })).toBeVisible();
    await expect(page.locator(".hd-tabs a.on")).toHaveText(/Problems/);
});

test("a stage page adds a crumb to the area", async ({ page }) => {
    await page.goto("/courses/bustub/1a-01");
    await expect(page.locator(".hd-crumb")).toContainText("1a-01");
});

test("search opens with Ctrl+K, filters by name, opens a result with Enter and closes with Escape", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const box = page.getByRole("dialog", { name: "Search" });
    await expect(box).toBeVisible();
    await box.getByRole("textbox").fill("3i-04");
    await expect(box.getByRole("option").first()).toContainText("3i-04");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/courses\/bustub\/3i-04$/);
    await page.keyboard.press("Control+k");
    await expect(box).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(box).toHaveCount(0);
});

test("main screens are full width with no width option; reading screens keep their measure", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 900 });
    await page.goto("/courses");
    await expect.poll(async () => (await page.locator(".k-wrap").first().boundingBox())!.width).toBeGreaterThan(1700);
    await page.locator(".acct .av").click();
    await expect(page.getByRole("radio", { name: "Wide" })).toHaveCount(0);
    await page.goto("/courses/bustub/1a-01");
    const read = page.locator(".k-read2").first();
    await expect(read).toBeVisible();
    expect((await read.boundingBox())!.width).toBeLessThanOrEqual(1040);
});

test("Progress has its own place beside the areas, with Settings and Quick log as its pages", async ({ page }) => {
    await page.goto("/dsa");
    const areas = page.getByRole("navigation", { name: "Areas" }).first();
    await expect(page.locator(".hd-tabs a", { hasText: "Progress" })).toHaveCount(0);
    await areas.getByRole("link", { name: "Progress" }).click();
    await expect(page).toHaveURL(/\/progress$/);
    await expect(areas.getByRole("link", { name: "Progress" })).toHaveAttribute("aria-current", "page");
    await expect(areas.getByRole("link", { name: /DSA/ })).not.toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Progress");
    await expect(page.locator(".hd-tabs a", { hasText: "Settings" })).toBeVisible();
});

test("on a phone the areas are a bar at the bottom", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/courses");
    const dock = page.locator(".hd-dock");
    await expect(dock).toBeVisible();
    await expect(dock.getByRole("link", { name: /Courses/ })).toHaveAttribute("aria-current", "page");
    await expect(page.locator(".hd-areas")).toBeHidden();
});
