import { expect, test } from "@playwright/test";

// The starter code of this problem does not compile, which is a fast, deterministic run result.
test("the run result stays pinned above the cases while they scroll", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    const strip = page.locator(".tstrip");
    const run = strip.getByRole("button", { name: /Run tests|Testing/ });
    await expect(run).toBeEnabled();
    await run.click();
    // the button reads "Testing…" while the run is in progress and the strip then shows its result
    await expect(strip.getByRole("button", { name: "Run tests" })).toBeEnabled({ timeout: 60_000 });
    await expect(strip).toContainText("Doesn't compile");
    // one segment per test; a solved problem also shows its hidden tests, so this must not depend on what ran before
    expect(await strip.locator(".passbar span").count()).toBeGreaterThanOrEqual(5);

    const body = page.locator(".pane.r .pbody");
    await body.evaluate((e) => (e.scrollTop = e.scrollHeight));
    await expect.poll(() => body.evaluate((e) => e.scrollTop)).toBeGreaterThan(0);
    const b = await body.boundingBox();
    const s = await strip.boundingBox();
    expect(Math.abs(s!.y - b!.y)).toBeLessThan(3);
    await expect(strip.getByRole("button", { name: "Run tests" })).toBeVisible();
});

test("cases that did not run show a ring, not a coloured square", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    await expect(page.locator(".tcase-h svg.cicon").first()).toBeVisible();
    await expect(page.locator(".tcase-h .sqr")).toHaveCount(0);
});

test("compiler output is drawn in the theme's terminal colours, with no escape codes in its text", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    const run = page.locator(".tstrip").getByRole("button", { name: /Run tests|Testing/ });
    await run.click();
    await expect(page.locator(".tstrip")).toContainText("Doesn't compile", { timeout: 60_000 });
    const body = page.locator(".dcard.error .dcard-b").first();
    await expect(body).toBeVisible();
    await expect(body.locator('span[style*="--ansi-bright-red"]').first()).toBeVisible();
    expect(await body.innerText()).not.toMatch(/\x1b|\[0m|\[1m/);
});
