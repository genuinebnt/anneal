import { expect, test, type Page } from "@playwright/test";

const clean = async (page: Page, ...ids: string[]) => {
    for (const id of ids) await page.request.delete(`/api/dsa/problems/${id}/log`);
};

test("the DSA home shows the Next up card and its button without scrolling", async ({ page }) => {
    await page.goto("/dsa");
    await expect(page.locator(".s-next h3")).toBeInViewport();
    await expect(page.locator(".s-next .dn-solve")).toBeInViewport();
});

test("the header says what is left of today", async ({ page }) => {
    await page.goto("/dsa");
    await expect(page.locator(".hd-today")).toContainText("today");
});

test("a link to my own notes is kept per problem and follows to the server", async ({ page }) => {
    await page.goto("/d/valid-sudoku");
    await page.getByRole("button", { name: "+ Link to my notes" }).click();
    await page.getByLabel("Link to my notes").fill("not a link");
    await expect(page.getByText("Start with http:// or https://")).toBeVisible();
    await page.getByLabel("Link to my notes").fill("https://example.com/my-sudoku-notes");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByRole("link", { name: /Open my notes/ })).toHaveAttribute("href", "https://example.com/my-sudoku-notes");
    await expect.poll(async () => (await (await page.request.get("/api/prefs")).json())["notes:lc-valid-sudoku"]).toBe("https://example.com/my-sudoku-notes");
    await page.reload();
    await expect(page.getByRole("link", { name: /Open my notes/ })).toBeVisible();
});

test("after Attempted the toast asks why, and the answer is kept with the attempt", async ({ page }) => {
    await page.goto("/d/valid-sudoku");
    try {
        await expect(page.locator(".pp-status")).toContainText("Not started");
        await page.keyboard.press("3");
        await page.locator(".d-why").getByRole("button", { name: "edge cases" }).click();
        await expect(page.locator(".k-tstack")).toContainText("noted, edge cases");
        const file = await (await page.request.get("/api/export")).json();
        expect(file.tables.attempts.some((a: { problem_id: string; why: string | null }) => a.problem_id === "lc-valid-sudoku" && a.why === "edge_cases")).toBe(true);
    } finally {
        await clean(page, "lc-valid-sudoku");
    }
});

test("a day on the heatmap can be clicked to see what was solved", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "good" } });
    try {
        await page.goto("/progress");
        await page.locator("svg rect.d[style*='pointer']").last().click();
        await expect(page.locator(".heat-day")).toContainText("Valid Sudoku");
    } finally {
        await clean(page, "lc-valid-sudoku");
    }
});

test("the search box logs a problem: 'log <name>' offers the four outcomes", async ({ page }) => {
    await page.goto("/dsa");
    try {
        await page.keyboard.press("Control+k");
        await page.locator(".hd-pal input").fill("log valid sudoku");
        await page.getByRole("option", { name: /Log Valid Sudoku: on my own/ }).click();
        await expect(page.getByText("Valid Sudoku: on my own")).toBeVisible();
        await expect.poll(async () => (await (await page.request.get("/api/dsa")).json()).problems.find((p: { id: string }) => p.id === "lc-valid-sudoku").state.solved).toBe(true);
    } finally {
        await clean(page, "lc-valid-sudoku");
    }
});

test("weak spots: reviews lapsed twice are offered with a filter", async ({ page }) => {
    for (let i = 0; i < 2; i++) await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "again" } });
    try {
        await page.goto("/dsa/review");
        await expect(page.locator(".rv-ahead").getByRole("button", { name: /Valid Sudoku/ })).toHaveCount(1);
        await page.getByLabel(/Weak spots only/).check();
        await expect(page.locator(".rv-ahead").getByRole("button", { name: /Valid Sudoku/ })).toHaveCount(1);
    } finally {
        await clean(page, "lc-valid-sudoku");
    }
});

test.describe("on a phone", () => {
    test.use({ viewport: { width: 390, height: 800 } });
    for (const path of ["/dsa", "/d/two-sum", "/dsa/review", "/dsa/mock", "/dsa/calendar", "/dsa/plan", "/progress", "/settings", "/rust", "/courses"]) {
        test(`${path} has no horizontal scroll`, async ({ page }) => {
            await page.goto(path);
            await page.waitForTimeout(900);
            const over = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
            expect(over, `${path} is ${over}px wider than the screen`).toBeLessThanOrEqual(1);
        });
    }
});
