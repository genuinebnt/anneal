import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

const names = ["n0", "n1", "n2", "n3", "n4", "n5"];
const run = (request: APIRequestContext, stage: string, status: ("ok" | "bad")[], commit: string) =>
    request.post("/api/courses/bustub/runs", { data: { stage_id: stage, tests: status.map((s, i) => ({ name: names[i], ok: s === "ok", detail: s === "ok" ? "" : "assertion failed" })), commit, duration_ms: 1200 } });

test.describe("Last run", () => {
    const stage = "4a-05";
    test.beforeAll(async ({ request }) => {
        await request.post("/api/courses/bustub/runs", { data: { stage_id: stage, problem: "error[E0308]: mismatched types\n  --> src/x.rs:1:1\n", commit: "1111111", duration_ms: 500 } });
        await run(request, stage, ["ok", "bad", "bad", "ok", "bad", "bad"], "2222222"); // 2 passed
        await run(request, stage, ["ok", "bad", "ok", "ok", "bad", "ok"], "3333333"); // 4 passed: n2 and n5 fixed
        await run(request, stage, ["ok", "bad", "ok", "bad", "bad", "ok"], "4444444"); // 3 passed: n3 newly failing
    });
    const open = async (page: Page) => {
        await page.goto(`/courses/bustub/${stage}#run`);
        await expect(page.locator(".k-rsm")).toBeVisible();
    };

    test("the history shows the recent runs, the newest selected", async ({ page }) => {
        await open(page);
        expect(await page.locator(".k-rn").count()).toBeGreaterThanOrEqual(4);
        await expect(page.locator(".k-rn").last()).toHaveAttribute("aria-pressed", "true");
        await expect(page.locator(".k-rsm")).toContainText("3 failed");
    });

    test("the newest run says how it differs from the one before", async ({ page }) => {
        await open(page);
        await expect(page.locator(".k-dlt")).toContainText("1 fewer passing");
        await expect(page.locator(".k-dlt")).toContainText("1 newly failing");
    });

    test("picking an older run shows that run and its own comparison", async ({ page }) => {
        await open(page);
        const bars = page.locator(".k-rn");
        await bars.nth((await bars.count()) - 2).click(); // the run with 4 passed
        await expect(page.locator(".k-rsm")).toContainText("2 failed");
        await expect(page.locator(".k-rsm")).toContainText("4 passed");
        await expect(page.locator(".k-dlt")).toContainText("2 more passing");
    });

    test("the Failed and Passed filters show one side each", async ({ page }) => {
        await open(page);
        await page.locator(".k-rtool .k-grp").getByRole("button", { name: "Failed" }).click();
        await expect(page.locator(".k-rt.k-bad")).toHaveCount(3);
        await expect(page.locator(".k-rpass")).toHaveCount(0);
        await page.locator(".k-rtool .k-grp").getByRole("button", { name: "Passed" }).click();
        await expect(page.locator(".k-rt.k-bad")).toHaveCount(0);
        await expect(page.locator(".k-rrow")).toHaveCount(3);
    });

    test("compare marks what is newly failing, still failing and fixed", async ({ page }) => {
        await open(page);
        await page.getByRole("switch", { name: "Compare with the previous run" }).click();
        await expect(page.locator(".k-rt.k-bad", { hasText: "n3" }).locator(".k-cm2")).toHaveText("NEWLY FAILING");
        await expect(page.locator(".k-rt.k-bad", { hasText: "n1" }).locator(".k-cm2")).toHaveText("STILL FAILING");
        // the run before: its two fixed tests
        const bars = page.locator(".k-rn");
        await bars.nth((await bars.count()) - 2).click();
        // the compare switch stays on when another run is picked
        await expect(page.getByRole("switch", { name: "Compare with the previous run" })).toHaveAttribute("aria-checked", "true");
        await page.locator(".k-rph").click();
        await expect(page.locator(".k-rrow .k-cm2.k-fix")).toHaveCount(2);
    });

    test("a run with a compile error is shown as one that did not run", async ({ page }) => {
        await open(page);
        const bars = page.locator(".k-rn");
        const n = await bars.count();
        // the compile-error run is the oldest of the four seeded: three bars before the newest
        await bars.nth(n - 4).click();
        await expect(page.locator(".k-rsm")).toContainText("Did not run");
        await expect(page.locator(".k-rcomp")).toContainText("error[E0308]");
    });

    test("a run that arrives while the tab is open becomes the selected one", async ({ page, request }) => {
        await open(page);
        const before = await page.locator(".k-rn").count();
        await run(request, stage, ["ok", "ok", "bad", "ok", "ok", "ok"], "5555555");
        await expect(page.locator(".k-rsm")).toContainText("1 failed", { timeout: 15_000 });
        expect(await page.locator(".k-rn").count()).toBeGreaterThanOrEqual(Math.min(before + 1, 10));
    });
});

