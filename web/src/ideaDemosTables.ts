// The animations of the dynamic programming and greedy Idea tabs: a table that fills with each cell lit with the cells it reads, and one-pass scans.
import { MAX_CELLS, numbers, smallInts } from "./ideaDemoKit.ts";
import type { Demo, DemoInput, Frame, GridMark, Reading, Work } from "./ideaDemos.ts";

const INF = Number.POSITIVE_INFINITY;
const show = (v: number) => (v === INF ? "∞" : String(v));

// ---------------------------------------------------------------- 1-D DP: coin change

const coinsCode = [
    "def coin_change(coins, amount):",
    "    dp = [0] + [inf] * amount  # dp[a]: fewest coins for a",
    "    for a in range(1, amount + 1):",
    "        for c in coins:",
    "            if c <= a:",
    "                dp[a] = min(dp[a], dp[a - c] + 1)",
    "    return dp[amount] if dp[amount] < inf else -1",
];

function simulateCoins(input: DemoInput): Frame[] {
    const coins = input.items.map(Number).sort((a, b) => a - b);
    const A = input.param ?? 0;
    const dp: number[] = [0];
    // calls a plain recursion would make for each amount (no table)
    const calls: number[] = [1];
    for (let a = 1; a <= A; a++) calls[a] = 1 + coins.filter((c) => c <= a).reduce((s, c) => s + calls[a - c]!, 0);
    const table = (marks: GridMark[]): NonNullable<Frame["grid"]> => ({
        cells: [Array.from({ length: A + 1 }, (_, a) => (a < dp.length ? show(dp[a]!) : "·"))],
        colLabels: Array.from({ length: A + 1 }, (_, a) => String(a)),
        rowLabels: ["coins"],
        marks,
        small: A > 8,
    });
    const work = (a: number): Work => ({
        title: "EACH AMOUNT IS SOLVED ONCE",
        bars: [{ label: "Calls with no table", value: calls[a] ?? 1, of: Math.max(calls[Math.max(A, 1)]!, 1), tone: "bad" }, { label: "Cells filled", value: Math.min(dp.length, a + 1), of: Math.max(calls[Math.max(A, 1)]!, 1), tone: "good" }],
        note: "Without the table, \"fewest coins for 11\" asks for 10, 9 and 6, which each ask again, so the same amounts are solved over and over and the calls grow exponentially. The table solves each amount once and then only reads.",
    });
    const read = (): Reading[] => [{ label: "COINS", value: coins.join(" ") }, { label: "FEWEST FOR NOW", value: dp.length ? show(dp[dp.length - 1]!) : "–" }];
    const out: Frame[] = [{ pointers: [], grid: table([{ r: 0, c: 0, tone: "done" }]), lines: [2], read: read(), work: work(0), say: `dp[a] is the fewest coins that make a. Zero coins make 0, so dp[0] = 0. Every later cell is built from smaller cells already filled.` }];
    for (let a = 1; a <= A; a++) {
        let best = INF;
        const reads: GridMark[] = [];
        const parts: string[] = [];
        for (const c of coins) {
            if (c > a) continue;
            const via = dp[a - c]! + 1;
            reads.push({ r: 0, c: a - c, tone: "read" });
            parts.push(`${c}: dp[${a - c}] + 1 = ${show(via)}`);
            best = Math.min(best, via);
        }
        dp[a] = best;
        out.push({ pointers: [], grid: table([...reads, { r: 0, c: a, tone: "cur" }]), lines: [3, 4, 5, 6], read: read(), work: work(a), say: parts.length ? `Amount ${a}: try each coin that fits: ${parts.join(", ")}. The fewest is ${show(best)}.` : `Amount ${a}: no coin fits, so it cannot be made.` });
    }
    const ans = dp[A]!;
    out.push({ pointers: [], grid: table([{ r: 0, c: A, tone: "hit" }]), lines: [7], read: read(), work: work(A), result: ans === INF ? "impossible" : `${ans} coins`, say: ans === INF ? `${A} cannot be made from these coins.` : `The answer is dp[${A}] = ${ans}, found by filling ${A + 1} cells instead of exploring ${calls[A]} calls.` });
    return out;
}

