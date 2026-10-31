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
    await expect(page.locator(".sd-srcs li").first()).toBeVisible();
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
