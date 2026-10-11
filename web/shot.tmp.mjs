import { chromium } from "@playwright/test";
const [concept, out, fig = "0", ...times] = process.argv.slice(2);
const b = await chromium.launch({ channel: "chrome" });
const p = await b.newPage({ viewport: { width: 1100, height: 900 } });
await p.goto(`http://127.0.0.1:8791/courses/bustub/concept/${concept}`);
await p.waitForSelector(".cx-diagram svg");
const f = p.locator(".cx-diagram").nth(Number(fig));
await f.scrollIntoViewIfNeeded();
let prev = 0;
for (const [i, t] of times.entries()) { await p.waitForTimeout(Number(t) - prev); prev = Number(t); await f.screenshot({ path: `${out}-${i}.png` }); }
await b.close();
