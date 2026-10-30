import { expect, test, type Page } from "@playwright/test";

const rows = ".k-mod .k-row";

/** Closes every module: from a partly open course the button first reads "expand all". */
async function collapseAll(page: Page) {
    if (await page.getByRole("button", { name: "EXPAND ALL" }).count()) await page.getByRole("button", { name: "EXPAND ALL" }).click();
    await page.getByRole("button", { name: "COLLAPSE ALL" }).click();
    await expect(page.locator(".k-mod.k-open")).toHaveCount(0);
}

test.beforeEach(async ({ page }) => {
    await page.goto("/courses");
    await expect(page.locator(".k-map")).toBeVisible();
});

test("planned modules are hidden until you ask, and then they carry a PLANNED tag", async ({ page }) => {
    const course = await (await page.request.get("/api/courses/bustub")).json();
    const modules: { code: string; planned: boolean }[] = course.projects.flatMap((p: { modules: { code: string; planned: boolean }[] }) => p.modules);
    const planned = modules.filter((m) => m.planned);
    await page.goto("/courses/bustub");
    await expect(page.locator(".k-mod").first()).toBeVisible();
    // finished modules are listed, planned ones are not
    for (const m of modules) await expect(page.locator(`#mod-${m.code}`)).toHaveCount(m.planned ? 0 : 1);
    await expect(page.locator(".k-planned:not(.k-optional)")).toHaveCount(0);
    if (planned.length === 0) return;
    await page.getByRole("button", { name: `SHOW PLANNED (${planned.length})` }).click();
    for (const m of planned) await expect(page.locator(`#mod-${m.code} .k-planned`)).toHaveText("PLANNED");
    await expect(page.locator("#mod-1a .k-planned")).toHaveCount(0);
    await page.getByRole("button", { name: "HIDE PLANNED" }).click();
    await expect(page.locator(`#mod-${planned[0].code}`)).toHaveCount(0);
});

test("the optional Rust on-ramp is listed first and tagged OPTIONAL, not PLANNED", async ({ page }) => {
    await page.goto("/courses/bustub");
    await expect(page.locator("#mod-r .k-optional")).toHaveText("OPTIONAL");
    await expect(page.locator("#mod-1a .k-optional")).toHaveCount(0);
    const first = await page.locator(".k-mod").first().getAttribute("id");
    expect(first).toBe("mod-r");
});

test("modules and extras that are not part of BusTub carry a BEYOND tag", async ({ page }) => {
    await page.goto("/courses/bustub");
    await expect(page.locator("#mod-4d .k-mh .k-beyond")).toHaveText("BEYOND BUSTUB");
    await expect(page.locator("#mod-1a .k-mh .k-beyond")).toHaveCount(0);
    await page.getByRole("button", { name: "EXPAND ALL" }).click();
    const challenge = page.locator("#mod-1a .k-row", { hasText: "CHALLENGE" }).first();
    await expect(challenge.locator(".k-beyond")).toHaveText("BEYOND");
    // inside a module that is beyond BusTub as a whole, the rows do not repeat the tag
    await expect(page.locator("#mod-4d .k-row .k-beyond")).toHaveCount(0);
});

test("Reset progress forgets one module after you type reset, and leaves the others", async ({ page, request }) => {
    const pass = (id: string) => request.post("/api/courses/bustub/runs", { data: { stage_id: id, tests: [{ name: "a", ok: true, detail: "" }] } });
    await pass("4c-01");
    await pass("4b-01");
    const state = async (id: string) => (await (await request.get(`/api/courses/bustub/stages/${id}`)).json()).state;
    expect(await state("4c-01")).not.toBe("todo");
    await page.goto("/courses/bustub");
    await page.getByRole("button", { name: "RESET PROGRESS" }).click();
    const panel = page.getByRole("group", { name: "Reset progress" });
    await panel.getByRole("button", { name: "What to reset" }).click();
    await page.getByRole("option", { name: /^Module 4C/ }).click();
    await expect(panel.getByRole("button", { name: "What to reset" })).toContainText("Module 4C");
    const go = panel.getByRole("button", { name: "Reset progress" });
    await expect(go).toBeDisabled();
    await panel.getByPlaceholder("reset").fill("reset");
    await go.click();
    await expect(page.locator(".k-tt.k-ok")).toContainText("Progress reset");
    await expect(panel).toHaveCount(0);
    expect(await state("4c-01")).toBe("todo");
    expect(await state("4b-01")).not.toBe("todo");
});

test("the map has a button for every module and opens the one you click", async ({ page }) => {
    const n = await page.locator(".k-node").count();
    expect(n).toBeGreaterThanOrEqual(18);
    const header = page.locator("#mod-4a .k-mh");
    await expect(header).toHaveAttribute("aria-expanded", "false");
    await page.locator(".k-node", { hasText: "4A" }).click();
    await expect(header).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator("#mod-4a .k-row").first()).toBeVisible();
});

