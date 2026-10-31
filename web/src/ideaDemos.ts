// The animations of the Idea tabs (docs/DSA_IDEA_TAB.md): each is a small simulator that turns an input into the frames the page steps through.
// Plain functions with no React, so they can be tested with `node --test web/src/ideaDemos.test.ts`. A demo's name here must also be in
// `IDEA_DEMOS` in `crates/content/src/dsa.rs`, which is what a content file may ask for.

/** What the learner can change: a row of cells and, for some demos, one number (a window size, a target). */
export interface DemoInput {
    items: string[];
    param?: number;
}

export interface Reading {
    label: string;
    value: string | number;
    /** When set, the reading is drawn as chips (the members of a set). */
    chips?: string[];
}

/** A marker over a cell: `r` copper above, `l` blue below, `m` green above (a middle). Markers on one cell stack. */
export interface Pointer {
    at: number;
    label: string;
    tone: "l" | "r" | "m";
    below?: boolean;
}

/** What a method saves, as bars: each is `value` of `of`. */
export interface Work {
    title: string;
    /** `ratio` shows it as "value/of" (a count out of a known total) and not just the value. */
    bars: { label: string; value: number; of: number; tone: "good" | "bad"; ratio?: boolean }[];
    note: string;
}

/** A node of a drawn graph or tree: `x` and `y` are 0..1 in the picture. `tone` is how it looks now. */
export interface GNode {
    id: number;
    x: number;
    y: number;
    label: string;
    tone?: "cur" | "seen" | "done" | "hit" | "new" | "front" | "dim";
    /** A small note under the node (a distance, a count). */
    sub?: string;
}

export interface GEdge {
    a: number;
    b: number;
    tone?: "on" | "new" | "cut" | "hit";
    directed?: boolean;
    /** A weight, drawn on the edge. */
    w?: string;
}

/** A table of cells (a DP table, a board, a matrix). `marks` colour single cells: `cur` is being written, `read` is being read from. */
export interface GridMark {
    r: number;
    c: number;
    tone: "cur" | "read" | "done" | "hit" | "bad" | "new";
}

/** Bars on a number line: each `from`..`to` on its own lane (`lane`), with a marker (a sweep) optionally. */
export interface Range {
    from: number;
    to: number;
    label: string;
    lane: number;
    tone?: "cur" | "keep" | "drop" | "merged" | "done";
}

/** One step. The window `win` is `[l, r)` and is drawn as a brace; `dim` cells are ruled out; `cur` cells are being looked at; `hit` cells are the answer;
 *  `enter`, `leave` and `dup` mark a cell coming in, going out and repeating; `cells` replaces the values (an array being written in place);
 *  `stack` draws a second structure beside the row; `lines` are the 1-based code lines that run. */
export interface Frame {
    win?: [number, number];
    pointers: Pointer[];
    cells?: string[];
    dim?: number[];
    cur?: number[];
    hit?: number[];
    /** Cells answered yes (green) and no (red). */
    ok?: number[];
    nope?: number[];
    /** The arrows of a linked list: `from` points to `to` (-1 is null). `hot` marks the one just changed. */
    links?: { from: number; to: number; hot?: boolean }[];
    enter?: number;
    leave?: number;
    dup?: number;
    stack?: { label: string; items: string[]; top?: "push" | "pop" };
    /** A drawn graph or tree beside (or instead of) the row. */
    graph?: { nodes: GNode[]; edges: GEdge[]; w?: number; h?: number };
    /** A table: `cells[r][c]`, with optional labels and coloured cells. */
    grid?: { cells: string[][]; rowLabels?: string[]; colLabels?: string[]; marks?: GridMark[]; small?: boolean };
    /** Intervals on a line over `axis` = [min, max]; `marker` is a vertical sweep line. */
    ranges?: { bars: Range[]; axis: [number, number]; marker?: number; lanes: number };
    /** A queue drawn as a row, front on the left. */
    queue?: { label: string; items: string[]; top?: "push" | "pop" };
    /** A hash map beside the row: `new` is the entry just written, `hit` the one a lookup found. */
    map?: { label: string; rows: { k: string; v: string; tone?: "new" | "hit" }[] };
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
    /** Draw each cell in a colour of its letter. */
    letters?: boolean;
    /** Typed as one string, without commas ("FFTT"). */
    compact?: boolean;
    /** Cells set wider apart, so an arrow fits between them (linked lists). */
    wide?: boolean;
    /** Each cell holds a short word, so the cells are wider and the type smaller. */
    words?: boolean;
    /** No row of cells: the picture is a graph, a grid or ranges. */
    noRow?: boolean;
    /** The labels of what can be typed: the cells, and the number if there is one. */
    fields: { text: string; param?: { label: string; min: number; max: number } };
    /** Reads what was typed; null when it is not a valid input. */
    parse: (text: string, param: string) => DemoInput | null;
    simulate: (input: DemoInput) => Frame[];
}

const MAX_CELLS = 10;
const numbers = (text: string) => text.split(",").map((x) => x.trim()).filter(Boolean);
const smallInts = (xs: string[]) => xs.every((x) => /^-?\d{1,3}$/.test(x));

// ---------------------------------------------------------------- sliding window

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

/** The two ends of a window: L under its left cell, R over the cell being looked at (or its right end). */
const windowPointers = (l: number, r: number, probe?: number): Pointer[] => {
    const right = probe ?? (r > l ? r - 1 : null);
    return [{ at: l, label: "L", tone: "l", below: true }, ...(right === null ? [] : [{ at: right, label: "R", tone: "r" as const }])];
};

