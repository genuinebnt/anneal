import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";

// The blocks of a hand-written cookbook lesson. The page is made up: it is what the loader makes of the test fixture
// (crates/content/tests/fixtures/lesson.toml; a Rust test keeps the two the same), served in place of the API.
const LESSON = JSON.parse(readFileSync(new URL("./fixtures/lesson-page.json", import.meta.url), "utf8"));
const URL_OF_LESSON = "/cookbook/p/e2e-lesson";

async function open(page: Page, width = 1500, extra: Record<string, unknown> = {}) {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.route("**/api/cookbook/pages/e2e-lesson", (route) => route.fulfill({ json: { page: { ...LESSON, ...extra, id: "e2e-lesson" }, previous: null, next: null, linked_from: [] } }));
    await page.setViewportSize({ width, height: width < 500 ? 844 : 950 });
    await page.goto(URL_OF_LESSON);
    await expect(page.locator(".cb-head h1")).toHaveText("Fixture lesson");
    // the web fonts arrive after the first paint and move the page a little; measurements wait for them
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready.then(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)))));
    return errors;
}

const sideways = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

test("A lesson opens with In short, and its ways to the code and the parts move the keyboard there", async ({ page }) => {
    const errors = await open(page);
    const short = page.getByRole("region", { name: "In short" });
    await expect(short).toContainText("A fixture summary");
    // beside the "On this page" panel the box lists no parts of its own, and keeps to the reading column
    const parts = short.getByRole("navigation", { name: "Jump to" });
    await expect(parts).toBeHidden();
    expect((await short.boundingBox())!.width).toBeLessThanOrEqual(664);
    // without the panel (hidden, or a narrower window) it does
    await page.keyboard.press("]");
    await expect(parts).toBeVisible();
    await page.keyboard.press("]");
    await page.setViewportSize({ width: 1100, height: 950 });
    await expect(parts.getByRole("link")).toHaveText(["How it works", "Connections"]);
    await short.getByRole("link", { name: "Jump to code" }).click();
    await expect(page.locator("#cb-lab-0")).toBeFocused();
    await expect(page.locator("#cb-lab-0 h2")).toHaveText("Full code");
    await expect(page.locator("#cb-lab-0 h2")).toBeInViewport();
    await short.getByRole("link", { name: "How it works" }).click();
    await expect(page.locator("#cb-how-it-works")).toBeFocused();
    await expect(page).toHaveURL(/#cb-how-it-works$/);
    await expect(page.locator(".cb-toc button")).toHaveText([/How it works/, /Full code/, /Connections/]);
    // a lesson is never folded, and an address can name a part that is not a section
    await expect(page.locator(".cb-fold")).toHaveCount(0);
    await page.goto("about:blank");
    await page.goto(`${URL_OF_LESSON}#cb-connections`);
    await expect(page.locator("#cb-connections h2")).toBeInViewport();
    expect(errors).toEqual([]);
});

test("The stepper moves only when asked, by button or key, and every frame is there to be read", async ({ page }) => {
    const errors = await open(page);
    const step = page.getByRole("figure", { name: "A box fills" });
    const now = step.locator(".cb-frames > li.on");
    // all three frames are in the accessibility tree before anything is pressed
    for (const text of ["Nothing yet.", "One thing.", "Two things."]) await expect(step.getByRole("listitem").filter({ hasText: text })).toHaveCount(1);
    await expect(step.getByRole("button", { name: /Play|Pause/ })).toHaveCount(0);
    await page.waitForTimeout(1800);
    await expect(step.locator(".id-n")).toHaveText("step 1 of 3");
    await expect(now).toContainText("Nothing yet.");
    await expect(now).toContainText("0 of 2 used");
    await expect(step.getByRole("button", { name: "← Back" })).toHaveAttribute("aria-disabled", "true");

    const next = step.getByRole("button", { name: "Next →" });
    const below = () => page.locator("#cb-lab-0").evaluate((e) => Math.round(e.getBoundingClientRect().top + window.scrollY));
    const before = await below();
    await next.focus();
    await page.keyboard.press("Enter");
    await expect(now).toContainText("One thing.");
    await expect(step.getByRole("status")).toHaveText("Step 2 of 3: One");
    await page.keyboard.press("Space");
    await expect(now).toContainText("Two things.");
    // at the end the keyboard stays on Next, and nothing below has moved
    await expect(next).toBeFocused();
    await expect(next).toHaveAttribute("aria-disabled", "true");
    expect(await below()).toBe(before);
    await page.keyboard.press("ArrowLeft");
    await expect(step.locator(".id-n")).toHaveText("step 2 of 3");
    await page.keyboard.press("Home");
    await expect(step.locator(".id-n")).toHaveText("step 1 of 3");
    await page.keyboard.press("End");
    await expect(step.locator(".id-n")).toHaveText("step 3 of 3");
    await step.getByRole("button", { name: "Restart" }).click();
    await expect(now).toContainText("Nothing yet.");
    expect(errors).toEqual([]);
});

for (const width of [1500, 390]) {
    test(`The walkthrough highlights a step's lines and brings them into view, ${width} wide`, async ({ page }) => {
        await open(page, width);
        const walk = page.getByRole("figure", { name: "Code walkthrough of x.rs" });
        const lit = walk.locator(".sd-ln.sd-hl .sd-lnum");
        await expect(lit).toHaveText(["1", "2"]);
        await expect(walk.locator(".cb-walk-say").first()).toContainText("The first lines.");
        const second = walk.getByRole("button", { name: /Near the end/ });
        await second.scrollIntoViewIfNeeded();
        const at = await page.evaluate(() => window.scrollY);
        // pressed from inside the page: Playwright's own click may scroll the page first
        await second.evaluate((b: HTMLButtonElement) => b.click());
        await expect(lit).toHaveText(["50", "51"]);
        await expect(second).toHaveAttribute("aria-current", "step");
        // the code scrolls inside its own box; the page stays where it was
        const inBox = () => walk.locator("pre").evaluate((pre) => {
            const line = pre.querySelector('[data-line="50"]')!.getBoundingClientRect();
            const box = pre.getBoundingClientRect();
            return line.top >= box.top && line.bottom <= box.bottom;
        });
        await expect.poll(inBox).toBe(true);
        expect(Math.abs((await page.evaluate(() => window.scrollY)) - at)).toBeLessThanOrEqual(1);
        await expect(walk.getByRole("region", { name: /lines 50 to 51 highlighted/ })).toBeVisible();
        await second.focus();
        await page.keyboard.press("ArrowLeft");
        await expect(lit).toHaveText(["1", "2"]);
        await expect(walk.getByRole("button", { name: /Start/ })).toBeFocused();
        // the code can be reached and scrolled by keyboard
        await expect(walk.locator("pre")).toHaveAttribute("tabindex", "0");
        // the words of the other step are still there for a screen reader
        await expect(walk.getByText("Lines far down the file.")).toHaveCount(1);
        expect(await sideways(page)).toBeLessThanOrEqual(0);
    });
}

test("Wrong and fixed are labelled in words, and an exact exercise stays closed until asked", async ({ page }) => {
    await open(page);
    const pair = page.getByRole("group", { name: "Wrong and fixed: Without, then With" });
    await expect(pair.locator(".cb-cmp-t")).toHaveText([/Wrong\s*Without/, /Fixed\s*With/]);
    await expect(pair).toContainText("What goes wrong");
    await expect(pair).toContainText("Something happens.");
    await expect(pair).toContainText("It costs a line.");
    const box = page.locator("#cb-connections");
    await expect(box.locator("h3")).toHaveText(["Know first", "Practise", "In real projects"]);
    await expect(box.getByRole("link", { name: "A problem" })).toBeHidden();
    await expect(box.getByText("To try it.")).toBeHidden();
    const ask = box.locator("summary");
    await expect(ask).toHaveText(/the exact exercise/);
    await expect(box.locator(".cb-when-closed")).toBeVisible();
    await ask.focus();
    await page.keyboard.press("Enter");
    await expect(box.getByRole("link", { name: "A problem" })).toBeVisible();
    await expect(box.locator(".cb-when-open")).toBeVisible();
    await expect(box.locator(".cb-when-closed")).toBeHidden();
    await expect(box.getByRole("link", { name: "A real program" })).toHaveCount(0);
});

test("A lesson keeps its text to a readable line, fits every width and stores nothing", async ({ page }) => {
    await open(page, 1500);
    const stored = () => page.evaluate(() => JSON.stringify([Object.keys(localStorage).sort(), Object.keys(sessionStorage).sort()]));
    const before = await stored();
    await page.getByRole("button", { name: "Next →" }).click();
    await page.getByRole("button", { name: /Near the end/ }).click();
    await page.locator("#cb-connections summary").click();
    expect(await stored()).toBe(before);
    for (const width of [2200, 1500, 1100, 768, 390]) {
        await page.setViewportSize({ width, height: 900 });
        expect(await sideways(page), `${width}`).toBeLessThanOrEqual(0);
        const line = await page.locator("#cb-how-it-works .sd-md > p").first().evaluate((e) => e.getBoundingClientRect().width);
        // 39 times the text size, about 80 characters; the text is a little larger on a very wide screen
        expect(line, `text at ${width}`).toBeLessThanOrEqual(width >= 1900 ? 703 : 664);
    }
    // on a phone the controls are big enough to touch
    for (const name of ["← Back", "Next →", "Restart"]) expect((await page.getByRole("button", { name }).boundingBox())!.height, name).toBeGreaterThanOrEqual(44);
    expect((await page.locator(".cb-walk-steps button").first().boundingBox())!.height).toBeGreaterThanOrEqual(44);
});

for (const colorScheme of ["dark", "light"] as const) {
    test(`A lesson's text is readable in ${colorScheme}`, async ({ page }) => {
        await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
        await open(page);
        await page.locator("#cb-connections summary").click();
        const low = await page.evaluate(() => {
            const ctx = Object.assign(document.createElement("canvas"), { width: 1, height: 1 }).getContext("2d", { willReadFrequently: true })!;
            const rgba = (c: string) => { ctx.clearRect(0, 0, 1, 1); ctx.fillStyle = c; ctx.fillRect(0, 0, 1, 1); const d = ctx.getImageData(0, 0, 1, 1).data; return [d[0]!, d[1]!, d[2]!, d[3]! / 255]; };
            const lum = (c: number[]) => { const f = (v: number) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c[0]!) + 0.7152 * f(c[1]!) + 0.0722 * f(c[2]!); };
            const over = (top: number[], under: number[]) => [0, 1, 2].map((i) => top[i]! * top[3]! + under[i]! * (1 - top[3]!));
            const background = (el: Element) => {
                const layers: number[][] = [];
                for (let e: Element | null = el; e; e = e.parentElement) layers.push(rgba(getComputedStyle(e).backgroundColor));
                return layers.reverse().reduce((under, top) => over(top, under), [255, 255, 255]);
            };
            const bad: string[] = [];
            for (const el of document.querySelectorAll(".cb-art :is(.cb-short, .cb-sec, .cb-conn) *")) {
                const s = getComputedStyle(el);
                // frames and words kept only for screen readers are not drawn; a button that can do nothing is exempt
                if (!el.getBoundingClientRect().width || s.visibility === "hidden" || el.closest(".cb-sr, .cb-frames > li:not(.on), .cb-walk-steps li:not(.on) .cb-walk-say, [aria-disabled=true]")) continue;
                if (![...el.childNodes].some((n) => n.nodeType === 3 && n.textContent!.trim())) continue;
                const bg = background(el);
                const [a, b] = [lum(over(rgba(s.color), bg)), lum(bg)].sort((x, y) => y - x);
                const ratio = (a! + 0.05) / (b! + 0.05);
                if (ratio < 4.5) bad.push(`${el.tagName}.${el.getAttribute("class")} ${ratio.toFixed(2)} ${el.textContent!.trim().slice(0, 30)}`);
            }
            return bad;
        });
        expect(low).toEqual([]);
    });
}

