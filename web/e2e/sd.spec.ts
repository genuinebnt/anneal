import { expect, test } from "@playwright/test";

test("System design is an area: home, the groups, and a concept page with its drawings and checks", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/sd");
    await expect(page.locator(".hd-tabs a.on")).toHaveText("Home");
    await expect(page.locator(".sd-sec", { hasText: "Key technologies" })).toBeVisible();
    await page.locator(".hd-tabs a", { hasText: "Learn" }).click();
    await expect(page.locator(".sd-group#technology")).toContainText("Kafka");
    await page.locator(".sd-group#technology .ly-it", { has: page.getByText("Kafka", { exact: true }) }).click();
    await expect(page.locator(".ly-h1")).toHaveText("Kafka");
    await expect(page.locator(".sd-fig svg").first()).toBeVisible();
    await expect(page.locator(".sd-md table").first()).toBeVisible();
    // an adapted page credits its licensed source and lists where its facts come from
    await page.goto("/sd/learn/caching");
    await expect(page.locator(".sd-credit")).toContainText("CC BY 4.0");
    await expect(page.locator(".rd-srcs a").first()).toBeVisible();
});

test("a concept check marked as missed comes back in Review, and grading schedules it", async ({ page }) => {
    await page.goto("/sd/learn/consistent-hashing");
    const q = page.locator(".sd-q").first();
    await q.locator("textarea").fill("most keys move with modulo");
    await q.getByRole("button", { name: "Compare" }).click();
    await expect(q.locator(".sd-ref")).toBeVisible();
    await q.getByRole("button", { name: "Missed" }).click();
    await expect(q).toContainText("Back in Review today");
    await page.goto("/sd/review");
    await expect(page.locator(".sd-art .sd-k")).toContainText("CONCEPT CHECK");
    await page.getByRole("button", { name: "Show the reference" }).click();
    await page.getByRole("button", { name: "Good" }).click();
    await expect(page.locator(".ly-tl", { hasText: "Reviewed" })).toContainText("1");
});

test("a drill runs through the framework to Compare, and marking every point saves a score", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/sd/practice/rate-limiter");
    await page.getByRole("button", { name: /Start the clock/ }).click();
    await page.locator(".sd-art textarea").fill("Limit per API key; 429 with Retry-After.");
    await page.getByRole("button", { name: /Next: Core entities/ }).click();
    await page.getByRole("button", { name: /Next: API/ }).click();
    await page.getByRole("button", { name: /Next: High-level design/ }).click();
    await page.locator(".sd-tool input").fill("Gateway");
    await page.getByRole("button", { name: "+ Service" }).click();
    await expect(page.locator(".sd-node")).toHaveCount(1);
    await page.getByRole("button", { name: /Next: Deep dives/ }).click();
    await page.getByRole("button", { name: "Ask me a follow-up" }).click();
    await expect(page.locator(".sd-probe")).toHaveCount(1);
    await page.getByRole("button", { name: /I'm done, compare/ }).click();
    await expect(page.locator(".sd-cmp").first()).toContainText("REFERENCE");
    for (const b of await page.locator(".sd-rub .sd-seg").all()) await b.getByRole("button", { name: "Got it" }).click();
    await page.getByRole("button", { name: "Save my marks" }).click();
    await expect(page.locator(".ly-tl", { hasText: "Score" })).toContainText("100%");
});

test("System design pages fit a phone", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    for (const path of ["/sd", "/sd/learn", "/sd/learn/kafka", "/sd/problems", "/sd/practice/news-feed", "/sd/review"]) {
        await page.goto(path);
        await expect(page.locator(".ly-h1")).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth), path).toBeLessThanOrEqual(1);
    }
});

test("⌘K finds system design pages and drills", async ({ page }) => {
    await page.goto("/dsa");
    await page.keyboard.press("Control+k");
    await page.keyboard.type("caching");
    const hit = page.locator(".pk-opt", { hasText: "Caching" }).first();
    await expect(hit).toBeVisible();
    await expect(hit).toContainText("page");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+k");
    await page.keyboard.type("payment system");
    await expect(page.locator(".pk-opt", { hasText: "Payment system" }).first()).toContainText("drill");
});