// ---------------------------------------------------------------- 1-D DP: take or skip (house robber)

const robCode = [
    "def rob(nums):",
    "    dp = [0, nums[0]]  # best for 0 and for 1 houses",
    "    for i in range(2, len(nums) + 1):",
    "        skip = dp[i - 1]  # leave house i",
    "        take = dp[i - 2] + nums[i - 1]  # rob it, so skip i-1",
    "        dp.append(max(skip, take))",
    "    return dp[-1]",
];

function simulateRob(input: DemoInput): Frame[] {
    const v = input.items.map(Number);
    const n = v.length;
    const dp: number[] = [0, v[0]!];
    const naive: number[] = [1, 1];
    for (let i = 2; i <= n; i++) naive[i] = 1 + naive[i - 1]! + naive[i - 2]!;
    const table = (marks: GridMark[]): NonNullable<Frame["grid"]> => ({
        cells: [["–", ...v.map(String)], Array.from({ length: n + 1 }, (_, i) => (i < dp.length ? String(dp[i]) : "·"))],
        colLabels: Array.from({ length: n + 1 }, (_, i) => String(i)),
        rowLabels: ["house", "best"],
        marks,
    });
    const work = (i: number): Work => ({
        title: "EACH PREFIX IS SOLVED ONCE",
        bars: [{ label: "Calls with no table", value: naive[Math.min(i, n)]!, of: naive[n]!, tone: "bad" }, { label: "Cells filled", value: Math.min(dp.length, i + 1), of: naive[n]!, tone: "good" }],
        note: "\"Best for the first i houses\" depends only on the best for i−1 and i−2. Without a table the recursion re-solves the same prefixes again and again (it grows like Fibonacci). With one, each prefix is solved once.",
    });
    const read = (): Reading[] => [{ label: "BEST SO FAR", value: dp[dp.length - 1]! }, { label: "HOUSES", value: n }];
    const out: Frame[] = [{ pointers: [], grid: table([{ r: 1, c: 0, tone: "done" }, { r: 1, c: 1, tone: "done" }]), lines: [2], read: read(), work: work(1), say: `No two neighbouring houses can be robbed. dp[i] is the best total from the first i houses. With none it is 0; with one house it is just that house, ${v[0]}.` }];
    for (let i = 2; i <= n; i++) {
        const skip = dp[i - 1]!;
        const take = dp[i - 2]! + v[i - 1]!;
        dp.push(Math.max(skip, take));
        out.push({ pointers: [], grid: table([{ r: 1, c: i - 1, tone: "read" }, { r: 1, c: i - 2, tone: "read" }, { r: 0, c: i, tone: "new" }, { r: 1, c: i, tone: "cur" }]), lines: [3, 4, 5, 6], read: read(), work: work(i), say: `House ${i} (${v[i - 1]}): skip it and keep dp[${i - 1}] = ${skip}, or take it and add to dp[${i - 2}] = ${dp[i - 2]}: ${take}. The better is ${Math.max(skip, take)}.` });
    }
    out.push({ pointers: [], grid: table([{ r: 1, c: n, tone: "hit" }]), lines: [7], read: read(), work: work(n), result: String(dp[n]), say: `The answer is dp[${n}] = ${dp[n]}, from ${n - 1} small decisions, each reading two cells.` });
    return out;
}

// ---------------------------------------------------------------- 2-D DP: longest common subsequence

const lcsCode = [
    "def lcs(a, b):",
    "    dp = [[0] * (len(b) + 1) for _ in range(len(a) + 1)]",
    "    for i in range(1, len(a) + 1):",
    "        for j in range(1, len(b) + 1):",
    "            if a[i - 1] == b[j - 1]:",
    "                dp[i][j] = dp[i - 1][j - 1] + 1  # extend a match",
    "            else:",
    "                dp[i][j] = max(dp[i - 1][j], dp[i][j - 1])",
    "    return dp[-1][-1]",
];

