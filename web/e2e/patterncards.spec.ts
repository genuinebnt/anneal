import { expect, test } from "@playwright/test";

test("every pattern card on the DSA home has its drawing, its progress and a next button", async ({ page }) => {
    await page.goto("/dsa");
    const cards = page.locator(".s-trk.pt");
    await expect(cards.first()).toBeVisible();
    expect(await cards.count()).toBe(18);
    // a drawing for each pattern, a ring, five progress segments, the three destinations
    await expect(page.locator(".s-trk.pt .pt-fig svg")).toHaveCount(18);
    await expect(page.locator(".s-trk.pt .pt-ring")).toHaveCount(18);
    await expect(page.locator(".s-trk.pt").first().locator(".pt-sg i")).toHaveCount(5);
    await expect(page.locator(".s-trk.pt").first().locator(".pt-ft > *")).toHaveCount(3);
    // the pattern with an Idea tab says so
    await expect(page.locator(".s-trk.pt", { hasText: "Sliding Window" }).locator(".pt-t-idea")).toBeVisible();
    // the next problem is a button that opens it, and the card itself still opens the lesson
    const first = cards.first();
    const title = (await first.locator(".pt-nx b").innerText()).trim();
    await first.locator(".pt-nx").click();
    await expect(page).toHaveURL(/\/d\//);
    await expect(page.getByRole("heading", { name: title }).first()).toBeVisible();
    await page.goto("/dsa");
    await cards.first().locator(".pt-hd").click();
    await expect(page).toHaveURL(/\/dsa\/patterns\//);
});
