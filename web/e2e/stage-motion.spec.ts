import { expect, test } from "@playwright/test";

const stage = "/courses/bustub/4a-04";

// A failing run of the stage, so the run strip has one to name; the tests no longer rely on another file having made it.
test.beforeAll(async ({ request }) => {
    const tests = ["a_passes", "b_fails", "c_passes"].map((name) => ({ name, ok: name !== "b_fails", detail: name === "b_fails" ? "assertion failed" : "" }));
    await request.post("/api/courses/bustub/runs", { data: { stage_id: "4a-04", tests, commit: "5e2d0c1a", duration_ms: 1200 } });
});

test.beforeEach(async ({ page }) => {
    await page.goto(stage);
    await expect(page.locator(".k-tabs")).toBeVisible();
});

test("a reading bar across the top grows with the scroll; the condensed header and back-to-top appear", async ({ page }) => {
    await expect(page.locator(".k-phead")).not.toHaveClass(/k-show/);
    await expect(page.locator(".k-totop")).not.toHaveClass(/k-show/);
    await page.evaluate(() => window.scrollTo(0, 700));
    await expect(page.locator(".k-phead")).toHaveClass(/k-show/);
    await expect(page.locator(".k-totop")).toHaveClass(/k-show/);
    await expect.poll(() => page.locator(".k-read").evaluate((e) => parseFloat((e as HTMLElement).style.width))).toBeGreaterThan(0);
    await expect(page.locator(".k-phead .k-pct")).toContainText("% read");
    await page.locator(".k-totop").click();
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBeLessThan(5);
});

test("the tab underline sits under the active tab and slides to the next", async ({ page }) => {
    const ul = page.locator(".k-tabs .k-ul");
    const under = async (name: RegExp) => {
        const tab = (await page.getByRole("tab", { name }).boundingBox())!;
        const line = (await ul.boundingBox())!;
        return Math.abs(line.x - tab.x) < 2 && Math.abs(line.width - tab.width) < 2;
    };
    await expect.poll(() => under(/^Instructions/)).toBe(true);
    await page.getByRole("tab", { name: /^Hints/ }).click();
    await expect.poll(() => under(/^Hints/)).toBe(true);
});

test("arrow keys move between the tabs", async ({ page }) => {
    await page.getByRole("tab", { name: /^Instructions/ }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("tab", { name: /^Hints/ })).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("End");
    await expect(page.getByRole("tab", { name: /^Last run/ })).toHaveAttribute("aria-selected", "true");
});

test("hints open in order: a later one is locked and says so", async ({ page }) => {
    await page.getByRole("tab", { name: /^Hints/ }).click();
    const h3 = page.locator("#h3");
    await expect(h3).toContainText("open after hint 2");
    await h3.locator(".k-hh").click();
    await expect(page.locator(".k-tt", { hasText: "Open the previous hint first" })).toBeVisible();
});

test("the solution is blurred behind a gate until it is revealed", async ({ page }) => {
    await page.getByRole("tab", { name: /^Solution/ }).click();
    const sol = page.locator(".k-sol").first();
    await expect(sol).not.toHaveClass(/k-rev/);
    expect(await sol.locator("pre").evaluate((e) => getComputedStyle(e).filter)).toContain("blur");
    await expect(sol.locator(".k-gate")).toBeVisible();
});

test("an opened solution is syntax highlighted, with its diff lines marked", async ({ page }) => {
    await page.route("**/api/courses/bustub/stages/4a-04", async (route) => {
        const res = await route.fetch();
        const body = await res.json();
        const lines = [" fn get(&self) -> u32 {", "-    todo!()", "+    // one way\n".trim(), "+    let n: u32 = 7; n", " }"];
        body.solution = { available: true, open: true, files: [{ path: "src/x.rs", lines }] };
        await route.fulfill({ response: res, json: body });
    });
    await page.reload();
    await expect(page.locator(".k-tabs")).toBeVisible();
    await page.getByRole("tab", { name: /^Solution/ }).click();
    const sol = page.locator(".k-sol").first();
    await expect(sol.locator(".k-add")).toHaveCount(2);
    await expect(sol.locator(".k-del")).toHaveCount(1);
    await expect(sol.locator(".t-k", { hasText: "fn" }).first()).toBeVisible();
    await expect(sol.locator(".t-c", { hasText: "one way" })).toBeVisible();
    await expect(sol.locator(".t-t", { hasText: "u32" }).first()).toBeVisible();
    const kw = await sol.locator(".t-k").first().evaluate((e) => getComputedStyle(e).color);
    const plain = await sol.locator("pre").evaluate((e) => getComputedStyle(e).color);
    expect(kw).not.toBe(plain);
});

test("the run strip names the last run and its logs slide open", async ({ page }) => {
    await expect(page.locator("#stx")).toContainText("passing");
    await expect(page.locator("#strip")).not.toHaveClass(/k-open/);
    await page.locator("#logb").click();
    await expect(page.locator("#strip")).toHaveClass(/k-open/);
    await expect(page.locator("#logb")).toHaveText("HIDE LOGS");
});

test("Copy test command copies it and says the page is listening", async ({ page, context }) => {
    await expect(page.locator("#runbtn")).toContainText("Copy test command");
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.locator("#runbtn").click();
    await expect(page.locator(".k-tt.k-ok")).toContainText("Command copied");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("anneal course test 4a-04");
});

test("the run strip says it is listening, shows a run in flight, then the result", async ({ page, request }) => {
    const stage = "4c-08";
    const bar = page.locator("#strip");
    await page.goto(`/courses/bustub/${stage}`);
    await expect(bar.locator("#stx")).toContainText("listening for your next run");
    // the CLI starts a run: within a poll the strip says so and the button is busy
    expect((await request.post("/api/courses/bustub/runs/start", { data: { stage_id: stage } })).ok()).toBe(true);
    await expect(bar.locator("#stx")).toContainText("running tests", { timeout: 10_000 });
    await expect(bar.locator("#runbtn")).toContainText("Running…");
    // the report arrives: the strip shows the result and the running state is gone
    const run = { stage_id: stage, tests: [{ name: "a", ok: true, detail: "" }, { name: "b", ok: false, detail: "left 1 right 2" }], commit: "abc1234", duration_ms: 300 };
    expect((await request.post("/api/courses/bustub/runs", { data: run })).ok()).toBe(true);
    await expect(bar.locator("#stx")).toContainText("1 / 2 passing", { timeout: 10_000 });
    await expect(bar.locator("#runbtn")).toContainText("Copy test command");
});

test("the outline marker follows the section you are in", async ({ page }) => {
    await page.locator("#toc a", { hasText: "Tests" }).click();
    await expect(page.locator("#toc a.k-on")).toHaveText("Tests");
});

test("the pages are centred and have no grid behind them", async ({ page }) => {
    // the DSA home is in the full-width three-panel frame now, centred the same way
    for (const path of ["/rust", "/courses", "/dsa"]) {
        await page.goto(path);
        const wrap = page.locator(".s-wrap, .k-wrap, .ly-page").first();
        await expect(wrap).toBeVisible();
        const box = (await wrap.boundingBox())!;
        expect(Math.abs(box.x + box.width / 2 - 720)).toBeLessThan(8);
        expect(await page.evaluate(() => (() => { const m = document.querySelector("main, .page"); return m ? getComputedStyle(m).backgroundImage : "none"; })())).toBe("none");
    }
});
