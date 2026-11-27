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

/** The widest the reading column gets at a window width. */
const column = (width: number) => 1 + 40 * (width >= 1900 ? 20 : width >= 1400 ? 18 : 17);

const sideways = (page: Page) => page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);

test("A lesson opens with In short, and its ways to the code and the parts move the keyboard there", async ({ page }) => {
    const errors = await open(page);
    const short = page.getByRole("region", { name: "In short" });
    await expect(short).toContainText("A fixture summary");
    // beside the "On this page" panel the box lists no parts of its own, and keeps to the reading column
    const parts = short.getByRole("navigation", { name: "Jump to" });
    await expect(parts).toBeHidden();
    expect((await short.boundingBox())!.width).toBeLessThanOrEqual(721);
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
        // 40 times the text size, about 80 characters; the text is larger on a wider screen
        expect(line, `text at ${width}`).toBeLessThanOrEqual(column(width));
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
            for (const el of document.querySelectorAll(".cb-art :is(.cb-short, .cb-sec, .cb-conn, .cb-margin) *")) {
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

const FLOW = ".cb-art > :not(.cb-sec, .cb-lab, .cb-margin), .cb-art :is(.cb-sec, .cb-lab) > :not(.sd-md), .cb-art :is(.cb-sec, .cb-lab) > .sd-md > *";

async function read(page: Page, id: string, width: number) {
    await page.setViewportSize({ width, height: width < 500 ? 844 : 950 });
    await page.goto(`/cookbook/p/${id}`);
    await expect(page.locator(".cb-head h1")).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready.then(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)))));
}

/** The frame is centred between the panels, and every block in the page's flow starts at its left edge and stays inside it.
 *  What is inside a component (a card's words, a viewer's lines) is the component's own business. */
async function expectOneFrame(page: Page, where: string) {
    const found = await page.evaluate((FLOW) => {
        const art = document.querySelector(".cb-art")!.getBoundingClientRect();
        const frame = document.querySelector(".cb-head")!.getBoundingClientRect();
        const blocks = [...document.querySelectorAll<HTMLElement>(FLOW)].filter((e) => e.getBoundingClientRect().width > 2);
        return {
            count: blocks.length,
            offCentre: Math.abs(frame.left - art.left - (art.right - frame.right)),
            off: blocks
                .map((e) => ({ what: `${e.tagName.toLowerCase()}.${e.classList[0] ?? ""}`, left: e.getBoundingClientRect().left - frame.left, over: e.getBoundingClientRect().right - frame.right }))
                .filter((b) => Math.abs(b.left) > 1.5 || b.over > 1.5)
                .map((b) => `${b.what} starts ${Math.round(b.left)}, past the end ${Math.round(b.over)}`),
        };
    }, FLOW);
    expect(found.count, where).toBeGreaterThan(8);
    expect(found.offCentre, where).toBeLessThanOrEqual(1.5);
    expect(found.off, where).toEqual([]);
    expect(await sideways(page), where).toBeLessThanOrEqual(0);
}

/** Programs and snippets that run past the right edge of their box. */
const clipped = (page: Page) =>
    page.evaluate(() => [...document.querySelectorAll<HTMLElement>(".cb-cmp pre, .cb-sec .sd-code pre")].filter((e) => e.clientWidth && e.scrollWidth > e.clientWidth + 1).map((e) => `${e.scrollWidth - e.clientWidth}px: ${e.textContent!.slice(0, 40)}`));

/** Characters to a line, over the first long paragraphs. */
const perLine = (page: Page) =>
    page.evaluate(() => {
        let letters = 0;
        let lines = 0;
        for (const p of [...document.querySelectorAll(".cb-sec > .sd-md > p")].slice(0, 12)) {
            const range = document.createRange();
            range.selectNodeContents(p);
            const rows = Math.round(p.getBoundingClientRect().height / Number.parseFloat(getComputedStyle(p).lineHeight));
            if (rows < 3) continue;
            letters += p.textContent!.length;
            lines += rows - 0.5; // the last line is, on average, half full
        }
        return letters / lines;
    });

const box = async (page: Page, s: string) => (await page.locator(s).first().boundingBox())!;

for (const width of [2560, 2200, 1728, 1440, 1280, 390]) {
    test(`A lesson is one frame with one left edge and no clipped code, ${width} wide`, async ({ page }) => {
        await read(page, "bounded-channel", width);
        await expectOneFrame(page, `${width}`);
        // on a phone code scrolls in its own box and fades, as tested above
        if (width > 700) expect(await clipped(page)).toEqual([]);
        const [frame, text] = [await box(page, ".cb-head"), await box(page, "#cb-why-put-a-limit-on-a-queue .sd-md > p")];
        expect(text.width).toBeLessThanOrEqual(column(width));
        if (width > 700) {
            const chars = await perLine(page);
            expect(chars, "characters to a line").toBeGreaterThanOrEqual(65);
            expect(chars, "characters to a line").toBeLessThanOrEqual(86);
        }
        // blocks that need room span the frame
        for (const s of [".cb-cmp", ".cb-walk", ".cb-lab .cb-view", ".cb-conn", ".cb-sources"]) expect(Math.round((await box(page, s)).width), s).toBe(Math.round(frame.width));
        // two programs are side by side only where both fit
        const [a, b] = [await box(page, ".cb-cmp-side.wrong"), await box(page, ".cb-cmp-side.fixed")];
        expect(Math.round(a.y) === Math.round(b.y)).toBe(width >= 2200);
        const beside = page.getByRole("complementary", { name: "Beside the text" });
        if (width >= 1728) {
            // the column beside the text ends with the frame and starts clear of the text
            const card = await box(page, ".cb-m-card");
            expect(Math.abs(card.x + card.width - (frame.x + frame.width))).toBeLessThanOrEqual(1.5);
            expect(card.x - (text.x + text.width)).toBeGreaterThanOrEqual(40);
            expect(card.width).toBeGreaterThanOrEqual(339);
            expect(card.width).toBeLessThanOrEqual(481);
        } else await expect(beside).toBeHidden();
        if (width > 1180) {
            // the panels are at the page's edges, in line with the bar above them
            const [crumb, left, right] = [await box(page, ".cb-crumb"), await box(page, ".cb-left"), await box(page, ".cb-right")];
            expect([Math.round(crumb.x), Math.round(left.x), Math.round(width - right.x - right.width)]).toEqual([20, 20, 20]);
        }
    });
}

