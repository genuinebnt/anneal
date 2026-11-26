import { readFileSync } from "node:fs";
import type { APIRequestContext } from "@playwright/test";

/** A series part with a quick reference solution: a spec can solve it to reach the completion view. */
export const PART = "l1-raii-span-guard";
export const SOLUTION = () => readFileSync(new URL("../../content/tracks/l1-ownership-moves/problems/raii-span-guard/solution.rs", import.meta.url), "utf8");

/** Submits the reference solution, as the Submit button would. It adds a run to the test database. */
export async function solvePart(request: APIRequestContext) {
    const res = await request.post(`/api/problems/${PART}/submit`, { data: { code: SOLUTION() }, timeout: 180_000 });
    if (!res.ok() || !(await res.json()).attempt.solved) throw new Error(`could not solve ${PART}: ${res.status()}`);
}