test.describe("Concepts", () => {
    // a stage marks at most one concept to read first and the rest optional; the counting tests skip themselves when it marks none
    const stage = "1a-01";
    let required = 0;
    let total = 0;
    // what the header says once `n` required concepts are read
    const progress = (n: number) => (n === required ? "Required reading done" : `${n} of ${required} done`);
    // Read state lives on the server: start each test from "nothing read".
    test.beforeEach(async ({ page, request }) => {
        const st = await (await request.get(`/api/courses/bustub/stages/${stage}`)).json();
        required = st.concepts.filter((k: { required: boolean }) => k.required).length;
        total = st.concepts.length;
        for (const k of st.concepts) await request.put(`/api/courses/bustub/concepts/${k.id}/read`, { data: { read: false } });
        await page.goto(`/courses/bustub/${stage}#concepts`);
        await expect(page.locator(".k-chdr")).toBeVisible();
    });

    test("the header counts the required reading and each concept has a card", async ({ page }) => {
        test.skip(required === 0, "this stage has no required concepts");
        await expect(page.locator(".k-chdr")).toContainText(`Required reading: 0 of ${required} done`);
        await expect(page.locator(".k-ccard")).toHaveCount(total);
        await expect(page.locator(".k-badge2.k-req")).toHaveCount(required);
        await expect(page.getByRole("tab", { name: /^Concepts/ })).toContainText(`0/${required}`);
    });

    test("marking one as read updates the header, the tab and the page panel, and it is kept", async ({ page }) => {
        test.skip(required === 0, "this stage has no required concepts");
        const card = page.locator(".k-ccard", { has: page.locator(".k-badge2.k-req") }).first();
        await card.getByRole("button", { name: /Preview/ }).click();
        await card.getByRole("switch").click();
        await expect(page.locator(".k-chdr")).toContainText(progress(1));
        await expect(page.getByRole("tab", { name: /^Concepts/ })).toContainText(`1/${required}`);
        await expect(page.locator(".k-toc .k-cc2:not(.k-todo)")).toHaveCount(1);
        await expect(page.locator(".k-ccard.k-isread")).toHaveCount(1);
        await page.reload();
        await expect(page.locator(".k-chdr")).toContainText(progress(1));
        // and it can be undone
        await card.getByRole("button", { name: /Preview/ }).click();
        await card.getByRole("switch").click();
        await expect(page.locator(".k-chdr")).toContainText(progress(0));
    });

    test("Preview opens a card and Read opens the article", async ({ page }) => {
        const first = page.locator(".k-ccard").first();
        const btn = first.getByRole("button", { name: /Preview|Close/ });
        await expect(btn).toHaveAttribute("aria-expanded", "false");
        await btn.click();
        await expect(btn).toHaveAttribute("aria-expanded", "true");
        await first.getByRole("link", { name: /Read/ }).click();
        await expect(page).toHaveURL(/\/courses\/bustub\/concept\//);
    });

    test("marking all the required ones read says so", async ({ page }) => {
        test.skip(required === 0, "this stage has no required concepts");
        for (let i = 0; i < required; i++) {
            const c = page.locator(".k-ccard", { has: page.locator(".k-badge2.k-req") }).nth(i);
            await c.getByRole("button", { name: /Preview/ }).click();
            await c.getByRole("switch").click();
        }
        await expect(page.locator(".k-chdr")).toContainText("Required reading done");
    });
});
