// The animations of the Graphs and Advanced Graphs Idea tabs: breadth-first search with a queue, flood fill on a grid, Dijkstra and union-find.
import { MAX_CELLS, numbers } from "./ideaDemoKit.ts";
import type { Demo, DemoInput, Frame, GEdge, GNode, GridMark, Reading, Work } from "./ideaDemos.ts";

/** Nodes 0..n-1 on a circle, so any small graph is laid out without a layout step. */
const ring = (n: number, tone: (i: number) => GNode["tone"], sub?: (i: number) => string | undefined): GNode[] =>
    Array.from({ length: n }, (_, i) => {
        const a = (2 * Math.PI * i) / n - Math.PI / 2;
        return { id: i, x: 0.5 + 0.46 * Math.cos(a), y: 0.5 + 0.46 * Math.sin(a), label: String(i), tone: tone(i), sub: sub?.(i) };
    });

/** Edges written `a-b` (or `a-b:w` with a weight). */
function parseEdges(items: string[], weighted: boolean): { a: number; b: number; w: number }[] | null {
    const re = weighted ? /^(\d)-(\d):(\d{1,2})$/ : /^(\d)-(\d)$/;
    const out: { a: number; b: number; w: number }[] = [];
    for (const it of items) {
        const m = re.exec(it);
        if (!m || m[1] === m[2]) return null;
        out.push({ a: Number(m[1]), b: Number(m[2]), w: weighted ? Number(m[3]) : 1 });
    }
    return out;
}
const nodeCount = (es: { a: number; b: number }[]) => Math.max(...es.flatMap((e) => [e.a, e.b])) + 1;

// ---------------------------------------------------------------- breadth-first search

const bfsCode = [
    "def bfs(graph, start):",
    "    dist = {start: 0}",
    "    queue = deque([start])",
    "    while queue:",
    "        node = queue.popleft()",
    "        for nxt in graph[node]:",
    "            if nxt not in dist:  # first time seen",
    "                dist[nxt] = dist[node] + 1",
    "                queue.append(nxt)",
    "    return dist",
];

