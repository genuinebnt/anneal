import { expect, test } from "@playwright/test";

test("only a pattern with content has an Idea tab, and the API says which", async ({ page, request }) => {
    const o = await (await request.get("/api/dsa")).json();
    const withIdea = (o.patterns as { name: string; has_idea: boolean }[]).filter((p) => p.has_idea).map((p) => p.name);
    expect(withIdea).toContain("Sliding Window");
    await page.goto("/dsa/patterns/D3");
    await expect(page.getByRole("tab", { name: /^Idea/ })).toBeVisible();
    await page.goto("/dsa/patterns/D1");
    await expect(page.getByRole("tab", { name: /^Learn/ })).toBeVisible();
    await expect(page.getByRole("tab", { name: /^Idea/ })).toHaveCount(0);
    // the old tabs keep their keys
    await page.goto("/dsa/patterns/D3");
    await expect(page.getByRole("tab", { name: /^Idea/ })).toBeVisible();
    await page.keyboard.press("i");
    await expect(page).toHaveURL(/\/dsa\/patterns\/D3\/idea$/);
    await page.keyboard.press("2");
    await expect(page).toHaveURL(/\/dsa\/patterns\/D3\/problems$/);
    await page.keyboard.press("1");
    await expect(page).toHaveURL(/\/dsa\/patterns\/D3$/);
});

test("the Idea tab has every section, with time and space", async ({ page }) => {
    await page.goto("/dsa/patterns/D3/idea");
    for (const h of ["WHAT IT IS", "WATCH IT RUN", "WHERE IT APPLIES", "IS IT SLIDING WINDOW?", "WHICH SLIDING WINDOW?", "TIME AND SPACE", "WITH OTHER PATTERNS", "HOW TO APPLY IT"]) {
        await expect(page.getByRole("heading", { name: h })).toBeVisible();
    }
    const cost = page.locator("table.id-cost tbody tr");
    expect(await cost.count()).toBeGreaterThanOrEqual(5);
    await expect(cost.first().locator(".id-cx.id-bad")).toContainText("O(n·k)");
    await expect(cost.nth(1).locator(".id-cx.id-good")).toContainText("O(1)");
    // every cost row says why
    for (const t of await cost.locator("td:last-child").allInnerTexts()) expect(t.length).toBeGreaterThan(40);
});

test("the demo steps, lights the running lines, highlights the Python, and takes your own input", async ({ page }) => {
    await page.goto("/dsa/patterns/D3/idea");
    const say = page.locator(".id-say");
    const first = await say.innerText();
    await expect(page.locator(".id-code .py-k").first()).toBeVisible();
    await expect(page.locator(".id-code .id-ln.id-on").first()).toBeVisible();
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(say).not.toHaveText(first);
    await expect(page.locator(".id-cell.id-enter")).toHaveCount(1);
    await expect(page.locator(".id-cell.id-leave")).toHaveCount(1);
    // the counters: sliding adds less than recomputing
    const [brute, slide] = await page.locator(".id-work .id-wr em").allInnerTexts();
    expect(Number(slide)).toBeLessThan(Number(brute));
    // run to the end by the ticks: the answer is shown
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("ANSWER");
    // own input: a bad one is flagged and ignored, a good one restarts
    const box = page.getByLabel("Numbers, separated by commas");
    await box.fill("1,2");
    await page.getByLabel("Window size k").fill("5");
    await page.getByLabel("Window size k").blur();
    await expect(box).toHaveClass(/id-bad/);
    await box.fill("5,4,3,2,1,9,8");
    await page.getByLabel("Window size k").fill("3");
    await page.getByLabel("Window size k").blur();
    await expect(box).not.toHaveClass(/id-bad/);
    await expect(page.locator(".id-cell")).toHaveCount(7);
    // the other demo
    await page.getByRole("radio", { name: /Variable/ }).click();
    await expect(page.locator(".id-chips")).toBeVisible();
    await page.getByRole("button", { name: "Step 11" }).click();
    await expect(page.locator(".id-cell.id-dup, .id-cell.id-leave").first()).toBeVisible();
    // keys, once the demo has focus
    await page.getByRole("button", { name: "Restart" }).click();
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".id-ctl .id-n")).toContainText("step 2 of");
});

