import { expect, test } from "@playwright/test";

test("the Graph tab draws the graph, ranks where to start, and a lead opens in the inspector", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/dsa");
    await page.locator(".hd-tabs a", { hasText: "Graph" }).click();
    await expect(page).toHaveURL(/\/dsa\/graph$/);
    await expect(page.locator(".gr-wrap canvas")).toBeVisible();
    const nodes = Number(await page.locator(".gr-stat b").first().innerText());
    expect(nodes).toBeGreaterThan(100);
    const lead = page.locator(".ly-lp .ly-it").first();
    await expect(lead).toBeVisible();
    const name = (await lead.locator("h5").innerText()).trim();
    await lead.click();
    await expect(page.locator(".gr-insp h3")).toHaveText(name);
    // looks like: surroundings works for a technique
    await page.locator(".gr-insp").getByRole("button", { name: "Looks like" }).click();
    await expect(page.locator(".ly-lp .ly-it").first()).toContainText("alike");
    // the legend hides a pattern
    const first = page.locator(".gr-li").first();
    await first.click();
    await expect(first).toHaveAttribute("aria-pressed", "false");
});

test("Path from here, then a neighbour, traces the path; P switches to the Path mode", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/dsa/graph");
    await expect(page.locator(".gr-wrap canvas")).toBeVisible();
    await page.locator(".ly-lp .ly-it").first().click();
    await page.locator(".gr-insp").getByRole("button", { name: "Path from here" }).click();
    await expect(page.getByRole("tab", { name: /Path/ })).toHaveAttribute("aria-selected", "true");
    await page.locator(".gr-nb button").first().click();
    await expect(page.locator(".ly-lp .ly-ps")).toHaveCount(2);
    await page.keyboard.press("l");
    await expect(page.getByRole("tab", { name: /Start/ })).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("p");
    await expect(page.getByRole("tab", { name: /Path/ })).toHaveAttribute("aria-selected", "true");
});

test("the graph fits a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/dsa/graph");
    await expect(page.locator(".gr-wrap canvas")).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
});
