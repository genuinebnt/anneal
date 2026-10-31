import { expect, test } from "@playwright/test";

test("the Learn tab is short: cards are folded, one is open, and a card opens and folds on a click", async ({ page }) => {
    await page.goto("/dsa/patterns/D1");
    const cards = page.locator(".lr-card");
    await expect(cards.first()).toBeVisible();
    expect(await cards.count()).toBeGreaterThan(20);
    await expect(page.locator(".lr-card.open")).toHaveCount(1);
    // it used to be about 45,000 px
    expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeLessThan(9000);
    const second = cards.nth(1);
    await second.locator(".lr-head").click();
    await expect(second).toHaveClass(/open/);
    await expect(second.getByText("USE IT WHEN")).toBeVisible();
    await second.locator(".lr-head").click();
    await expect(second).not.toHaveClass(/open/);
    await page.locator(".lr-tools").getByRole("button", { name: "open all" }).click();
    await expect(page.locator(".lr-card.open")).toHaveCount(await cards.count());
    await page.locator(".lr-tools").getByRole("button", { name: "fold all" }).click();
    await expect(page.locator(".lr-card.open")).toHaveCount(0);
});

test("the rail finds a technique and opens its card; a link with a card's address opens it too", async ({ page }) => {
    await page.goto("/dsa/patterns/D1");
    await page.getByLabel("Find a technique").fill("hash set");
    const hit = page.locator(".lr-rail .lr-tl button", { hasText: "Hash set for membership" });
    await expect(hit).toBeVisible();
    await hit.click();
    await expect(page.locator(".lr-card.open", { hasText: "Hash set for membership" })).toBeVisible();
    await page.goto("/dsa/patterns/D3#t-fixed-sum");
    await expect(page.locator("#t-fixed-sum")).toHaveClass(/open/);
});

test("a long list of problems scrolls inside a box as tall as the implementation, with every tag still on a row", async ({ page, request }) => {
    const d = await (await request.get("/api/dsa/patterns/D1")).json();
    const biggest = [...d.techniques, ...d.extras.map((e: { id: string; name: string; examples: unknown[]; lesson: unknown }) => ({ id: e.id, name: e.name, problems: e.examples }))].sort((a, b) => b.problems.length - a.problems.length)[0];
    expect(biggest.problems.length).toBeGreaterThan(6);
    await page.goto(`/dsa/patterns/D1#t-${biggest.id.split(":")[1]}`);
    const card = page.locator(`#t-${biggest.id.split(":")[1]}`);
    await expect(card).toHaveClass(/open/);
    const box = card.locator(".lr-rin");
    await expect(box).toBeVisible();
    await page.waitForTimeout(500);
    const m = await card.evaluate((el) => ({ left: el.querySelector(".lr-left")!.getBoundingClientRect().height, right: el.querySelector(".lr-right")!.getBoundingClientRect().height, scroll: el.querySelector(".lr-rin")!.scrollHeight, client: el.querySelector(".lr-rin")!.clientHeight }));
    expect(m.scroll).toBeGreaterThan(m.client);
    expect(Math.abs(m.right - Math.max(m.left, 340))).toBeLessThanOrEqual(2);
    await expect(card.locator(".lr-more.on")).toBeVisible();
    // the rows keep the spec's tags
    const row = card.locator(".lr-p").first();
    await expect(row.locator(".lr-pill.lv")).toBeVisible();
    await expect(row.locator(".lr-pill").nth(1)).toBeVisible();
    await box.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(card.locator(".lr-more.on")).toHaveCount(0);
    // the idea is hidden until asked for, and the button shows it
    const idea = card.locator(".lr-idea").first();
    await idea.scrollIntoViewIfNeeded();
    await idea.click();
    await expect(card.locator(".lr-ins").first()).toBeVisible();
});

test("on a phone the rail is a Jump to sheet, and nothing scrolls sideways", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/dsa/patterns/D1");
    await expect(page.locator(".lr-card").first()).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
    await page.getByRole("button", { name: /Jump to/ }).click();
    const sheet = page.getByRole("dialog", { name: "Jump to a technique" });
    await expect(sheet).toBeVisible();
    await sheet.getByLabel("Find a technique").fill("simulation");
    await sheet.locator(".lr-tl button", { hasText: "Direct simulation" }).click();
    await expect(sheet).toHaveCount(0);
    await expect(page.locator(".lr-card.open", { hasText: "Direct simulation" })).toBeVisible();
});

test("a technique an Idea tab describes has a moving thumbnail in its card header, and one that is not described has none", async ({ page }) => {
    await page.goto("/dsa/patterns/D3");
    const card = page.locator("#t-fixed-sum");
    await expect(card.locator(".lr-head .lr-thumb svg")).toBeVisible();
    // the window in the picture slides
    expect(await card.locator(".lr-thumb .id-gw").first().evaluate((e) => getComputedStyle(e).animationName)).toBe("lr-gw");
    await expect(page.locator("#t-running-best .lr-thumb")).toHaveCount(0);
    await page.goto("/dsa/patterns/D1");
    await expect(page.locator("#t-seen-set .lr-head .lr-thumb svg")).toBeVisible();
    await expect(page.locator("#t-simulation")).toBeVisible();
    await expect(page.locator("#t-simulation .lr-thumb")).toHaveCount(0);
});
