import assert from "node:assert/strict";
import { test } from "node:test";
import { lineRange } from "./pages/cookbook/lines.ts";

test("a walkthrough step names one line or a range, both ends included", () => {
    assert.deepEqual(lineRange("10-18"), [10, 18]);
    assert.deepEqual(lineRange(" 12 "), [12, 12]);
    assert.deepEqual(lineRange("3 - 4"), [3, 4]);
});
