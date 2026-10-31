import { expect, test } from "@playwright/test";

const premiumSwitch = (page: import("@playwright/test").Page) => page.getByRole("switch", { name: "Show and count premium problems" });

test("premium is not counted by default; switching it on raises the goal, switching it off restores it", async ({ page }) => {
    await page.goto("/dsa");
    const goal = page.locator(".d-goal").first().locator(":scope > b");
    await expect(goal).toBeVisible();
    const off = (await goal.innerText()).replace(/\s+/g, " ");

    await page.goto("/settings");
    await expect(premiumSwitch(page)).toHaveAttribute("aria-checked", "false");
    await expect(page.locator(".st-note")).toContainText("Turning this on shows them and makes the goal");
    await premiumSwitch(page).click();
    await expect(premiumSwitch(page)).toHaveAttribute("aria-checked", "true");
    await expect(page.locator(".st-note")).toContainText("All of them are shown and count now");

    await page.goto("/dsa");
    await expect(page.locator(".d-goal").first().locator(":scope > b")).not.toHaveText(off);

    await page.goto("/settings");
    await premiumSwitch(page).click();
    await expect(premiumSwitch(page)).toHaveAttribute("aria-checked", "false");
    await page.goto("/dsa");
    await expect(page.locator(".d-goal").first().locator(":scope > b")).toHaveText(off);
});

test("a premium problem is listed, and completing it counts toward the streak while the totals stay as they were", async ({ page }) => {
    const o = await (await page.request.get("/api/dsa")).json();
    const prem = o.problems.find((p: { premium: boolean; lists: string[]; state: { solved: boolean } }) => p.premium && p.lists.includes("neetcode150") && !p.state.solved);
    const before = o.plan.goal_done;
    const streakBefore = (await (await page.request.get("/api/activity?sections=D")).json()).streak;
    await page.goto(`/d/${prem.slug}`);
    await expect(page.locator(".pp-badges, .pp-band").first()).toBeVisible();
    await page.locator(".pp-log").getByRole("button", { name: /On my own/ }).click();
    await expect(page.locator(".pp-status")).toContainText("Solved on my own");
    const after = await (await page.request.get("/api/dsa")).json();
    expect(after.plan.goal_done).toBe(before);
    const streakAfter = (await (await page.request.get("/api/activity?sections=D")).json()).streak;
    expect(streakAfter).toBeGreaterThanOrEqual(Math.max(1, streakBefore));
    // leave the throwaway database as it was: take the log back
    await page.request.post(`/api/dsa/problems/${prem.id}/undo`);
});

test("the numbers bar can be hidden from Settings, and the choice is kept on the server", async ({ page }) => {
    await page.goto("/settings");
    await expect(page.locator(".sb")).toBeVisible();
    await page.getByRole("switch", { name: "Show the numbers bar" }).click();
    await expect(page.locator(".sb")).toHaveCount(0);
    await expect.poll(async () => (await (await page.request.get("/api/prefs")).json())["statusbar.hidden"]).toBe(true);
    await page.reload();
    await expect(page.locator(".sb")).toHaveCount(0);
    await page.getByRole("switch", { name: "Show the numbers bar" }).click();
    await expect(page.locator(".sb")).toBeVisible();
    await expect.poll(async () => (await (await page.request.get("/api/prefs")).json())["statusbar.hidden"]).toBe(false);
});

test("the backup downloads as an anneal backup and the theme can be set to system", async ({ page }) => {
    const res = await page.request.get("/api/export");
    expect(res.headers()["content-disposition"]).toContain("anneal-backup");
    expect((await res.json()).format).toBe("anneal-backup");
    await page.goto("/settings");
    await page.getByRole("radio", { name: "Light" }).click();
    expect(await page.evaluate(() => document.documentElement.dataset.theme)).toBe("light");
    await page.getByRole("radio", { name: "System" }).click();
    expect(await page.evaluate(() => document.documentElement.dataset.theme ?? "system")).toBe("system");
});

test("Hide LeetCode Premium in the filters and the Settings switch are the same setting", async ({ page }) => {
    await page.goto("/dsa");
    // the filters are always in the left panel
    const hide = page.getByRole("button", { name: "Hide LeetCode Premium" });
    await expect(hide).toHaveAttribute("aria-pressed", "true");
    await hide.click();
    await expect(hide).toHaveAttribute("aria-pressed", "false");
    await page.goto("/settings");
    await expect(premiumSwitch(page)).toHaveAttribute("aria-checked", "true");
    await premiumSwitch(page).click();
    await expect(premiumSwitch(page)).toHaveAttribute("aria-checked", "false");
});
