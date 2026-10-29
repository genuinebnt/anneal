import { expect, test } from "@playwright/test";

test("each weekday takes its own number of new and practice problems, and it survives a reload", async ({ page, request }) => {
    const before = (await (await request.get("/api/dsa")).json()).settings;
    try {
        await page.goto("/dsa/plan");
        const wed = page.locator(".p-day").nth(2);
        await expect(wed.locator(".n")).toHaveText("WED");
        // make sure Wednesday is a solve day, then ask for three new problems and four practice ones
        if (!(await wed.getAttribute("class"))?.includes("solve")) {
            while (!(await wed.getAttribute("class"))?.includes("solve")) await wed.locator(".mode").click();
        }
        const solve = wed.locator(".p-cap").nth(1);
        const practice = wed.locator(".p-cap").nth(0);
        await expect(solve.locator("span").first()).toHaveText("solve");
        await expect(practice.locator("span").first()).toHaveText("practice");
        while (Number(await solve.locator("b").innerText()) < 3) await solve.getByRole("button", { name: "more" }).click();
        while (Number(await practice.locator("b").innerText()) < 4) await practice.getByRole("button", { name: "more" }).click();
        await expect(page.locator(".p-bar").nth(2).locator("b")).toHaveText("7");
        await page.getByRole("button", { name: "Save plan" }).click();
        await expect(page.getByText("Saved.")).toBeVisible();
        await page.reload();
        const again = page.locator(".p-day").nth(2);
        await expect(again.locator(".p-cap").nth(1).locator("b")).toHaveText("3");
        await expect(again.locator(".p-cap").nth(0).locator("b")).toHaveText("4");
        // the other solve days keep the usual number
        await expect(page.locator(".p-day.solve").first().locator(".p-cap").nth(1).locator("b")).toHaveText(/^[1-9]/);
    } finally {
        await request.put("/api/settings/srs", { data: before });
    }
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