test("a row of 'which one' opens its technique card, and a 'with' row opens the other pattern or its example", async ({ page }) => {
    await page.goto("/dsa/patterns/D3/idea");
    await page.getByRole("link", { name: "See the technique →" }).first().click();
    await expect(page).toHaveURL(/\/dsa\/patterns\/D3#t-fixed-sum$/);
    await expect(page.locator("#t-fixed-sum")).toBeVisible();
    await page.goto("/dsa/patterns/D3/idea");
    const w = page.locator(".id-w", { hasText: "Stack" }).first();
    await w.locator(".id-ex").click();
    await expect(page).toHaveURL(/\/d\/sliding-window-maximum$/);
    await page.goto("/dsa/patterns/D3/idea");
    await page.locator(".id-w .id-who", { hasText: "Two Pointers" }).click();
    await expect(page).toHaveURL(/\/dsa\/patterns\/D2\/idea$/);
});

test("the Idea tab does not scroll sideways on a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    await page.goto("/dsa/patterns/D3/idea");
    await expect(page.getByRole("heading", { name: "TIME AND SPACE" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
});

test("every pattern that has an Idea tab shows every section, steps its animations to an answer, and fits a phone", async ({ page, request }) => {
    test.setTimeout(30_000 + 40_000 * 20); // a pass over every pattern that has one
    const o = await (await request.get("/api/dsa")).json();
    const withIdea = (o.patterns as { code: string; name: string; has_idea: boolean }[]).filter((p) => p.has_idea);
    expect(withIdea.length).toBeGreaterThan(1);
    for (const p of withIdea) {
        await page.setViewportSize({ width: 1440, height: 900 });
        await page.goto(`/dsa/patterns/${p.code}/idea`);
        for (const h of ["WHAT IT IS", "WATCH IT RUN", "WHERE IT APPLIES", `IS IT ${p.name.toUpperCase()}?`, `WHICH ${p.name.toUpperCase()}?`, "TIME AND SPACE", "HOW TO APPLY IT"]) {
            await expect(page.getByRole("heading", { name: h }), `${p.name}: ${h}`).toBeVisible();
        }
        // each animation runs to its end and shows an answer
        const demos = page.getByRole("radiogroup", { name: "Demo" }).getByRole("radio");
        const n = (await demos.count()) || 1;
        for (let d = 0; d < n; d++) {
            if ((await demos.count()) > 0) await demos.nth(d).click();
            await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
            await expect(page.locator(".id-res"), `${p.name} demo ${d + 1}`).toContainText("ANSWER");
        }
        expect(await page.locator("table.id-cost tbody tr").count(), `${p.name} costs`).toBeGreaterThanOrEqual(4);
        await page.setViewportSize({ width: 390, height: 800 });
        await expect(page.getByRole("heading", { name: "TIME AND SPACE" })).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth), `${p.name} on a phone`).toBeLessThanOrEqual(0);
    }
});

test("Two Pointers: labelled pointers close in on a pair, and the reader and writer keep what is new", async ({ page }) => {
    await page.goto("/dsa/patterns/D2/idea");
    await expect(page.locator(".id-ptr span").filter({ hasText: "lo" })).toBeVisible();
    await expect(page.locator(".id-ptr span").filter({ hasText: "hi" })).toBeVisible();
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(page.locator(".id-cell.id-cur")).toHaveCount(2);
    await page.getByRole("button", { name: "Next →" }).click();
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(page.locator(".id-cell.id-gone")).toHaveCount(2);
    await expect(page.locator(".id-cell.id-hit")).toHaveCount(2);
    await expect(page.locator(".id-res")).toContainText("3 + 11");
    // the savings bars: far fewer pairs than trying every one
    const [all, used] = await page.locator(".id-work .id-wr em").allInnerTexts();
    expect(parseInt(used!, 10)).toBeLessThan(parseInt(all!, 10));
    // a target with no pair ends with none, and a typed row is sorted for you
    await page.getByLabel("Target", { exact: true }).fill("100");
    await page.getByLabel("Target", { exact: true }).blur();
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("none");
    // the other animation
    await page.getByRole("radio", { name: /Same direction/ }).click();
    await expect(page.locator(".id-ptr span").filter({ hasText: "slow" })).toBeVisible();
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("4");
    // a row of "which one" opens its technique
    await page.getByRole("link", { name: "See the technique →" }).first().click();
    await expect(page).toHaveURL(/\/dsa\/patterns\/D2#t-opposite$/);
    await expect(page.locator("#t-opposite")).toHaveClass(/open/);
});

test("Binary Search: the middle is looked at, half the row falls away, and a first True is found with few questions", async ({ page }) => {
    await page.goto("/dsa/patterns/D5/idea");
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(page.locator(".id-ptr span").filter({ hasText: "mid" })).toBeVisible();
    await page.getByRole("button", { name: "Next →" }).click();
    expect(await page.locator(".id-cell.id-gone").count()).toBeGreaterThanOrEqual(4);
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("index 5");
    // a target that is not there
    await page.getByLabel("Target", { exact: true }).fill("24");
    await page.getByLabel("Target", { exact: true }).blur();
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("not found");
    // first True: most cells are never asked, so they still show a question mark
    await page.getByRole("radio", { name: /First True/ }).click();
    await expect(page.getByLabel("Falses then trues, such as FFFFTT")).toHaveValue("FFFFFFTTTT");
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("index 6");
    expect(await page.locator(".id-cell", { hasText: "?" }).count()).toBeGreaterThanOrEqual(5);
    await page.getByLabel("Falses then trues, such as FFFFTT").fill("FTFT");
    await page.getByLabel("Falses then trues, such as FFFFTT").blur();
    await expect(page.getByLabel("Falses then trues, such as FFFFTT")).toHaveClass(/id-bad/);
    // a row of "which one" opens its technique card
    await page.getByRole("link", { name: "See the technique →" }).nth(3).click();
    await expect(page).toHaveURL(/\/dsa\/patterns\/D5#t-on-answer$/);
    await expect(page.locator("#t-on-answer")).toHaveClass(/open/);
});

test("Stack: brackets wait on the stack and match, a crossing is invalid, and the monotonic stack answers next greater", async ({ page }) => {
    await page.goto("/dsa/patterns/D4/idea");
    await expect(page.locator(".id-stack")).toBeVisible();
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(page.locator(".id-stack .id-cell")).toHaveCount(1);
    await page.getByRole("button", { name: "Next →" }).click();
    await expect(page.locator(".id-stack .id-cell")).toHaveCount(2);
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("valid");
    await expect(page.locator(".id-stack .id-cell")).toHaveCount(0);
    // a crossing
    await page.getByLabel("Brackets, such as ([]{})").fill("(]");
    await page.getByLabel("Brackets, such as ([]{})").blur();
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("invalid");
    await expect(page.locator(".id-cell.id-nope")).toHaveCount(2);
    // the monotonic stack
    await page.getByRole("radio", { name: /Monotonic stack/ }).click();
    await page.getByRole("button", { name: /^Step \d+$/ }).last().click();
    await expect(page.locator(".id-res")).toContainText("5 5 6 6 -1 -1");
    await page.getByRole("link", { name: "See the technique →" }).nth(2).click();
    await expect(page).toHaveURL(/\/dsa\/patterns\/D4#t-mono-stack$/);
    await expect(page.locator("#t-mono-stack")).toHaveClass(/open/);
});

test("each Idea tab opens with a loop and its sentence; the loop moves, and stands still under reduced motion", async ({ page, request, browser }) => {
    const o = await (await request.get("/api/dsa")).json();
    for (const p of (o.patterns as { code: string; name: string; has_idea: boolean }[]).filter((x) => x.has_idea)) {
        const d = await (await request.get(`/api/dsa/patterns/${p.code}`)).json();
        expect(d.idea.tagline.length, `${p.name} tagline`).toBeGreaterThan(20);
        await page.goto(`/dsa/patterns/${p.code}/idea`);
        const hero = page.locator(".id-hero");
        await expect(hero).toContainText(d.idea.tagline);
        await expect(hero.locator("svg.lp")).toBeVisible();
    }
    // Sliding Window's brace is animated; with reduced motion it is not
    await page.goto("/dsa/patterns/D3/idea");
    const running = await page.locator(".id-hero .lp-sw").evaluate((e) => getComputedStyle(e).animationName);
    expect(running).toBe("lp-slide");
    const calm = await browser.newContext({ reducedMotion: "reduce" });
    const q = await calm.newPage();
    await q.goto("/dsa/patterns/D3/idea");
    expect(await q.locator(".id-hero .lp-sw").evaluate((e) => getComputedStyle(e).animationName)).toBe("none");
    await calm.close();
});