test("Code that gives away an exercise says so and stays closed, even for Jump to code", async ({ page }) => {
    const errors = await open(page, 1500, { code_spoils: [{ link: "/p/c1-blocking-queue", title: "A blocking queue" }] });
    const block = page.locator("#cb-lab-0");
    await page.getByRole("link", { name: "Jump to code" }).click();
    await expect(block).toBeFocused();
    await expect(block.locator(".cb-spoil-note")).toContainText("This code gives away much of A blocking queue. Try the exercise first");
    await expect(block.getByRole("link", { name: "A blocking queue" })).toHaveAttribute("href", "/p/c1-blocking-queue");
    await expect(block.locator("summary")).toBeInViewport();
    await expect(block.locator(".cb-view")).toHaveCount(0);
    await block.locator("summary").focus();
    await page.keyboard.press("Enter");
    await expect(block.locator(".cb-view .sd-ln").first()).toBeVisible();
    // an address that names the code lands on the closed block too, and nothing remembers it was opened
    await page.goto("about:blank");
    await page.goto(`${URL_OF_LESSON}#cb-lab-0`);
    await expect(block.locator(".cb-spoil-note")).toBeInViewport();
    await expect(block.locator(".cb-view")).toHaveCount(0);
    expect(errors).toEqual([]);
});

