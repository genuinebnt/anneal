// The animations of the Backtracking Idea tab: a decision tree of subsets that grows and undoes, and N-Queens with pruning on a board.
import { numbers, smallInts, treeEdges, treeNodes } from "./ideaDemoKit.ts";
import type { Demo, DemoInput, Frame, GridMark, Reading, Work } from "./ideaDemos.ts";

const subsetsCode = [
    "def subsets(nums):",
    "    res, path = [], []",
    "    def go(i):",
    "        if i == len(nums):",
    "            res.append(path[:])  # a complete choice",
    "            return",
    "        go(i + 1)  # skip nums[i]",
    "        path.append(nums[i])  # choose it",
    "        go(i + 1)",
    "        path.pop()  # undo: backtrack",
    "    go(0)",
    "    return res",
];

function simulateSubsets(input: DemoInput): Frame[] {
    const a = input.items;
    const n = a.length;
    const total = 2 ** (n + 1) - 1;
    const labels = Array.from({ length: total }, (_, i) => {
        if (i === 0) return "·";
        const parent = (i - 1) >> 1;
        const depth = Math.floor(Math.log2(parent + 1));
        return i % 2 === 1 ? "–" : `+${a[depth]}`;
    });
    const reached = new Set<number>();
    const done = new Set<number>();
    const path: string[] = [];
    const onPath: number[] = [];
    const res: string[] = [];
    let undos = 0;
    let cur = -1;
    const fmt = (p: string[]) => `[${p.join(",")}]`;
    const frame = (say: string, lines: number[], top?: "push" | "pop", result?: string): Frame => ({
        pointers: [],
        graph: {
            h: 250,
            w: Math.max(380, 2 ** n * 52),
            nodes: treeNodes(labels, (i) => (i === cur ? "cur" : onPath.includes(i) ? "front" : done.has(i) ? "seen" : undefined), (i) => (done.has(i) && i >= 2 ** n - 1 ? res[i - (2 ** n - 1)] : undefined)).filter((nd) => reached.has(nd.id)),
            edges: treeEdges(labels, (c) => (onPath.includes(c) ? "on" : undefined)).filter((e) => reached.has(e.b)),
        },
        stack: { label: "PATH", items: [...path], top },
        lines,
        read: [{ label: "SUBSETS FOUND", value: res.length }, { label: "UNDOS", value: undos }] as Reading[],
        work: {
            title: "EVERY CHOICE IS MADE, EXPLORED AND UNDONE",
            bars: [{ label: "Subsets found", value: res.length, of: 2 ** n, tone: "good", ratio: true }, { label: "Undos (path.pop)", value: undos, of: 2 ** n - 1, tone: "bad", ratio: true }],
            note: `Each of the ${n} items is either skipped or taken, so there are 2^${n} = ${2 ** n} subsets and the decision tree has that many leaves. Backtracking does not remember the tree: it keeps one path and undoes the last choice to move to the next branch. Time is O(n · 2^n), because each subset is copied out.`,
        } as Work,
        say,
        result,
    });
    reached.add(0);
    const out: Frame[] = [frame(`Every item is either skipped or taken. The tree below is every way to decide; backtracking walks it depth-first and keeps only the current path.`, [3, 11])];
    const go = (i: number, node: number) => {
        reached.add(node);
        cur = node;
        if (i === n) {
            res.push(fmt(path));
            done.add(node);
            out.push(frame(`All ${n} items decided: ${fmt(path)} is a complete subset. Record a copy.`, [4, 5, 6]));
            return;
        }
        out.push(frame(`Decide ${a[i]}: first skip it.`, [4, 7], path.length ? undefined : undefined));
        onPath.push(node);
        go(i + 1, 2 * node + 1);
        path.push(a[i]!);
        onPath.pop();
        onPath.push(node);
        cur = node;
        out.push(frame(`Back at ${a[i]}: now take it. Put ${a[i]} on the path.`, [8], "push"));
        go(i + 1, 2 * node + 2);
        path.pop();
        undos++;
        onPath.pop();
        done.add(node);
        cur = node;
        out.push(frame(`Both branches of ${a[i]} are done. Take ${a[i]} off the path (undo), so the caller sees the path as it was.`, [9, 10], "pop"));
    };
    go(0, 0);
    cur = -1;
    out.push(frame(`The tree is finished: ${res.length} subsets, ${res.join(" ")}.`, [12], undefined, `${res.length} subsets`));
    return out;
}

