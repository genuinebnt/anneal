import { expect, test } from "@playwright/test";

test("today is three steps on the DSA home", async ({ page }) => {
    await page.goto("/dsa");
    await expect(page.locator(".d-today .d-step")).toHaveCount(3);
    await expect(page.locator(".d-today")).toContainText("Reviews");
    await expect(page.locator(".d-today")).toContainText("New problem");
});

test("comfort settings change the whole page and are remembered", async ({ page }) => {
    await page.goto("/settings");
    const attr = (name: string) => page.evaluate((n) => document.documentElement.getAttribute(n), name);
    await page.getByRole("switch", { name: "Reduce motion" }).click();
    expect(await attr("data-motion")).toBe("reduce");
    await page.getByRole("switch", { name: "High contrast" }).click();
    expect(await attr("data-contrast")).toBe("high");
    await page.getByRole("radio", { name: "110%" }).click();
    expect(await attr("data-zoom")).toBe("110");
    await page.reload();
    expect(await attr("data-contrast")).toBe("high");
    expect(await attr("data-zoom")).toBe("110");
    await page.getByRole("switch", { name: "Reduce motion" }).click();
    await page.getByRole("switch", { name: "High contrast" }).click();
    await page.getByRole("radio", { name: "100%" }).click();
    expect(await attr("data-zoom")).toBeNull();
});

test("the list can be compact, and a pattern's heading folds its problems", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("2");
    await expect(page.locator(".d-fold").first()).toBeVisible();
    const first = page.locator(".d-group").first();
    await expect(first.locator(".d-pcard").first()).toBeVisible();
    await first.locator(".d-fold").click();
    await expect(first.locator(".d-fold")).toHaveAttribute("aria-expanded", "false");
    await expect(first.locator(".d-pcard")).toHaveCount(0);
    await page.getByRole("button", { name: "Expand all" }).click();
    await expect(first.locator(".d-pcard").first()).toBeVisible();
    await page.getByRole("button", { name: "Filters" }).first().click();
    await page.getByRole("radio", { name: "Compact" }).click();
    expect(await page.evaluate(() => document.documentElement.dataset.density)).toBe("compact");
    await page.getByRole("radio", { name: "Comfortable" }).click();
});

test("a reviewed problem shows its recall curve, and reading mode hides the side column", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "good" } });
    try {
        await page.goto("/d/valid-sudoku");
        const curve = page.locator(".rc-card svg");
        await expect(curve).toBeVisible();
        await expect(curve).toHaveAttribute("aria-label", /Recall chance over the next 60 days/);
        await page.getByRole("button", { name: "Reading mode" }).click();
        await expect(page.locator("main.pp.reading")).toBeVisible();
        await expect(page.locator(".pp-rail")).toBeHidden();
        await page.getByRole("button", { name: "Reading mode" }).click();
        await expect(page.locator(".pp-rail")).toBeVisible();
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});

test("a written solution has line numbers", async ({ page }) => {
    await page.goto("/d/two-sum");
    await expect(page.locator("#pp-approaches .d-ln").first()).toBeVisible();
});

test("the interface has no What's new list and no bulk 'already solved' section", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.getByText("COUNTING", { exact: true })).toBeVisible();
    await expect(page.getByText(/what's new/i)).toHaveCount(0);
    await expect(page.getByText(/already solved before anneal/i)).toHaveCount(0);
    await page.goto("/dsa");
    await page.getByRole("button", { name: /^Notifications/ }).click();
    await expect(page.getByRole("dialog", { name: "Notifications" })).not.toContainText("was updated");
});
