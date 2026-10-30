// The animations of the Idea tabs (docs/DSA_IDEA_TAB.md): each is a small simulator that turns an input into the frames the page steps through.
// Plain functions with no React, so they can be tested with `node --test web/src/ideaDemos.test.ts`. A demo's name here must also be in
// `IDEA_DEMOS` in `crates/content/src/dsa.rs`, which is what a content file may ask for.

/** What the learner can change: a row of numbers and a window size, or a string of letters. */
export type DemoInput = { type: "numbers"; items: string[]; k: number } | { type: "letters"; items: string[] };

export interface Reading {
    label: string;
    value: string | number;
    /** When set, the reading is drawn as chips (the members of a set). */
    chips?: string[];
}

export type Work = { kind: "fixed"; brute: number; slide: number } | { kind: "variable"; enters: number; leaves: number; n: number };

/** One step: the window is `[l, r)`; `probe` is the right end being looked at; `enter`, `leave` and `dup` mark cells; `lines` are the 1-based code lines that run. */
export interface Frame {
    l: number;
    r: number;
    probe?: number;
    enter?: number;
    leave?: number;
    dup?: number;
    lines: number[];
    read: Reading[];
    work: Work;
    say: string;
    /** The last step: the answer, shown as a badge. */
    result?: string;
}

export interface Demo {
    title: string;
    code: string[];
    initial: DemoInput;
    /** Reads what was typed; null when it is not a valid input. */
    parse: (text: string, k: string) => DemoInput | null;
    simulate: (input: DemoInput) => Frame[];
}

const MAX_CELLS = 10;

const fixedCode = [
    "def max_sum(nums, k):",
    "    total = sum(nums[:k])  # the first window",
    "    best = total",
    "    for i in range(k, len(nums)):",
    "        total += nums[i]  # enters",
    "        total -= nums[i - k]  # leaves",
    "        best = max(best, total)",
    "    return best",
];

const variableCode = [
    "def longest_unique(s):",
    "    seen = set()  # the summary",
    "    left = best = 0",
    "    for right in range(len(s)):",
    "        while s[right] in seen:  # broken?",
    "            seen.remove(s[left])",
    "            left += 1  # shrink",
    "        seen.add(s[right])",
    "        best = max(best, right - left + 1)",
    "    return best",
];

function simulateFixed(input: DemoInput): Frame[] {
    if (input.type !== "numbers") return [];
    const a = input.items.map(Number);
    const k = input.k;
    if (k < 1 || k > a.length) return [];
    let sum = a.slice(0, k).reduce((x, y) => x + y, 0);
    let best = sum;
    let slide = k;
    let brute = k;
    const reading = (): Reading[] => [{ label: "WINDOW SUM", value: sum }, { label: "BEST SO FAR", value: best }];
    const out: Frame[] = [
        { l: 0, r: k, lines: [2, 3], read: reading(), work: { kind: "fixed", brute, slide }, say: `Start with the first ${k} items: ${a.slice(0, k).join(" + ")} = ${sum}. This is the only time ${k} numbers are added together.` },
    ];
    for (let j = k; j < a.length; j++) {
        const before = sum;
        sum += a[j]! - a[j - k]!;
        const better = sum > best;
        best = Math.max(best, sum);
        brute += k;
        slide += 2;
        out.push({
            l: j - k + 1,
            r: j + 1,
            probe: j,
            enter: j,
            leave: j - k,
            lines: [5, 6, 7],
            read: reading(),
            work: { kind: "fixed", brute, slide },
            say: `${a[j]} comes in, ${a[j - k]} goes out: ${before} + ${a[j]} − ${a[j - k]} = ${sum}.${better ? " A new best." : ""}`,
        });
    }
    const last = out[out.length - 1]!;
    out.push({ l: last.l, r: last.r, lines: [8], read: last.read, work: last.work, result: String(best), say: `Done. The answer is ${best}. Each item came in once and went out once: ${slide} additions, where recomputing every window would take ${brute}.` });
    return out;
}

function simulateVariable(input: DemoInput): Frame[] {
    if (input.type !== "letters") return [];
    const s = input.items;
    if (!s.length) return [];
    let l = 0;
    let best = 0;
    let enters = 0;
    let leaves = 0;
    const set = new Set<string>();
    const reading = (): Reading[] => [{ label: "IN THE SET", value: [...set].join(""), chips: [...set] }, { label: "BEST LENGTH", value: best }];
    const work = (extraLeave = 0): Work => ({ kind: "variable", enters, leaves: leaves + extraLeave, n: s.length });
    const out: Frame[] = [{ l: 0, r: 0, lines: [2, 3], read: reading(), work: work(), say: "Start with an empty window and an empty set. The set is the summary: it says which letters are inside." }];
    for (let r = 0; r < s.length; r++) {
        while (set.has(s[r]!)) {
            out.push({ l, r, probe: r, dup: s.indexOf(s[r]!, l), leave: l, lines: [5, 6, 7], read: reading(), work: work(1), say: `“${s[r]}” is already in the window, so the window is broken. Drop “${s[l]}” from the left.` });
            set.delete(s[l]!);
            l++;
            leaves++;
        }
        set.add(s[r]!);
        enters++;
        const len = r - l + 1;
        const better = len > best;
        best = Math.max(best, len);
        out.push({ l, r: r + 1, probe: r, enter: r, lines: [5, 8, 9], read: reading(), work: work(), say: `Take in “${s[r]}”. The window “${s.slice(l, r + 1).join("")}” has no repeats, length ${len}.${better ? " A new best." : ""}` });
    }
    const last = out[out.length - 1]!;
    out.push({ l: last.l, r: last.r, lines: [10], read: last.read, work: last.work, result: String(best), say: `Done. The answer is ${best}. The right end only moved forward and the left end never moved back, so the inner loop does not make it slower: ${enters} in, ${leaves} out.` });
    return out;
}

export const DEMOS: Record<string, Demo> = {
    "window-fixed": {
        title: "Fixed size · best sum",
        code: fixedCode,
        initial: { type: "numbers", items: ["2", "1", "5", "1", "3", "2", "6", "1"], k: 4 },
        parse: (text, k) => {
            const items = text.split(",").map((x) => x.trim()).filter(Boolean);
            const size = Number(k);
            const ok = items.length >= 2 && items.length <= MAX_CELLS && items.every((x) => /^-?\d{1,3}$/.test(x)) && Number.isInteger(size) && size >= 1 && size <= items.length;
            return ok ? { type: "numbers", items, k: size } : null;
        },
        simulate: simulateFixed,
    },
    "window-variable": {
        title: "Variable · longest without a repeat",
        code: variableCode,
        initial: { type: "letters", items: [..."abcabcbb"] },
        parse: (text) => {
            const s = text.toLowerCase().replace(/[^a-z]/g, "");
            return s.length >= 2 && s.length <= 14 ? { type: "letters", items: [...s] } : null;
        },
        simulate: simulateVariable,
    },
};