test("Connections: what links somewhere looks like a link, and what does not is plain", async ({ page }) => {
    await open(page);
    const box = page.locator("#cb-connections");
    const colour = (l: ReturnType<typeof box.locator>) => l.evaluate((e) => getComputedStyle(e).color);
    const linked = box.getByRole("link", { name: "The other page" });
    const plain = box.locator(".cb-conn-t span", { hasText: "A real program" });
    expect(await colour(linked)).not.toBe(await colour(plain));
    expect(await linked.evaluate((e) => getComputedStyle(e, "::after").content)).toContain("→");
    expect(await plain.evaluate((e) => getComputedStyle(e, "::after").content)).toBe("none");
});

test("Snippet line numbers are not tab stops, and wide compare code scrolls in its own box on a phone", async ({ page }) => {
    const long = "        let a_rather_long_name = another_long_function_name(first_argument, second_argument)?;";
    const sections = LESSON.sections.map((s: { heading: string; body: string }) => ({
        ...s,
        body: s.body.replace('"code":"fn b() {}"', `"code":${JSON.stringify(`fn b() {\n${long}\n}`)}`) + "\n```rust\nfn c() {}\nfn d() {}\n```\n",
    }));
    await open(page, 390, { sections });
    await expect(page.locator(".cb-art .sd-lnum").first()).toBeAttached();
    expect(await page.locator('.cb-art button.sd-lnum:not([tabindex="-1"])').count()).toBe(0);
    const pre = page.locator(".cb-cmp-side.fixed pre");
    await pre.scrollIntoViewIfNeeded();
    const at = await pre.evaluate((e) => ({ wide: e.scrollWidth - e.clientWidth, space: getComputedStyle(e.querySelector(".sd-ln > span")!).whiteSpace, bar: e.offsetHeight - e.clientHeight, rows: new Set([...e.querySelectorAll(".sd-ln")].map((l) => Math.round(l.getBoundingClientRect().height))).size }));
    expect(at.wide).toBeGreaterThan(0);
    expect([at.space, at.bar, at.rows]).toEqual(["pre", 0, 1]);
    await expect(pre).toHaveAttribute("tabindex", "0");
    await expect(pre).toHaveAttribute("data-more", "");
    await pre.evaluate((e) => (e.scrollLeft = e.scrollWidth));
    await expect(pre).not.toHaveAttribute("data-more", "");
    expect(await sideways(page)).toBeLessThanOrEqual(0);
    // "Wrap code" still wraps it
    await page.getByRole("button", { name: "Wrap code" }).click();
    await expect.poll(() => pre.evaluate((e) => e.scrollWidth - e.clientWidth)).toBeLessThanOrEqual(0);
    await page.getByRole("button", { name: "Wrap code" }).click();
});

