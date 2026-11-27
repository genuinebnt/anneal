// The three series screens (docs/SERIES.md): the series page and its entry, a part in the workspace, and the completion view.
// Uses content/series/web-server.toml. One test submits a reference solution, so use a database you don't mind adding to.
import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { PART, SOLUTION, solvePart } from "./seriesSolve";

test("the Rust home leads to the series, and Rust has a Series tab", async ({ page }) => {
    await page.goto("/rust");
    const entry = page.getByRole("region", { name: "Series" }).getByRole("link", { name: /From threads to a web server/ });
    await expect(entry).toContainText(/of \d+ parts solved/);
    await expect(page.locator(".hd-tabs a", { hasText: /series/i })).toHaveCount(1);
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
        await expect(page).toHaveURL(new RegExp(`/p/${s.next.id}\\?series=web-server$`));
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
    await page.goto(`/p/${PART}?series=web-server`);
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

test("the Series tab under Rust lists every series and opens one", async ({ page }) => {
    await page.goto("/rust");
    await page.locator(".hd-tabs").getByRole("link", { name: "Series", exact: true }).click();
    await expect(page).toHaveURL(/\/rust\/series$/);
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Series");
    await expect(page.getByRole("heading", { level: 1, name: "Series" })).toBeVisible();
    await page.getByRole("region", { name: "All series" }).getByRole("link").first().click();
    await expect(page).toHaveURL(/\/rust\/series\/.+/);
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Series");
});

test("the pager of a series problem follows the series: its count, its order, both ends and the capstone", async ({ page, request }) => {
    test.setTimeout(120_000);
    const s = await (await request.get("/api/series/web-server")).json();
    type Item = { kind: "part" | "step"; number: number; id: string | null };
    const parts: Item[] = s.groups.flatMap((g: { rows: (Item & { row: string })[] }) => g.rows.filter((r) => r.row === "item"));
    parts.sort((a, b) => a.number - b.number);
    const steps: Item[] = s.capstone.steps;
    const written = [...parts, ...steps].filter((x) => x.id !== null);
    // the ends, the second part (the one that used to point back at part 3), and both sides of the way into the capstone
    const firstStep = written.findIndex((x) => x.kind === "step");
    const at = [...new Set([0, 1, firstStep - 1, firstStep, written.length - 1].filter((i) => i >= 0))];
    for (const i of at) {
        const item = written[i]!;
        await page.goto(`/p/${item.id}?series=web-server`);
        const pager = page.locator(".subbar .sbr > div").first();
        await expect(pager.locator("span", { hasText: " / " })).toHaveText(`${item.number} / ${item.kind === "part" ? parts.length : steps.length}`);
        for (const [name, to] of [["Previous problem", written[i - 1]], ["Next problem", written[i + 1]]] as const) {
            const link = pager.getByRole("link", { name });
            if (to) await expect(link).toHaveAttribute("href", `/p/${to.id}?series=web-server`);
            else await expect(link).toHaveCount(0);
        }
    }
    // and the links work
    await page.goto(`/p/${written[1]!.id}?series=web-server`);
    await page.locator(".subbar").getByRole("link", { name: "Previous problem" }).click();
    await expect(page).toHaveURL(new RegExp(`/p/${written[0]!.id}\\?series=web-server$`));
});

test("outside a series the pager still follows the track", async ({ page, request }) => {
    const id = "l1-fix-use-after-move";
    expect(await (await request.get(`/api/problems/${id}/series`)).json()).toBeNull();
    const p = await (await request.get(`/api/problems/${id}`)).json();
    await page.goto(`/p/${id}`);
    const pager = page.locator(".subbar .sbr > div").first();
    await expect(pager.locator("span", { hasText: " / " })).toHaveText(`${p.position} / ${p.count}`);
    for (const [name, to] of [["Previous problem", p.prev], ["Next problem", p.next]] as const) {
        if (to) await expect(pager.getByRole("link", { name })).toHaveAttribute("href", `/p/${to}`);
        else await expect(pager.getByRole("link", { name })).toHaveCount(0);
    }
});

// ---- a problem that more than one series has: the one being read is kept (`?series=`) ----

const crumbOf = (page: Page) => page.getByRole("navigation", { name: "Breadcrumb" });
const placeIn = async (request: APIRequestContext, series: string) => (await request.get(`/api/problems/${PART}/series?series=${series}`)).json();
const ordinal = (p: { kind: string; number: number; total: number }) => `${p.kind === "part" ? "Part" : "Capstone step"} ${p.number} of ${p.total}`;

for (const id of ["web-server", "rate-limiter"]) {
    test(`a reader of ${id} stays in it: from the series page, through Submit, the next part and Back to the series`, async ({ page, request }) => {
        test.setTimeout(240_000);
        const mine = await placeIn(request, id);
        expect(mine.series.id).toBe(id);
        expect((await request.put(`/api/problems/${PART}/draft`, { data: { code: SOLUTION() } })).ok()).toBeTruthy();

        await page.goto(`/rust/series/${id}`);
        await page.locator(".sx-part a", { hasText: (await (await request.get(`/api/problems/${PART}`)).json()).title }).click();
        const here = new RegExp(`/p/${PART}\\?series=${id}$`);
        await expect(page).toHaveURL(here);
        const check = async () => {
            await expect(crumbOf(page).getByRole("link", { name: mine.series.title })).toBeVisible();
            await expect(crumbOf(page)).toContainText(ordinal(mine));
            await expect(page.locator(".clock")).toContainText(`/ ${mine.minutes}:00`);
        };
        await check();
        await page.reload();
        await check();

        await page.getByRole("button", { name: "Submit", exact: true }).click();
        const done = page.getByRole("region", { name: "Solved" });
        await expect(done).toBeVisible({ timeout: 180_000 });
        await expect(done).toContainText(`${ordinal(mine)} is done`);
        await expect(page).toHaveURL(here);
        await check();

        const next = (await placeIn(request, id)).done.next;
        if (next?.id) {
            await done.locator(".sx-acts .sx-btn").click();
            await expect(page).toHaveURL(new RegExp(`/p/${next.id}\\?series=${id}$`));
            await expect(crumbOf(page).getByRole("link", { name: mine.series.title })).toBeVisible();
            await page.goBack();
            await expect(page).toHaveURL(here);
            await check();
            await page.goForward();
            await expect(page).toHaveURL(new RegExp(`/p/${next.id}\\?series=${id}$`));
            await page.goBack();
        }
        // the links of Connections keep it too, and those to problems outside the series name none
        const links = page.getByRole("region", { name: "Connections" });
        for (const [list, label] of [[mine.required, "required"], [mine.helpful, "helpful"]] as const) {
            for (const l of list.filter((x: { id: string | null }) => x.id)) {
                await expect(links.locator(`a[href^="/p/${l.id}"]`).first(), label).toHaveAttribute("href", l.number === null ? `/p/${l.id}` : `/p/${l.id}?series=${id}`);
            }
        }
        await page.getByRole("button", { name: "Solved", exact: true }).click();
        await done.getByRole("link", { name: "Back to the series" }).click();
        await expect(page).toHaveURL(new RegExp(`/rust/series/${id}$`));
    });
}

test("every problem link on a series page carries the series, except the optional side paths", async ({ page, request }) => {
    for (const s of await (await request.get("/api/series")).json()) {
        await page.goto(`/rust/series/${s.id}`);
        await expect(page.locator(".sx-part").first()).toBeVisible();
        const links = page.locator('.sx-page a[href^="/p/"]:not(.sx-side a)');
        expect(await links.count()).toBeGreaterThan(0);
        for (const href of await links.evaluateAll((all) => all.map((a) => a.getAttribute("href")!))) expect(href).toMatch(new RegExp(`\\?series=${s.id}$`));
    }
});

test("a shared problem opened without a series offers a switch that stays on the problem", async ({ page, request }) => {
    const picked = await (await request.get(`/api/problems/${PART}/series`)).json();
    expect(picked.others).toHaveLength(1);
    const other = await placeIn(request, picked.others[0].id);
    await page.goto("/rust");
    await page.goto(`/p/${PART}`);
    await expect(crumbOf(page).getByRole("link", { name: picked.series.title })).toBeVisible();
    // the code in the editor stays put
    await page.locator(".edit-host:not([hidden]) .cm-content").click();
    const code = await page.locator(".edit-host:not([hidden]) .cm-content").innerText();
    const swap = page.getByRole("list", { name: "Other series with this problem" }).getByRole("link");
    await expect(swap).toHaveText(`Switch series: ${other.series.title} (part ${other.number} there)`);
    await swap.focus();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(new RegExp(`/p/${PART}\\?series=${other.series.id}$`));
    await expect(crumbOf(page).getByRole("link", { name: other.series.title })).toBeVisible();
    await expect(crumbOf(page)).toContainText(ordinal(other));
    await expect(page.locator(".clock")).toContainText(`/ ${other.minutes}:00`);
    expect(await page.locator(".edit-host:not([hidden]) .cm-content").innerText()).toBe(code);
    await expect(swap).toContainText(picked.series.title);
    // a switch replaces the entry, so Back leaves the problem
    await page.goBack();
    await expect(page).toHaveURL(/\/rust$/);
});

test("a series in the address that is unknown or lacks the problem is passed over", async ({ page, request }) => {
    const picked = await (await request.get(`/api/problems/${PART}/series`)).json();
    for (const bad of ["no-such-series", "regex-engine", "7", ""]) {
        await page.goto(`/p/${PART}?series=${bad}`);
        await expect(crumbOf(page).getByRole("link", { name: picked.series.title })).toBeVisible();
        await expect(page.locator(".pane.l .sx-meta")).toBeVisible();
    }
    // a problem in one series offers no switch, and one in none ignores the address
    await page.goto("/p/b7-route-table?series=rate-limiter");
    await expect(crumbOf(page).getByRole("link", { name: "From threads to a web server" })).toBeVisible();
    await expect(page.getByRole("list", { name: "Other series with this problem" })).toHaveCount(0);
    await page.goto("/p/l1-fix-use-after-move?series=web-server");
    await expect(page.locator(".pane.l .ptitle")).toBeVisible();
    await expect(page.locator(".sx-crumb")).toHaveCount(0);
});

test("on a phone the switch sits close to its neighbours without losing its size or covering them", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/p/y5-catch-the-broken-queue");
    const link = page.locator(".sx-also a");
    await expect(link).toBeVisible();
    const m = await link.evaluate((a) => {
        const r = a.getBoundingClientRect();
        const text = document.createRange();
        text.selectNodeContents(a);
        const t = text.getBoundingClientRect();
        const meta = document.querySelector(".pane.l .sx-meta")!.getBoundingClientRect();
        const after = document.querySelector(".sx-also")!.nextElementSibling!.getBoundingClientRect();
        // the whole padded box answers a press
        const hits = [r.top + 2, r.bottom - 2].every((y) => a.contains(document.elementFromPoint(r.left + 20, y)));
        return { tap: r.height, above: t.top - meta.bottom, below: after.top - t.bottom, hits, textClear: t.top >= meta.bottom && t.bottom <= after.top };
    });
    expect(m.tap).toBeGreaterThanOrEqual(44);
    expect(m.above).toBeLessThanOrEqual(14);
    expect(m.below).toBeLessThanOrEqual(24);
    expect(m.hits).toBe(true);
    expect(m.textClear).toBe(true);
});

test("on a phone the switch is a full-size target and nothing is wider than the screen", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/p/${PART}?series=rate-limiter`);
    const swap = page.getByRole("list", { name: "Other series with this problem" }).getByRole("link");
    await expect(swap).toBeVisible();
    expect((await swap.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await swap.click();
    await expect(page).toHaveURL(/series=web-server$/);
});

test("a Connections box is drawn only when it has something in it", async ({ page, request }) => {
    type Place = { know_first: string | null; required: unknown[]; helpful: unknown[]; cookbook: unknown[] };
    const empty = (p: Place) => !p.know_first && p.required.length + p.helpful.length + p.cookbook.length === 0;
    for (const [id, series] of [["l1-raii-span-guard", "rate-limiter"], ["y5-catch-the-broken-queue", "regex-engine"], ["l1-raii-span-guard", "web-server"], ["c2-thread-pool", "web-server"]] as const) {
        const place: Place = await (await request.get(`/api/problems/${id}/series?series=${series}`)).json();
        await page.goto(`/p/${id}?series=${series}`);
        await expect(page.locator(".pane.l .sx-meta")).toBeVisible();
        const box = page.getByRole("region", { name: "Connections" });
        await expect(box, `${id} in ${series}`).toHaveCount(empty(place) ? 0 : 1);
    }
    // the case that showed an empty box
    expect(empty(await (await request.get("/api/problems/l1-raii-span-guard/series?series=rate-limiter")).json())).toBeTruthy();
});