function simulateFixed(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const k = input.param ?? 0;
    if (k < 1 || k > a.length) return [];
    let sum = a.slice(0, k).reduce((x, y) => x + y, 0);
    let best = sum;
    let slide = k;
    let brute = k;
    const reading = (): Reading[] => [{ label: "WINDOW SUM", value: sum }, { label: "BEST SO FAR", value: best }];
    const work = (): Work => ({
        title: "ADDITIONS SO FAR",
        bars: [{ label: "Recompute each window", value: brute, of: Math.max(brute, 1), tone: "bad" }, { label: "Slide", value: slide, of: Math.max(brute, 1), tone: "good" }],
        note: "Recomputing adds k numbers for every window. Sliding adds one and takes one away. Make k bigger above and the gap grows.",
    });
    const out: Frame[] = [
        { win: [0, k], pointers: windowPointers(0, k), lines: [2, 3], read: reading(), work: work(), say: `Start with the first ${k} items: ${a.slice(0, k).join(" + ")} = ${sum}. This is the only time ${k} numbers are added together.` },
    ];
    for (let j = k; j < a.length; j++) {
        const before = sum;
        sum += a[j]! - a[j - k]!;
        const better = sum > best;
        best = Math.max(best, sum);
        brute += k;
        slide += 2;
        out.push({
            win: [j - k + 1, j + 1],
            pointers: windowPointers(j - k + 1, j + 1, j),
            enter: j,
            leave: j - k,
            lines: [5, 6, 7],
            read: reading(),
            work: work(),
            say: `${a[j]} comes in, ${a[j - k]} goes out: ${before} + ${a[j]} − ${a[j - k]} = ${sum}.${better ? " A new best." : ""}`,
        });
    }
    const last = out[out.length - 1]!;
    out.push({ ...last, pointers: windowPointers(last.win![0], last.win![1]), enter: undefined, leave: undefined, lines: [8], result: String(best), say: `Done. The answer is ${best}. Each item came in once and went out once: ${slide} additions, where recomputing every window would take ${brute}.` });
    return out;
}

function simulateVariable(input: DemoInput): Frame[] {
    const s = input.items;
    if (!s.length) return [];
    let l = 0;
    let best = 0;
    let enters = 0;
    let leaves = 0;
    const set = new Set<string>();
    const reading = (): Reading[] => [{ label: "IN THE SET", value: [...set].join(""), chips: [...set] }, { label: "BEST LENGTH", value: best }];
    const work = (extraLeave = 0): Work => ({
        title: "EACH LETTER COMES IN ONCE AND GOES OUT AT MOST ONCE",
        bars: [{ label: "Came in", value: enters, of: s.length, tone: "good", ratio: true }, { label: "Went out", value: leaves + extraLeave, of: s.length, tone: "bad", ratio: true }],
        note: "The inner while loop looks like extra work, but the left end never moves back, so it can never move more than n times in total. That is why this is O(n).",
    });
    const out: Frame[] = [{ win: [0, 0], pointers: windowPointers(0, 0), lines: [2, 3], read: reading(), work: work(), say: "Start with an empty window and an empty set. The set is the summary: it says which letters are inside." }];
    for (let r = 0; r < s.length; r++) {
        while (set.has(s[r]!)) {
            out.push({ win: [l, r], pointers: windowPointers(l, r, r), dup: s.indexOf(s[r]!, l), leave: l, lines: [5, 6, 7], read: reading(), work: work(1), say: `“${s[r]}” is already in the window, so the window is broken. Drop “${s[l]}” from the left.` });
            set.delete(s[l]!);
            l++;
            leaves++;
        }
        set.add(s[r]!);
        enters++;
        const len = r - l + 1;
        const better = len > best;
        best = Math.max(best, len);
        out.push({ win: [l, r + 1], pointers: windowPointers(l, r + 1, r), enter: r, lines: [5, 8, 9], read: reading(), work: work(), say: `Take in “${s[r]}”. The window “${s.slice(l, r + 1).join("")}” has no repeats, length ${len}.${better ? " A new best." : ""}` });
    }
    const last = out[out.length - 1]!;
    out.push({ ...last, pointers: windowPointers(last.win![0], last.win![1]), enter: undefined, lines: [10], result: String(best), say: `Done. The answer is ${best}. The right end only moved forward and the left end never moved back, so the inner loop does not make it slower: ${enters} in, ${leaves} out.` });
    return out;
}

// ---------------------------------------------------------------- two pointers

const oppositeCode = [
    "def pair_sum(nums, target):  # nums is sorted",
    "    lo, hi = 0, len(nums) - 1",
    "    while lo < hi:",
    "        s = nums[lo] + nums[hi]",
    "        if s == target:",
    "            return [lo, hi]",
    "        if s < target:",
    "            lo += 1  # too small: only a bigger left helps",
    "        else:",
    "            hi -= 1  # too big: only a smaller right helps",
    "    return []",
];