test("every system design page renders cleanly: math typeset, figures with step controls, no errors, no sideways scroll on a phone", async ({ page, request }) => {
    test.setTimeout(180_000);
    const o = await (await request.get("/api/sd")).json();
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
    await page.setViewportSize({ width: 390, height: 820 });
    for (const c of o.concepts as { id: string }[]) {
        await page.goto(`/sd/learn/${c.id}`);
        await expect(page.locator(".rd-art h2")).toBeVisible();
        await expect(page.locator(".katex-error"), c.id).toHaveCount(0);
        const figs = await page.locator(".sd-fig").count();
        await expect(page.locator(".sd-fig .id-ctl"), c.id).toHaveCount(figs);
        expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth), c.id).toBeLessThanOrEqual(1);
    }
    for (const p of o.problems as { id: string }[]) {
        await page.goto(`/sd/practice/${p.id}`);
        await expect(page.locator(".ly-h1")).toBeVisible();
    }
    expect(errors).toEqual([]);
});

test("the reading view: outline follows, J moves on, focus mode, settings stick, code tabs and line links, step-through and fold", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    await page.goto("/sd/learn/caching");
    await expect(page.locator(".rd-toc a.on")).toContainText("Why caching works");
    await page.keyboard.press("j");
    await expect(page.locator(".rd-toc a.on")).toContainText("Where caches sit");
    // focus mode drops both panels and comes back with F
    await page.keyboard.press("f");
    await expect(page.locator(".rd-toc")).toHaveCount(0);
    await page.keyboard.press("f");
    await expect(page.locator(".rd-toc")).toBeVisible();
    // the text size sticks across a reload
    await page.getByRole("radiogroup", { name: "Size" }).getByRole("radio", { name: "L", exact: true }).click();
    await page.reload();
    await expect(page.getByRole("radiogroup", { name: "Size" }).getByRole("radio", { name: "L", exact: true })).toHaveAttribute("aria-checked", "true");
    await page.getByRole("radiogroup", { name: "Size" }).getByRole("radio", { name: "M", exact: true }).click();
    // code: highlighted, a tab per language, a line number lights its line
    const code = page.locator(".sd-code").first();
    await code.scrollIntoViewIfNeeded();
    await expect(code.locator(".t-k").first()).toBeVisible();
    await expect(code.locator(".sd-ln.sd-hl")).toHaveCount(1);
    await code.getByRole("tab", { name: "Python" }).click();
    await expect(code.locator(".sd-code-t")).toHaveText("cache_aside.py");
    await code.getByRole("button", { name: "Copy a link to line 2" }).click();
    await expect(code.locator(".sd-ln.sd-lit")).toHaveCount(1);
    // the step-through moves with Next, the fold opens
    const steps = page.locator(".sd-steps-fig");
    await steps.scrollIntoViewIfNeeded();
    await expect(steps.locator(".id-n")).toHaveText("step 1 of 4");
    await steps.getByRole("button", { name: "Next →" }).click();
    await expect(steps.locator(".id-n")).toHaveText("step 2 of 4");
    await page.locator(".sd-deeper summary").click();
    await expect(page.locator(".sd-deeper .sd-md")).toBeVisible();
    // the key terms follow the section being read
    await expect(page.locator(".rd-terms")).toBeVisible();
});

test("on a phone, margin notes open in place and the round button opens the outline", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 820 });
    await page.goto("/sd/learn/caching");
    const ref = page.locator(".sn-ref").first();
    await ref.scrollIntoViewIfNeeded();
    await expect(page.locator(".sn").first()).toBeHidden();
    await ref.click();
    await expect(page.locator(".sn").first()).toBeVisible();
    await page.locator(".rd-fab").click();
    await expect(page.locator(".rd-sheet .rd-toc")).toBeVisible();
    await page.locator(".rd-sheet .rd-toc a", { hasText: "HTTP caching" }).click();
    await expect(page.locator(".rd-sheet")).toHaveCount(0);
});

test("with AI help on, moving between problem pages inside the app never crashes (an effect once returned scrollTo's promise)", async ({ page }) => {
    await page.route("**/api/ai", (r) => r.fulfill({ json: { enabled: true, model: "test-model", provider: "gemini" } }));
    await page.goto("/d/permutation-in-string");
    await expect(page.locator(".ai-pn")).toBeVisible();
    const go = (to: string) => page.evaluate((to) => { window.history.pushState({}, "", to); window.dispatchEvent(new PopStateEvent("popstate")); }, to);
    await go("/dsa");
    await expect(page.locator(".ly-h1")).toBeVisible();
    await go("/d/two-sum");
    await expect(page.locator(".ai-pn")).toBeVisible();
    await go("/sd/learn/caching");
    await expect(page.locator(".rd-art")).toBeVisible();
    await expect(page.getByText("This page ran into a problem")).toHaveCount(0);
});
