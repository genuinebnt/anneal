import { expect, test, type Page } from "@playwright/test";

const noSideScroll = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

test("The front page: headline, featured pages, the shelf of subjects and a search over everything", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/cookbook");
    await expect(page.locator(".hd-areas a.on")).toHaveAttribute("aria-label", "Cookbook");
    await expect(page.locator(".hd-tabs a")).toHaveText(["Overview", "Rust", "Algorithms", "Systems", "Data", "Distributed", "Compilers", "Testing", "Projects"]);
    await expect(page.locator(".cb-lead")).toBeVisible();
    await expect(page.locator(".cb-mini")).toHaveCount(3);
    await expect(page.locator(".cb-book")).toHaveCount(8);

    // "/" moves to the search; a word narrows, a subject narrows further, and clearing brings the shelf back
    await page.locator("body").press("/");
    await expect(page.getByLabel("Search the cookbook")).toBeFocused();
    await page.keyboard.type("buffer pool");
    await expect(page.locator(".cb-rows li").first()).toContainText(/buffer pool/i);
    await page.locator(".cb-kbar .cb-chips button", { hasText: "Compilers" }).click();
    await page.getByLabel("Search the cookbook").fill("no such page at all");
    await page.getByRole("button", { name: "Clear the search" }).click();
    await expect(page.locator(".cb-book")).toHaveCount(8);

    await page.locator(".cb-book", { has: page.getByRole("heading", { name: "Data", exact: true }) }).locator("li a", { hasText: "Query execution" }).click();
    await expect(page.locator(".cb-banner h1")).toHaveText("Data.");
    await expect(page.locator(".cb-chapter", { hasText: "Query execution" }).first()).toBeInViewport();
    expect(errors).toEqual([]);
});

test("A subject: its chapters in order, filters by words and kind, and the other books beside it", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/cookbook/systems");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Systems");
    await expect(page.locator(".cb-bk.on")).toContainText("Systems");
    const all = await page.locator(".cb-rows li").count();
    expect(all).toBeGreaterThan(50);
    await page.locator(".cb-sub .cb-chips button", { hasText: "Example" }).click();
    expect(await page.locator(".cb-rows li").count()).toBeLessThan(all);
    await page.getByLabel("Filter the pages").fill("no such page at all");
    await page.getByRole("button", { name: "Clear the filter" }).click();
    await expect(page.locator(".cb-rows li")).toHaveCount(all);
    await page.locator(".cb-bk-h", { hasText: "Algorithms" }).click();
    await expect(page.locator(".cb-banner h1")).toHaveText("Algorithms.");
    await page.goto("/cookbook/nothing-here");
    await expect(page.getByRole("alert")).toContainText("no subject");
});