function simulateOpposite(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const target = input.param ?? 0;
    const n = a.length;
    if (n < 2) return [];
    const all = (n * (n - 1)) / 2;
    let lo = 0;
    let hi = n - 1;
    let looked = 0;
    const ruledOut = () => [...a.keys()].filter((i) => i < lo || i > hi);
    const ptrs = (): Pointer[] => (lo === hi ? [{ at: lo, label: "lo = hi", tone: "l", below: true }] : [{ at: lo, label: "lo", tone: "l", below: true }, { at: hi, label: "hi", tone: "r" }]);
    const work = (): Work => ({
        title: "PAIRS LOOKED AT",
        bars: [{ label: "Try every pair", value: all, of: all, tone: "bad" }, { label: "Two pointers", value: looked, of: all, tone: "good", ratio: true }],
        note: `Each step rules out a whole row of pairs for good, so at most n − 1 = ${n - 1} pairs are ever looked at, not ${all}.`,
    });
    const out: Frame[] = [{ pointers: ptrs(), lines: [2], read: [{ label: "LEFT", value: a[lo]! }, { label: "RIGHT", value: a[hi]! }, { label: "TARGET", value: target }], work: work(), say: `Look for two numbers that add to ${target}. The row is sorted, so start with the smallest and the largest.` }];
    while (lo < hi) {
        const s = a[lo]! + a[hi]!;
        looked++;
        const read: Reading[] = [{ label: "SUM", value: s }, { label: "TARGET", value: target }];
        if (s === target) {
            out.push({ pointers: ptrs(), dim: ruledOut(), hit: [lo, hi], lines: [4, 5, 6], read, work: work(), result: `${a[lo]} + ${a[hi]}`, say: `${a[lo]} + ${a[hi]} = ${s}. Found, at indexes ${lo} and ${hi}, after looking at ${looked} pair${looked === 1 ? "" : "s"}.` });
            return out;
        }
        if (s < target) {
            out.push({ pointers: ptrs(), dim: ruledOut(), cur: [lo, hi], lines: [4, 7, 8], read, work: work(), say: `${a[lo]} + ${a[hi]} = ${s}, too small. ${a[lo]} can't pair with anything smaller than ${a[hi]}, so it can never be part of the answer. Move the left pointer in.` });
            lo++;
        } else {
            out.push({ pointers: ptrs(), dim: ruledOut(), cur: [lo, hi], lines: [4, 9, 10], read, work: work(), say: `${a[lo]} + ${a[hi]} = ${s}, too big. ${a[hi]} can't pair with anything bigger than ${a[lo]}, so it can never be part of the answer. Move the right pointer in.` });
            hi--;
        }
    }
    out.push({ pointers: [], dim: [...a.keys()], lines: [11], read: [{ label: "SUM", value: "–" }, { label: "TARGET", value: target }], work: work(), result: "none", say: `The pointers met, and no pair added to ${target}. ${looked} pairs were enough to be sure.` });
    return out;
}

const sameCode = [
    "def remove_duplicates(nums):  # nums is sorted",
    "    slow = 0  # the last unique value",
    "    for fast in range(1, len(nums)):",
    "        if nums[fast] != nums[slow]:  # a new value",
    "            slow += 1",
    "            nums[slow] = nums[fast]  # write it",
    "    return slow + 1",
];

function simulateSame(input: DemoInput): Frame[] {
    const a = [...input.items];
    const n = a.length;
    if (n < 2) return [];
    let slow = 0;
    let writes = 0;
    const work = (fast: number): Work => ({
        title: "READS AND WRITES",
        bars: [{ label: "Items read", value: fast + 1, of: n, tone: "good", ratio: true }, { label: "Items written", value: writes, of: n, tone: "bad", ratio: true }],
        note: "The fast pointer reads every item once. The slow pointer only moves when there is something new to keep, so nothing is ever moved twice.",
    });
    const read = (): Reading[] => [{ label: "KEPT SO FAR", value: a.slice(0, slow + 1).join(" ") }, { label: "COUNT", value: slow + 1 }];
    const ptrs = (fast: number | null): Pointer[] => [{ at: slow, label: "slow", tone: "l", below: true }, ...(fast === null ? [] : [{ at: fast, label: "fast", tone: "r" as const }])];
    const out: Frame[] = [{ win: [0, 1], pointers: ptrs(1), cells: [...a], lines: [2], read: read(), work: work(0), say: "Keep the first value. The slow pointer marks the end of the part that is already clean; the fast pointer reads ahead." }];
    for (let fast = 1; fast < n; fast++) {
        if (a[fast] !== a[slow]) {
            slow++;
            a[slow] = a[fast]!;
            writes++;
            out.push({ win: [0, slow + 1], pointers: ptrs(fast), cells: [...a], enter: slow, cur: [fast], lines: [3, 4, 5, 6], read: read(), work: work(fast), say: `${a[fast]} is new, so the slow pointer steps on and the value is written there.` });
        } else {
            out.push({ win: [0, slow + 1], pointers: ptrs(fast), cells: [...a], dup: fast, lines: [3, 4], read: read(), work: work(fast), say: `${a[fast]} is the same as the last value kept, so it is skipped: the slow pointer stays.` });
        }
    }
    out.push({ win: [0, slow + 1], pointers: ptrs(null), cells: [...a], dim: [...a.keys()].filter((i) => i > slow), lines: [7], read: read(), work: work(n - 1), result: String(slow + 1), say: `Done: ${slow + 1} unique values, kept at the front, in one pass and with no extra array.` });
    return out;
}

// ---------------------------------------------------------------- binary search

const findCode = [
    "def search(nums, target):  # nums is sorted",
    "    lo, hi = 0, len(nums) - 1",
    "    while lo <= hi:",
    "        mid = (lo + hi) // 2",
    "        if nums[mid] == target:",
    "            return mid",
    "        if nums[mid] < target:",
    "            lo = mid + 1  # everything up to mid is too small",
    "        else:",
    "            hi = mid - 1  # everything from mid is too big",
    "    return -1",
];