for (const width of [1500, 390]) {
    test(`A walkthrough in a closed fold shows its step when opened, and grows to hold a long one, ${width} wide`, async ({ page }) => {
        const sections = LESSON.sections.map((s: { heading: string; body: string }) => ({
            ...s,
            body: s.body.replace(/```walkthrough\n(.*)\n```/, (_m: string, json: string) => {
                const data = JSON.parse(json);
                data.step = [{ title: "Far down", lines: "40-41", text: "Far." }, { title: "A long one", lines: "10-34", text: "Long." }];
                return `:::deeper Walk through it\n\`\`\`walkthrough\n${JSON.stringify(data)}\n\`\`\`\n:::`;
            }),
        }));
        await open(page, width, { sections });
        const walk = page.locator(".cb-walk");
        await expect(walk).toBeHidden();
        await page.locator(".sd-deeper > summary", { hasText: "Walk through it" }).click();
        const seen = () => walk.locator("pre").evaluate((pre) => {
            const box = pre.getBoundingClientRect();
            const lit = [...pre.querySelectorAll(".sd-hl")].map((l) => l.getBoundingClientRect());
            return lit.filter((r) => r.top >= box.top - 1 && r.bottom <= box.bottom + 1).length;
        });
        const height = () => walk.locator("pre").evaluate((e) => e.getBoundingClientRect().height);
        await expect.poll(seen).toBe(2);
        const short = await height();
        await walk.getByRole("button", { name: /A long one/ }).click();
        // all twenty-five lines fit on a desktop; a phone shows as many as its limit allows
        if (width > 700) await expect.poll(seen).toBe(25);
        else await expect.poll(seen).toBeGreaterThanOrEqual(18);
        expect(await height()).toBeGreaterThan(short);
        expect(await height()).toBeLessThanOrEqual(860);
    });
}