test("only the module you are in starts open, and Expand all opens the rest", async ({ page }) => {
    const open = page.locator(".k-mod.k-open");
    await expect(open).toHaveCount(1);
    await expect(open.first()).toHaveAttribute("id", /mod-/);
    await page.getByRole("button", { name: "EXPAND ALL" }).click();
    const all = await page.locator(".k-mod").count();
    await expect(page.locator(".k-mod.k-open")).toHaveCount(all);
    await page.getByRole("button", { name: "COLLAPSE ALL" }).click();
    await expect(page.locator(".k-mod.k-open")).toHaveCount(0);
});

test("collapsed rows cannot be tabbed into", async ({ page }) => {
    await collapseAll(page);
    expect(await page.locator("#mod-1b div[inert]").count()).toBe(1);
    await page.locator("#mod-1b .k-mh").click();
    expect(await page.locator("#mod-1b div[inert]").count()).toBe(0);
});

test("an open module is remembered after a reload", async ({ page }) => {
    await collapseAll(page);
    await page.locator("#mod-2b .k-mh").click();
    await expect(page.locator("#mod-2b")).toHaveClass(/k-open/);
    await page.reload();
    await expect(page.locator("#mod-2b")).toHaveClass(/k-open/);
    await expect(page.locator("#mod-1a")).not.toHaveClass(/k-open/);
});

test("find narrows to matching stages and opens their modules; clearing restores the choice", async ({ page }) => {
    await page.getByLabel("Find a stage").fill("watermark");
    await expect(page.locator(rows)).toHaveCount(1);
    await expect(page.locator(rows).first()).toContainText("watermark");
    await expect(page.locator(".k-mod.k-open")).toHaveCount(1);
    await page.getByLabel("Find a stage").fill("zzzz-no-stage");
    await expect(page.locator(".k-none")).toContainText("No stage matches");
    await page.getByRole("button", { name: "Clear it" }).click();
    await expect(page.locator(".k-none")).toHaveCount(0);
    await expect(page.locator(".k-mod.k-open")).toHaveCount(1);
});

test("the boss filter shows only BusTub test stages", async ({ page }) => {
    await page.getByRole("button", { name: "boss", exact: true }).click();
    await expect(page.getByRole("button", { name: "boss", exact: true })).toHaveAttribute("aria-pressed", "true");
    const n = await page.locator(rows).count();
    expect(n).toBeGreaterThan(5);
    await expect(page.locator(`${rows} .k-boss`)).toHaveCount(n);
    await page.getByRole("button", { name: "all", exact: true }).click();
    expect(await page.locator(".k-mod.k-open .k-row").count()).toBeGreaterThan(0);
});

test("the to-do and passed filters split the stages", async ({ page }) => {
    await page.getByRole("button", { name: "to do", exact: true }).click();
    const todo = await page.locator(rows).count();
    await page.getByRole("button", { name: "passed", exact: true }).click();
    const done = await page.locator(rows).count().catch(() => 0);
    expect(todo).toBeGreaterThan(50);
    // planned modules are hidden, so only the stages of the others are listed
    const course = await (await page.request.get("/api/courses/bustub")).json();
    const total = course.projects.flatMap((p: { modules: { planned: boolean; stages: unknown[] }[] }) => p.modules).filter((m: { planned: boolean }) => !m.planned).reduce((n: number, m: { stages: unknown[] }) => n + m.stages.length, 0);
    expect(todo + done).toBe(total);
});

test("/ focuses the find box, and Escape leaves it", async ({ page }) => {
    await page.keyboard.press("/");
    await expect(page.getByLabel("Find a stage")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.getByLabel("Find a stage")).not.toBeFocused();
});

test("each get-started command has its own copy button", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    // five steps to get started, then three for another laptop
    await expect(page.locator(".k-cmd")).toHaveCount(8);
    const third = page.locator(".k-cmd").nth(2);
    await third.getByRole("button").click();
    await expect(third.getByRole("button")).toHaveText("COPIED");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("cd bustub-rs");
    await expect(page.locator(".k-cmd").first()).toContainText("/install.sh | sh");
    await expect(page.locator(".k-cmd").last()).toContainText("anneal course restore");
});

test("a project's progress bar opens that project's first module", async ({ page }) => {
    await collapseAll(page);
    await page.locator(".k-pr", { hasText: "Project 2" }).click();
    await expect(page.locator("#mod-2a")).toHaveClass(/k-open/);
});

test("a link to a planned Rust track lands on a planned page, not an error", async ({ page }) => {
    await page.goto("/t/c1-threads-shared-state");
    await expect(page.getByTestId("planned-track")).toContainText("C1 · Threads & shared state");
    await expect(page.getByTestId("planned-track")).toContainText("planned");
});

test("the get started card says what the CLI last reported", async ({ page, request }) => {
    // a stage no other test opens, so this does not change what they see
    await request.post("/api/courses/bustub/runs", { data: { stage_id: "0d-01", tests: [{ name: "a", ok: true, detail: "" }] } });
    await page.goto("/courses/bustub");
    await expect(page.getByTestId("cli-status")).toContainText("Last run from the CLI: 0d-01");
});
