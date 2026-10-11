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
