// The three series screens (docs/SERIES.md): the series page and its entry, a part in the workspace, and the completion view.
// Uses content/series/web-server.toml. One test submits a reference solution, so use a database you don't mind adding to.
import { expect, test } from "@playwright/test";
import { PART, SOLUTION, solvePart } from "./seriesSolve";

test("the Rust home leads to the series without a new header tab", async ({ page }) => {
    await page.goto("/rust");
    const entry = page.getByRole("region", { name: "Series" }).getByRole("link", { name: /From threads to a web server/ });
    await expect(entry).toContainText(/of \d+ parts solved/);
    await expect(page.locator(".hd-tabs a", { hasText: /series/i })).toHaveCount(0);
    await entry.click();
    await expect(page).toHaveURL(/\/rust\/series\/web-server$/);
    await expect(page.getByRole("heading", { level: 1, name: "From threads to a web server" })).toBeVisible();
});

test("the series page shows the path, the capstone and its closed solution", async ({ page, request }) => {
    const s = await (await request.get("/api/series/web-server")).json();
    await page.goto("/rust/series/web-server");
    await expect(page.locator(".sx-part")).toHaveCount(s.segments.length);
    await expect(page.locator(".sx-part .sx-state", { hasText: "To write" })).toHaveCount(s.to_write);
    // a part still to be written is not a link
    await expect(page.locator(".sx-part.is-to_write a")).toHaveCount(0);
    await expect(page.locator(".sx-cap ol li")).toHaveCount(s.capstone.steps.length);
    const solution = page.locator(".sx-cap details");
    await expect(solution).not.toHaveAttribute("open", "");
    await expect(solution.getByRole("link")).toBeHidden();
    if (s.next) {
        await page.locator(".sx-next .sx-btn").click();
        await expect(page).toHaveURL(new RegExp(`/p/${s.next.id}$`));
    }
});

test("a part keeps every workspace control and its helpful links start closed", async ({ page, request }) => {
    const place = await (await request.get("/api/problems/c2-thread-pool/series")).json();
    await page.goto("/p/c2-thread-pool");
    const crumb = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(crumb).toContainText(`Part ${place.number} of ${place.total}`);
    await expect(crumb.getByRole("link", { name: place.series.title })).toBeVisible();
    await expect(page.locator(".clock")).toContainText(`/ ${place.minutes}:00`);
    const links = page.getByRole("region", { name: "Connections" });
    await expect(links).toBeVisible();
    if (!place.done) {
        await expect(links.locator("details")).not.toHaveAttribute("open", "");
        await expect(links.getByRole("link", { name: /Cookbook:/ }).first()).toBeHidden();
        await links.getByText("Show helpful ideas").click();
        await expect(links.getByRole("link", { name: /Cookbook:/ }).first()).toBeVisible();
        await expect(page.getByRole("button", { name: "Solved", exact: true })).toHaveCount(0);
    }
    for (const name of ["Problem", /^Hints/, "Solution", "Related"]) await expect(page.getByRole("tab", { name, exact: typeof name === "string" }).first()).toBeVisible();
    for (const name of [/src\/lib\.rs/, /main\.rs/, /tests\.rs/, /Run tests/, "Reset", "Submit"]) await expect(page.getByRole("button", { name }).first()).toBeVisible();
    await expect(page.getByRole("group", { name: "Layout" }).getByRole("button")).toHaveCount(3);
    await expect(page.locator(".pane.r .ft")).toBeVisible();
    await expect(page.locator(".ws > .pane")).toHaveCount(3);
});

test("nothing that names the method shows on an unsolved part", async ({ page, request }) => {
    const id = "c2-thread-pool";
    const p = await (await request.get(`/api/problems/${id}`)).json();
    test.skip(p.attempt.solved, "needs a database where this part is unsolved");
    await page.goto(`/p/${id}`);
    await expect(page.getByRole("region", { name: "Connections" })).toBeVisible();
    await expect(page.getByText("WHAT THIS TEACHES")).toHaveCount(0);
    await expect(page.getByText("INTERVIEW FOLLOW-UP")).toHaveCount(0);
    await expect(page.locator(".pill", { hasText: p.tags[0] })).toHaveCount(0);
    await page.getByRole("button", { name: /Show tags/ }).click();
    await expect(page.locator(".sx-tags .pill")).toHaveCount(p.tags.length);
});

test("on a phone a part keeps its breadcrumb", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/p/c2-thread-pool");
    const link = page.getByRole("navigation", { name: "Breadcrumb" }).getByRole("link", { name: "From threads to a web server" });
    await expect(link).toBeVisible();
    expect((await link.boundingBox())!.width).toBeGreaterThan(150);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
});

test("a problem outside the series has no series parts, and shows its title once", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    // the problem pane carries the title; the bar shows it only when that pane is hidden
    await expect(page.locator(".pane.l .ptitle")).toBeVisible();
    await expect(page.locator(".subbar .wtitle")).toBeHidden();
    await page.getByRole("button", { name: /Hide the problem panel/ }).click();
    await expect(page.locator(".subbar .wtitle")).toBeVisible();
    await page.getByRole("button", { name: /Show the problem panel/ }).click();
    await expect(page.getByRole("region", { name: "Connections" })).toHaveCount(0);
});

test("a passing submit opens the completion view as a tab beside the code", async ({ page, request }) => {
    test.setTimeout(240_000);
    const put = await request.put(`/api/problems/${PART}/draft`, { data: { code: SOLUTION() } });
    expect(put.ok()).toBeTruthy();
    await page.goto(`/p/${PART}`);
    await expect(page.getByRole("region", { name: "Connections" })).toBeVisible();
    await page.getByRole("button", { name: "Submit", exact: true }).click();
    const done = page.getByRole("region", { name: "Solved" });
    await expect(done).toBeVisible({ timeout: 180_000 });
    await expect(done).toContainText(/Part \d+ of \d+ is done/);
    await expect(done.getByRole("heading", { name: "What you practised" })).toBeVisible();
    await expect(done.getByRole("heading", { name: "Where it returns" })).toBeVisible();
    await expect(done.getByRole("heading", { name: "Repeat later" })).toBeVisible();
    await expect(done.getByRole("link", { name: "Back to the series" })).toBeVisible();
    // once solved, what the part teaches is shown
    await expect(page.getByText("WHAT THIS TEACHES")).toBeVisible();
    await expect(done.locator("textarea, input")).toHaveCount(0);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    // the code is one tab away, and the view comes back
    await page.getByRole("button", { name: /src\/lib\.rs/ }).click();
    await expect(done).toHaveCount(0);
    await expect(page.locator(".edit-host:not([hidden]) .cm-editor")).toBeVisible();
    await page.getByRole("button", { name: "Solved", exact: true }).click();
    await expect(done).toBeVisible();
    await done.getByRole("link", { name: "Back to the series" }).click();
    await expect(page).toHaveURL(/\/rust\/series\/web-server$/);
});

test("on a phone the completion view is in the Code pane and nothing is wider than the screen", async ({ page, request }) => {
    test.setTimeout(240_000);
    await solvePart(request);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/p/${PART}`);
    await expect(page.locator(".pane.l .sx-meta")).toContainText(/Part \d+ of \d+/);
    await page.getByRole("tab", { name: "Code" }).click();
    await page.getByRole("button", { name: "Solved", exact: true }).click();
    await expect(page.getByRole("region", { name: "Solved" })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
});
