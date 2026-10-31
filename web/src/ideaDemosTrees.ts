// The animations of the Trees Idea tab: a depth-first walk with its call stack, and a breadth-first walk with its queue.
import { levelOf, parseTree, treeEdges, treeNodes, treeShape } from "./ideaDemoKit.ts";
import type { Demo, Frame, Pointer, Reading, Work } from "./ideaDemos.ts";

const dfsCode = [
    "def preorder(node, out):",
    "    if node is None:  # nothing here",
    "        return",
    "    out.append(node.val)  # visit",
    "    preorder(node.left, out)",
    "    preorder(node.right, out)",
];

function simulateDfs(input: { items: string[] }): Frame[] {
    const v = input.items;
    const ex = treeShape(v);
    const n = ex.filter(Boolean).length;
    const kids = (i: number) => [2 * i + 1, 2 * i + 2].filter((c) => c < v.length && ex[c]);
    const height = Math.max(...v.map((_, i) => (ex[i] ? levelOf(i) + 1 : 0)));
    const order: number[] = [];
    const path: number[] = [];
    const none: Pointer[] = [];
    const frame = (say: string, lines: number[], top?: "push" | "pop", result?: string): Frame => ({
        pointers: none,
        graph: {
            nodes: treeNodes(v, (i) => (path[path.length - 1] === i ? "cur" : path.includes(i) ? "front" : order.includes(i) ? "seen" : undefined)),
            edges: treeEdges(v, (c) => (order.includes(c) ? "on" : undefined)),
        },
        stack: { label: "CALL STACK", items: path.map((i) => v[i]!), top },
        lines,
        read: [{ label: "VISITED", value: order.map((i) => v[i]).join(" ") || "none" }, { label: "STACK DEPTH", value: path.length }] as Reading[],
        work: {
            title: "EVERY NODE IS ENTERED ONCE",
            bars: [{ label: "Nodes visited", value: order.length, of: n, tone: "good", ratio: true }, { label: "Stack depth", value: path.length, of: height, tone: "bad", ratio: true }],
            note: "Each node is visited exactly once, so the time is O(n). The stack only ever holds the path from the root down to where you are, so the memory is the height of the tree, not its size.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame("A depth-first walk goes as deep as it can before it backs up. The call stack remembers the way back.", [1])];
    const go = (i: number) => {
        path.push(i);
        order.push(i);
        const k = kids(i);
        out.push(frame(`Enter ${v[i]}: visit it (add it to the order), then go left.`, [2, 4, 5], "push"));
        for (const c of k) go(c);
        out.push(frame(k.length ? `Both subtrees of ${v[i]} are done. Return to ${path.length > 1 ? v[path[path.length - 2]!] : "the caller"}.` : `${v[i]} has no children: both calls hit \`None\` and return straight away.`, k.length ? [6] : [2, 3], "pop"));
        path.pop();
    };
    go(0);
    out.push({ ...frame(`The stack is empty and every node was visited once, in the order ${order.map((i) => v[i]).join(", ")}. That is preorder: a node before its children.`, [6], undefined, order.map((i) => v[i]).join(" ")), stack: { label: "CALL STACK", items: [] } });
    return out;
}

const bfsCode = [
    "def level_order(root):",
    "    queue = deque([root])",
    "    while queue:",
    "        node = queue.popleft()  # oldest first",
    "        visit(node)",
    "        for child in (node.left, node.right):",
    "            if child:",
    "                queue.append(child)",
];

function simulateBfs(input: { items: string[] }): Frame[] {
    const v = input.items;
    const ex = treeShape(v);
    const n = ex.filter(Boolean).length;
    const kids = (i: number) => [2 * i + 1, 2 * i + 2].filter((c) => c < v.length && ex[c]);
    const order: number[] = [];
    let queue: number[] = [0];
    let widest = 1;
    let cur = -1;
    const frame = (say: string, lines: number[], top?: "push" | "pop", result?: string): Frame => ({
        pointers: [],
        graph: {
            nodes: treeNodes(v, (i) => (i === cur ? "cur" : queue.includes(i) ? "front" : order.includes(i) ? "seen" : undefined)),
            edges: treeEdges(v, (c) => (order.includes(c) || queue.includes(c) ? "on" : undefined)),
        },
        queue: { label: "QUEUE", items: queue.map((i) => v[i]!), top },
        lines,
        read: [{ label: "VISITED", value: order.map((i) => v[i]).join(" ") || "none" }, { label: "LEVEL", value: cur >= 0 ? levelOf(cur) : "–" }] as Reading[],
        work: {
            title: "EVERY NODE IS QUEUED ONCE",
            bars: [{ label: "Nodes visited", value: order.length, of: n, tone: "good", ratio: true }, { label: "Widest the queue got", value: widest, of: n, tone: "bad", ratio: true }],
            note: "Each node joins the queue once and leaves once: O(n) time. The queue can hold a whole level at a time, which for a bushy tree is about half the nodes, so BFS needs more memory than DFS, which only held one path.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame("A breadth-first walk goes level by level. The queue holds the nodes found but not visited yet, oldest at the front.", [1, 2])];
    while (queue.length) {
        cur = queue[0]!;
        queue = queue.slice(1);
        order.push(cur);
        const k = kids(cur);
        queue = [...queue, ...k];
        widest = Math.max(widest, queue.length);
        out.push(frame(k.length ? `Take ${v[cur]} from the front and visit it. Its children ${k.map((c) => v[c]).join(" and ")} go to the back, behind everything already waiting.` : `Take ${v[cur]} from the front and visit it. It has no children, so nothing joins the queue.`, [3, 4, 5, 6, 7], k.length ? "push" : "pop"));
    }
    cur = -1;
    out.push(frame(`The queue is empty. Nodes came out level by level: ${order.map((i) => v[i]).join(", ")}.`, [3], undefined, order.map((i) => v[i]).join(" ")));
    return out;
}

const initial = { items: ["4", "2", "6", "1", "3", "5", "7"] };
const base = {
    initial,
    noRow: true,
    fields: { text: "A tree level by level, with _ for an empty place" },
    parse: (text: string) => {
        const items = parseTree(text);
        return items ? { items } : null;
    },
};

export const TREE_DEMOS: Record<string, Demo> = {
    "tree-dfs": { ...base, title: "Depth-first · the call stack", code: dfsCode, simulate: simulateDfs },
    "tree-bfs": { ...base, title: "Breadth-first · the queue", code: bfsCode, simulate: simulateBfs },
};