for (const [width, most] of [[2200, 0.05], [2560, 0.18]] as const) {
    test(`On a ${width} wide screen the frame leaves little unused`, async ({ page }) => {
        await read(page, "bounded-channel", width);
        const [art, frame] = [await box(page, ".cb-art"), await box(page, ".cb-head")];
        expect(frame.width).toBeGreaterThanOrEqual(1240);
        expect((art.width - frame.width) / width).toBeLessThanOrEqual(most);
        // and the frame is used to its right edge at the top of the page, by something and not by an empty column
        const card = (await page.locator(".cb-m-card").first().boundingBox({ timeout: 3000 }))!;
        expect(Math.abs(card.x + card.width - (frame.x + frame.width))).toBeLessThanOrEqual(1.5);
        expect(card.y + card.height).toBeLessThan(950);
    });
}

test("A title is on one line wherever it fits the frame", async ({ page }) => {
    for (const width of [1440, 1728, 2200, 2560]) {
        await read(page, "bounded-channel", width);
        const h1 = await page.locator(".cb-head h1").evaluate((e) => ({ height: e.getBoundingClientRect().height, line: Number.parseFloat(getComputedStyle(e).lineHeight) }));
        expect(h1.height, `${width}`).toBeLessThan(h1.line * 1.5);
    }
});

test("The cards beside the text stay in sight and never meet a block or each other", async ({ page }) => {
    await read(page, "bounded-channel", 2200);
    await expect(page.locator(".cb-m-card:visible")).toHaveCount(5);
    await expect(page.locator(".cb-m-card h3")).toHaveText(["Know first", "Practise", "Where it comes back", "In real projects", "Sources"]);
    const result = await page.evaluate(async (FLOW) => {
        const frame = () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
        const meet = (a: DOMRect, b: DOMRect) => a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1;
        const bad = new Set<string>();
        let stuck = 0;
        for (let y = 0; y < document.documentElement.scrollHeight; y += 250) {
            scrollTo({ top: y, behavior: "instant" });
            await frame();
            const cards = [...document.querySelectorAll<HTMLElement>(".cb-m-card")].filter((c) => c.offsetWidth);
            for (const c of cards) {
                const at = c.getBoundingClientRect();
                const top = Number.parseFloat(c.style.top);
                if (Math.abs(at.top - top) < 1) stuck++;
                for (const e of document.querySelectorAll(FLOW)) if (e.getBoundingClientRect().width > 2 && meet(at, e.getBoundingClientRect())) bad.add(`${c.textContent!.slice(0, 20)} meets ${e.className}`);
                for (const other of cards) if (other !== c && meet(at, other.getBoundingClientRect())) bad.add(`${c.textContent!.slice(0, 20)} meets a card`);
            }
        }
        return { bad: [...bad], stuck };
    }, FLOW);
    expect(result.bad).toEqual([]);
    expect(result.stuck, "times a card was held in place while the page moved").toBeGreaterThan(3);
});

test("An exact exercise is not offered beside the text", async ({ page }) => {
    await open(page, 2200);
    const beside = page.getByRole("complementary", { name: "Beside the text" });
    await expect(beside.getByRole("link", { name: "The other page" })).toHaveCount(1);
    await expect(beside.getByText("A problem")).toHaveCount(0);
});

for (const width of [2200, 1440, 390]) {
    test(`A guide with nothing to put beside the text has one column and no empty one, ${width} wide`, async ({ page }) => {
        await read(page, "how-to-use-this-guide", width);
        await expectOneFrame(page, `${width}`);
        await expect(page.locator(".cb-margin")).toHaveCount(0);
        const [frame, text] = [await box(page, ".cb-head"), await box(page, ".cb-sec .sd-md > p")];
        expect(text.width).toBeLessThanOrEqual(column(width));
        expect(frame.width - text.width, "room beside the text inside the frame").toBeLessThanOrEqual(width >= 1900 ? 122 : 220);
    });
}

test("With the panels hidden or the text larger, a lesson stays in one frame with nothing clipped", async ({ page }) => {
    await read(page, "bounded-channel", 1440);
    for (const key of ["[", "]", "["]) {
        await page.keyboard.press(key);
        await expectOneFrame(page, `after ${key}`);
        await expect.poll(() => clipped(page), `after ${key}`).toEqual([]);
    }
    await page.keyboard.press("]");
    await page.getByRole("button", { name: "Larger text" }).click();
    await expectOneFrame(page, "larger text");
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