function simulateLcs(input: DemoInput): Frame[] {
    const [a, b] = input.items as [string, string];
    const m = a.length;
    const n = b.length;
    const dp = Array.from({ length: m + 1 }, () => Array<number | null>(n + 1).fill(null));
    for (let i = 0; i <= m; i++) dp[i]![0] = 0;
    for (let j = 0; j <= n; j++) dp[0]![j] = 0;
    const calls = (i: number, j: number): number => (i === 0 || j === 0 ? 1 : 1 + (a[i - 1] === b[j - 1] ? calls(i - 1, j - 1) : calls(i - 1, j) + calls(i, j - 1)));
    const naive = calls(m, n);
    let filled = 0;
    const table = (marks: GridMark[]): NonNullable<Frame["grid"]> => ({
        cells: dp.map((row) => row.map((x) => (x === null ? "·" : String(x)))),
        colLabels: ["", ...b],
        rowLabels: ["", ...a],
        marks,
        small: true,
    });
    const work = (): Work => ({
        title: "EVERY PAIR OF PREFIXES IS SOLVED ONCE",
        bars: [{ label: "Cells filled", value: filled, of: m * n, tone: "good", ratio: true }, { label: "Calls with no table", value: naive, of: Math.max(naive, 1), tone: "bad" }],
        note: "Each cell is the longest common subsequence of a prefix of a and a prefix of b. It needs only its three neighbours, so the table has (m+1)(n+1) cells and each is one comparison. Without the table the same prefix pairs are solved again and again.",
    });
    const read = (v: string): Reading[] => [{ label: "a", value: a }, { label: "b", value: b }, { label: "LCS SO FAR", value: v }];
    const out: Frame[] = [{ pointers: [], grid: table([]), lines: [2], read: read("0"), work: work(), say: `dp[i][j] is the length of the longest common subsequence of the first i letters of a and the first j letters of b. An empty prefix matches nothing, so the first row and column are 0.` }];
    for (let i = 1; i <= m; i++) {
        for (let j = 1; j <= n; j++) {
            const same = a[i - 1] === b[j - 1];
            const v = same ? dp[i - 1]![j - 1]! + 1 : Math.max(dp[i - 1]![j]!, dp[i]![j - 1]!);
            dp[i]![j] = v;
            filled++;
            out.push({
                pointers: [],
                grid: table(same ? [{ r: i - 1, c: j - 1, tone: "read" }, { r: i, c: j, tone: "cur" }] : [{ r: i - 1, c: j, tone: "read" }, { r: i, c: j - 1, tone: "read" }, { r: i, c: j, tone: "cur" }]),
                lines: same ? [3, 4, 5, 6] : [3, 4, 7, 8],
                read: read(String(v)),
                work: work(),
                say: same ? `“${a[i - 1]}” = “${b[j - 1]}”: a match. Extend the best of the prefixes without these letters: dp[${i - 1}][${j - 1}] + 1 = ${v}.` : `“${a[i - 1]}” ≠ “${b[j - 1]}”. Drop one letter or the other and keep the better: max(${dp[i - 1]![j]}, ${dp[i]![j - 1]}) = ${v}.`,
            });
        }
    }
    out.push({ pointers: [], grid: table([{ r: m, c: n, tone: "hit" }]), lines: [9], read: read(String(dp[m]![n])), work: work(), result: String(dp[m]![n]), say: `The corner holds the answer: the longest common subsequence has length ${dp[m]![n]}, after filling ${filled} cells.` });
    return out;
}

// ---------------------------------------------------------------- 2-D DP: grid path

const pathCode = [
    "def min_path(grid):",
    "    R, C = len(grid), len(grid[0])",
    "    dp = [[0] * C for _ in range(R)]",
    "    for r in range(R):",
    "        for c in range(C):",
    "            best = min(dp[r - 1][c] if r else inf,",
    "                       dp[r][c - 1] if c else inf)",
    "            dp[r][c] = grid[r][c] + (best if best < inf else 0)",
    "    return dp[-1][-1]",
];