function simulateFind(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const target = input.param ?? 0;
    const n = a.length;
    if (n < 2) return [];
    let lo = 0;
    let hi = n - 1;
    let looks = 0;
    const dim = () => [...a.keys()].filter((i) => i < lo || i > hi);
    const ptrs = (mid?: number): Pointer[] => [{ at: lo, label: "lo", tone: "l", below: true }, { at: hi, label: "hi", tone: "r" }, ...(mid === undefined ? [] : [{ at: mid, label: "mid", tone: "m" as const }])];
    const work = (): Work => ({
        title: "ITEMS LOOKED AT",
        bars: [{ label: "Look at every item", value: n, of: n, tone: "bad" }, { label: "Binary search", value: looks, of: n, tone: "good", ratio: true }],
        note: `Each look throws away half of what is left, so ${n} items need at most ${Math.floor(Math.log2(n)) + 1} looks. A million items need 20.`,
    });
    const range = (): Reading[] => [{ label: "LEFT", value: lo }, { label: "RIGHT", value: hi }, { label: "TARGET", value: target }];
    const out: Frame[] = [{ pointers: ptrs(), lines: [2], read: range(), work: work(), say: `Look for ${target}. The row is sorted, so it can only be at lo, hi or between them. Both ends start at the ends.` }];
    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        looks++;
        const v = a[mid]!;
        if (v === target) {
            out.push({ pointers: ptrs(mid), dim: dim(), hit: [mid], lines: [4, 5, 6], read: [{ label: "MIDDLE", value: v }, { label: "TARGET", value: target }], work: work(), result: `index ${mid}`, say: `${v} is the target. Found at index ${mid} after ${looks} look${looks === 1 ? "" : "s"}.` });
            return out;
        }
        if (v < target) {
            out.push({ pointers: ptrs(mid), dim: dim(), cur: [mid], lines: [4, 7, 8], read: [{ label: "MIDDLE", value: v }, { label: "TARGET", value: target }], work: work(), say: `${v} < ${target}, so ${v} and everything before it is too small. Throw that half away: lo moves to ${mid + 1}.` });
            lo = mid + 1;
        } else {
            out.push({ pointers: ptrs(mid), dim: dim(), cur: [mid], lines: [4, 9, 10], read: [{ label: "MIDDLE", value: v }, { label: "TARGET", value: target }], work: work(), say: `${v} > ${target}, so ${v} and everything after it is too big. Throw that half away: hi moves to ${mid - 1}.` });
            hi = mid - 1;
        }
    }
    out.push({ pointers: [], dim: [...a.keys()], lines: [11], read: [{ label: "TARGET", value: target }, { label: "LOOKS", value: looks }], work: work(), result: "not found", say: `lo passed hi: nothing is left to look at, so ${target} is not in the row. ${looks} looks were enough to be sure.` });
    return out;
}

const firstTrueCode = [
    "def first_true(lo, hi, ok):  # ok(x): False ... then True",
    "    while lo < hi:",
    "        mid = (lo + hi) // 2",
    "        if ok(mid):",
    "            hi = mid  # mid could be the first True: keep it",
    "        else:",
    "            lo = mid + 1  # mid is too early: drop it",
    "    return lo",
];

function simulateFirstTrue(input: DemoInput): Frame[] {
    const a = input.items;
    const n = a.length;
    if (n < 2) return [];
    let lo = 0;
    let hi = n - 1;
    let asked = 0;
    const known = new Map<number, string>();
    const shown = () => a.map((_, i) => known.get(i) ?? "?");
    const dim = () => [...a.keys()].filter((i) => i < lo || i > hi);
    const tint = () => ({ ok: [...known].filter(([, v]) => v === "T").map(([i]) => i), nope: [...known].filter(([, v]) => v === "F").map(([i]) => i) });
    const ptrs = (mid?: number): Pointer[] => (lo === hi ? [{ at: lo, label: "lo = hi", tone: "l", below: true }] : [{ at: lo, label: "lo", tone: "l", below: true }, { at: hi, label: "hi", tone: "r" }, ...(mid === undefined ? [] : [{ at: mid, label: "mid", tone: "m" as const }])]);
    const work = (): Work => ({
        title: "QUESTIONS ASKED",
        bars: [{ label: "Ask about every index", value: n, of: n, tone: "bad" }, { label: "Binary search", value: asked, of: n, tone: "good", ratio: true }],
        note: "Each answer rules out half of the range for good, so the first True is found in about log₂ n questions. When asking is slow, that is the difference between seconds and years.",
    });
    const read = (): Reading[] => [{ label: "LEFT", value: lo }, { label: "RIGHT", value: hi }, { label: "ASKED", value: asked }];
    const out: Frame[] = [{ cells: shown(), pointers: ptrs(), lines: [2], read: read(), work: work(), say: "ok(x) is False up to some point and True after it. Find the first True. A ? is an answer not asked for yet: the search never looks at most of them." }];
    while (lo < hi) {
        const mid = (lo + hi) >> 1;
        asked++;
        known.set(mid, a[mid]!);
        if (a[mid] === "T") {
            out.push({ cells: shown(), pointers: ptrs(mid), dim: dim(), ...tint(), cur: [mid], lines: [3, 4, 5], read: read(), work: work(), say: `ok(${mid}) is True. ${mid} could be the first True, so keep it and throw away everything after it: hi = ${mid}.` });
            hi = mid;
        } else {
            out.push({ cells: shown(), pointers: ptrs(mid), dim: dim(), ...tint(), cur: [mid], lines: [3, 4, 6, 7], read: read(), work: work(), say: `ok(${mid}) is False, so ${mid} and everything before it is too early. Throw it away: lo = ${mid + 1}.` });
            lo = mid + 1;
        }
    }
    out.push({ cells: shown(), pointers: ptrs(), dim: dim(), ...tint(), hit: [lo], lines: [8], read: read(), work: work(), result: `index ${lo}`, say: `lo and hi met at ${lo}: the first True, found with ${asked} question${asked === 1 ? "" : "s"} out of ${n} possible.` });
    return out;
}

// ---------------------------------------------------------------- stack

const bracketsCode = [
    "def is_valid(s):",
    "    pairs = {')': '(', ']': '[', '}': '{'}",
    "    stack = []",
    "    for ch in s:",
    "        if ch in pairs:  # a closer",
    "            if not stack or stack.pop() != pairs[ch]:",
    "                return False",
    "        else:",
    "            stack.append(ch)  # an opener waits",
    "    return not stack",
];

const PAIRS: Record<string, string> = { ")": "(", "]": "[", "}": "{" };