test("On a phone the stepper's controls and lanes fit on the screen together, and its ticks are easy to hit", async ({ page }) => {
    await open(page, 390);
    const step = page.locator(".cb-step");
    expect((await step.boundingBox())!.height).toBeLessThanOrEqual(420);
    for (const tick of await step.locator(".id-ticks button").all()) {
        const box = (await tick.boundingBox())!;
        expect(Math.min(box.width, box.height)).toBeGreaterThanOrEqual(44);
    }
});

/** Every block in the article's flow, with the room left and right of it inside the article. What is inside a component
 *  (a card's words, a viewer's lines) is the component's own business and is not listed. */
const gaps = (page: Page) =>
    page.evaluate(() => {
        const art = document.querySelector(".cb-art")!.getBoundingClientRect();
        const flow = ".cb-art > :not(.cb-sec, .cb-lab), :is(.cb-sec, .cb-lab) > :not(.sd-md), :is(.cb-sec, .cb-lab) > .sd-md > *";
        return [...document.querySelectorAll<HTMLElement>(flow)]
            .filter((e) => e.getBoundingClientRect().width > 2 && getComputedStyle(e).position !== "absolute")
            .map((e) => ({ what: `${e.tagName.toLowerCase()}.${e.classList[0] ?? ""}`, left: e.getBoundingClientRect().left - art.left, right: art.right - e.getBoundingClientRect().right }));
    });

async function read(page: Page, id: string, width: number) {
    await page.setViewportSize({ width, height: width < 500 ? 844 : 950 });
    await page.goto(`/cookbook/p/${id}`);
    await expect(page.locator(".cb-head h1")).toBeVisible();
    await page.evaluate(() => document.fonts.ready.then(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)))));
}