function simulateBfs(input: DemoInput): Frame[] {
    const es = parseEdges(input.items, false)!;
    const n = nodeCount(es);
    const start = input.param ?? 0;
    const adj: number[][] = Array.from({ length: n }, () => []);
    for (const e of es) {
        adj[e.a]!.push(e.b);
        adj[e.b]!.push(e.a);
    }
    const dist = new Map<number, number>([[start, 0]]);
    const used = new Set<string>();
    let queue = [start];
    let cur = -1;
    let looked = 0;
    const key = (a: number, b: number) => `${Math.min(a, b)}-${Math.max(a, b)}`;
    const frame = (say: string, lines: number[], top?: "push" | "pop", result?: string): Frame => ({
        pointers: [],
        graph: {
            nodes: ring(n, (i) => (i === cur ? "cur" : queue.includes(i) ? "front" : dist.has(i) ? "seen" : undefined), (i) => (dist.has(i) ? `d=${dist.get(i)}` : undefined)),
            edges: es.map((e): GEdge => ({ a: e.a, b: e.b, tone: used.has(key(e.a, e.b)) ? "on" : undefined })),
        },
        queue: { label: "QUEUE", items: queue.map(String), top },
        lines,
        read: [{ label: "FOUND", value: dist.size }, { label: "FARTHEST", value: Math.max(...dist.values()) }] as Reading[],
        work: {
            title: "EACH NODE AND EACH EDGE IS LOOKED AT ONCE",
            bars: [{ label: "Nodes taken from the queue", value: dist.size - queue.length - (cur >= 0 ? 0 : 0), of: n, tone: "good", ratio: true }, { label: "Neighbours checked", value: looked, of: 2 * es.length, tone: "bad", ratio: true }],
            note: "A node is queued the first time it is seen and never again, so the queue handles each node once. Every edge is looked at from both ends. The total is O(V + E), and because it goes level by level the first time a node is seen is by the fewest edges.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame(`Breadth-first search from ${start}: it finds every node one edge away, then two, and so on, so the first time it sees a node is by the fewest edges.`, [2, 3])];
    while (queue.length) {
        cur = queue[0]!;
        queue = queue.slice(1);
        const fresh: number[] = [];
        for (const nx of adj[cur]!) {
            looked++;
            if (!dist.has(nx)) {
                dist.set(nx, dist.get(cur)! + 1);
                fresh.push(nx);
                used.add(key(cur, nx));
            }
        }
        queue = [...queue, ...fresh];
        out.push(frame(fresh.length ? `Take ${cur} (distance ${dist.get(cur)}). Its new neighbours ${fresh.join(" and ")} get distance ${dist.get(cur)! + 1} and join the queue.` : `Take ${cur} (distance ${dist.get(cur)}). Every neighbour was already seen, so nothing joins.`, [5, 6, 7, 8, 9], fresh.length ? "push" : "pop"));
    }
    cur = -1;
    const unreached = Array.from({ length: n }, (_, i) => i).filter((i) => !dist.has(i));
    out.push(frame(`The queue is empty. Distances from ${start}: ${[...dist].sort((a, b) => a[0] - b[0]).map(([k, d]) => `${k}→${d}`).join(", ")}${unreached.length ? `. ${unreached.join(", ")} cannot be reached` : ""}.`, [10], undefined, `${Math.max(...dist.values())} steps`));
    return out;
}

// ---------------------------------------------------------------- flood fill on a grid

const islandsCode = [
    "def num_islands(grid):",
    "    seen, count = set(), 0",
    "    for r in range(len(grid)):",
    "        for c in range(len(grid[0])):",
    "            if grid[r][c] == \"1\" and (r, c) not in seen:",
    "                count += 1  # a new island",
    "                flood(r, c)  # marks all of it seen",
    "    return count",
    "",
    "def flood(r, c):",
    "    if not inside(r, c) or grid[r][c] == \"0\" or (r, c) in seen:",
    "        return",
    "    seen.add((r, c))",
    "    for dr, dc in ((1, 0), (-1, 0), (0, 1), (0, -1)):",
    "        flood(r + dr, c + dc)",
];

function simulateIslands(input: DemoInput): Frame[] {
    const g = input.items;
    const R = g.length;
    const C = g[0]!.length;
    const land = g.flatMap((row, r) => [...row].map((ch, c) => (ch === "1" ? `${r},${c}` : null)).filter(Boolean) as string[]).length;
    const seen = new Set<string>();
    const done: string[] = [];
    const now: string[] = [];
    let count = 0;
    let curCell = "";
    const marks = (): GridMark[] => [
        ...done.map((k) => ({ r: +k.split(",")[0]!, c: +k.split(",")[1]!, tone: "done" as const })),
        ...now.map((k) => ({ r: +k.split(",")[0]!, c: +k.split(",")[1]!, tone: k === curCell ? ("cur" as const) : ("new" as const) })),
    ];
    const frame = (say: string, lines: number[], result?: string): Frame => ({
        pointers: [],
        grid: { cells: g.map((row) => [...row].map((ch) => (ch === "1" ? "▮" : "·"))), marks: marks(), small: C > 6 },
        lines,
        read: [{ label: "ISLANDS", value: count }, { label: "LAND SEEN", value: `${seen.size}/${land}` }] as Reading[],
        work: {
            title: "EVERY CELL IS FLOODED AT MOST ONCE",
            bars: [{ label: "Land cells flooded", value: seen.size, of: Math.max(land, 1), tone: "good", ratio: true }, { label: "Islands found", value: count, of: Math.max(land, 1), tone: "bad", ratio: true }],
            note: "The scan visits every cell once. Flood fill enters a land cell only if it is not in the seen set, so each land cell is flooded once however many neighbours point at it. Time is O(rows × columns).",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame("Scan the grid. Each time the scan reaches land that has not been seen, that is a new island: flood it, so the rest of its cells are never counted again.", [2, 3])];
    const flood = (r: number, c: number) => {
        if (r < 0 || c < 0 || r >= R || c >= C || g[r]![c] !== "1" || seen.has(`${r},${c}`)) return;
        seen.add(`${r},${c}`);
        now.push(`${r},${c}`);
        curCell = `${r},${c}`;
        out.push(frame(`Flood (${r}, ${c}): mark it seen, then try its four neighbours.`, [10, 13, 14, 15]));
        for (const [dr, dc] of [[1, 0], [-1, 0], [0, 1], [0, -1]] as const) flood(r + dr, c + dc);
    };
    for (let r = 0; r < R; r++) {
        for (let c = 0; c < C; c++) {
            if (g[r]![c] === "1" && !seen.has(`${r},${c}`)) {
                count++;
                now.length = 0;
                curCell = "";
                out.push(frame(`The scan reaches (${r}, ${c}): land that is not seen. That is island number ${count}.`, [5, 6, 7]));
                flood(r, c);
                done.push(...now);
                now.length = 0;
                curCell = "";
            }
        }
    }
    out.push(frame(`The scan reached the end. ${count} island${count === 1 ? "" : "s"}, and no land cell was visited twice.`, [8], String(count)));
    return out;
}

export const GRAPH_DEMOS: Record<string, Demo> = {
    "graph-bfs": {
        title: "Breadth-first · fewest edges",
        code: bfsCode,
        initial: { items: ["0-1", "0-2", "1-3", "2-3", "3-4", "4-5"], param: 0 },
        noRow: true,
        fields: { text: "Edges such as 0-1, separated by commas", param: { label: "Start node", min: 0, max: 6 } },
        parse: (text, param) => {
            const items = numbers(text);
            const es = items.length >= 2 && items.length <= MAX_CELLS - 1 ? parseEdges(items, false) : null;
            const s = Number(param);
            return es && Number.isInteger(s) && param.trim() !== "" && s >= 0 && s < nodeCount(es) ? { items, param: s } : null;
        },
        simulate: simulateBfs,
    },
    "grid-islands": {
        title: "Flood fill · count the islands",
        code: islandsCode,
        initial: { items: ["11000", "11000", "00100", "00011"] },
        noRow: true,
        fields: { text: "Rows of 0 and 1, separated by commas" },
        parse: (text) => {
            const items = numbers(text);
            return items.length >= 2 && items.length <= 5 && items.every((r) => /^[01]{2,7}$/.test(r) && r.length === items[0]!.length) ? { items } : null;
        },
        simulate: simulateIslands,
    },
};

// ---------------------------------------------------------------- Dijkstra

const dijkstraCode = [
    "def dijkstra(graph, start):",
    "    dist = {start: 0}",
    "    heap = [(0, start)]",
    "    while heap:",
    "        d, node = heappop(heap)  # closest not yet settled",
    "        if d > dist[node]:",
    "            continue  # a stale entry",
    "        for nxt, w in graph[node]:",
    "            nd = d + w",
    "            if nd < dist.get(nxt, inf):",
    "                dist[nxt] = nd  # a shorter way",
    "                heappush(heap, (nd, nxt))",
    "    return dist",
];

function simulateDijkstra(input: DemoInput): Frame[] {
    const es = parseEdges(input.items, true)!;
    const n = nodeCount(es);
    const start = input.param ?? 0;
    const adj: { to: number; w: number; e: number }[][] = Array.from({ length: n }, () => []);
    es.forEach((e, i) => {
        adj[e.a]!.push({ to: e.b, w: e.w, e: i });
        adj[e.b]!.push({ to: e.a, w: e.w, e: i });
    });
    const dist = new Map<number, number>([[start, 0]]);
    const settled = new Set<number>();
    const via = new Map<number, number>();
    let heap: [number, number][] = [[0, start]];
    let cur = -1;
    let hot = new Set<number>();
    let pushes = 1;
    const frame = (say: string, lines: number[], top?: "push" | "pop", result?: string): Frame => ({
        pointers: [],
        graph: {
            nodes: ring(n, (i) => (i === cur ? "cur" : settled.has(i) ? "done" : heap.some(([, v]) => v === i) ? "front" : undefined), (i) => (dist.has(i) ? String(dist.get(i)) : undefined)),
            edges: es.map((e, i): GEdge => ({ a: e.a, b: e.b, w: String(e.w), tone: hot.has(i) ? "new" : [...via.values()].includes(i) && settled.has(e.a) && settled.has(e.b) ? "on" : undefined })),
        },
        queue: { label: "HEAP · closest first", items: heap.map(([d, v]) => `${v}:${d}`), top },
        lines,
        read: [{ label: "SETTLED", value: settled.size }, { label: "SHORTEST SO FAR", value: [...dist].sort((a, b) => a[0] - b[0]).map(([k, d]) => `${k}=${d}`).join(" ") }] as Reading[],
        work: {
            title: "EACH EDGE IS RELAXED AT MOST TWICE",
            bars: [{ label: "Nodes settled", value: settled.size, of: n, tone: "good", ratio: true }, { label: "Heap pushes", value: pushes, of: 2 * es.length + 1, tone: "bad", ratio: true }],
            note: "A node's distance is final the moment it comes off the heap, because every edge is non-negative and anything still waiting is at least as far. Each edge can push at most once from each end, and each heap operation costs log V: O((V + E) log V).",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame(`Dijkstra from ${start}: always settle the closest node not yet settled. Its distance can no longer improve, because every other route starts at something at least as far away.`, [2, 3])];
    while (heap.length) {
        heap = [...heap].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
        const [d, node] = heap[0]!;
        heap = heap.slice(1);
        cur = node;
        hot = new Set();
        if (d > dist.get(node)!) {
            out.push(frame(`Take ${node}:${d} off the heap. A shorter way to ${node} (${dist.get(node)}) was found since, so this entry is stale: skip it.`, [5, 6, 7], "pop"));
            continue;
        }
        settled.add(node);
        const better: string[] = [];
        for (const { to, w, e } of adj[node]!) {
            if (settled.has(to)) continue;
            const nd = d + w;
            if (nd < (dist.get(to) ?? Infinity)) {
                dist.set(to, nd);
                via.set(to, e);
                heap = [...heap, [nd, to]];
                pushes++;
                hot.add(e);
                better.push(`${to} = ${nd}`);
            }
        }
        out.push(frame(better.length ? `Settle ${node} at distance ${d}. Going through it gives a shorter way to: ${better.join(", ")}.` : `Settle ${node} at distance ${d}. None of its edges give a shorter way.`, [5, 8, 9, 10, 11, 12], better.length ? "push" : "pop"));
    }
    cur = -1;
    hot = new Set();
    const far = Math.max(...dist.values());
    out.push(frame(`The heap is empty. Every reachable node is settled, in order of distance. The farthest is ${far} away.`, [13], undefined, `max ${far}`));
    return out;
}

// ---------------------------------------------------------------- Kruskal and union-find

const kruskalCode = [
    "def kruskal(n, edges):",
    "    parent = list(range(n))",
    "    def find(x):",
    "        while parent[x] != x:",
    "            parent[x] = parent[parent[x]]  # path halving",
    "            x = parent[x]",
    "        return x",
    "    total = 0",
    "    for w, a, b in sorted(edges):",
    "        ra, rb = find(a), find(b)",
    "        if ra != rb:  # different groups: no cycle",
    "            parent[ra] = rb",
    "            total += w",
    "    return total",
];

function simulateKruskal(input: DemoInput): Frame[] {
    const es = parseEdges(input.items, true)!;
    const n = nodeCount(es);
    const order = es.map((e, i) => ({ ...e, i })).sort((x, y) => x.w - y.w || x.i - y.i);
    const parent = Array.from({ length: n }, (_, i) => i);
    const find = (x: number): number => (parent[x] === x ? x : find(parent[x]!));
    const taken = new Set<number>();
    const skipped = new Set<number>();
    let total = 0;
    let left = order.map((e) => e.i);
    let at = -1;
    const groups = () => new Set(parent.map((_, i) => find(i))).size;
    const label = (i: number) => `${es[i]!.a}-${es[i]!.b}:${es[i]!.w}`;
    const frame = (say: string, lines: number[], top?: "pop", result?: string): Frame => ({
        pointers: [],
        graph: {
            nodes: ring(n, (i) => (at >= 0 && (es[at]!.a === i || es[at]!.b === i) ? "cur" : undefined), (i) => `g${find(i)}`),
            edges: es.map((e, i): GEdge => ({ a: e.a, b: e.b, w: String(e.w), tone: taken.has(i) ? "hit" : skipped.has(i) ? "cut" : i === at ? "new" : undefined })),
        },
        queue: { label: "EDGES · lightest first", items: left.map(label), top },
        lines,
        read: [{ label: "TOTAL WEIGHT", value: total }, { label: "GROUPS", value: groups() }] as Reading[],
        work: {
            title: "SORT ONCE, THEN ONE FIND PER ENDPOINT",
            bars: [{ label: "Edges looked at", value: taken.size + skipped.size, of: es.length, tone: "good", ratio: true }, { label: "Edges kept", value: taken.size, of: Math.max(n - 1, 1), tone: "bad", ratio: true }],
            note: "Sorting the edges costs E log E. Each edge then does two finds, which with path halving are close to constant. An edge is kept only when it joins two different groups, so a spanning tree stops at n − 1 edges.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame("Kruskal: take the lightest edge each time, and keep it unless it would close a cycle. \"Closes a cycle\" means both ends are already in the same group, which union-find answers fast.", [2, 9])];
    for (const e of order) {
        at = e.i;
        left = left.slice(1);
        const ra = find(e.a);
        const rb = find(e.b);
        if (ra !== rb) {
            parent[ra] = rb;
            total += e.w;
            taken.add(e.i);
            out.push(frame(`Edge ${e.a}-${e.b} (weight ${e.w}): the ends are in different groups, so keep it and merge them. Total ${total}.`, [10, 11, 12, 13], "pop"));
        } else {
            skipped.add(e.i);
            out.push(frame(`Edge ${e.a}-${e.b} (weight ${e.w}): both ends are already in the same group, so it would close a cycle. Skip it.`, [10, 11], "pop"));
        }
    }
    at = -1;
    const connected = groups() === 1;
    out.push(frame(connected ? `Every node is in one group: a minimum spanning tree of weight ${total}, using ${taken.size} edges.` : `The graph is not connected, so this is a spanning forest of weight ${total} with ${groups()} groups.`, [14], undefined, `weight ${total}`));
    return out;
}

const wedges = {
    fields: { text: "Weighted edges such as 0-1:4, separated by commas", param: { label: "Start node", min: 0, max: 6 } },
    parse: (text: string, param: string) => {
        const items = numbers(text);
        const es = items.length >= 2 && items.length <= MAX_CELLS - 1 ? parseEdges(items, true) : null;
        const s = Number(param);
        return es && Number.isInteger(s) && param.trim() !== "" && s >= 0 && s < nodeCount(es) ? { items, param: s } : null;
    },
};

export const ADV_GRAPH_DEMOS: Record<string, Demo> = {
    "graph-dijkstra": { title: "Dijkstra · cheapest route", code: dijkstraCode, initial: { items: ["0-1:4", "0-2:1", "2-1:2", "1-3:1", "2-3:5", "3-4:3"], param: 0 }, noRow: true, ...wedges, simulate: simulateDijkstra },
    "graph-kruskal": { title: "Kruskal · spanning tree", code: kruskalCode, initial: { items: ["0-1:4", "0-2:1", "2-1:2", "1-3:1", "2-3:5", "3-4:3"], param: 0 }, noRow: true, fields: { text: wedges.fields.text }, parse: (text: string) => { const r = wedges.parse(text, "0"); return r ? { items: r.items } : null; }, simulate: simulateKruskal },
};
