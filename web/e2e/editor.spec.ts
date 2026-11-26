import { expect, test } from "@playwright/test";

// Rust-analyzer isn't installed for every run of these tests, so they cover what works without it.
const PROBLEM = "/p/l1-fix-use-after-move"; // its starter doesn't compile

test("a run's error cards leave the code once it is edited, and come back with the code that ran", async ({ page }) => {
    await page.goto(PROBLEM);
    const run = page.locator(".tstrip").getByRole("button", { name: /Run tests|Testing/ });
    await expect(run).toBeEnabled();
    await run.click();
    const cards = page.locator(".cm-content .cm-lens");
    await expect(cards.first()).toBeVisible({ timeout: 60_000 });

    await page.locator(".cm-content .cm-line").first().click();
    await page.keyboard.press("End");
    await page.keyboard.type(" ");
    await expect(cards).toHaveCount(0);
    await page.keyboard.press("Backspace");
    await expect(cards.first()).toBeVisible();
});

test("in Vim's normal mode Space w saves, and in insert mode it is typed", async ({ page }) => {
    await page.goto(PROBLEM);
    const toggleVim = async () => {
        await page.getByTitle("Editor settings").first().click();
        await page.getByRole("switch", { name: "Vim mode" }).click();
        await page.getByTitle("Editor settings").first().click();
    };
    const wasOn = (await page.locator(".cm-vim-panel").count()) > 0;
    if (!wasOn) await toggleVim();
    await expect(page.locator(".cm-vim-panel").first()).toBeVisible();
    try {
        let saves = 0;
        page.on("request", (r) => {
            if (r.url().endsWith("/api/format")) saves += 1;
        });
        const first = page.locator(".cm-content .cm-line").first();
        await first.click();
        await page.keyboard.press("Escape");
        await page.keyboard.type("gg0");
        await page.keyboard.press("Space");
        await page.keyboard.press("w");
        await expect.poll(() => saves).toBe(1);

        await page.keyboard.type("i w");
        await expect(first).toHaveText(/^ w/);
        expect(saves).toBe(1);
        await page.keyboard.press("Escape");
        await page.keyboard.type("u");
    } finally {
        if (!wasOn) await toggleVim();
    }
});