function simulatePath(input: DemoInput): Frame[] {
    const g = input.items.map((row) => [...row].map(Number));
    const R = g.length;
    const C = g[0]!.length;
    const dp: (number | null)[][] = g.map((row) => row.map(() => null));
    let filled = 0;
    const table = (marks: GridMark[]): NonNullable<Frame["grid"]> => ({
        cells: g.map((row, r) => row.map((x, c) => (dp[r]![c] === null ? `(${x})` : String(dp[r]![c])))),
        marks,
        small: false,
    });
    const work = (): Work => ({
        title: "EACH CELL IS COMPUTED ONCE",
        bars: [{ label: "Cells filled", value: filled, of: R * C, tone: "good", ratio: true }, { label: "Routes to try one by one", value: Math.round(binom(R + C - 2, R - 1)), of: Math.max(Math.round(binom(R + C - 2, R - 1)), 1), tone: "bad" }],
        note: "Moving only right and down, the cheapest way to a cell is that cell's cost plus the cheaper of the cell above and the cell to the left. Each cell reads two others, so the whole grid costs one pass instead of listing every route.",
    });
    const out: Frame[] = [{ pointers: [], grid: table([]), lines: [2, 3], read: [{ label: "CHEAPEST TO HERE", value: "–" }], work: work(), say: "Find the cheapest route from the top left to the bottom right, moving only right or down. Each cell will hold the cheapest cost of reaching it; the cost in brackets is what the cell itself adds." }];
    for (let r = 0; r < R; r++) {
        for (let c = 0; c < C; c++) {
            const up = r > 0 ? dp[r - 1]![c]! : INF;
            const left = c > 0 ? dp[r]![c - 1]! : INF;
            const best = Math.min(up, left);
            dp[r]![c] = g[r]![c]! + (best === INF ? 0 : best);
            filled++;
            const marks: GridMark[] = [{ r, c, tone: "cur" }];
            if (r > 0) marks.push({ r: r - 1, c, tone: "read" });
            if (c > 0) marks.push({ r, c: c - 1, tone: "read" });
            out.push({ pointers: [], grid: table(marks), lines: [4, 5, 6, 7, 8], read: [{ label: "CHEAPEST TO HERE", value: dp[r]![c]! }], work: work(), say: best === INF ? `The start: it costs ${g[r]![c]} to stand here.` : `(${r}, ${c}) costs ${g[r]![c]}. Arriving from ${up <= left ? "above" : "the left"} (${show(best)}) is cheaper than from ${up <= left ? "the left" : "above"} (${show(up <= left ? left : up)}), so ${g[r]![c]} + ${best} = ${dp[r]![c]}.` });
        }
    }
    // trace the route back from the corner
    const path: GridMark[] = [];
    let r = R - 1;
    let c = C - 1;
    for (;;) {
        path.push({ r, c, tone: "hit" });
        if (r === 0 && c === 0) break;
        if (r === 0) c--;
        else if (c === 0) r--;
        else if (dp[r - 1]![c]! <= dp[r]![c - 1]!) r--;
        else c--;
    }
    out.push({ pointers: [], grid: table(path), lines: [9], read: [{ label: "CHEAPEST ROUTE", value: dp[R - 1]![C - 1]! }], work: work(), result: String(dp[R - 1]![C - 1]), say: `The corner holds the answer, ${dp[R - 1]![C - 1]}. Following the cheaper neighbour back from the corner traces the route itself.` });
    return out;
}

const binom = (n: number, k: number): number => {
    let x = 1;
    for (let i = 1; i <= k; i++) x = (x * (n - k + i)) / i;
    return x;
};

// ---------------------------------------------------------------- greedy: jump game

const jumpCode = [
    "def can_jump(nums):",
    "    reach = 0  # the farthest index known to be reachable",
    "    for i, x in enumerate(nums):",
    "        if i > reach:",
    "            return False  # stranded",
    "        reach = max(reach, i + x)",
    "    return True",
];

