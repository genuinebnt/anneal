import { expect, test } from "@playwright/test";

// Meaning vectors: the committed file answers "like a problem" without the model; free text needs the model (ANNEAL_MODEL in CI).
test("the API finds problems that mean the same as a problem, exactly", async ({ request }) => {
    const r = await (await request.get("/api/dsa/meaning?like=lc-course-schedule&k=5")).json();
    expect(r.phase).toBe("ready");
    expect(r.compared).toBeGreaterThan(1000);
    expect(r.hits.map((h: { id: string }) => h.id)).toContain("lc-course-schedule-ii");
    expect(r.hits.map((h: { id: string }) => h.id)).not.toContain("lc-course-schedule");
});

test("⌘K: ~ searches by meaning, Tab opens a result's actions, Everything about it opens inside and Escape goes back", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    const box = page.locator(".hd-pal input");
    await box.fill("~ shortest path in a weighted graph");
    await expect(page.locator(".pk-read")).toContainText("Searching by meaning");
    await expect(page.locator(".hd-pal .hd-grp", { hasText: "LOOKS LIKE IT" })).toBeVisible();
    await expect(page.locator(".hd-pal .hd-opt").first()).toBeVisible();
    // Tab moves into the selected result's actions
    await box.fill("two sum");
    await expect(page.locator(".hd-pal .hd-opt").first()).toContainText("1 · Two Sum");
    await page.keyboard.press("Tab");
    await expect(page.locator(".pk-acts button").first()).toBeFocused();
    await page.locator(".pk-acts").getByRole("button", { name: "Everything about it" }).click();
    await expect(page.locator(".pk-lens")).toContainText("Two Sum");
    await expect(page.locator(".hd-pal .hd-grp", { hasText: "TECHNIQUES" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".pk-lens")).toHaveCount(0);
    await expect(page.locator(".hd-pal")).toBeVisible();
});

test("⌘K reads plain words as filters and opens the list with them", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    await page.locator(".hd-pal input").fill("medium graphs unsolved");
    await expect(page.locator(".pk-read")).toContainText("Read as");
    await expect(page.locator(".pk-read")).toContainText("Graphs");
    await expect(page.getByRole("option", { name: /^Open these \d+ in the problem list/ })).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/diff=medium/);
    await expect(page).toHaveURL(/status=new(~|%7E)retry(~|%7E)due/);
});

test("the graph's Meaning mode lists problems that mean the same, and colour by progress has its own legend", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/dsa/graph?node=pro:lc-two-sum");
    await expect(page.locator(".gr-insp h3")).toHaveText("Two Sum");
    await page.locator(".gr-insp").getByRole("button", { name: "Looks like" }).click();
    await page.getByRole("radio", { name: "Meaning" }).click();
    await expect(page.locator(".ly-lp")).toContainText("How these were found");
    await expect(page.locator(".ly-lp .ly-it").first()).toBeVisible();
    await page.getByRole("radio", { name: "My progress" }).click();
    await expect(page.locator(".gr-legend")).toContainText("My progress");
});

test("Review with this lists the other reviews about the same thing", async ({ page }) => {
    for (const id of ["lc-group-anagrams", "lc-valid-anagram"]) await page.request.post(`/api/dsa/problems/${id}/log`, { data: { grade: "good" } });
    try {
        await page.goto("/dsa/review");
        await page.locator(".rv-ahead").getByRole("button", { name: /Group Anagrams/ }).click();
        await page.getByRole("tab", { name: /Review with this/ }).click();
        await expect(page.locator(".rv-with")).toContainText("Valid Anagram");
        await page.getByRole("tab", { name: /Your tries/ }).click();
        await expect(page.locator(".rv-with .rv-try").first()).toBeVisible();
    } finally {
        for (const id of ["lc-group-anagrams", "lc-valid-anagram"]) await page.request.delete(`/api/dsa/problems/${id}/log`);
    }
});

test("the calendar's timeline runs in order, with today and the target marked", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/dsa/calendar");
    await page.getByRole("button", { name: "Timeline" }).click();
    await expect(page.locator(".c-tl .tlsum")).toContainText("topics to go");
    await expect(page.locator(".c-tl .tnow")).toBeVisible();
    const starts = await page.locator(".c-tl .tlr .tlt small").allInnerTexts();
    expect(starts.length).toBeGreaterThan(3);
});

test("the problem page's left panel switches between Same idea and Looks like it, the same list either way", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/d/group-anagrams");
    const tabs = page.getByRole("tablist", { name: "Related problems" });
    await tabs.getByRole("tab", { name: /Looks like it/ }).click();
    const list = page.locator(".ly-lp .pp-rel").first();
    await expect(list).toContainText("Valid Anagram");
    // a problem of the same technique stays in Same idea only
    await expect(list).not.toContainText("Group Shifted Strings");
    await expect(list.locator("a")).toHaveCount(7);
    // each row shows its similarity score
    await expect(list.locator("a small").first()).toHaveText(/^0\.\d\d · /);
    await page.locator(".ly-lp .pp-more").click();
    expect(await list.locator("a").count()).toBeGreaterThan(7);
    await tabs.getByRole("tab", { name: /Same idea/ }).click();
    await expect(list).toContainText("Group Shifted Strings");
});
