// Run with: node --test web/src/ansi.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import { ansiCss, ansiLines, lineText, stripAnsi } from "./workspace/ansiText.ts";

const E = "\x1b";
const runs = (s: string) => ansiLines(s).flat().map((r) => [r.text, r.style.color ?? "", !!r.style.bold]);

// What rustc 1.98 sends in `rendered` (captured from json-diagnostic-rendered-ansi).
const RUSTC = `${E}[1m${E}[91merror[E0502]${E}[0m${E}[1m: cannot borrow${E}[0m\n ${E}[1m${E}[94m--> ${E}[0msrc/lib.rs:1:56`;

test("rustc's rendering keeps its colours and reads as plain text", () => {
  assert.equal(stripAnsi(RUSTC), "error[E0502]: cannot borrow\n --> src/lib.rs:1:56");
  assert.deepEqual(runs(RUSTC).slice(0, 2), [["error[E0502]", "var(--ansi-bright-red)", true], [": cannot borrow", "", true]]);
});

test("text stored before colour is unchanged", () => {
  const old = "error[E0502]: cannot borrow\n  |\n7 |\tfor t in [1] { }\n";
  assert.equal(stripAnsi(old), old);
  assert.deepEqual(ansiLines(old).flat().map((r) => ansiCss(r.style)), [null, null, null]);
});

test("256-colour and true colour map to theme tokens, backgrounds are consumed", () => {
  assert.equal(runs(`${E}[38;5;9mx`)[0]![1], "var(--ansi-bright-red)");
  assert.equal(runs(`${E}[38;5;12mx`)[0]![1], "var(--ansi-bright-blue)");
  assert.equal(runs(`${E}[38;5;208mx`)[0]![1], "var(--ansi-yellow)");
  assert.equal(runs(`${E}[38;2;0;200;0mx`)[0]![1], "var(--ansi-green)");
  assert.deepEqual(runs(`${E}[48;5;31;1mx`), [["x", "", true]]);
  assert.deepEqual(runs(`${E}[38;5;999mx`), [["x", "", false]]);
});

test("cursor moves, links, titles and control characters are removed", () => {
  const hostile = `a${E}[2J${E}[10;10Hb${E}]8;;http://evil.example\x07link${E}]8;;\x07${E}]0;title${E}\\c${E}(Bd\x07\x08\x00e${E}7f${E}[?25l\x9b31mg`;
  assert.equal(stripAnsi(hostile), "ablinkcdefg");
});

test("markup stays text, and no input reaches CSS", () => {
  const s = `<img src=x onerror=alert(1)> ${E}[31;url(javascript:1)m"`;
  assert.ok(stripAnsi(s).startsWith("<img src=x onerror=alert(1)> "));
  for (const r of ansiLines(`${E}[38;2;999;-1;zzmq`).flat()) if (r.style.color) assert.match(r.style.color, /^var\(--ansi-[a-z-]+\)$/);
});

test("output cut inside a sequence leaves nothing of it, and keeps the next line", () => {
  for (const cut of [E, `${E}[`, `${E}[3`, `${E}[38;5;`, `${E}]8;;http://x`])
    assert.equal(stripAnsi(`ok ${cut}\n… output cut`), "ok \n… output cut", JSON.stringify(cut));
});

test("a style carries over lines until reset; a carriage return overwrites", () => {
  const lines = ansiLines(`${E}[32mone\ntwo${E}[0m\n10%\r100%\r\nend\r`);
  assert.deepEqual(lines.map(lineText), ["one", "two", "100%", "end"]);
  assert.equal(lines[1]![0]!.style.color, "var(--ansi-green)");
});
