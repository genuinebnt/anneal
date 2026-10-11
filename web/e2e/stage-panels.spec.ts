import { expect, test, type Page } from "@playwright/test";

const stage = "/courses/bustub/4a-04";
const width = async (page: Page, sel: string) => Math.round((await page.locator(sel).boundingBox())!.width);

test.beforeEach(async ({ page }) => {
    await page.goto(stage);
    await expect(page.locator(".k-tabs")).toBeVisible();
});

test("the panels start wide: the course panel about 300px, the page panel about 300px", async ({ page }) => {
    expect(await width(page, ".k-tree")).toBeGreaterThanOrEqual(298);
    expect(await width(page, ".k-tree")).toBeLessThan(330);
    expect(await width(page, ".k-toc")).toBeGreaterThanOrEqual(280);
    expect(await width(page, ".k-toc")).toBeLessThan(325);
});

test("the tree opens on the current stage and scrolls it into view", async ({ page }) => {
    await expect(page.locator(".k-tlist .k-tl.k-cur")).toBeVisible();
    await expect(page.locator(".k-tlist .k-tl.k-cur")).toHaveAttribute("aria-current", "page");
});

test("dragging the edge resizes a panel, it is remembered, and a double-click resets it", async ({ page }) => {
    const handle = page.locator(".k-rz.k-l");
    const box = (await handle.boundingBox())!;
    await page.mouse.move(box.x + 4, box.y + 200);
    await page.mouse.down();
    await page.mouse.move(box.x + 4 + 70, box.y + 200, { steps: 6 });
    await page.mouse.up();
    await expect.poll(() => width(page, ".k-tree")).toBeGreaterThan(365);
    await page.reload();
    await expect(page.locator(".k-tree")).toBeVisible();
    expect(await width(page, ".k-tree")).toBeGreaterThan(365);
    await page.locator(".k-rz.k-l").dblclick();
    await expect.poll(() => width(page, ".k-tree")).toBeLessThan(330);
});

test("a panel never gets narrower than its minimum", async ({ page }) => {
    const box = (await page.locator(".k-rz.k-l").boundingBox())!;
    await page.mouse.move(box.x + 4, box.y + 200);
    await page.mouse.down();
    await page.mouse.move(box.x - 400, box.y + 200, { steps: 8 });
    await page.mouse.up();
    expect(await width(page, ".k-tree")).toBeGreaterThanOrEqual(248);
});

test("the course panel collapses to a rail of stage dots and comes back; the choice survives a reload", async ({ page }) => {
    // one dot per stage of the current module, challenges included: count its rows in the open tree first
    const rows = await page.locator(".k-tlist section:has(.k-tl.k-cur) .k-tl").count();
    expect(rows).toBeGreaterThan(5);
    await page.getByRole("button", { name: "Collapse the course panel" }).click();
    await expect(page.locator(".k-stage.k-lc")).toBeVisible();
    await expect.poll(() => width(page, ".k-tree")).toBeLessThan(90);
    await expect(page.locator(".k-rail .k-tl")).toHaveCount(rows);
    await page.reload();
    await expect(page.locator(".k-stage.k-lc")).toBeVisible();
    await page.getByRole("button", { name: "Expand the course panel" }).click();
    await expect(page.locator(".k-stage.k-lc")).toHaveCount(0);
    await expect.poll(() => width(page, ".k-tree")).toBeGreaterThanOrEqual(298);
});

test("the page panel hides and a button brings it back", async ({ page }) => {
    await page.getByRole("button", { name: "Hide sidebar" }).click();
    await expect(page.locator(".k-stage.k-rc")).toBeVisible();
    await page.getByRole("button", { name: "Show the page panel" }).click();
    await expect(page.locator(".k-stage.k-rc")).toHaveCount(0);
});

test("Ctrl+B and Ctrl+. toggle the panels", async ({ page }) => {
    await page.keyboard.press("Control+b");
    await expect(page.locator(".k-stage.k-lc")).toBeVisible();
    await page.keyboard.press("Control+b");
    await expect(page.locator(".k-stage.k-lc")).toHaveCount(0);
    await page.keyboard.press("Control+.");
    await expect(page.locator(".k-stage.k-rc")).toBeVisible();
});

test("filtering the tree shows only the matching stages, with their modules open", async ({ page }) => {
    await page.getByLabel("Filter stages").fill("watermark");
    await expect(page.locator(".k-tlist .k-tl")).toHaveCount(1);
    await expect(page.locator(".k-tlist .k-tl")).toContainText("watermark");
    await page.getByLabel("Filter stages").fill("zzzz-nothing");
    await expect(page.locator("text=No stage matches that.")).toBeVisible();
});

test("a module row opens and closes its stages", async ({ page }) => {
    const row = page.locator(".k-tmh", { hasText: "1A" });
    await expect(row).toHaveAttribute("aria-expanded", "false");
    await row.click();
    await expect(row).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator(".k-tlist .k-tl", { hasText: "Pages that survive" })).toBeVisible();
});

test("the page panel lists the concepts and links to them", async ({ page }) => {
    const first = page.locator(".k-toc a.k-cc2").first();
    await expect(first).toContainText("min read");
    await first.click();
    await expect(page).toHaveURL(/\/courses\/bustub\/concept\//);
});

test("the stage card shows the command, the last run and the hints used", async ({ page }) => {
    const card = page.locator(".k-rcard", { hasText: "THIS STAGE" });
    await expect(card).toContainText("anneal course test 4a-04");
    await expect(card).toContainText("Last run");
    await expect(card).toContainText("Hints used");
});

test.describe("on a phone", () => {
    test.use({ viewport: { width: 420, height: 880 } });

    test("the panels are drawers: closed, opened from the bar, closed by Escape or the backdrop", async ({ page }) => {
        await expect(page.locator(".k-tree.k-open")).toHaveCount(0);
        await page.getByRole("button", { name: /Stages/ }).click();
        await expect(page.locator(".k-tree.k-open")).toHaveCount(1);
        await page.keyboard.press("Escape");
        await expect(page.locator(".k-tree.k-open")).toHaveCount(0);
        await page.getByRole("button", { name: /Page & notes/ }).click();
        await expect(page.locator(".k-toc.k-open")).toHaveCount(1);
        await expect(page.locator(".k-toc.k-open .k-rcard", { hasText: "THIS STAGE" })).toBeVisible();
        await page.locator(".k-dbk").evaluate((el) => (el as HTMLElement).click());
        await expect(page.locator(".k-toc.k-open")).toHaveCount(0);
    });

    test("choosing a stage in the drawer goes there and closes it", async ({ page }) => {
        await page.getByRole("button", { name: /Stages/ }).click();
        await page.locator(".k-tlist .k-tl", { hasText: "The snapshot scan" }).click();
        await expect(page).toHaveURL(/4a-05/);
        await expect(page.locator(".k-tree.k-open")).toHaveCount(0);
    });

    test("there is no horizontal scroll", async ({ page }) => {
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
    });
});