const queensCode = [
    "def solve(n):",
    "    cols, d1, d2 = set(), set(), set()",
    "    def place(r):",
    "        if r == n:",
    "            return True  # every row has a queen",
    "        for c in range(n):",
    "            if c in cols or r - c in d1 or r + c in d2:",
    "                continue  # attacked: prune",
    "            cols.add(c); d1.add(r - c); d2.add(r + c)  # choose",
    "            if place(r + 1):",
    "                return True",
    "            cols.remove(c); d1.remove(r - c); d2.remove(r + c)  # undo",
    "        return False",
    "    return place(0)",
];

function simulateQueens(input: DemoInput): Frame[] {
    const n = input.param ?? 4;
    const q: number[] = [];
    let tries = 0;
    let backs = 0;
    const attacker = (r: number, c: number) => q.findIndex((qc, qr) => qc === c || qr - qc === r - c || qr + qc === r + c);
    const frame = (say: string, lines: number[], extra: GridMark[], result?: string): Frame => ({
        pointers: [],
        grid: {
            cells: Array.from({ length: n }, (_, r) => Array.from({ length: n }, (_, c) => (q[r] === c ? "♛" : "·"))),
            marks: [...q.map((c, r): GridMark => ({ r, c, tone: "done" })), ...extra],
            small: n > 4,
        },
        lines,
        read: [{ label: "QUEENS PLACED", value: q.length }, { label: "BACKTRACKS", value: backs }] as Reading[],
        work: {
            title: "PRUNING SKIPS WHOLE BRANCHES",
            bars: [{ label: "Squares tried", value: tries, of: n ** n, tone: "good", ratio: true }, { label: "Full boards a brute force checks", value: n ** n, of: n ** n, tone: "bad" }],
            note: `A brute force would check all ${n}^${n} = ${n ** n} ways to put one queen in each row. Backtracking drops a branch the moment two queens attack, so everything below that square is never looked at.`,
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame(`Put ${n} queens on an ${n}×${n} board so none attack another. Go row by row, try each column, and when a row has no safe square, undo the previous row's queen and try its next column.`, [3, 14], [])];
    const place = (r: number): boolean => {
        if (r === n) {
            out.push(frame(`Every row has a queen and none attacks another: a solution.`, [4, 5], [], `columns ${q.map((c) => c + 1).join(" ")}`));
            return true;
        }
        for (let c = 0; c < n; c++) {
            tries++;
            const by = attacker(r, c);
            if (by >= 0) {
                out.push(frame(`Row ${r + 1}, column ${c + 1}: attacked by the queen in row ${by + 1}. Prune: skip it.`, [6, 7, 8], [{ r, c, tone: "bad" }]));
                continue;
            }
            q.push(c);
            out.push(frame(`Row ${r + 1}, column ${c + 1}: safe. Place a queen and move to row ${r + 2}.`, [6, 9, 10], [{ r, c, tone: "cur" }]));
            if (place(r + 1)) return true;
            q.pop();
            backs++;
            out.push(frame(`Nothing fits in row ${r + 2} from here, so take the queen in row ${r + 1} off (undo) and try the next column.`, [11, 12], [{ r, c, tone: "bad" }]));
        }
        if (r > 0) out.push(frame(`No column in row ${r + 1} is safe. Return to the row above.`, [13], []));
        return false;
    };
    place(0);
    return out;
}

export const SEARCH_DEMOS: Record<string, Demo> = {
    "bt-subsets": {
        title: "All subsets · choose, explore, undo",
        code: subsetsCode,
        initial: { items: ["1", "2", "3"] },
        noRow: true,
        fields: { text: "Two or three numbers, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= 3 && smallInts(items) ? { items } : null;
        },
        simulate: simulateSubsets,
    },
    "bt-queens": {
        title: "N-Queens · prune the impossible",
        code: queensCode,
        initial: { items: [], param: 4 },
        noRow: true,
        fields: { text: "Nothing to type: change the board size", param: { label: "Board size", min: 4, max: 5 } },
        parse: (_text, param) => {
            const k = Number(param);
            return Number.isInteger(k) && k >= 4 && k <= 5 ? { items: [], param: k } : null;
        },
        simulate: simulateQueens,
    },
};
