// The animations of the Intervals, Math & Geometry and Bit Manipulation Idea tabs: ranges on a line, a matrix being turned, numbers as rows of bits.
import { MAX_CELLS, numbers } from "./ideaDemoKit.ts";
import type { Demo, DemoInput, Frame, GridMark, Range, Reading, Work } from "./ideaDemos.ts";

// ---------------------------------------------------------------- intervals

const parseRanges = (items: string[]) => items.map((s) => /^(\d{1,2})-(\d{1,2})$/.exec(s)).map((m) => (m ? [Number(m[1]), Number(m[2])] as [number, number] : null));
const validRanges = (text: string, min: number, max: number) => {
    const items = numbers(text);
    const rs = parseRanges(items);
    return items.length >= min && items.length <= max && rs.every((r) => r && r[0] < r[1] && r[1] <= 20) ? { items } : null;
};

const mergeCode = [
    "def merge(intervals):",
    "    intervals.sort()  # by start",
    "    out = [intervals[0]]",
    "    for s, e in intervals[1:]:",
    "        if s <= out[-1][1]:  # overlaps the last one",
    "            out[-1][1] = max(out[-1][1], e)",
    "        else:",
    "            out.append([s, e])",
    "    return out",
];

function simulateMerge(input: DemoInput): Frame[] {
    const rs = (parseRanges(input.items) as [number, number][]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    const n = rs.length;
    const hi = Math.max(...rs.map((r) => r[1]));
    const out: [number, number][] = [];
    let at = -1;
    let compared = 0;
    const bars = (): Range[] => [
        ...rs.map((r, i): Range => ({ from: r[0], to: r[1], label: `${r[0]}-${r[1]}`, lane: i, tone: i === at ? "cur" : i < at ? "keep" : undefined })),
        ...out.map((r): Range => ({ from: r[0], to: r[1], label: `${r[0]}-${r[1]}`, lane: n, tone: "merged" })),
    ];
    const frame = (say: string, lines: number[], result?: string): Frame => ({
        pointers: [],
        ranges: { bars: bars(), axis: [0, hi], lanes: n + 1 },
        lines,
        read: [{ label: "MERGED SO FAR", value: out.map((r) => `${r[0]}-${r[1]}`).join(" ") || "none" }, { label: "INTERVALS", value: `${Math.max(at + 1, 0)}/${n}` }] as Reading[],
        work: {
            title: "AFTER SORTING, ONE LOOK PER INTERVAL",
            bars: [{ label: "Compare every pair", value: (Math.max(at, 0) * (Math.max(at, 0) + 1)) / 2, of: Math.max((n * (n - 1)) / 2, 1), tone: "bad" }, { label: "Compare with the last merged", value: compared, of: Math.max((n * (n - 1)) / 2, 1), tone: "good" }],
            note: "Once the intervals are sorted by start, a new interval can only overlap the last merged one: anything earlier ends before the last one starts. So each interval is compared once, with one neighbour. The sort is the cost.",
        } as Work,
        say,
        result,
    });
    const res: Frame[] = [frame("Sort the intervals by start. Then walk them: each one either overlaps the last merged interval, and stretches it, or starts a new one.", [2, 3])];
    for (let i = 0; i < n; i++) {
        at = i;
        const [s, e] = rs[i]!;
        const last = out[out.length - 1];
        if (i > 0) compared++;
        if (last && s <= last[1]) {
            const before = last[1];
            last[1] = Math.max(last[1], e);
            res.push(frame(`${s}-${e} starts at ${s}, which is no later than the end of ${last[0]}-${before}, so they overlap. Stretch the merged interval to end at ${last[1]}.`, [4, 5, 6]));
        } else {
            out.push([s, e]);
            res.push(frame(last ? `${s}-${e} starts at ${s}, after the last merged one ends (${last[1]}): a gap. Start a new merged interval.` : `The first interval, ${s}-${e}, starts the first merged interval.`, last ? [4, 7, 8] : [3]));
        }
    }
    at = n;
    res.push(frame(`Done: ${out.map((r) => `${r[0]}-${r[1]}`).join(", ")}. ${n} intervals became ${out.length}.`, [9], `${out.length} interval${out.length === 1 ? "" : "s"}`));
    return res;
}

const sweepCode = [
    "def min_rooms(meetings):",
    "    events = []",
    "    for s, e in meetings:",
    "        events += [(s, 1), (e, -1)]  # a meeting starts, ends",
    "    rooms = best = 0",
    "    for t, d in sorted(events):  # ends sort before starts at a tie",
    "        rooms += d",
    "        best = max(best, rooms)",
    "    return best",
];

function simulateSweep(input: DemoInput): Frame[] {
    const rs = (parseRanges(input.items) as [number, number][]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    const n = rs.length;
    const hi = Math.max(...rs.map((r) => r[1]));
    const events = rs.flatMap(([s, e]) => [[s, 1], [e, -1]] as [number, number][]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    let rooms = 0;
    let best = 0;
    let t = -1;
    const active = (x: number) => (i: number) => rs[i]![0] <= x && x < rs[i]![1];
    const frame = (say: string, lines: number[], result?: string): Frame => ({
        pointers: [],
        ranges: { bars: rs.map((r, i): Range => ({ from: r[0], to: r[1], label: `${r[0]}-${r[1]}`, lane: i, tone: t >= 0 && active(t)(i) ? "cur" : undefined })), axis: [0, hi], lanes: n, marker: t >= 0 ? t : undefined },
        lines,
        read: [{ label: "ROOMS IN USE", value: rooms }, { label: "MOST AT ONCE", value: best }] as Reading[],
        work: {
            title: "ONE SORT, THEN ONE PASS OVER THE EVENTS",
            bars: [{ label: "Events handled", value: Math.max(events.findIndex((e) => e[0] === t) + 1, 0), of: events.length, tone: "good", ratio: true }, { label: "Checking every time point", value: hi, of: hi, tone: "bad" }],
            note: "The busiest moment is always the instant a meeting starts, so only the 2n start and end events matter. Sorting them and keeping a running count finds the peak without comparing meetings with each other.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame("How many rooms do these meetings need? Turn each meeting into two events, a start (+1) and an end (−1), sort them, and sweep a line across, keeping the count of meetings in progress.", [2, 3, 4])];
    for (const [time, d] of events) {
        t = time;
        rooms += d;
        const up = best < rooms;
        best = Math.max(best, rooms);
        out.push(frame(d === 1 ? `At ${time} a meeting starts: ${rooms} in progress.${up ? " A new peak." : ""}` : `At ${time} a meeting ends: ${rooms} in progress.`, [5, 6, 7]));
    }
    out.push(frame(`The most meetings at one moment was ${best}, so ${best} room${best === 1 ? "" : "s"} are enough.`, [8], `${best} room${best === 1 ? "" : "s"}`));
    return out;
}

// ---------------------------------------------------------------- math and geometry

const rotateCode = [
    "def rotate(m):",
    "    n = len(m)",
    "    for i in range(n):  # transpose",
    "        for j in range(i + 1, n):",
    "            m[i][j], m[j][i] = m[j][i], m[i][j]",
    "    for row in m:  # then flip each row",
    "        row.reverse()",
];

function simulateRotate(input: DemoInput): Frame[] {
    const n = input.param ?? 3;
    const m = Array.from({ length: n }, (_, r) => Array.from({ length: n }, (_, c) => r * n + c + 1));
    let swaps = 0;
    const grid = (marks: GridMark[]): NonNullable<Frame["grid"]> => ({ cells: m.map((row) => row.map(String)), marks, small: false });
    const work = (): Work => ({
        title: "TWO SIMPLE MOVES, NO SECOND MATRIX",
        bars: [{ label: "Swaps made", value: swaps, of: (n * (n - 1)) / 2 + n * Math.floor(n / 2), tone: "good", ratio: true }, { label: "Extra cells if you copy", value: n * n, of: n * n, tone: "bad" }],
        note: "A 90° turn clockwise is a transpose (mirror along the diagonal) followed by a flip of each row. Each is swaps inside the same matrix, so no second matrix is needed: O(1) extra space, O(n²) time because every cell moves once.",
    });
    const out: Frame[] = [{ pointers: [], grid: grid([]), lines: [2], read: [{ label: "STEP", value: "start" }], work: work(), say: "Turn the picture 90° clockwise, in place. First transpose: swap each cell above the diagonal with its mirror below it." }];
    for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
            [m[i]![j], m[j]![i]] = [m[j]![i]!, m[i]![j]!];
            swaps++;
            out.push({ pointers: [], grid: grid([{ r: i, c: j, tone: "cur" }, { r: j, c: i, tone: "cur" }]), lines: [3, 4, 5], read: [{ label: "STEP", value: "transpose" }], work: work(), say: `Swap (${i}, ${j}) with (${j}, ${i}): rows become columns.` });
        }
    }
    out.push({ pointers: [], grid: grid([]), lines: [6], read: [{ label: "STEP", value: "flip rows" }], work: work(), say: "The matrix is transposed. Now reverse each row, which turns the mirror image into a rotation." });
    for (let r = 0; r < n; r++) {
        m[r]!.reverse();
        swaps += Math.floor(n / 2);
        out.push({ pointers: [], grid: grid(Array.from({ length: n }, (_, c) => ({ r, c, tone: "new" as const }))), lines: [6, 7], read: [{ label: "STEP", value: "flip rows" }], work: work(), say: `Reverse row ${r}.` });
    }
    out.push({ pointers: [], grid: grid(Array.from({ length: n }, (_, r) => Array.from({ length: n }, (_, c) => ({ r, c, tone: "done" as const }))).flat()), lines: [7], read: [{ label: "STEP", value: "done" }], work: work(), result: "rotated 90°", say: "The first row is now the last column, read top to bottom, and so on: the matrix is turned, with no extra space." });
    return out;
}

const powCode = [
    "def power(x, n):",
    "    result = 1",
    "    while n:",
    "        if n & 1:  # this bit is set",
    "            result *= x",
    "        x *= x  # square for the next bit",
    "        n >>= 1",
    "    return result",
];

function simulatePow(input: DemoInput): Frame[] {
    const base = BigInt(input.items[0]!);
    const exp = input.param ?? 1;
    const bits = exp.toString(2).split("");
    const len = bits.length;
    let result = 1n;
    let x = base;
    let mults = 0;
    const work = (): Work => ({
        title: "ABOUT LOG n MULTIPLICATIONS",
        bars: [{ label: "Multiply one at a time", value: exp - 1, of: Math.max(exp - 1, 1), tone: "bad" }, { label: "Square and multiply", value: mults, of: Math.max(exp - 1, 1), tone: "good" }],
        note: "Every number is a sum of powers of two, so x^n is a product of x, x², x⁴, x⁸ … for the bits that are set. Squaring gets the next power in one multiplication, so the work follows the number of bits of n, not n itself.",
    });
    const read = (): Reading[] => [{ label: "RESULT", value: String(result) }, { label: "x (SQUARED SO FAR)", value: String(x) }];
    const out: Frame[] = [{ pointers: [], cells: bits, lines: [2], read: read(), work: work(), say: `Compute ${base}^${exp}. In binary the exponent is ${bits.join("")}. Read its bits from the right: for each set bit multiply the result by x, and square x before the next bit.` }];
    for (let k = 0; k < len; k++) {
        const bit = bits[len - 1 - k]!;
        const at = len - 1 - k;
        if (bit === "1") {
            result *= x;
            mults++;
            out.push({ pointers: [{ at, label: "bit", tone: "r" }], cells: bits, cur: [at], lines: [3, 4, 5], read: read(), work: work(), say: `Bit ${k} is 1, so x^${2 ** k} is part of the answer: multiply the result by x = ${x}.` });
        } else {
            out.push({ pointers: [{ at, label: "bit", tone: "r" }], cells: bits, cur: [at], dim: [at], lines: [3, 4], read: read(), work: work(), say: `Bit ${k} is 0, so x^${2 ** k} is not part of it. Skip the multiply.` });
        }
        if (k < len - 1) {
            x *= x;
            mults++;
            out.push({ pointers: [{ at, label: "bit", tone: "r" }], cells: bits, ok: bits.map((_, j) => j).filter((j) => j >= at), lines: [6, 7], read: read(), work: work(), say: `Square x: it is now ${x}, which is ${base}^${2 ** (k + 1)}, ready for the next bit.` });
        }
    }
    out.push({ pointers: [], cells: bits, hit: bits.map((_, j) => j), lines: [8], read: read(), work: work(), result: String(result), say: `${base}^${exp} = ${result}, using ${mults} multiplications instead of ${exp - 1}.` });
    return out;
}

// ---------------------------------------------------------------- bit manipulation

const bitsOf = (v: number, w: number) => v.toString(2).padStart(w, "0").split("");

const xorCode = [
    "def single_number(nums):",
    "    result = 0",
    "    for x in nums:",
    "        result ^= x  # equal pairs cancel to 0",
    "    return result",
];

function simulateXor(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const n = a.length;
    let result = 0;
    const W = 4;
    const labels = ["8", "4", "2", "1"];
    const out: Frame[] = [];
    const work = (i: number): Work => ({
        title: "ONE NUMBER, NO SET",
        bars: [{ label: "Numbers a set would hold", value: i, of: n, tone: "bad", ratio: true }, { label: "Numbers kept here", value: 1, of: n, tone: "good", ratio: true }],
        note: "A bit XOR itself is 0 and a bit XOR 0 is itself, and XOR does not care about order. So every value that appears twice cancels out, whatever order it came in, and only the value that is left over survives. One pass, one number.",
    });
    out.push({ pointers: [], grid: { cells: [bitsOf(0, W), bitsOf(0, W)], rowLabels: ["x", "result"], colLabels: labels, marks: [] }, lines: [2], read: [{ label: "RESULT", value: 0 }, { label: "NUMBERS", value: a.join(" ") }], work: work(0), say: "Every number but one appears twice. XOR them all together: XOR is its own undo, so each pair cancels and only the single number is left." });
    for (let i = 0; i < n; i++) {
        const before = result;
        result ^= a[i]!;
        const flipped = bitsOf(before, W).map((b, c) => (b !== bitsOf(result, W)[c] ? c : -1)).filter((c) => c >= 0);
        out.push({
            pointers: [],
            grid: { cells: [bitsOf(a[i]!, W), bitsOf(result, W)], rowLabels: ["x", "result"], colLabels: labels, marks: [...flipped.map((c): GridMark => ({ r: 1, c, tone: "cur" })), ...bitsOf(a[i]!, W).flatMap((b, c): GridMark[] => (b === "1" ? [{ r: 0, c, tone: "read" }] : []))] },
            lines: [3, 4],
            read: [{ label: "RESULT", value: result }, { label: "x", value: a[i]! }],
            work: work(i + 1),
            say: `${before} ^ ${a[i]} = ${result}. Each 1 in x flips that bit of the result; bits that were set twice have flipped back.`,
        });
    }
    out.push({ pointers: [], grid: { cells: [bitsOf(result, W), bitsOf(result, W)], rowLabels: ["", "result"], colLabels: labels, marks: bitsOf(result, W).map((_, c): GridMark => ({ r: 1, c, tone: "hit" })) }, lines: [5], read: [{ label: "RESULT", value: result }, { label: "NUMBERS", value: a.join(" ") }], work: work(n), result: String(result), say: `Everything that came in pairs cancelled. ${result} is the number that appears an odd number of times.` });
    return out;
}

const countCode = [
    "def count_bits(n):",
    "    count = 0",
    "    while n:",
    "        n &= n - 1  # clears the lowest set bit",
    "        count += 1",
    "    return count",
];

function simulateCount(input: DemoInput): Frame[] {
    let n = input.param ?? 1;
    const W = 8;
    const total = n.toString(2).split("").filter((b) => b === "1").length;
    let count = 0;
    const work = (): Work => ({
        title: "ONE STEP PER SET BIT",
        bars: [{ label: "Steps here", value: count, of: W, tone: "good", ratio: true }, { label: "Checking every bit", value: W, of: W, tone: "bad", ratio: true }],
        note: "Subtracting 1 turns the lowest set bit into 0 and every zero below it into 1, so n & (n − 1) clears exactly that lowest 1 and nothing above it. The loop runs once per set bit, not once per bit.",
    });
    const frame = (say: string, lines: number[], hit: number[], result?: string): Frame => ({ pointers: [], cells: bitsOf(n, W), hit, ok: [], lines, read: [{ label: "n", value: n }, { label: "BITS COUNTED", value: count }] as Reading[], work: work(), say, result });
    const out: Frame[] = [frame(`Count the 1 bits of ${n} (${bitsOf(n, W).join("")}). Rather than test all ${W} bits, remove the lowest set bit each time and count the removals.`, [2, 3], [])];
    while (n) {
        const low = n & -n;
        const pos = W - 1 - Math.log2(low);
        n &= n - 1;
        count++;
        out.push(frame(`n & (n − 1) clears the lowest set bit (the one worth ${low}). That is bit number ${count}.`, [4, 5], [pos]));
    }
    out.push(frame(`n is 0: no set bits are left. ${total} bit${total === 1 ? "" : "s"} were set, found in ${count} step${count === 1 ? "" : "s"}.`, [6], [], String(count)));
    return out;
}

export const LINE_DEMOS: Record<string, Demo> = {
    "intervals-merge": {
        title: "Merge overlapping intervals",
        code: mergeCode,
        initial: { items: ["1-3", "2-6", "8-10", "15-18"] },
        noRow: true,
        fields: { text: "Intervals such as 1-3, separated by commas" },
        parse: (text) => validRanges(text, 2, 5),
        simulate: simulateMerge,
    },
    "intervals-sweep": {
        title: "Sweep a line · how many rooms",
        code: sweepCode,
        initial: { items: ["1-4", "2-5", "3-7", "6-9", "8-10"] },
        noRow: true,
        fields: { text: "Meetings such as 1-4, separated by commas" },
        parse: (text) => validRanges(text, 2, 5),
        simulate: simulateSweep,
    },
    "math-rotate": {
        title: "Rotate a matrix in place",
        code: rotateCode,
        initial: { items: [], param: 3 },
        noRow: true,
        fields: { text: "Nothing to type: change the size", param: { label: "Size", min: 3, max: 4 } },
        parse: (_t, p) => {
            const k = Number(p);
            return Number.isInteger(k) && k >= 3 && k <= 4 ? { items: [], param: k } : null;
        },
        simulate: simulateRotate,
    },
    "math-pow": {
        title: "Fast power · square and multiply",
        code: powCode,
        initial: { items: ["3"], param: 13 },
        fields: { text: "The base, a digit 2 to 9", param: { label: "Exponent", min: 1, max: 30 } },
        parse: (text, p) => {
            const k = Number(p);
            return /^[2-9]$/.test(text.trim()) && Number.isInteger(k) && k >= 1 && k <= 30 && p.trim() !== "" ? { items: [text.trim()], param: k } : null;
        },
        simulate: simulatePow,
    },
    "bits-xor": {
        title: "XOR · the number without a pair",
        code: xorCode,
        initial: { items: ["4", "1", "2", "1", "2"] },
        noRow: true,
        fields: { text: "Numbers 0 to 15, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 3 && items.length <= MAX_CELLS && items.every((x) => /^(1[0-5]|\d)$/.test(x)) ? { items } : null;
        },
        simulate: simulateXor,
    },
    "bits-count": {
        title: "Count set bits · clear the lowest",
        code: countCode,
        initial: { items: [], param: 148 },
        fields: { text: "Nothing to type: change the number", param: { label: "n", min: 1, max: 255 } },
        parse: (_t, p) => {
            const k = Number(p);
            return Number.isInteger(k) && k >= 1 && k <= 255 && p.trim() !== "" ? { items: [], param: k } : null;
        },
        simulate: simulateCount,
    },
};