/** Each block sits in the middle, and between them they use no more than two widths: reading, and the wider one. */
async function expectBalanced(page: Page, where: string) {
    const all = await gaps(page);
    expect(all.length, where).toBeGreaterThan(8);
    expect(all.filter((b) => Math.abs(b.left - b.right) > 1.5).map((b) => `${b.what} ${Math.round(b.left)} | ${Math.round(b.right)}`), `lopsided blocks, ${where}`).toEqual([]);
    const edges = [...new Set(all.map((b) => Math.round(b.left)))].sort((a, b) => a - b);
    expect(edges.length, `left edges ${edges.join(", ")}, ${where}`).toBeLessThanOrEqual(2);
    // two widths either match or are plainly different: a few pixels apart would look like a mistake
    if (edges.length === 2) expect(edges[1]! - edges[0]!, where).toBeGreaterThanOrEqual(32);
    expect(await sideways(page), where).toBeLessThanOrEqual(0);
}

for (const width of [2200, 1440, 390]) {
    test(`A lesson is balanced and its blocks share their edges, ${width} wide`, async ({ page }) => {
        await read(page, "bounded-channel", width);
        await expectBalanced(page, `${width}`);
        const box = async (s: string) => (await page.locator(s).first().boundingBox())!;
        const text = await box("#cb-why-put-a-limit-on-a-queue .sd-md > p");
        // reading blocks have exactly the edges of the text
        for (const s of [".cb-head", ".cb-short", ".cb-step", ".sd-tbl", ".cb-conn", ".cb-sources", ".cb-pn"]) {
            const b = await box(s);
            expect([Math.round(b.x), Math.round(b.width)], s).toEqual([Math.round(text.x), Math.round(text.width)]);
        }
        // and the blocks that need room have one pair of edges between them, with two programs side by side when it is wide
        const wide = await box(".cb-cmp");
        for (const s of [".cb-walk", ".cb-lab .cb-view"]) {
            const b = await box(s);
            expect([Math.round(b.x), Math.round(b.width)], s).toEqual([Math.round(wide.x), Math.round(wide.width)]);
        }
        const [a, b] = [await box(".cb-cmp-side.wrong"), await box(".cb-cmp-side.fixed")];
        expect(Math.round(a.y) === Math.round(b.y)).toBe(width === 2200);
        if (width > 1180) {
            // the page as a whole is in the middle too: the panels leave the same room at both sides
            const [l, r] = [await box(".cb-left"), await box(".cb-right")];
            expect(Math.abs(l.x - (width - r.x - r.width))).toBeLessThanOrEqual(1);
        }
    });

    test(`A guide page follows the same two widths, ${width} wide`, async ({ page }) => {
        await read(page, "how-to-use-this-guide", width);
        await expectBalanced(page, `${width}`);
        const line = (await page.locator(".cb-sec .sd-md > p").first().boundingBox())!.width;
        expect(line).toBeLessThanOrEqual(width >= 1900 ? 703 : 664);
    });
}

test("With the panels hidden or the text larger, a lesson stays balanced", async ({ page }) => {
    await read(page, "bounded-channel", 1440);
    for (const key of ["[", "]", "["]) {
        await page.keyboard.press(key);
        await expectBalanced(page, `after ${key}`);
    }
    await page.keyboard.press("]");
    await page.getByRole("button", { name: "Larger text" }).click();
    await expectBalanced(page, "larger text");
    await page.getByRole("button", { name: "Larger text" }).click();
});

test("Full window takes the whole window, not the article's column", async ({ page }) => {
    await read(page, "bounded-channel", 1440);
    await page.locator(".cb-lab").getByRole("button", { name: "Full window" }).click();
    const box = (await page.locator(".cb-view.full").boundingBox())!;
    expect([box.x, box.width].map(Math.round)).toEqual([10, 1420]);
});

test("A page that is not a lesson has none of this", async ({ page }) => {
    await page.goto("/cookbook/p/join-algorithms");
    await expect(page.locator(".cb-art .sd-md").first()).toBeVisible();
    await expect(page.locator(".cb-lesson, .cb-short, .cb-step, .cb-walk, .cb-cmp, .cb-conn")).toHaveCount(0);
});