function simulateBrackets(input: DemoInput): Frame[] {
    const s = input.items;
    const n = s.length;
    if (n < 2) return [];
    const stack: { ch: string; at: number }[] = [];
    let matched: number[] = [];
    let pushes = 0;
    let pops = 0;
    const sk = (top?: "push" | "pop") => ({ label: "STACK", items: stack.map((x) => x.ch), top });
    const work = (): Work => ({
        title: "EVERY CHARACTER IS PUSHED ONCE AND POPPED AT MOST ONCE",
        bars: [{ label: "Pushed", value: pushes, of: n, tone: "good", ratio: true }, { label: "Popped", value: pops, of: n, tone: "bad", ratio: true }],
        note: "A character is pushed when it opens and popped when its partner arrives, never more than once each, so a string of n characters costs O(n) however deeply it nests.",
    });
    const read = (verdict: string): Reading[] => [{ label: "STACK", value: stack.map((x) => x.ch).join(" ") || "empty" }, { label: "VERDICT", value: verdict }];
    const out: Frame[] = [{ pointers: [], stack: sk(), lines: [2, 3], read: read("?"), work: work(), say: "Every closer must meet its own opener, and the most recent opener first. The stack holds the openers still waiting, newest on top." }];
    for (let i = 0; i < n; i++) {
        const ch = s[i]!;
        const p: Pointer[] = [{ at: i, label: "i", tone: "r" }];
        if (ch in PAIRS) {
            const top = stack[stack.length - 1];
            if (!top || top.ch !== PAIRS[ch]) {
                out.push({ pointers: p, ok: matched, nope: [i, ...(top ? [top.at] : [])], stack: sk(), lines: [4, 5, 6, 7], read: read("invalid"), work: work(), result: "invalid", say: top ? `“${ch}” needs “${PAIRS[ch]}” on top, but the top is “${top.ch}”. The brackets cross, so the string is invalid.` : `“${ch}” closes something, but the stack is empty: nothing is open. Invalid.` });
                return out;
            }
            stack.pop();
            pops++;
            matched = [...matched, top.at, i];
            out.push({ pointers: p, ok: matched, stack: sk("pop"), lines: [4, 5, 6], read: read("?"), work: work(), say: `“${ch}” closes, and the top is “${top.ch}”: a match. Pop it.` });
        } else {
            stack.push({ ch, at: i });
            pushes++;
            out.push({ pointers: p, ok: matched, cur: [i], stack: sk("push"), lines: [4, 8, 9], read: read("?"), work: work(), say: `“${ch}” opens something, so it waits on the stack until its partner comes.` });
        }
    }
    const left = stack.length;
    out.push({ pointers: [], ok: matched, nope: stack.map((x) => x.at), stack: sk(), lines: [10], read: read(left ? "invalid" : "valid"), work: work(), result: left ? "invalid" : "valid", say: left ? `The string ended with ${left} bracket${left === 1 ? "" : "s"} still open, so it is invalid.` : "The stack is empty: every opener found its partner, in the right order. Valid." });
    return out;
}

const monoCode = [
    "def next_greater(nums):",
    "    res = [-1] * len(nums)",
    "    stack = []  # indexes, values decreasing",
    "    for i, x in enumerate(nums):",
    "        while stack and nums[stack[-1]] < x:",
    "            res[stack.pop()] = x  # x is their next greater",
    "        stack.append(i)",
    "    return res",
];

function simulateMono(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const n = a.length;
    if (n < 2) return [];
    const res: (number | null)[] = Array(n).fill(null);
    const stack: number[] = [];
    let pushes = 0;
    let pops = 0;
    const done = () => res.map((v, i) => (v === null ? -1 : i)).filter((i) => res[i] !== null);
    const sk = (top?: "push" | "pop") => ({ label: "STACK", items: stack.map((j) => String(a[j])), top });
    const work = (): Work => ({
        title: "EVERY INDEX IS PUSHED ONCE AND POPPED AT MOST ONCE",
        bars: [{ label: "Pushed", value: pushes, of: n, tone: "good", ratio: true }, { label: "Popped", value: pops, of: n, tone: "bad", ratio: true }],
        note: "A step may pop several items, but each item can only be popped once in the whole run, so the pops add up to at most n. That is why the nested while is still O(n).",
    });
    const read = (): Reading[] => [{ label: "STACK", value: stack.map((j) => a[j]).join(" ") || "empty" }, { label: "NEXT GREATER", value: res.map((v) => (v === null ? "–" : v)).join(" ") }];
    const out: Frame[] = [{ pointers: [], stack: sk(), lines: [2, 3], read: read(), work: work(), say: "For each number, find the next one to its right that is bigger. The stack holds the numbers still waiting for theirs, biggest at the bottom." }];
    for (let i = 0; i < n; i++) {
        const x = a[i]!;
        const p: Pointer[] = [{ at: i, label: "i", tone: "r" }];
        while (stack.length && a[stack[stack.length - 1]!]! < x) {
            const j = stack.pop()!;
            res[j] = x;
            pops++;
            out.push({ pointers: p, ok: done(), cur: [i], stack: sk("pop"), lines: [5, 6], read: read(), work: work(), say: `${a[j]} < ${x}, so ${x} is the next greater number for ${a[j]} (index ${j}). Pop it and record ${x}.` });
        }
        stack.push(i);
        pushes++;
        out.push({ pointers: p, ok: done(), cur: [i], stack: sk("push"), lines: [4, 7], read: read(), work: work(), say: `Push ${x}. Everything left on the stack is bigger, so the stack stays in decreasing order and only its top ever needs checking.` });
    }
    out.push({ pointers: [], ok: done(), stack: sk(), lines: [8], read: read(), work: work(), result: res.map((v) => (v === null ? -1 : v)).join(" "), say: `Done. The numbers left on the stack have no greater number to their right, so they get -1. Each index was pushed once and popped at most once.` });
    return out;
}

// ---------------------------------------------------------------- linked list

const reverseCode = [
    "def reverse(head):",
    "    prev, cur = None, head",
    "    while cur:",
    "        nxt = cur.next  # remember the rest",
    "        cur.next = prev  # flip this link",
    "        prev, cur = cur, nxt",
    "    return prev",
];

