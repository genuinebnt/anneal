import { expect, test } from "@playwright/test";

// docs/DSA_LEARN_PAGE_SPEC.md: the Learn page of a pattern.

test("a technique card has its implementation on the left and its problems, in the lists or not, on the right", async ({ page }) => {
    // a link to the card opens it (the cards are folded)
    await page.goto("/dsa/patterns/D11#t-dfs");
    await expect(page.getByRole("heading", { name: "Traversal", level: 2 })).toBeVisible();

    // one card, two versions of the template, each tab its own code
    const dfs = page.locator("#t-dfs");
    await expect(dfs).toHaveClass(/open/);
    await expect(dfs.locator(".lr-left pre").first()).toContainText("def dfs_recursive");
    await dfs.getByRole("tab", { name: /iterative \(explicit stack\)/i }).click();
    await expect(dfs.locator(".lr-left pre").first()).toContainText("def dfs_iterative");
    // the traps are part of the implementation side, the problems are the right side only
    await expect(dfs.locator(".lr-left .lr-traps")).toBeVisible();
    await expect(dfs.locator(".lr-right .lr-traps")).toHaveCount(0);
    expect(await dfs.locator(".lr-right .lr-p").count()).toBeGreaterThanOrEqual(3);
});

test("every problem row carries difficulty, priority, the narrowest list (none outside the lists), companies and topics", async ({ page, request }) => {
    // a card that has problems outside the NeetCode lists
    const d = await (await request.get("/api/dsa/patterns/D2")).json();
    const withOutside = d.techniques.find((t: { problems: { list_tag: string | null }[] }) => t.problems.some((p) => p.list_tag === null));
    expect(withOutside, "Two Pointers has a technique with a problem outside the lists").toBeTruthy();
    await page.goto(`/dsa/patterns/D2#t-${withOutside.id.split(":")[1]}`);
    const card = page.locator(`#t-${withOutside.id.split(":")[1]}`);
    await expect(card).toHaveClass(/open/);
    const out = card.locator(".lr-p.out").first();
    await expect(out).toBeVisible();
    await expect(out.locator(".lr-pill.lv")).toBeVisible();
    await expect(out.locator(".lr-pill").filter({ hasText: /^(MUST|STRONG|PRACTICE|WARM-UP)$/ })).toHaveCount(1);
    await expect(out.locator(".lr-q .lst")).toHaveCount(0); // no list tag outside the lists
    expect(await out.locator(".lr-co span").count()).toBeGreaterThan(0);
    await expect(out.locator(".lr-tp")).not.toBeEmpty();
    // a list problem has exactly one list tag, the narrowest
    const inList = card.locator(".lr-p:not(.out)").first();
    await expect(inList.locator(".lr-q .lst")).toHaveCount(1);
});

test("a problem row opens the site's problem page, which links to LeetCode", async ({ page, request }) => {
    const d = await (await request.get("/api/dsa/patterns/D2")).json();
    const outside = d.techniques.flatMap((t: { problems: { list_tag: string | null; slug: string; title: string }[] }) => t.problems).find((p: { list_tag: string | null }) => p.list_tag === null);
    await page.goto("/dsa/patterns/D2");
    await page.locator(".lr-tools").getByRole("button", { name: "open all" }).click();
    await page.locator(`.lr-p a[href="/d/${outside.slug}"]`).first().click();
    await expect(page).toHaveURL(new RegExp(`/d/${outside.slug}$`));
    await expect(page.getByRole("heading", { name: outside.title })).toBeVisible();
    await expect(page.locator(`a[href="https://leetcode.com/problems/${outside.slug}/"]`).first()).toBeVisible();
});

test("a technique with no problem is only named, at the bottom", async ({ page }) => {
    await page.goto("/dsa/patterns/D11");
    await expect(page.locator(".l-listed")).toHaveCount(0);
    const other = page.locator(".l-other");
    await expect(other).toBeVisible();
    await other.getByRole("button").click();
    await expect(other.locator(".l-onames li").first()).toBeVisible();
    // names only: nothing in it opens or links
    await expect(other.locator("a, button.l-lchip")).toHaveCount(0);
});