function simulateJump(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const n = a.length;
    // jump sequences a plain search may try
    const memo: number[] = [];
    const calls = (i: number): number => (memo[i] ??= i >= n - 1 ? 1 : 1 + Array.from({ length: a[i]! }, (_, k) => calls(i + k + 1)).reduce((s, x) => s + x, 0));
    const naive = calls(0);
    let reach = 0;
    const work = (looked: number): Work => ({
        title: "ONE NUMBER IS ALL IT REMEMBERS",
        bars: [{ label: "Items looked at", value: looked, of: n, tone: "good", ratio: true }, { label: "Jumps a search may try", value: naive, of: Math.max(naive, n), tone: "bad" }],
        note: "Reaching an index means every index before it is reachable too, so the whole set of reachable places is one number: the farthest. Taking the best at each step can never hurt, so there is nothing to undo.",
    });
    const picture = (i: number, ptr: boolean): Pick<Frame, "pointers" | "ok" | "dim" | "cur"> => ({
        pointers: [...(ptr ? [{ at: i, label: "i", tone: "r" as const }] : []), { at: Math.min(reach, n - 1), label: "reach", tone: "m" as const, below: true }],
        ok: a.map((_, j) => j).filter((j) => j <= reach && j !== i),
        dim: a.map((_, j) => j).filter((j) => j > reach),
        cur: ptr ? [i] : [],
    });
    const out: Frame[] = [{ ...picture(0, false), lines: [2], read: [{ label: "REACH", value: 0 }], work: work(0), say: "Each number is the farthest you can jump from that index. Track the farthest index you can possibly reach, and walk forward as long as you are inside it." }];
    for (let i = 0; i < n; i++) {
        if (i > reach) {
            out.push({ ...picture(i, true), lines: [3, 4, 5], read: [{ label: "REACH", value: reach }], work: work(i), result: "stranded", say: `Index ${i} is beyond the reach of ${reach}: nothing before it can jump this far, so the end cannot be reached.` });
            return out;
        }
        const before = reach;
        reach = Math.max(reach, i + a[i]!);
        const done = reach >= n - 1;
        out.push({ ...picture(i, true), lines: [3, 6], read: [{ label: "REACH", value: reach }], work: work(i + 1), result: done ? "reachable" : undefined, say: reach > before ? `At ${i} you can jump ${a[i]}, up to ${i + a[i]!}. That is farther than ${before}, so reach becomes ${reach}.${done ? " It covers the last index: the end is reachable." : ""}` : `At ${i} you can jump ${a[i]}, only to ${i + a[i]!}. That does not beat the reach of ${reach}, so nothing changes.` });
        if (done) return out;
    }
    return out;
}

// ---------------------------------------------------------------- greedy: Kadane

const kadaneCode = [
    "def max_subarray(nums):",
    "    best = cur = nums[0]",
    "    for x in nums[1:]:",
    "        cur = max(x, cur + x)  # extend, or restart here",
    "        best = max(best, cur)",
    "    return best",
];

