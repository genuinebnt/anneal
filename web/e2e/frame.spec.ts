import { expect, test } from "@playwright/test";

// The header of the 2026-10-31 redesign: date and time, the focus timer shared with the problem page, the accent picker, no Continue.

test("the header shows the date and time, has no Continue, and its Settings gear goes to Settings", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto("/dsa");
    await expect(page.locator(".hd-clock b")).toHaveText(/\d{1,2}:\d{2}/);
    await expect(page.locator(".hd-cta")).toHaveCount(0);
    await page.getByRole("link", { name: "Settings" }).first().click();
    await expect(page).toHaveURL(/\/settings$/);
});

test("the header's focus timer is the same timer as the problem page's card, and keeps running between pages", async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto("/d/two-sum");
    const card = page.getByRole("button", { name: /^(Start|Resume)$/ });
    await card.click();
    await expect(page.locator(".hd-timer")).toHaveClass(/run/);
    await page.getByRole("link", { name: "Calendar" }).first().click();
    await expect(page.locator(".hd-timer")).toHaveClass(/run/);
    const t1 = await page.locator(".hd-timer b").innerText();
    await page.waitForTimeout(1600);
    expect(await page.locator(".hd-timer b").innerText()).not.toBe(t1);
    await page.getByRole("button", { name: "Pause the focus timer" }).click();
    await expect(page.locator(".hd-timer")).not.toHaveClass(/run/);
    await page.locator(".hd-timer").hover();
    await page.getByRole("button", { name: "Reset the focus timer" }).click();
    await expect(page.locator(".hd-timer b")).toHaveText("25:00");
});

test("the accent picker sets a deep colour or any colour, the page follows, and the choice is saved", async ({ page, request }) => {
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto("/dsa");
    await page.getByRole("button", { name: "Accent colour" }).click();
    await page.getByRole("radio", { name: "Red" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-accent", "red");
    await expect.poll(async () => (await (await request.get("/api/settings")).json()).appearance.accent).toBe("red");
    // any colour: its hue is kept, its lightness held readable
    await page.getByLabel("Pick any colour").fill("#2266ff");
    await expect(page.locator("html")).toHaveAttribute("data-accent", "custom");
    await expect.poll(async () => (await (await request.get("/api/settings")).json()).appearance).toEqual({ accent: "custom", custom: "#2266ff" });
    const acc = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--acc"));
    expect(acc.length).toBeGreaterThan(3);
    // back to the default
    await page.getByRole("button", { name: "Back to the default accent" }).click();
    await expect(page.locator("html")).not.toHaveAttribute("data-accent", /./);
    await expect.poll(async () => (await (await request.get("/api/settings")).json()).appearance.accent).toBe("copper");
});