function simulateReverse(input: DemoInput): Frame[] {
    const v = input.items;
    const n = v.length;
    if (n < 2) return [];
    const links = v.map((_, i) => ({ from: i, to: i < n - 1 ? i + 1 : -1, hot: false }));
    let prev = -1;
    let cur = 0;
    let flips = 0;
    const name = (i: number) => (i === -1 ? "None" : v[i]!);
    const ptrs = (nxt: number): Pointer[] => [
        ...(prev >= 0 ? [{ at: prev, label: "prev", tone: "l" as const, below: true }] : []),
        ...(cur >= 0 ? [{ at: cur, label: "cur", tone: "r" as const }] : []),
        ...(nxt >= 0 ? [{ at: nxt, label: "next", tone: "m" as const }] : []),
    ];
    const work = (): Work => ({
        title: "WORK DONE",
        bars: [{ label: "Pointers flipped", value: flips, of: n, tone: "good", ratio: true }, { label: "New nodes made", value: 0, of: n, tone: "bad", ratio: true }],
        note: "Each node is visited once and its one pointer is flipped once. No node is copied and nothing extra is stored: O(n) time, O(1) space.",
    });
    const read = (): Reading[] => [{ label: "PREV", value: name(prev) }, { label: "CUR", value: name(cur) }];
    const out: Frame[] = [{ links: links.map((l) => ({ ...l })), pointers: ptrs(cur + 1 < n ? cur + 1 : -1).filter((p) => p.label !== "next"), cur: [0], lines: [2], read: read(), work: work(), say: "Reverse the list in place. prev is the part already reversed (empty, so None); cur is the node about to be turned around." }];
    while (cur !== -1) {
        const nxt = cur + 1 < n ? cur + 1 : -1;
        const was = cur;
        links[was] = { from: was, to: prev, hot: true };
        flips++;
        const before = prev;
        out.push({ links: links.map((l) => ({ ...l, hot: l.from === was })), pointers: ptrs(nxt), cur: [was], lines: [3, 4, 5, 6], read: read(), work: work(), say: `Remember the rest (${name(nxt)}), then point ${v[was]} back at ${name(before)}. Then both pointers step: prev becomes ${v[was]}, cur becomes ${name(nxt)}.` });
        for (const l of links) l.hot = false;
        prev = was;
        cur = nxt;
    }
    out.push({ links: links.map((l) => ({ ...l, hot: false })), pointers: [{ at: prev, label: "head", tone: "l", below: true }], hit: [...v.keys()], lines: [7], read: [{ label: "NEW ORDER", value: [...v].reverse().join(" ") }], work: work(), result: [...v].reverse().join(" "), say: `cur ran off the end, so prev is the new head: ${[...v].reverse().join(" → ")}. Every link now points the other way.` });
    return out;
}

const cycleCode = [
    "def has_cycle(head):",
    "    slow = fast = head",
    "    while fast and fast.next:",
    "        slow = slow.next  # one step",
    "        fast = fast.next.next  # two steps",
    "        if slow is fast:",
    "            return True",
    "    return False",
];

function simulateCycle(input: DemoInput): Frame[] {
    const v = input.items;
    const n = v.length;
    const tail = input.param !== undefined && input.param >= 0 && input.param < n ? input.param : -1;
    if (n < 2) return [];
    const next = (i: number) => (i < n - 1 ? i + 1 : tail);
    const links = v.map((_, i) => ({ from: i, to: next(i) }));
    let slow = 0;
    let fast = 0;
    let steps = 0;
    const seen = new Set<number>([0]);
    const work = (): Work => ({
        title: "EXTRA MEMORY",
        bars: [{ label: "Remember every node", value: seen.size, of: n, tone: "bad", ratio: true }, { label: "Slow and fast", value: 2, of: n, tone: "good", ratio: true }],
        note: "A set of visited nodes grows with the list. Two pointers are always two pointers: if there is a loop, the fast one laps the slow one inside it, and they meet.",
    });
    const ptrs = (s: number, f: number): Pointer[] => [{ at: s, label: "slow", tone: "l" }, ...(f >= 0 ? [{ at: f, label: "fast", tone: "r" as const }] : [])];
    const read = (): Reading[] => [{ label: "STEPS", value: steps }, { label: "SLOW AT", value: v[slow]! }, { label: "FAST AT", value: fast >= 0 ? v[fast]! : "end" }];
    const out: Frame[] = [{ links, pointers: ptrs(0, 0), cur: [0], lines: [2], read: read(), work: work(), say: tail >= 0 ? `The last node points back to ${v[tail]}, so this list has a loop. Both pointers start at the head.` : "The last node points to nothing (null), so this list has no loop. Both pointers start at the head." }];
    while (fast !== -1 && next(fast) !== -1) {
        slow = next(slow);
        fast = next(next(fast));
        steps++;
        seen.add(slow);
        if (fast !== -1) seen.add(fast);
        if (slow === fast) {
            out.push({ links, pointers: [{ at: slow, label: "slow = fast", tone: "r" }], hit: [slow], lines: [4, 5, 6, 7], read: read(), work: work(), result: "cycle", say: `slow and fast are on ${v[slow]} at the same time: the fast pointer lapped the slow one, which can only happen inside a loop. Found in ${steps} step${steps === 1 ? "" : "s"}.` });
            return out;
        }
        out.push({ links, pointers: ptrs(slow, fast), cur: [slow, ...(fast >= 0 ? [fast] : [])], lines: [3, 4, 5], read: read(), work: work(), say: `slow takes one step, fast takes two. slow is on ${v[slow]}, fast is on ${fast >= 0 ? v[fast] : "the end"}.` });
    }
    out.push({ links, pointers: ptrs(slow, -1), dim: [], lines: [3, 8], read: read(), work: work(), result: "no cycle", say: `fast reached the end of the list, so there is no loop. Two steps at a time took only ${steps} step${steps === 1 ? "" : "s"}, with no memory beyond the two pointers.` });
    return out;
}

// ---------------------------------------------------------------- arrays and hashing

