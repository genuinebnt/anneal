import { expect, test } from "@playwright/test";

const tests = (names: string[], bad: string[]) => names.map((name) => ({ name, ok: !bad.includes(name), detail: bad.includes(name) ? "assertion failed\n left: 4\nright: 3" : "" }));

test.beforeAll(async ({ request }) => {
    // Failing runs only: nothing becomes solved.
    await request.post("/api/courses/bustub/runs", {
        data: {
            stage_id: "4a-04",
            tests: tests(["a_passes", "b_fails", "c_passes", "d_passes", "e_fails", "f_passes"], ["b_fails", "e_fails"]),
            commit: "4f1c9ab1",
            duration_ms: 2400,
        },
    });
    await request.post("/api/courses/bustub/runs", {
        data: { stage_id: "4a-03", problem: "error[E0308]: mismatched types\n  --> src/x.rs:1:1\n", commit: "c70aa13f", duration_ms: 900 },
    });
});

test("the Run tab lists failures first and open, with a summary and a segment per test", async ({ page }) => {
    await page.goto("/courses/bustub/4a-04#run");
    await expect(page.locator(".k-rsm")).toContainText("2 failed");
    await expect(page.locator(".k-rsm")).toContainText("4 passed");
    await expect(page.locator(".k-rsb i")).toHaveCount(6);
    await expect(page.locator(".k-rsb i.k-f")).toHaveCount(2);
    await expect(page.locator(".k-rt.k-bad")).toHaveCount(2);
    await expect(page.locator(".k-rt.k-bad").first()).toContainText("b_fails");
    await expect(page.locator(".k-rt.k-bad").first()).toContainText("same message as 1 other test");
    // the failures come before the passes in the page
    const fail = await page.locator(".k-rt.k-bad").first().boundingBox();
    const pass = await page.locator(".k-rpass").boundingBox();
    expect(fail!.y).toBeLessThan(pass!.y);
});

test("passing tests are folded until you open them", async ({ page }) => {
    await page.goto("/courses/bustub/4a-04#run");
    await expect(page.locator(".k-rpass")).not.toHaveClass(/k-open/);
    await page.locator(".k-rph").click();
    await expect(page.locator(".k-rpass")).toHaveClass(/k-open/);
    await expect(page.locator(".k-rrow")).toHaveCount(4);
    await expect(page.locator(".k-rpl > div")).not.toHaveAttribute("inert", "");
    await page.locator(".k-rph").click();
    await expect(page.locator(".k-rpass")).not.toHaveClass(/k-open/);
    await expect(page.locator(".k-rpl > div")).toHaveAttribute("inert", "");
});

test("the run-again command is copyable", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/courses/bustub/4a-04#run");
    await expect(page.locator(".k-rcmd code")).toHaveText("anneal course test 4a-04 --only -f b_fails");
    await page.locator(".k-rcmd button").click();
    await expect(page.locator(".k-rcmd button")).toHaveText("COPIED");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("anneal course test 4a-04 --only -f b_fails");
});

test("a run that did not compile shows the compiler output and no segments", async ({ page }) => {
    await page.goto("/courses/bustub/4a-03#run");
    await expect(page.locator(".k-rsm")).toContainText("Did not run");
    await expect(page.locator(".k-rcomp")).toContainText("error[E0308]");
    await expect(page.locator(".k-rsb")).toHaveCount(0);
    await expect(page.locator(".k-rcmd")).toContainText("compile locally");
});

test("the stage header has the test command with a copy button", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/courses/bustub/4a-04");
    await expect(page.locator(".k-cli code")).toHaveText("anneal course test 4a-04");
    await page.locator(".k-cli button").click();
    await expect(page.locator(".k-cli button")).toHaveText("COPIED");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("anneal course test 4a-04");
});

test("each tab remembers its scroll position", async ({ page }) => {
    await page.goto("/courses/bustub/4a-04");
    await expect(page.locator(".k-tabs")).toBeVisible();
    // far enough to matter, but with the tab bar still on screen so that clicking a tab does not scroll the page itself
    await page.evaluate(() => window.scrollTo(0, 300));
    await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBeGreaterThan(250);
    const before = await page.evaluate(() => Math.round(window.scrollY));
    // click in the page: Playwright's own click scrolls the target into view first, which would move the page
    const clickTab = (name: RegExp) => page.getByRole("tab", { name }).evaluate((el) => (el as HTMLElement).click());
    await clickTab(/^Hints/);
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeLessThan(50);
    await clickTab(/^Instructions/);
    await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBeGreaterThan(before - 40);
});

test("the course page's continue card shows the module ring", async ({ page }) => {
    await page.goto("/courses");
    await expect(page.locator(".k-cont .k-ring")).toBeVisible();
    await expect(page.locator(".k-cont .k-grow small")).toContainText("·");
});

