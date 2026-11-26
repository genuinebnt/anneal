import { expect, test } from "@playwright/test";

test("cookbook pages stay closed until asked, labelled by how much they give away", async ({ page }) => {
    await page.goto("/p/c4-drain-with-deadline");
    await page.getByRole("tab", { name: /^Hints/ }).click();
    const help = page.locator(".cbhelp");
    await expect(help.locator("summary strong")).toHaveText(["Related", "Similar"]);
    await expect(help.locator("details[open]")).toHaveCount(0);
    await expect(help.getByRole("link").first()).toBeHidden();

    await help.locator("summary").first().click();
    const link = help.getByRole("link").first();
    await expect(link).toHaveAttribute("href", "/cookbook/p/graceful-shutdown");
    await expect(link).toHaveAttribute("target", "_blank");
    await expect(help.locator("details[open]")).toHaveCount(1);
});

test("a section reference links to its heading, and all three levels come in order", async ({ page }) => {
    await page.goto("/p/y14-pratt-parser");
    await page.getByRole("tab", { name: /^Hints/ }).click();
    await expect(page.locator(".cbhelp summary strong")).toHaveText(["Related", "Exact"]);
    await page.goto("/p/y5-catch-the-broken-queue");
    await page.getByRole("tab", { name: /^Hints/ }).click();
    const similar = page.locator(".cbhelp details").nth(1);
    await similar.locator("summary").click();
    await expect(similar.getByRole("link").nth(1)).toHaveAttribute("href", "/cookbook/p/testing-unsafe-code#cb-show-that-each-check-can-fail");
});

test("a problem with no references shows no cookbook block", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    await page.getByRole("tab", { name: /^Hints/ }).click();
    await expect(page.locator(".hint").first()).toBeVisible();
    await expect(page.locator(".cbhelp")).toHaveCount(0);
});