test("every problem of the site has companies and topic tags; the list tag is the narrowest list", async ({ request }) => {
    const o = await (await request.get("/api/dsa")).json();
    const problems = o.problems as { id: string; companies: unknown[]; tags: string[]; lists: string[]; list_tag: string | null; priority: string }[];
    // the only exceptions are the NeetCode-list problems no company of the site's set asks (content/dsa/no_company.txt)
    const noCompany = problems.filter((p) => p.companies.length === 0);
    expect(noCompany.length).toBeLessThanOrEqual(22);
    expect(noCompany.every((p) => p.lists.some((l) => l !== "practice")), "only list problems may lack a company").toBe(true);
    expect(problems.filter((p) => p.tags.length === 0)).toHaveLength(0);
    for (const p of problems) {
        const narrowest = (["blind75", "neetcode150", "neetcode250", "all"] as const).find((l) => p.lists.includes(l)) ?? null;
        expect(p.list_tag, p.id).toBe(narrowest);
        expect(["must", "strong", "practice", "warmup"]).toContain(p.priority);
    }
});

test("every topic's pattern page is grouped, and every technique shown has an implementation", async ({ request }) => {
    const overview = await (await request.get("/api/dsa")).json();
    expect(overview.patterns.length).toBe(18);
    for (const p of overview.patterns as { code: string; name: string }[]) {
        const d = await (await request.get(`/api/dsa/patterns/${p.code}`)).json();
        expect(d.groups?.length, `${p.name} declares sections`).toBeGreaterThan(0);
        for (const t of d.techniques as { id: string; lesson: { group?: string } | null; problems: unknown[] }[]) {
            // L4: a technique that has problems has an implementation
            if (t.problems.length > 0) expect(t.lesson, `${t.id} has problems, so it needs an implementation`).not.toBeNull();
            if (t.lesson?.group) expect(d.groups, `${p.name}: ${t.lesson.group}`).toContain(t.lesson.group);
        }
        // L5: no problem hangs off a section
        expect(d.group_problems).toBeUndefined();
        expect(d.group_tags).toBeUndefined();
    }
});

test("the Learn tab counts the techniques that are not taught by a NeetCode problem too", async ({ page, request }) => {
    const p = await (await request.get("/api/dsa/patterns/D11")).json();
    await page.goto("/dsa/patterns/D11");
    await expect(page.getByRole("tab", { name: /Learn/ })).toContainText(`${p.techniques.length + p.extras.length} techniques`);
});

test("every problem has a one-line key insight, shown under the title and on the problem page", async ({ page, request }) => {
    const plain = (p: { id: string; insight: string }) => !!p.insight && p.insight.length <= 110 && !p.insight.includes("`");
    const o = await (await request.get("/api/dsa")).json();
    expect(o.problems.length).toBeGreaterThan(900);
    expect(o.problems.filter((p: { id: string; insight: string }) => !plain(p)).map((p: { id: string }) => p.id), "the lists").toHaveLength(0);
    // the practice problems come with the pattern pages
    let rows = 0;
    for (const pat of o.patterns as { code: string }[]) {
        const d = await (await request.get(`/api/dsa/patterns/${pat.code}`)).json();
        const all = [...d.techniques.flatMap((t: { problems: unknown[] }) => t.problems), ...d.extras.flatMap((x: { examples: unknown[] }) => x.examples)] as { id: string; insight: string }[];
        rows += all.length;
        expect(all.filter((p) => !plain(p)).map((p) => p.id), `${pat.code} rows`).toHaveLength(0);
    }
    expect(rows).toBeGreaterThan(1500);
    // the first row on the page, whichever technique the sections put first
    await page.goto("/dsa/patterns/D2");
    const row = page.locator(".lr-p").first();
    const slug = (await row.locator("a.lr-pt").getAttribute("href"))!.replace("/d/", "");
    const all = await (await request.get("/api/dsa/problems/lc-" + slug + "/row")).json();
    expect(all.insight).toBeTruthy();
    // the idea gives the problem away, so it is hidden until asked for
    await expect(row.locator(".lr-idea")).toBeVisible();
    await expect(row).not.toContainText(all.insight);
    await row.locator(".lr-idea").click();
    await expect(row.locator(".lr-ins")).toHaveText(all.insight);
    await page.goto(`/d/${slug}`);
    await expect(page.locator(".pp-ins")).not.toContainText(all.insight);
    await page.locator(".pp-ins").getByRole("button", { name: "Show" }).click();
    await expect(page.locator(".pp-ins")).toContainText(all.insight);
});
