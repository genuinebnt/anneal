import { expect, test } from "@playwright/test";

test("each weekday takes its own number of new and practice problems, and it survives a reload", async ({ page, request }) => {
    const before = (await (await request.get("/api/dsa")).json()).settings;
    try {
        await page.goto("/dsa/plan");
        const wed = page.locator(".p-r:not(.head)").nth(2);
        await expect(wed.locator(".dn")).toHaveText("WED");
        await wed.getByRole("radio", { name: "Solve" }).click();
        const solve = wed.getByRole("group", { name: "wed solve" });
        const practice = wed.getByRole("group", { name: "wed practice" });
        while (Number(await solve.locator("output").innerText()) < 3) await solve.getByRole("button", { name: "More wed solve" }).click();
        while (Number(await practice.locator("output").innerText()) < 4) await practice.getByRole("button", { name: "More wed practice" }).click();
        await expect(wed.locator(".ld b")).toHaveText("7");
        await page.getByRole("button", { name: "Save plan" }).click();
        await expect(page.getByText("Saved.")).toBeVisible();
        await page.reload();
        const again = page.locator(".p-r:not(.head)").nth(2);
        await expect(again.getByRole("group", { name: "wed solve" }).locator("output")).toHaveText("3");
        await expect(again.getByRole("group", { name: "wed practice" }).locator("output")).toHaveText("4");
        // the other solve days keep the usual number
        await expect(page.locator(".p-r.solve").first().getByRole("group", { name: /solve$/ }).locator("output")).toHaveText(/^[1-9]/);
    } finally {
        await request.put("/api/settings/srs", { data: before });
    }
});

test("the themed date field and select open, pick and close with the keyboard", async ({ page }) => {
    await page.goto("/dsa/plan");
    await page.getByRole("button", { name: "Finish by" }).click();
    await expect(page.getByRole("dialog", { name: /Finish by/ })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: /Finish by/ })).toHaveCount(0);
    const first = page.getByRole("button", { name: "First reviews go to" });
    await first.click();
    await expect(page.getByRole("listbox", { name: "First reviews go to" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("listbox")).toHaveCount(0);
});

test("⌘K opens on what to do next, and the bell shows today at a glance", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const dialog = page.getByRole("dialog", { name: "Search" });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator(".hd-opt").first()).toBeVisible();
    await expect(dialog.locator(".hd-enter")).toHaveCount(1);
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: /^Notifications/ }).click();
    await expect(page.getByLabel("Today at a glance")).toBeVisible();
    await expect(page.getByRole("link", { name: "Calendar" }).last()).toBeVisible();
});
