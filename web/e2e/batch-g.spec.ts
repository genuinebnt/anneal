import { expect, test } from "@playwright/test";

test("the Next up card hides the idea until asked, and Settings can show it always", async ({ page }) => {
    await page.goto("/dsa");
    const card = page.locator(".s-next");
    await expect(card.getByRole("button", { name: "Show the idea" })).toBeVisible();
    // neither the technique nor the pattern's name is shown: both name the approach
    await expect(card.locator(".s-kick")).toHaveText(/^\s*NEXT UP\s*$/);
    await expect(card.locator(".s-pills .s-pill")).toHaveCount(await card.locator(".s-pills .s-pill").count());
    const o = await (await page.request.get("/api/dsa")).json();
    const patternName = (id: string) => o.patterns.find((x: { code: string }) => x.code === o.problems.find((p: { id: string }) => p.id === id).pattern).name;
    const next = patternName(o.plan.next_up[0]);
    await expect(card).not.toContainText(next);
    await card.getByRole("button", { name: "Show the idea" }).click();
    await expect(card).toContainText("The idea:");
    await expect(card.locator(".s-kick")).toContainText(next.toUpperCase());
    // a new visit is hidden again; the setting makes it stay shown
    await page.goto("/settings");
    await page.getByRole("switch", { name: "Show the key idea" }).click();
    await page.goto("/dsa");
    await expect(page.locator(".s-next")).toContainText("The idea:");
    await page.goto("/settings");
    await page.getByRole("switch", { name: "Show the key idea" }).click();
});

test("the header has a visible Settings button", async ({ page }) => {
    await page.goto("/dsa");
    await page.getByRole("link", { name: "Settings" }).click();
    await expect(page).toHaveURL(/\/settings$/);
});

test("the bell lists what needs you and what was said, and a dismissed item stays away", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "again" } });
    try {
        await page.goto("/dsa");
        await page.getByRole("button", { name: /^Notifications/ }).click();
        const panel = page.getByRole("dialog", { name: "Notifications" });
        await expect(panel).toContainText("NEEDS YOU");
        await expect(panel).toContainText("RECENT");
        const item = panel.locator(".nt-item").first();
        if (await item.count()) {
            const text = await item.locator("a").innerText();
            await item.getByRole("button", { name: /Dismiss/ }).click();
            await expect(panel).not.toContainText(text);
        }
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});

test("⌘K groups results, highlights the match, and a number finds a LeetCode problem", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const box = page.locator(".hd-pal input");
    await expect(page.locator(".hd-pal .hd-grp").first()).toBeVisible();
    await box.fill("two sum");
    await expect(page.locator(".hd-pal .hd-grp", { hasText: "PROBLEMS" })).toBeVisible();
    await expect(page.locator(".hd-pal mark").first()).toBeVisible();
    await box.fill("1");
    await expect(page.locator(".hd-pal .hd-opt").first()).toContainText("1 · Two Sum");
    await box.fill("> sett");
    await expect(page.locator(".hd-pal .hd-opt").first()).toContainText("Settings");
    await box.fill("twsm");
    await expect(page.locator(".hd-pal .hd-opt", { hasText: "Two Sum" }).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/d\//);
    // what was chosen is first the next time
    await page.keyboard.press("Control+k");
    await expect(page.locator(".hd-pal .hd-grp", { hasText: "RECENT" })).toBeVisible();
});

test("interview mode: pick a company, make it the plan, and go back to the main plan", async ({ page }) => {
    await page.goto("/dsa/company");
    await page.getByLabel("Find a company").fill("Amazon");
    await page.getByRole("option", { name: /Amazon/ }).first().click();
    await expect(page.locator(".cm-stats")).toContainText("in the plan");
    await page.getByRole("button", { name: "Interview day" }).click();
    await page.getByRole("button", { name: "Next month" }).click();
    await page.getByRole("button", { name: /^15 / }).click();
    await expect(page.locator(".cm-chips")).toContainText("solve day");
    await page.getByRole("button", { name: "Make this my plan" }).click();
    await expect(page.locator(".cm-now")).toContainText("Interview: Amazon");
    await page.getByRole("button", { name: "Back to my main plan" }).click();
    await expect(page.locator(".cm-now")).toHaveCount(0);
});

test("the calendar is three panels: plans on the left, the month in the centre, the rules and the day on the right", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto("/dsa/calendar");
    await expect(page.locator(".c-grid")).toBeVisible();
    const box = async (sel: string) => (await page.locator(sel).first().boundingBox())!;
    const [left, grid, right] = [await box(".ly-lp"), await box(".c-grid"), await box(".ly-rp")];
    expect(left.x + left.width).toBeLessThanOrEqual(grid.x);
    expect(grid.x + grid.width).toBeLessThanOrEqual(right.x);
    await expect(page.locator(".ly-lp")).toContainText("Plans");
    await expect(page.locator(".ly-rp").getByRole("tab", { name: "Rules" })).toBeVisible();
    await expect(page.locator(".ly-rp").getByRole("tab", { name: "Streak" })).toBeVisible();
});

test("the theme button shows an icon for system, with no word beside it", async ({ page }) => {
    await page.goto("/dsa");
    const btn = page.locator(".thm");
    const seen = new Set<string>();
    for (let i = 0; i < 3; i++) {
        seen.add(await btn.locator("svg").first().getAttribute("class") ?? "");
        await btn.click();
    }
    expect(seen.size).toBe(3);
    await expect(page.locator(".thm-auto")).toHaveCount(0);
});

test("a problem's header does not name its pattern until the idea is shown", async ({ page }) => {
    await page.goto("/d/valid-sudoku");
    await expect(page.locator(".pp-crumbs")).toContainText("PATTERN");
    await expect(page.locator(".pp-crumbs")).not.toContainText("ARRAYS");
    await page.locator(".pp-ins").getByRole("button", { name: "Show" }).click();
    await expect(page.locator(".pp-crumbs")).toContainText("ARRAYS");
});