test("A code page shows its code first, in a viewer that finds symbols, wraps, resizes and fills the window", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/cookbook/mvcc-snapshot-isolation");
    await page.goto("/cookbook/p/mvcc-snapshot-isolation");
    const view = page.locator(".cb-view");
    await expect(view).toBeVisible();
    // the code comes before the notes
    expect(await page.evaluate(() => !!(document.querySelector(".cb-view")!.compareDocumentPosition(document.querySelector(".cb-sec")!) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
    await expect(view.locator(".sd-ln").first()).toBeVisible();
    await expect(view.locator("figcaption")).toContainText("Read-only");
    // the file's opening notes are the page's text already, so the viewer starts at the code
    await expect(view.locator(".sd-ln").first()).not.toContainText("//!");
    await view.getByRole("button", { name: /Show the file's opening notes/ }).click();
    await expect(view.locator(".sd-ln").first()).toContainText("//!");
    await view.getByRole("button", { name: /Hide the file's opening notes/ }).click();
    await view.getByRole("button", { name: /Symbols/ }).click();
    await view.locator(".cb-syms button", { hasText: "main" }).last().click();
    await expect(view.locator(".sd-ln.sd-lit")).toBeVisible();
    const size = () => view.locator("pre").evaluate((e) => parseFloat(getComputedStyle(e).fontSize));
    const before = await size();
    await view.getByRole("button", { name: "Larger code" }).click();
    expect(await size()).toBeGreaterThan(before);
    await view.getByRole("button", { name: "Smaller code" }).click();
    await view.getByRole("button", { name: "Wrap" }).click();
    await expect(view.locator(".sd-ln > span").first()).toHaveCSS("white-space", "pre-wrap");
    await view.getByRole("button", { name: "Wrap" }).click();
    await view.getByRole("button", { name: "Full window" }).click();
    await expect(view).toHaveCSS("position", "fixed");
    await page.keyboard.press("Escape");
    await expect(view).not.toHaveCSS("position", "fixed");

    // a project of many files has a tree
    await page.goto("/cookbook/p/cb-compiler");
    const files = page.locator(".cb-ftree button");
    await expect.poll(() => files.count()).toBeGreaterThan(20);
    await files.nth(5).click();
    await expect(page.locator(".cb-ftree button.on")).toHaveCount(1);
});

test("Reading: the open book, the chapter rail, contents, panels, shortcuts, search and a draft sample", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/cookbook/p/join-algorithms");
    await expect(page.locator(".cb-head h1")).toHaveText("Join algorithms");
    await expect(page.locator(".cb-where")).toContainText(/page \d+ of \d+/);
    await expect(page.locator(".cb-book-pages a.on")).toHaveText("Join algorithms");
    await expect(page.locator(".cb-rail a.on")).toHaveText("Join algorithms");
    await page.getByLabel("Find a page in this subject").fill("parallel hash");
    await expect(page.locator(".cb-found a").first()).toContainText(/hash/i);
    await page.getByLabel("Find a page in this subject").fill("");
    await page.getByLabel("Find a page in this subject").blur();
    await expect(page.locator(".cb-toc button.on")).toHaveText(/Overview|Key ideas/);
    await page.keyboard.press("j");
    await expect(page.locator(".cb-toc button.on")).not.toHaveText("Overview");
    await page.keyboard.press("[");
    await expect(page.locator(".cb-shelf")).toBeHidden();
    await page.reload();
    await expect(page.locator(".cb-head h1")).toHaveText("Join algorithms");
    await expect(page.locator(".cb-shelf")).toBeHidden();
    await page.keyboard.press("[");
    await expect(page.locator(".cb-shelf")).toBeVisible();

    await page.keyboard.press("ControlOrMeta+k");
    await page.keyboard.type("borrow until you must own");
    await page.keyboard.press("Enter");
    await expect(page.locator(".cb-draft")).toBeVisible();
    const open = page.getByRole("button", { name: /Open all \d+ sections/ });
    const before = await page.locator(".cb-sec .sd-md").count();
    await open.click();
    expect(await page.locator(".cb-sec .sd-md").count()).toBeGreaterThan(before);
    await expect(page.locator(".sd-code-fail").first()).toContainText("does not compile");
    await expect(page.locator(".cb-lab .cb-view")).toBeVisible();

    await page.goto("/cookbook/p/cow-borrow-until-you-must-own#cb-pitfalls");
    await expect(page.locator("#cb-pitfalls .sd-md").first()).toBeVisible();
    await page.goto("/cookbook/p/not-a-page");
    await expect(page.getByRole("alert")).toContainText("no cookbook page");
    expect(errors).toEqual([]);
});

test("Every cookbook page renders with its code and no leftover page syntax", async ({ page, request }) => {
    test.setTimeout(1_800_000);
    const index = await (await request.get("/api/cookbook")).json();
    const rows: { id: string; lines: number }[] = [...index.subjects.flatMap((s: { pages: unknown[] }) => s.pages), ...index.samples];
    expect(rows.length).toBeGreaterThan(690);
    const errors: string[] = [];
    let at = "";
    page.on("pageerror", (e) => errors.push(`${at}: ${e.message}`));
    await page.setViewportSize({ width: 1440, height: 900 });
    const bad: string[] = [];
    for (const row of rows) {
        at = row.id;
        await page.goto(`/cookbook/p/${row.id}`);
        await page.locator(".cb-head h1").waitFor();
        const open = page.getByRole("button", { name: /Open all \d+ sections/ });
        if (await open.count()) await open.click();
        const found = await page.evaluate(() => {
            const art = document.querySelector(".cb-art")!.cloneNode(true) as HTMLElement;
            art.querySelectorAll("pre, code, .sd-code, .cb-lab").forEach((x) => x.remove());
            const text = art.textContent ?? "";
            return {
                syntax: /```|:::|\[!(NOTE|TIP|WARNING|ASIDE|RUST|TESTING|PRODUCTION)\]|data-call|<\/?(p|div|aside|ul|li|table)\b|@@TEX|&#\d+;|&lt;|&amp;/.exec(text)?.[0] ?? "",
                wide: document.documentElement.scrollWidth - window.innerWidth,
                code: !!document.querySelector(".cb-view .sd-ln"),
            };
        });
        if (found.syntax || found.wide > 0 || (row.lines > 0 && !found.code)) bad.push(`${row.id}: ${found.syntax} ${found.wide > 0 ? `page ${found.wide}px too wide` : ""} ${row.lines > 0 && !found.code ? "no code shown" : ""}`);
    }
    expect(bad).toEqual([]);
    expect(errors).toEqual([]);
});

test("On a wide screen text and panels are as wide as the code", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1000 });
    for (const id of ["concurrent-memory", "mvcc-snapshot-isolation", "join-algorithms"]) {
        await page.goto(`/cookbook/p/${id}`);
        await page.locator(".cb-art .sd-md").first().waitFor();
        const gap = await page.evaluate(() => {
            const widest = (sel: string) => Math.max(0, ...[...document.querySelectorAll(sel)].map((e) => e.getBoundingClientRect().width));
            const full = widest(".cb-art .cb-head");
            return [".cb-art .sd-md", ".cb-art .sd-code, .cb-art .cb-view", ".cb-art .sd-call"].map((sel) => (widest(sel) ? Math.round(full - widest(sel)) : 0));
        });
        expect(Math.max(...gap), `${id}: text, code and notes short of the column by ${gap}`).toBeLessThanOrEqual(2);
    }
});

test("Cookbook pages fit a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    for (const path of ["/cookbook", "/cookbook/data", "/cookbook/rust", "/cookbook/p/join-algorithms", "/cookbook/p/testing-unsafe-code", "/cookbook/p/mvcc-snapshot-isolation", "/cookbook/p/cb-compiler"]) {
        await page.goto(path);
        await expect(page.locator(".cb-page, .cb-read").first()).toBeVisible();
        await page.waitForLoadState("networkidle");
        expect(await noSideScroll(page), path).toBeLessThanOrEqual(0);
    }
    await page.goto("/cookbook/p/join-algorithms");
    await page.getByRole("button", { name: "Pages", exact: true }).click();
    await expect(page.locator(".cb-shelf")).toBeVisible();
});

test("Cookbook pages fit a laptop and a tablet", async ({ page }) => {
    for (const width of [1280, 1024, 768]) {
        await page.setViewportSize({ width, height: 800 });
        for (const path of ["/cookbook", "/cookbook/compilers", "/cookbook/p/cb-compiler", "/cookbook/p/join-algorithms"]) {
            await page.goto(path);
            await page.waitForLoadState("networkidle");
            expect(await noSideScroll(page), `${path} at ${width}`).toBeLessThanOrEqual(0);
        }
    }
});

test("screenshots for review", async ({ page }) => {
    const dir = process.env.CB_SHOTS;
    test.skip(!dir, "set CB_SHOTS to a folder to take them");
    const shots = [["home", "/cookbook"], ["subject", "/cookbook/data"], ["code", "/cookbook/p/mvcc-snapshot-isolation"], ["project", "/cookbook/p/cb-compiler"], ["guide", "/cookbook/p/join-algorithms"], ["sample", "/cookbook/p/cow-borrow-until-you-must-own"]];
    for (const scheme of ["dark", "light"] as const) {
        await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
        await page.setViewportSize({ width: 1500, height: 950 });
        for (const [name, path] of shots) {
            await page.goto(path!);
            await page.waitForLoadState("networkidle");
            await page.screenshot({ path: `${dir}/${scheme}-${name}.png` });
        }
        await page.goto("/cookbook");
        await page.waitForLoadState("networkidle");
        await page.screenshot({ path: `${dir}/${scheme}-home-full.png`, fullPage: true });
        await page.setViewportSize({ width: 390, height: 844 });
        for (const [name, path] of [["home", "/cookbook"], ["code", "/cookbook/p/mvcc-snapshot-isolation"]]) {
            await page.goto(path!);
            await page.waitForLoadState("networkidle");
            await page.screenshot({ path: `${dir}/${scheme}-phone-${name}.png` });
        }
    }
});
