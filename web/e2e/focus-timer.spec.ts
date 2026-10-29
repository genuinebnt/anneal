import { expect, test } from "@playwright/test";

// The timer is the same component on the DSA problem page and in the Rust workspace; these tests drive it on a DSA problem page.
// The page's other timers (polling) are real; only timers created after the clock is installed are fake, which is the timer's own.
test.beforeEach(async ({ page }) => {
    await page.goto("/d/two-sum");
    await expect(page.locator(".ft")).toBeVisible();
    await page.clock.install();
});

/** Jumps the fake clock, then lets the page settle: with the clock frozen, the retrying checks below could not see the new state. */
async function jump(page: import("@playwright/test").Page, t: string) {
    await page.clock.runFor(t);
    await page.clock.resume();
}

const time = (page: import("@playwright/test").Page) => page.locator(".ft .ft-ring b");
const label = (page: import("@playwright/test").Page) => page.locator(".ft .ft-ring small");
const go = (page: import("@playwright/test").Page, name: string) => page.locator(".ft").getByRole("button", { name, exact: true });

test("it starts at 25:00 as the first of four focus sessions", async ({ page }) => {
    await expect(time(page)).toHaveText("25:00");
    await expect(label(page)).toHaveText("FOCUS 1 / 4");
    await expect(go(page, "Start")).toBeVisible();
});

test("Start counts down, Pause holds the time, Resume continues", async ({ page }) => {
    await go(page, "Start").click();
    await jump(page, "00:30");
    await expect(time(page)).toHaveText(/^24:(2[89]|3[01])$/);
    await go(page, "Pause").click();
    const held = await time(page).textContent();
    await jump(page, "00:30");
    await expect(time(page)).toHaveText(held!);
    await expect(go(page, "Resume")).toBeVisible();
    await go(page, "Resume").click();
    await jump(page, "00:10");
    await expect(time(page)).not.toHaveText(held!);
});

test("the stopwatch counts up and Reset zeroes it", async ({ page }) => {
    await page.locator(".ft .ft-modes").getByRole("button", { name: "STOPWATCH" }).click();
    await expect(time(page)).toHaveText("00:00");
    await go(page, "Start").click();
    await jump(page, "01:05");
    await expect(time(page)).toHaveText(/^01:0[4-6]$/);
    await go(page, "Reset").click();
    await expect(time(page)).toHaveText("00:00");
});

test("Skip jumps to the break but leaves a paused timer paused", async ({ page }) => {
    await go(page, "Skip to the next phase").click();
    await expect(label(page)).toHaveText("BREAK");
    await expect(go(page, "Start")).toBeVisible();
});

test("the lengths you choose and the mode are remembered after a reload", async ({ page }) => {
    await go(page, "Settings").click();
    await page.locator(".ft .ft-set input[type=range]").first().fill("10");
    await expect(time(page)).toHaveText("10:00");
    await page.locator(".ft .ft-modes").getByRole("button", { name: "TIMER" }).click();
    await page.reload();
    await expect(page.locator(".ft")).toBeVisible();
    await expect(page.locator(".ft .ft-modes button.ft-on")).toHaveText("TIMER");
    await page.locator(".ft .ft-modes").getByRole("button", { name: "POMODORO" }).click();
    await expect(time(page)).toHaveText("10:00");
});

// Short lengths set through the saved settings keep the fake-time jumps small (the page ticks four times a second).
test.describe("with short sessions", () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => localStorage.setItem("anneal:focus.cfg", JSON.stringify({ mode: "pom", focus: 5, brk: 3, timer: 5 })));
        await page.goto("/d/two-sum");
        await expect(page.locator(".ft")).toBeVisible();
        await page.clock.install();
    });

    test("a finished focus session moves to a break and the next one to focus 2", async ({ page }) => {
        await go(page, "Start").click();
        await jump(page, "05:01");
        await expect(label(page)).toHaveText("BREAK");
        await expect(page.locator(".ft .ft-sum")).toContainText("Focus session done");
        await expect(page.locator(".ft .ft-dots i.ft-done")).toHaveCount(1);
        await expect(time(page)).toHaveText(/^0[23]:/);
        await jump(page, "03:01");
        await expect(label(page)).toHaveText("FOCUS 2 / 4");
    });

    test("the countdown stops at zero and says so", async ({ page }) => {
        await page.locator(".ft .ft-modes").getByRole("button", { name: "TIMER" }).click();
        await expect(time(page)).toHaveText("05:00");
        await go(page, "Start").click();
        await jump(page, "05:02");
        await expect(time(page)).toHaveText("00:00");
        await expect(page.locator(".ft .ft-sum")).toContainText("Time is up");
        await expect(go(page, "Restart")).toBeVisible();
        await go(page, "Restart").click();
        await expect(time(page)).toHaveText(/^0[45]:/);
    });

});

test("the same timer is at the bottom of the Rust workspace's Tests panel, above Submit, and leaves the course page alone", async ({ page }) => {
    await page.goto("/p/l1-fix-use-after-move");
    const pane = page.locator(".pane.r");
    await expect(pane.locator(".ft")).toBeVisible();
    await expect(pane.locator(".ft .ft-ring b")).toHaveText("25:00");
    // above the Submit button, not below it
    const [timer, submit] = await Promise.all([pane.locator(".ft").boundingBox(), pane.getByRole("button", { name: "Submit" }).boundingBox()]);
    expect(timer!.y + timer!.height).toBeLessThanOrEqual(submit!.y + 1);
    // the existing practice clock in the top bar stays
    await expect(page.locator(".clock")).toBeVisible();
    // a course stage page has no focus timer
    await page.goto("/courses/bustub/4a-04");
    await expect(page.getByText("Last run").first()).toBeVisible();
    await expect(page.locator(".ft")).toHaveCount(0);
});

test("it never starts by itself: not on load, not when the mode changes, not as time passes", async ({ page }) => {
    await jump(page, "01:00");
    await expect(time(page)).toHaveText("25:00");
    for (const mode of ["TIMER", "STOPWATCH", "POMODORO"]) {
        await page.locator(".ft .ft-modes").getByRole("button", { name: mode }).click();
        await jump(page, "00:20");
        await expect(go(page, "Start")).toBeVisible();
    }
});

test("while it runs the tab title carries the time, and pausing restores the title", async ({ page }) => {
    const before = await page.title();
    await go(page, "Start").click();
    await jump(page, "00:05");
    await expect(page).toHaveTitle(/^\d\d:\d\d left · /);
    await go(page, "Pause").click();
    await expect(page).toHaveTitle(before);
});
