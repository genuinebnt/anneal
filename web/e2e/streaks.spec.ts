import { expect, test, type Page } from "@playwright/test";

/** A fortnight ending today with yesterday missed after a 4-day run, and 2.5 freezes to spend. The API is stubbed so the test needs no
 *  back-dated data; the real rules are tested in Rust (crates/api/src/streaks.rs, crates/api/tests/api.rs). */
function fortnight(today: string) {
    const day = (n: number) => {
        const d = new Date(`${today}T12:00:00`);
        d.setDate(d.getDate() + n);
        return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    };
    const states = ["kept", "kept", "perfect", "kept", "perfect", "perfect", "kept", "kept", "kept", "perfect", "kept", "perfect", "missed", "open"];
    const days = states.map((state, i) => ({ date: day(i - 13), state, planned_new: 1, planned_reviews: 2, done_new: state === "missed" ? 0 : 1, done_reviews: 0, earned: 0, mark_id: null as number | null }));
    return { day, days };
}

async function stub(page: Page, today: string, marked: { on: boolean }) {
    const { day, days } = fortnight(today);
    const body = () => {
        const ds = days.map((d) => (marked.on && d.date === day(-1) ? { ...d, state: "frozen", mark_id: 7 } : d));
        return {
            today, streak: marked.on ? 12 : 1, perfect: 0, longest: 12, freezes: marked.on ? 1.5 : 2.5, earned: 2.5, spent: marked.on ? 1 : 0, days: ds,
            repairs: marked.on ? [] : [{ date: day(-1), before: 11, after: 0 }],
        };
    };
    await page.route("**/api/streaks", (r) => r.fulfill({ json: body() }));
    await page.route("**/api/streaks/marks", async (r) => {
        marked.on = true;
        await r.fulfill({ json: body() });
    });
}

test("the morning after a miss the repair dialog opens once, and a freeze repairs the day", async ({ page }) => {
    const today = new Date();
    const iso = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
    const marked = { on: false };
    await stub(page, iso, marked);
    await page.goto("/dsa");
    const dlg = page.getByRole("dialog", { name: /was missed/ });
    await expect(dlg).toBeVisible();
    await expect(dlg).toContainText("11-day streak");
    await expect(dlg.locator(".sk-join")).toContainText("12 days");
    await dlg.getByRole("button", { name: "Repair" }).click();
    await expect(dlg).toBeHidden();
    // the header now shows the freezes left
    await expect(page.locator(".hd-chip", { hasText: "freezes" })).toContainText("1.5");

    // a second visit the same day does not open it again
    await page.reload();
    await expect(page.locator(".hd-chip", { hasText: "freezes" })).toBeVisible();
    await expect(page.getByRole("dialog", { name: /was missed/ })).toHaveCount(0);
});

test("Not now keeps the chance in the bell, and the Calendar's Streak tab shows the numbers and the days", async ({ page }) => {
    const t = new Date();
    const iso = `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`;
    await stub(page, iso, { on: false });
    await page.goto("/dsa");
    const dlg = page.getByRole("dialog", { name: /was missed/ });
    await dlg.getByRole("button", { name: "Not now" }).click();
    await expect(dlg).toBeHidden();
    await page.getByRole("button", { name: /^Notifications/ }).click();
    const pop = page.getByRole("dialog", { name: "Notifications" });
    await expect(pop.locator(".nt-item", { hasText: "streak broke" })).toBeVisible();
    await page.keyboard.press("Escape");

    await page.goto("/progress");
    await expect(page.locator(".sk-box")).toHaveCount(0);
    await page.goto("/dsa/calendar");
    await page.getByRole("tab", { name: "Streak" }).click();
    const box = page.locator(".c-sk");
    await expect(box.locator(".sk-stat.sk-freeze b")).toHaveText("2.5");
    await expect(box.locator(".sk-strip .sk-day")).toHaveCount(14);
    await expect(box.locator(".sk-strip .sk-day.missed")).toHaveCount(1);
    await expect(box.getByRole("button", { name: "Repair" })).toBeVisible();
    await box.getByText("How streaks work").click();
    await expect(box).toContainText("A freeze goes on a missed planned day");
});
