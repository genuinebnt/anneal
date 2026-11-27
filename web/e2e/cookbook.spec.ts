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

test("On a wide screen notes keep the reading column, and code and tables are never narrower than it", async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1000 });
    for (const id of ["concurrent-memory", "mvcc-snapshot-isolation", "join-algorithms"]) {
        await page.goto(`/cookbook/p/${id}`);
        await page.locator(".cb-art .sd-md").first().waitFor();
        const found = await page.evaluate(() => {
            const widths = (sel: string) => [...document.querySelectorAll(sel)].map((e) => Math.round(e.getBoundingClientRect().width)).filter((w) => w > 0);
            return {
                column: Math.max(0, ...widths(".cb-sec > .sd-md > p")),
                // the page's own flow only: a note or a paragraph inside a card is the card's business
                reading: widths(".cb-sec > .sd-call, .cb-sec > .sd-md > .sd-call, .cb-sec > .sd-md > p"),
                code: widths(".cb-sec > .sd-code, .cb-sec > .sd-md > .sd-tbl, .cb-art > .cb-lab > .cb-view"),
            };
        });
        expect(found.reading.filter((w) => Math.abs(w - found.column) > 2), `${id}: reading blocks off the column of ${found.column}`).toEqual([]);
        expect(found.code.filter((w) => w < found.column), `${id}: code narrower than the column`).toEqual([]);
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

/** Text a reader sees outside code that still carries Markdown: a backtick or a doubled asterisk. */
const rawMarkup = (page: Page) =>
    page.evaluate(() => {
        const out: string[] = [];
        const walk = document.createTreeWalker(document.querySelector("#root") ?? document.body, NodeFilter.SHOW_TEXT);
        for (let n = walk.nextNode(); n; n = walk.nextNode()) {
            const el = n.parentElement!;
            if (el.closest("pre, code, textarea, .cm-editor, script, style")) continue;
            const t = n.textContent ?? "";
            if (/`|\*\*/.test(t)) out.push(`${el.className || el.tagName}: ${t.slice(0, 80)}`);
        }
        for (const el of document.querySelectorAll("[title], [aria-label], [placeholder]"))
            for (const a of ["title", "aria-label", "placeholder"]) if (/`/.test(el.getAttribute(a) ?? "") && !el.closest("pre, .cm-editor")) out.push(`@${a}: ${el.getAttribute(a)!.slice(0, 80)}`);
        return out;
    });

test("a step-through shows its code as code, and a step without narration keeps no pipe", async ({ page }) => {
    await page.goto("/cookbook/p/mini-kv-store");
    // a long draft page keeps its later sections folded
    await page.evaluate(() => document.querySelectorAll("details").forEach((d) => (d.open = true)));
    const flush = page.locator(".sd-steps-fig", { hasText: "A flush" });
    await expect(flush).toBeVisible();
    await expect(flush.locator(".sd-steps-row button", { hasText: "cleared" }).locator("span")).toHaveText("cleared");
    await expect(flush.locator(".sd-steps-row code", { hasText: "set_len(0)" })).toBeVisible();
    // no step anywhere on the page ends in a pipe
    for (const t of await page.locator(".sd-steps-row button").allTextContents()) expect(t).not.toMatch(/\|\s*$/);
    expect(await rawMarkup(page)).toEqual([]);
});

test("page titles and descriptions show inline code, not backticks, wherever they are listed", async ({ page }) => {
    for (const path of ["/cookbook", "/cookbook/rust", "/cookbook/p/cow", "/cookbook/p/drop-raii", "/cookbook/p/mini-kv-store"]) {
        await page.goto(path);
        await expect(page.locator("h1").first()).toBeVisible();
        await expect.poll(() => rawMarkup(page), { message: path }).toEqual([]);
    }
    await page.goto("/cookbook/rust");
    await expect(page.locator(".cb-row-t code", { hasText: "Cow" }).first()).toHaveText("Cow");
    // search results too
    await page.goto("/cookbook");
    await page.getByLabel("Search the cookbook").fill("RAII");
    await expect(page.locator(".cb-rows li").first()).toBeVisible();
    expect(await rawMarkup(page)).toEqual([]);
});

// The HTML the guide pages keep (styles/cookbook-guide.css). What is checked holds with any font.
async function guide(page: Page, id: string, width: number) {
    await page.setViewportSize({ width, height: width < 500 ? 844 : 950 });
    await page.goto(`/cookbook/p/${id}`);
    await expect(page.locator(".cb-head h1")).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
}

const boxes = (page: Page, selector: string) => page.locator(selector).evaluateAll((all) => all.map((e) => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, bottom: r.bottom, width: r.width }; }));

