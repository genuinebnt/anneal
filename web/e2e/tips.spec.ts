import { expect, test } from "@playwright/test";

test("a metric explains itself on hover and keyboard focus", async ({ page }) => {
    await page.goto("/dsa/plan");
    const label = page.locator(".ly-tiles .hinted", { hasText: "New a week" });
    await label.focus();
    await expect.poll(() => label.evaluate((el) => getComputedStyle(el, "::after").opacity)).toBe("1");
    await expect(label).toHaveAttribute("data-tip", /New problems you will start/);
});

test("dragging a topic by its grip reorders the plan, and the arrows still work", async ({ page }) => {
    // tall, so the row dragged from and the row dropped on are both in view (a drag cannot scroll the page)
    await page.setViewportSize({ width: 1440, height: 1600 });
    await page.goto("/dsa/calendar");
    const rows = page.locator(".c-tr");
    await expect(rows.first()).toBeVisible();
    const names = async () => rows.locator(".nm b").allInnerTexts();
    const before = await names();
    try {
        await rows.nth(0).locator(".grip").dragTo(rows.nth(2));
        await expect.poll(async () => (await names())[2]).toBe(before[0]);
        expect((await names())[0]).toBe(before[1]);
        // the arrow buttons are still there
        await rows.nth(2).getByRole("button", { name: "Move up" }).click();
        await expect.poll(async () => (await names())[1]).toBe(before[0]);
    } finally {
        // put the order back, whatever happened: the arrow buttons move one place each
        for (let i = 0; i < before.length; i++) {
            const now = await names();
            if (now.join() === before.join()) break;
            const want = now.findIndex((n, k) => n !== before[k]);
            const from = now.indexOf(before[want]!);
            if (from < 0) break;
            for (let k = from; k > want; k--) await rows.nth(k).getByRole("button", { name: "Move up" }).click();
            await page.waitForTimeout(500);
        }
        expect(await names()).toEqual(before);
    }
});

test("a one-time tip shows for a person, is dismissed for good, and never shows in an automated browser", async ({ browser }) => {
    const human = await browser.newContext();
    await human.addInitScript(() => Object.defineProperty(Navigator.prototype, "webdriver", { get: () => false }));
    const page = await human.newPage();
    // this browser's choices are not sent to the shared test database, so the test can run again
    await page.route("**/api/prefs", (r) => r.fulfill({ json: {} }));
    await page.goto("/dsa");
    const tip = page.getByRole("note", { name: "Tip" });
    await expect(tip).toContainText("Search every page", { timeout: 8000 });
    await tip.getByRole("button", { name: "Got it" }).click();
    await expect(tip).toContainText("keyboard shortcut");
    await page.reload();
    await expect(page.getByRole("note", { name: "Tip" })).not.toContainText("Search every page", { timeout: 4000 });
    await human.close();

    const robot = await browser.newContext();
    const p2 = await robot.newPage();
    await p2.goto("/dsa");
    await p2.waitForTimeout(3200);
    await expect(p2.getByRole("note", { name: "Tip" })).toHaveCount(0);
    await robot.close();
});