const complementCode = [
    "def two_sum(nums, target):",
    "    seen = {}  # value -> index",
    "    for i, x in enumerate(nums):",
    "        need = target - x",
    "        if need in seen:  # one lookup",
    "            return [seen[need], i]",
    "        seen[x] = i  # remember x for later",
    "    return []",
];

function simulateComplement(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const target = input.param ?? 0;
    const n = a.length;
    if (n < 2) return [];
    const seen = new Map<number, number>();
    let lookups = 0;
    let pairs = 0;
    const rows = (tone?: "new" | "hit", key?: number) => [...seen].map(([k, v]) => ({ k: String(k), v: `at ${v}`, ...(tone && k === key ? { tone } : {}) }));
    const map = (tone?: "new" | "hit", key?: number) => ({ label: "SEEN  value → index", rows: rows(tone, key) });
    const work = (): Work => ({
        title: "WORK SO FAR",
        bars: [{ label: "Compare every pair", value: pairs, of: Math.max(pairs, 1), tone: "bad" }, { label: "One lookup per item", value: lookups, of: Math.max(pairs, 1), tone: "good" }],
        note: "Comparing every pair checks each earlier item again for each new one. The map remembers them, so the new item asks one question: is my partner already in there?",
    });
    const read = (need: string): Reading[] => [{ label: "NEED", value: need }, { label: "IN THE MAP", value: seen.size }];
    const out: Frame[] = [{ pointers: [], map: map(), lines: [2], read: read("?"), work: work(), say: `Find two numbers that add up to ${target}. The map will hold every number seen so far and where it was.` }];
    for (let i = 0; i < n; i++) {
        const x = a[i]!;
        const need = target - x;
        lookups++;
        pairs += i;
        const p: Pointer[] = [{ at: i, label: "i", tone: "r" }];
        const at = seen.get(need);
        if (at !== undefined) {
            out.push({ pointers: p, hit: [at, i], map: map("hit", need), lines: [3, 4, 5, 6], read: read(String(need)), work: work(), result: `${need} + ${x}`, say: `x = ${x}, so the partner is ${target} − ${x} = ${need}. It is in the map, at index ${at}. Found with ${lookups} lookup${lookups === 1 ? "" : "s"}; comparing pairs would have made ${pairs}.` });
            return out;
        }
        seen.set(x, i);
        out.push({ pointers: p, cur: [i], map: map("new", x), lines: [3, 4, 5, 7], read: read(String(need)), work: work(), say: `x = ${x}, so the partner is ${target} − ${x} = ${need}. Not in the map yet, so remember ${x} at index ${i}.` });
    }
    out.push({ pointers: [], map: map(), lines: [8], read: read("–"), work: work(), result: "none", say: "Every item was looked up and none had its partner waiting, so no two numbers add up to the target." });
    return out;
}

const signatureCode = [
    "def group_anagrams(words):",
    "    groups = {}  # signature -> words",
    "    for w in words:",
    "        key = \"\".join(sorted(w))  # same letters, same key",
    "        groups.setdefault(key, []).append(w)",
    "    return list(groups.values())",
];

function simulateSignature(input: DemoInput): Frame[] {
    const words = input.items;
    const n = words.length;
    if (n < 2) return [];
    const groups = new Map<string, string[]>();
    let lookups = 0;
    let pairs = 0;
    const map = (tone?: "new" | "hit", key?: string) => ({ label: "GROUPS  key → words", rows: [...groups].map(([k, v]) => ({ k, v: v.join(" "), ...(tone && k === key ? { tone } : {}) })) });
    const work = (): Work => ({
        title: "WORK SO FAR",
        bars: [{ label: "Compare every pair", value: pairs, of: Math.max(pairs, 1), tone: "bad" }, { label: "One key per word", value: lookups, of: Math.max(pairs, 1), tone: "good" }],
        note: "Asking \"are these two anagrams?\" for every pair grows with the square of the list. Give each word a canonical key and equal words land in the same slot with no comparing.",
    });
    const read = (key: string): Reading[] => [{ label: "KEY", value: key }, { label: "GROUPS", value: groups.size }];
    const out: Frame[] = [{ pointers: [], map: map(), lines: [2], read: read("?"), work: work(), say: "Two words are anagrams when their sorted letters match. So the sorted letters are the key, and every word with that key belongs together." }];
    for (let i = 0; i < n; i++) {
        const w = words[i]!;
        const key = [...w].sort().join("");
        lookups++;
        pairs += i;
        const fresh = !groups.has(key);
        groups.set(key, [...(groups.get(key) ?? []), w]);
        out.push({
            pointers: [{ at: i, label: "w", tone: "r" }],
            cur: [i],
            ok: words.slice(0, i).map((_, j) => j),
            map: map(fresh ? "new" : "hit", key),
            lines: [3, 4, 5],
            read: read(key),
            work: work(),
            say: fresh ? `“${w}” sorted is “${key}”. No group has that key yet, so start one.` : `“${w}” sorted is “${key}”, the key of a group that exists. It joins ${groups.get(key)!.slice(0, -1).map((x) => `“${x}”`).join(", ")}.`,
        });
    }
    out.push({ pointers: [], ok: words.map((_, j) => j), map: map(), lines: [6], read: read("–"), work: work(), result: `${groups.size} group${groups.size === 1 ? "" : "s"}`, say: `One pass, one key per word: ${groups.size} group${groups.size === 1 ? "" : "s"}. No pair of words was ever compared.` });
    return out;
}

import { ADV_GRAPH_DEMOS, GRAPH_DEMOS } from "./ideaDemosGraphs.ts";
import { TREE_DEMOS } from "./ideaDemosTrees.ts";

