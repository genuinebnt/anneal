import { expect, test } from "@playwright/test";

test("the mock pool can be only what is done, or that plus the same patterns", async ({ page }) => {
    await page.request.post("/api/dsa/problems/lc-valid-sudoku/log", { data: { grade: "good" } });
    try {
        const o = await (await page.request.get("/api/dsa")).json();
        type P = { id: string; pattern: string; premium: boolean; lists: string[]; state: { solved: boolean } };
        const base = (o.problems as P[]).filter((p) => !p.premium && p.lists.includes("neetcode150"));
        const solved = base.filter((p) => p.state.solved);
        const patterns = new Set((o.problems as P[]).filter((p) => p.state.solved).map((p) => p.pattern));
        const similar = base.filter((p) => p.state.solved || patterns.has(p.pattern));
        expect(solved.length).toBeGreaterThan(0);
        expect(similar.length).toBeGreaterThan(solved.length);

        await page.goto("/dsa/mock");
        const stat = page.locator(".s-stats", { hasText: "IN THE POOL" }).locator("div", { hasText: "IN THE POOL" }).first().locator("b");
        const which = page.getByRole("radiogroup", { name: "Which problems" }).or(page.getByRole("group", { name: "Which problems" }));
        await which.getByRole("radio", { name: "Problems I've done" }).or(which.getByRole("button", { name: "Problems I've done" })).first().click();
        await expect(stat).toHaveText(String(solved.length));
        await which.getByRole("radio", { name: "Done + similar patterns" }).or(which.getByRole("button", { name: "Done + similar patterns" })).first().click();
        await expect(stat).toHaveText(String(similar.length));
        await page.screenshot({ path: "test-results/mock-reach.png" });
    } finally {
        await page.goto("/dsa/mock");
        const which = page.getByRole("radiogroup", { name: "Which problems" }).or(page.getByRole("group", { name: "Which problems" }));
        await which.getByRole("radio", { name: "Everything in the lists" }).or(which.getByRole("button", { name: "Everything in the lists" })).first().click();
        await page.request.delete("/api/dsa/problems/lc-valid-sudoku/log");
    }
});