function simulateKadane(input: DemoInput): Frame[] {
    const a = input.items.map(Number);
    const n = a.length;
    let cur = a[0]!;
    let best = a[0]!;
    let start = 0;
    let bs = 0;
    let be = 1;
    const work = (i: number): Work => ({
        title: "ONE NUMBER PER STEP",
        bars: [{ label: "Subarrays a brute force sums", value: ((i + 1) * (i + 2)) / 2, of: (n * (n + 1)) / 2, tone: "bad", ratio: true }, { label: "Steps here", value: i + 1, of: (n * (n + 1)) / 2, tone: "good" }],
        note: "A running sum that has gone negative can only hurt whatever comes next, so the best subarray ending here either extends the last one or starts fresh. That one decision per item replaces checking every start and end.",
    });
    const frame = (i: number, say: string, lines: number[], result?: string): Frame => ({
        win: [start, i + 1],
        hit: Array.from({ length: be - bs }, (_, k) => bs + k),
        pointers: [{ at: start, label: "start", tone: "l", below: true }, { at: i, label: "i", tone: "r" }],
        lines,
        read: [{ label: "SUM OF THE RUN", value: cur }, { label: "BEST SO FAR", value: best }] as Reading[],
        work: work(i),
        say,
        result,
    });
    const out: Frame[] = [frame(0, `Start with the first number: ${a[0]} is both the current run and the best so far.`, [2])];
    for (let i = 1; i < n; i++) {
        const x = a[i]!;
        const extend = cur + x;
        if (x > extend) {
            start = i;
            cur = x;
        } else cur = extend;
        if (cur > best) {
            best = cur;
            bs = start;
            be = i + 1;
        }
        out.push(frame(i, x > extend ? `${x} on its own beats extending the old run (${extend}), because the old run was dragging it down. Start a new run here.` : `Extend the run: ${extend - x} + ${x} = ${cur}. That is at least as good as starting at ${x}.`, [3, 4, 5]));
    }
    out.push({ ...frame(n - 1, `The best run is ${a.slice(bs, be).join(", ")}, with sum ${best}, found in a single pass.`, [6], String(best)), win: [bs, be] });
    return out;
}

const numbersField = (min: number, max: number) => (text: string) => {
    const items = numbers(text);
    return items.length >= min && items.length <= max && smallInts(items) ? { items } : null;
};

export const TABLE_DEMOS: Record<string, Demo> = {
    "dp-coins": {
        title: "Coin change · fewest coins",
        code: coinsCode,
        initial: { items: ["1", "2", "5"], param: 11 },
        noRow: true,
        fields: { text: "Coin values, separated by commas", param: { label: "Amount", min: 1, max: 12 } },
        parse: (text, param) => {
            const items = numbers(text);
            const k = Number(param);
            return items.length >= 1 && items.length <= 4 && items.every((x) => /^[1-9]\d?$/.test(x)) && Number.isInteger(k) && param.trim() !== "" && k >= 1 && k <= 12 ? { items, param: k } : null;
        },
        simulate: simulateCoins,
    },
    "dp-rob": {
        title: "Take or skip · house robber",
        code: robCode,
        initial: { items: ["2", "7", "9", "3", "1", "8"] },
        noRow: true,
        fields: { text: "House values, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= 8 && items.every((x) => /^\d{1,2}$/.test(x)) ? { items } : null;
        },
        simulate: simulateRob,
    },
    "dp2-lcs": {
        title: "Longest common subsequence",
        code: lcsCode,
        initial: { items: ["abcde", "ace"] },
        noRow: true,
        fields: { text: "Two words, separated by a comma" },
        parse: (text) => {
            const items = text.toLowerCase().split(/[\s,]+/).filter(Boolean);
            return items.length === 2 && items.every((w) => /^[a-z]{1,5}$/.test(w)) ? { items } : null;
        },
        simulate: simulateLcs,
    },
    "dp2-path": {
        title: "Cheapest path through a grid",
        code: pathCode,
        initial: { items: ["131", "151", "421"] },
        noRow: true,
        fields: { text: "Rows of digits, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= 4 && items.every((r) => /^[1-9]{2,4}$/.test(r) && r.length === items[0]!.length) ? { items } : null;
        },
        simulate: simulatePath,
    },
    "greedy-jump": {
        title: "Jump game · the farthest reach",
        code: jumpCode,
        initial: { items: ["2", "3", "1", "1", "4"] },
        fields: { text: "Jump lengths, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= MAX_CELLS && items.every((x) => /^\d$/.test(x)) ? { items } : null;
        },
        simulate: simulateJump,
    },
    "greedy-kadane": {
        title: "Best run · extend or restart",
        code: kadaneCode,
        initial: { items: ["-2", "1", "-3", "4", "-1", "2", "1", "-5", "4"] },
        fields: { text: "Numbers, separated by commas" },
        parse: (text) => numbersField(2, MAX_CELLS)(text),
        simulate: simulateKadane,
    },
};