test("a test's code opens under its result, for failed and for passed tests", async ({ page, request }) => {
    const stage = "3d-02";
    const st = await (await request.get(`/api/courses/bustub/stages/${stage}`)).json();
    const names = Object.keys(st.stage.test_sources);
    expect(names.length).toBeGreaterThan(4);
    const [bad, good] = [names[0], names[1]];
    await request.post("/api/courses/bustub/runs", {
        data: { stage_id: stage, tests: [{ name: bad, ok: false, detail: "assertion failed" }, { name: good, ok: true, detail: "" }], commit: "abc1234", duration_ms: 900 },
    });
    await page.goto(`/courses/bustub/${stage}#run`);
    const failedRow = page.locator(".k-rt.k-bad", { hasText: bad });
    await expect(failedRow).toBeVisible();
    // a failed test shows its code under the message at once; a click on the row hides it and shows it again
    await expect(failedRow.getByTestId("test-code")).toContainText(`fn ${bad}(`);
    await failedRow.locator(".k-rth").click();
    await expect(failedRow.getByTestId("test-code")).toHaveCount(0);
    await failedRow.getByRole("button", { name: "show test code" }).click();
    await expect(failedRow.getByTestId("test-code")).toContainText(`fn ${bad}(`);
    // passed tests are folded under a summary row: open it, then a test's code
    await page.locator(".k-rph").click();
    const row = page.locator(".k-rwrap", { hasText: good });
    await expect(row.getByTestId("test-code")).toHaveCount(0);
    await row.locator(".k-rrow").click();
    await expect(row.getByTestId("test-code")).toContainText(`fn ${good}(`);
    await row.locator(".k-rrow").click();
    await expect(row.getByTestId("test-code")).toHaveCount(0);
});

test("a run stopped for taking too long says time limit exceeded, and keeps the tests that finished", async ({ page, request }) => {
    const stage = "3d-02";
    const st = await (await request.get(`/api/courses/bustub/stages/${stage}`)).json();
    const names = Object.keys(st.stage.test_sources);
    const [hung, fine] = [names[0], names[1]];
    const detail = "Time limit exceeded: the run was stopped after 180 s and this test had not finished (an endless loop? a latch that is never released?)";
    await request.post("/api/courses/bustub/runs", {
        data: { stage_id: stage, tests: [{ name: hung, ok: false, detail }, { name: fine, ok: true, detail: "" }], commit: "abc1234", duration_ms: 180_000 },
    });
    await page.goto(`/courses/bustub/${stage}#run`);
    // the summary says so, not "did not run" or a bare count, and the test that finished is still listed
    await expect(page.getByText(/time limit exceeded/i).first()).toBeVisible();
    await expect(page.getByText("did not run")).toHaveCount(0);
    await expect(page.locator(".k-rt.k-bad", { hasText: hung })).toBeVisible();
    await expect(page.locator("#logt")).toContainText("Time limit exceeded");
    await page.locator(".k-rph").click();
    await expect(page.locator(".k-rwrap", { hasText: fine })).toBeVisible();
});

for (const width of [320, 390, 760, 900]) {
    test(`at ${width} the stage's test bar sits above the bottom bar, whose links all stay clickable`, async ({ page }) => {
        await page.setViewportSize({ width, height: 844 });
        await page.goto("/courses/bustub/4a-04");
        const dock = page.locator(".hd-dock");
        await expect(dock).toBeVisible();
        await expect(page.locator(".k-strip")).toBeVisible();
        await page.evaluate(() => document.fonts.ready);
        const links = dock.getByRole("link");
        await expect(links).toHaveCount(6);
        await expect.poll(() => links.evaluateAll((all) => all.filter((e) => {
            const b = e.getBoundingClientRect();
            return !e.contains(document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2));
        }).length), { message: "covered bottom links" }).toBe(0);
        const strip = (await page.locator(".k-strip").boundingBox())!;
        const bar = (await dock.boundingBox())!;
        expect(strip.y + strip.height).toBeLessThanOrEqual(bar.y + 0.5);
        // the end of the page clears both bars
        await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
        const top = page.getByRole("button", { name: "Back to top" });
        await expect.poll(async () => { const t = (await top.boundingBox())!; return t.y + t.height - (await page.locator(".k-strip").boundingBox())!.y; }, { message: "back to top sits on the test bar" }).toBeLessThanOrEqual(0);
        const end = (await page.locator(".k-read2 > *").last().boundingBox())!;
        expect(end.y + end.height).toBeLessThanOrEqual((await page.locator(".k-strip").boundingBox())!.y + 0.5);
        // the drawer of stages ends above them too
        await page.locator(".k-mbar button").first().click();
        await expect.poll(async () => { const t = (await page.locator(".k-tree").boundingBox())!; return t.y + t.height; }).toBeLessThanOrEqual(strip.y + 0.5);
    });
}

