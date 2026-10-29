import { expect, test } from "@playwright/test";

test("Settings lists what is new", async ({ page }) => {
    await page.goto("/settings#new");
    await expect(page.getByText("WHAT'S NEW")).toBeVisible();
    await expect(page.getByText("Review: pick any review from the list")).toBeVisible();
});

test("the phone quick log finds a problem by name and logs it", async ({ page }) => {
    await page.goto("/log");
    try {
        await page.getByLabel("Find a problem").fill("valid sudoku");
        const row = page.locator(".lg-row", { hasText: "Valid Sudoku" });
        await row.getByRole("button", { name: "On my own" }).click();
        await expect(page.locator(".k-tstack")).toContainText("Valid Sudoku");
        await expect.poll(async () => (await (await page.request.get("/api/dsa")).json()).problems.find((p: { id: string }) => p.id === "lc-valid-sudoku").state.solved).toBe(true);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});

test("a premium problem points to free problems with the same idea", async ({ page }) => {
    const o = await (await page.request.get("/api/dsa")).json();
    const prem = o.problems.find((p: { premium: boolean }) => p.premium);
    await page.goto(`/d/${prem.slug}`);
    await expect(page.getByText("FREE INSTEAD")).toBeVisible();
    await expect(page.locator(".pp-card", { hasText: "FREE INSTEAD" }).locator("a")).not.toHaveCount(0);
});

test("already solved before anneal: check, mark, and take the batch back", async ({ page }) => {
    const o = await (await page.request.get("/api/dsa")).json();
    const p = o.problems.find((x: { state: { last_grade: string | null }; premium: boolean }) => !x.state.last_grade && !x.premium);
    await page.goto("/settings");
    await page.getByLabel("LeetCode problem numbers").fill(`${p.number}, 99999999`);
    await page.getByRole("button", { name: /^Check/ }).click();
    const note = page.locator("section[aria-labelledby='st-bulk'] .st-note");
    await expect(note).toContainText("would be marked");
    await expect(note).toContainText("Not in anneal");
    await page.getByRole("button", { name: /^Mark 1 as solved/ }).click();
    await expect(page.getByText(/Marked 1 problem as solved on 2020-01-01/)).toBeVisible();
    await expect.poll(async () => (await (await page.request.get("/api/dsa")).json()).problems.find((x: { id: string }) => x.id === p.id).state.solved).toBe(true);
    await page.getByRole("button", { name: /^Undo this batch/ }).click();
    await expect(page.getByText(/Took back 1/)).toBeVisible();
    await expect.poll(async () => (await (await page.request.get("/api/dsa")).json()).problems.find((x: { id: string }) => x.id === p.id).state.solved).toBe(false);
});

test("the calendar link can be made and replaced", async ({ page }) => {
    await page.goto("/settings");
    const input = page.getByLabel("Calendar link");
    await expect(page.getByRole("button", { name: /Make a link|New link/ })).toBeVisible();
    const before = (await input.count()) ? await input.inputValue() : "";
    await page.getByRole("button", { name: /Make a link|New link/ }).click();
    if (before) await expect(input).not.toHaveValue(before);
    else await expect(input).toBeVisible();
    const first = await input.inputValue();
    expect(first).toMatch(/\/api\/feed\/.+\.ics$/);
    expect((await page.request.get(new URL(first).pathname)).status()).toBe(200);
    await page.getByRole("button", { name: "New link" }).click();
    await expect(input).not.toHaveValue(first);
    expect((await page.request.get(new URL(first).pathname)).status()).toBe(404);
});

test("coming back from LeetCode asks how it went, and never logs by itself", async ({ page, context }) => {
    await context.route("https://leetcode.com/**", (route) => route.fulfill({ status: 200, contentType: "text/html", body: "<title>LeetCode</title>" }));
    await page.goto("/d/valid-sudoku");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        const popup = page.waitForEvent("popup");
        await page.locator(".pp-solve .pp-act.pri").click();
        await (await popup).close();
        await page.waitForTimeout(5500);
        await page.evaluate(() => window.dispatchEvent(new Event("focus")));
        const bar = page.getByRole("region", { name: "Back from LeetCode" });
        await expect(bar).toContainText("Done with Valid Sudoku?");
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await bar.getByRole("button", { name: "Not now" }).click();
        await expect(bar).toHaveCount(0);
    } finally {
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});
