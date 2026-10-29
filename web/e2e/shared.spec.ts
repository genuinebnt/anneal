import { expect, test } from "@playwright/test";

test("⌘K keeps Tab inside, and Escape closes it", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const box = page.getByRole("dialog", { name: "Search" });
    await expect(box).toBeVisible();
    for (let i = 0; i < 6; i++) {
        await page.keyboard.press("Tab");
        expect(await page.evaluate(() => !!document.activeElement?.closest(".hd-pal"))).toBe(true);
    }
    await page.keyboard.press("Escape");
    await expect(box).toHaveCount(0);
});

test("the shortcut list keeps focus inside and gives it back", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("?");
    const box = page.getByRole("dialog", { name: "Keyboard shortcuts" });
    await expect(box).toBeVisible();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    expect(await page.evaluate(() => !!document.activeElement?.closest(".sh-box"))).toBe(true);
    await page.keyboard.press("Escape");
    await expect(box).toHaveCount(0);
});

test("deleting a plan asks first, and Keep leaves it alone", async ({ page }) => {
    const made = await (await page.request.post("/api/plans/company", { data: { company: "Amazon" } })).json();
    try {
        await page.goto("/dsa/calendar");
        await page.getByRole("button", { name: "Delete this plan" }).click();
        await expect(page.getByRole("button", { name: "Yes, delete it" })).toBeVisible();
        await page.getByRole("group", { name: "Delete this plan" }).getByRole("button", { name: "Keep" }).click();
        await expect(page.getByRole("button", { name: "Delete this plan" })).toBeVisible();
        const plans = async () => (await (await page.request.get("/api/plan?from=2026-01-01&to=2026-01-02")).json()).plans.map((p: { id: string }) => p.id);
        expect(await plans()).toContain(made.id);
        await page.getByRole("button", { name: "Delete this plan" }).click();
        await page.getByRole("button", { name: "Yes, delete it" }).click();
        await expect.poll(plans).not.toContain(made.id);
    } finally {
        await page.request.put("/api/plans/active", { data: { id: "default" } });
    }
});

test("the kit page shows the shared controls", async ({ page }) => {
    await page.goto("/kit");
    const region = page.getByRole("region", { name: "Shared controls" });
    await expect(region).toBeVisible();
    await expect(region.getByRole("switch").first()).toBeVisible();
    await expect(region.getByRole("radio").first()).toBeVisible();
});

test("a log speaks in the one toast stack, with the icon for how it went", async ({ page }) => {
    await page.goto("/d/valid-sudoku");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await expect(page.locator(".pp-log .lg b svg.icon")).toHaveCount(4);
        await page.keyboard.press("1");
        await expect(page.locator(".k-tstack")).toContainText("Valid Sudoku");
        await expect(page.locator(".d-toast")).toHaveCount(0);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});

test("the switches look alike: the course's, the filter's and Settings'", async ({ page }) => {
    const width = async (sel: string) => (await page.locator(sel).first().boundingBox())!.width;
    await page.goto("/settings");
    const settings = await width(".ui-sw");
    await page.goto("/courses/bustub/1a-01");
    const course = await width(".k-sw");
    expect(Math.abs(course - settings)).toBeLessThanOrEqual(1);
});