// A phone on its side, and other windows with little height: the drawers must still hold a list one can use.
for (const [width, height] of [[844, 390], [667, 375], [740, 360], [900, 400], [1000, 420]] as const) {
    test(`at ${width}x${height} the stage drawers keep room for their lists, and their buttons can be pressed`, async ({ page }) => {
        await page.setViewportSize({ width, height });
        await page.goto("/courses/bustub/4a-04");
        const hit = (sel: string) => page.locator(sel).first().evaluate((e) => {
            const b = e.getBoundingClientRect();
            return b.height > 0 && b.bottom <= innerHeight + 0.5 && e.contains(document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2));
        });
        await page.locator(".k-mbar button", { hasText: "Stages" }).click();
        await expect.poll(() => page.locator(".k-tree").evaluate((e) => e.getBoundingClientRect().x)).toBe(0);
        expect((await page.locator(".k-tree .k-tlist").boundingBox())!.height).toBeGreaterThanOrEqual(120);
        for (const sel of [".k-tree .k-drawer-x", ".k-tree .k-tfoot .k-cta", ".k-tree .k-tsrch input"]) await expect.poll(() => hit(sel), { message: sel }).toBe(true);
        await page.locator(".k-tree .k-drawer-x").click();
        await expect(page.locator(".k-tree")).toBeHidden();
        await page.locator(".k-mbar button", { hasText: "Page & notes" }).click();
        await expect.poll(() => page.locator(".k-toc").evaluate((e) => Math.round(e.getBoundingClientRect().right) - innerWidth)).toBe(0);
        expect((await page.locator(".k-toc .k-tscroll").boundingBox())!.height).toBeGreaterThanOrEqual(200);
    });
}

for (const width of [320, 360, 390]) test(`at ${width} the sticky stage bar's labels and Copy button are all whole`, async ({ page }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.goto("/courses/bustub/r-02");
    await expect(page.locator(".k-ttl")).toBeVisible();
    await page.evaluate(() => window.scrollTo(0, 1400));
    const button = page.locator(".k-phead .k-cta");
    await expect(button).toBeVisible();
    const m = await button.evaluate((e) => ({ over: e.scrollWidth - e.clientWidth, right: e.getBoundingClientRect().right - innerWidth, button: getComputedStyle(e).color, label: getComputedStyle(e.querySelector(".k-lbl")!).color }));
    expect(m.over).toBeLessThanOrEqual(0);
    expect(m.right).toBeLessThanOrEqual(0);
    expect(m.label).toBe(m.button);
    // the labels beside it are whole too: nothing cut off, nothing under the button
    await expect.poll(() => page.locator(".k-phead").evaluate((bar) => {
        const cta = bar.querySelector(".k-cta")!.getBoundingClientRect();
        return [...bar.querySelectorAll<HTMLElement>(":scope>span:not(.k-sp)")].filter((e) => {
            const r = e.getBoundingClientRect();
            return e.scrollWidth > e.clientWidth + 1 || r.right > cta.left + 0.5 || r.bottom > bar.getBoundingClientRect().bottom + 0.5 || r.left < 0;
        }).map((e) => e.textContent);
    })).toEqual([]);
});

/** Course text a reader sees outside code that still carries Markdown. */
const rawCourseMarks = (page: import("@playwright/test").Page) =>
    page.evaluate(() => {
        const out: string[] = [];
        const walk = document.createTreeWalker(document.querySelector(".k-root")!, NodeFilter.SHOW_TEXT);
        for (let n = walk.nextNode(); n; n = walk.nextNode()) {
            if (n.parentElement!.closest("pre, code, textarea, svg, .cm-editor, .k-cb")) continue;
            const t = n.textContent ?? "";
            if (/`|\*\*[^*]+\*\*|(^|\s)\*[A-Za-z][^*\n]{1,60}[A-Za-z]\*(\s|[.,;:]|$)/i.test(t)) out.push(`${n.parentElement!.tagName}.${n.parentElement!.className}: ${t.slice(0, 80)}`);
        }
        for (const e of document.querySelectorAll(".k-root [title], .k-root [aria-label]")) for (const a of ["title", "aria-label"]) if ((e.getAttribute(a) ?? "").includes("`")) out.push(`@${a}: ${e.getAttribute(a)}`);
        return out;
    });

// one page for each place that showed the marks: headings, the outline, ledes, "you'll learn", note cards, callout titles, captions and table cells
for (const path of ["1b-01", "1g-c2", "3d-02", "3e-03", "concept/bytes-endianness-and-views", "concept/name-resolution-and-the-binder", "concept/shifts-masks-and-bit-tricks", "concept/reader-writer-latches", "concept/positional-io-and-short-reads", "concept/property-testing-and-fuzzing", "concept/durability-and-fsync", "concept/option-and-result-combinators", "concept/top-n-with-a-bounded-heap"]) {
    test(`course page ${path} shows no Markdown marks`, async ({ page }) => {
        await page.goto(`/courses/bustub/${path}`);
        await expect(page.locator(".k-ttl")).toBeVisible();
        await page.waitForLoadState("networkidle");
        await expect.poll(() => rawCourseMarks(page)).toEqual([]);
    });
}

test("code with a pipe in a table cell is whole", async ({ page }) => {
    await page.goto("/courses/bustub/3e-03");
    await expect(page.locator(".k-vt td", { hasText: ".iter().map(|e| e.evaluate(..)).collect::<Result<Vec<_>>>()?" })).toHaveCount(1);
    await page.goto("/courses/bustub/concept/option-and-result-combinators");
    await expect(page.locator(".k-vt td", { hasText: "opt.ok_or_else(|| err)" })).toHaveCount(1);
});