for (const width of [2200, 1440]) {
    test(`The pipeline diagram is a row of stages with each test under its stage, ${width} wide`, async ({ page }) => {
        await guide(page, "the-pipeline-and-where-tests-attach", width);
        const stages = await boxes(page, ".cb-g-node");
        expect(new Set(stages.map((s) => Math.round(s.y))).size).toBe(1);
        const tests = await boxes(page, ".cb-g-probe");
        tests.forEach((t, i) => {
            expect(Math.abs(t.x - stages[i + 1]!.x)).toBeLessThan(2);
            expect(t.y).toBeGreaterThanOrEqual(stages[i + 1]!.bottom);
        });
        expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
        await expect(page.locator(".cb-art")).not.toContainText("Its exercises are left out");
    });
}

test("On a phone the pipeline diagram reads from top to bottom, each stage followed by its tests", async ({ page }) => {
    await guide(page, "the-pipeline-and-where-tests-attach", 390);
    const [stages, tests] = [await boxes(page, ".cb-g-node"), await boxes(page, ".cb-g-probe")];
    stages.slice(1).forEach((s, i) => expect(s.y).toBeGreaterThanOrEqual(stages[i]!.bottom));
    tests.forEach((t, i) => {
        expect(t.y).toBeGreaterThanOrEqual(stages[i + 1]!.bottom);
        if (stages[i + 2]) expect(t.bottom).toBeLessThanOrEqual(stages[i + 2]!.y);
    });
    for (const s of stages) expect(s.width).toBeGreaterThan(200);
    expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
});

for (const theme of ["dark", "light"] as const) {
    test(`A drawing takes the ${theme} theme's colours, and on a phone it scrolls at a readable size with a fade and no bar`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: theme });
        await guide(page, "the-dbms-stack", 390);
        const paint = await page.locator(".cb-g-fig svg").evaluate((svg) => {
            const of = (s: string) => getComputedStyle(svg.querySelector(s)!).fill;
            return { box: of("rect.cb-g-box"), words: of("text:not([class])") };
        });
        // unstyled shapes and words are painted black
        expect(paint.box).not.toBe("rgb(0, 0, 0)");
        expect(paint.words).not.toBe("rgb(0, 0, 0)");
        expect(paint.words).not.toBe(paint.box);
        expect((await page.locator(".cb-g-fig svg").boundingBox())!.width).toBeGreaterThanOrEqual(800);
        const scroller = page.locator(".cb-g-fig-scroll");
        await expect(scroller).toHaveAttribute("data-more", "");
        await expect(scroller).toHaveAttribute("tabindex", "0");
        expect(await scroller.evaluate((e) => [getComputedStyle(e).maskImage !== "none", (e as HTMLElement).offsetHeight - e.clientHeight])).toEqual([true, 0]);
        await scroller.evaluate((e) => (e.scrollLeft = e.scrollWidth));
        await expect(scroller).not.toHaveAttribute("data-more", "");
        // the caption stays in place while the drawing scrolls
        const [caption, figure] = [(await page.locator(".cb-g-figcap").boundingBox())!, (await page.locator(".cb-g-fig").boundingBox())!];
        expect(caption.x).toBeGreaterThanOrEqual(figure.x);
        expect(caption.x + caption.width).toBeLessThanOrEqual(figure.x + figure.width + 1);
        expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
    });
}

test("A table the page wrote keeps its own box, and no guide block pushes a phone sideways", async ({ page }) => {
    for (const id of ["arithmetic", "dp-trees-and-specialized-algorithms", "tooling-around-a-new-language", "backends-hir-mir-llvm-and-cranelift", "reading-map-from-front-end-to-back-end", "the-slice-loop", "glossary-compilers", "how-to-use-this-guide", "recoverable-parsing-and-advanced-compiler-theory"]) {
        await guide(page, id, 390);
        const stray = await page.locator(".cb-art table:is(.cb-g-cmp, .cb-g-map)").evaluateAll((all) => all.filter((t) => !t.parentElement!.matches(".cb-g-tt")).length);
        expect(stray, id).toBe(0);
        // one box around a table, never a box inside a box
        await expect(page.locator(".cb-art .cb-g-tt .sd-tbl"), id).toHaveCount(0);
        // a box that was closed too early leaves what follows it outside the section's text
        expect(await page.locator(".cb-sec > :not(h2, .sd-md, .sd-code, .sd-deeper, .sd-math, .sd-steps-fig, .cb-methods)").count(), id).toBe(0);
        expect(await noSideScroll(page), id).toBeLessThanOrEqual(0);
    }
});