export const DEMOS: Record<string, Demo> = {
    ...TREE_DEMOS,
    ...GRAPH_DEMOS,
    ...ADV_GRAPH_DEMOS,
    "window-fixed": {
        title: "Fixed size · best sum",
        code: fixedCode,
        initial: { items: ["2", "1", "5", "1", "3", "2", "6", "1"], param: 4 },
        fields: { text: "Numbers, separated by commas", param: { label: "Window size k", min: 1, max: MAX_CELLS } },
        parse: (text, param) => {
            const items = numbers(text);
            const k = Number(param);
            return items.length >= 2 && items.length <= MAX_CELLS && smallInts(items) && Number.isInteger(k) && k >= 1 && k <= items.length ? { items, param: k } : null;
        },
        simulate: simulateFixed,
    },
    "window-variable": {
        title: "Variable · longest without a repeat",
        code: variableCode,
        initial: { items: [..."abcabcbb"] },
        letters: true,
        fields: { text: "A string of lowercase letters" },
        parse: (text) => {
            const s = text.toLowerCase().replace(/[^a-z]/g, "");
            return s.length >= 2 && s.length <= 14 ? { items: [...s] } : null;
        },
        simulate: simulateVariable,
    },
    "pointers-opposite": {
        title: "Opposite ends · find a pair",
        code: oppositeCode,
        initial: { items: ["1", "3", "4", "6", "8", "11", "15"], param: 14 },
        fields: { text: "Sorted numbers, separated by commas", param: { label: "Target", min: -999, max: 999 } },
        parse: (text, param) => {
            const items = numbers(text);
            const target = Number(param);
            if (items.length < 2 || items.length > MAX_CELLS || !smallInts(items) || !Number.isInteger(target) || param.trim() === "") return null;
            return { items: items.map(Number).sort((x, y) => x - y).map(String), param: target };
        },
        simulate: simulateOpposite,
    },
    "pointers-same": {
        title: "Same direction · remove duplicates",
        code: sameCode,
        initial: { items: ["1", "1", "2", "2", "2", "3", "4", "4"] },
        fields: { text: "Sorted numbers, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= MAX_CELLS && smallInts(items) ? { items: items.map(Number).sort((x, y) => x - y).map(String) } : null;
        },
        simulate: simulateSame,
    },
    "search-find": {
        title: "Find a value in a sorted row",
        code: findCode,
        initial: { items: ["2", "5", "8", "12", "16", "23", "38", "56", "72", "91"], param: 23 },
        fields: { text: "Sorted numbers, separated by commas", param: { label: "Target", min: -999, max: 999 } },
        parse: (text, param) => {
            const items = numbers(text);
            const target = Number(param);
            if (items.length < 2 || items.length > MAX_CELLS || !smallInts(items) || !Number.isInteger(target) || param.trim() === "") return null;
            return { items: items.map(Number).sort((x, y) => x - y).map(String), param: target };
        },
        simulate: simulateFind,
    },
    "search-first-true": {
        title: "First True in a yes/no that flips once",
        code: firstTrueCode,
        initial: { items: [..."FFFFFFTTTT"] },
        compact: true,
        fields: { text: "Falses then trues, such as FFFFTT" },
        parse: (text) => {
            const s = text.toUpperCase().replace(/[^FT]/g, "");
            return /^F*T+$/.test(s) && s.length >= 2 && s.length <= MAX_CELLS ? { items: [...s] } : null;
        },
        simulate: simulateFirstTrue,
    },
    "stack-brackets": {
        title: "Matching brackets",
        code: bracketsCode,
        initial: { items: [..."([]{})"] },
        compact: true,
        fields: { text: "Brackets, such as ([]{})" },
        parse: (text) => {
            const s = text.replace(/[^()[\]{}]/g, "");
            return s.length >= 2 && s.length <= MAX_CELLS ? { items: [...s] } : null;
        },
        simulate: simulateBrackets,
    },
    "stack-mono": {
        title: "Monotonic stack · next greater",
        code: monoCode,
        initial: { items: ["2", "1", "5", "3", "6", "4"] },
        fields: { text: "Numbers, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= MAX_CELLS && smallInts(items) ? { items } : null;
        },
        simulate: simulateMono,
    },
    "list-reverse": {
        title: "Reverse a list in place",
        code: reverseCode,
        initial: { items: ["1", "2", "3", "4", "5"] },
        wide: true,
        fields: { text: "Node values, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= 7 && smallInts(items) ? { items } : null;
        },
        simulate: simulateReverse,
    },
    "list-cycle": {
        title: "Find a loop with slow and fast",
        code: cycleCode,
        initial: { items: ["1", "2", "3", "4", "5", "6"], param: 2 },
        wide: true,
        fields: { text: "Node values, separated by commas", param: { label: "Last node points to index", min: -1, max: 6 } },
        parse: (text, param) => {
            const items = numbers(text);
            const tail = Number(param);
            return items.length >= 2 && items.length <= 7 && smallInts(items) && Number.isInteger(tail) && tail >= -1 && tail < items.length && param.trim() !== "" ? { items, param: tail } : null;
        },
        simulate: simulateCycle,
    },
    "hash-complement": {
        title: "Two Sum · remember what you have seen",
        code: complementCode,
        initial: { items: ["3", "8", "5", "1", "9", "6"], param: 14 },
        fields: { text: "Numbers, separated by commas", param: { label: "Target", min: -999, max: 999 } },
        parse: (text, param) => {
            const items = numbers(text);
            const target = Number(param);
            return items.length >= 2 && items.length <= MAX_CELLS && smallInts(items) && Number.isInteger(target) && param.trim() !== "" ? { items, param: target } : null;
        },
        simulate: simulateComplement,
    },
    "hash-signature": {
        title: "Group anagrams by a shared key",
        code: signatureCode,
        words: true,
        initial: { items: ["eat", "tea", "tan", "ate", "nat", "bat"] },
        fields: { text: "Short words, separated by commas" },
        parse: (text) => {
            const items = text.toLowerCase().split(/[\s,]+/).filter(Boolean);
            return items.length >= 2 && items.length <= 8 && items.every((w) => /^[a-z]{1,4}$/.test(w)) ? { items } : null;
        },
        simulate: simulateSignature,
    },
};
