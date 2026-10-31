// The animations of the Heap / Priority Queue and Tries Idea tabs: a min-heap sifting up and down, a size-k heap over a stream, and a trie growing.
import { MAX_CELLS, numbers, smallInts, treeEdges, treeNodes } from "./ideaDemoKit.ts";
import type { Demo, DemoInput, Frame, GEdge, GNode, Pointer, Reading, Work } from "./ideaDemos.ts";

// ---------------------------------------------------------------- heap

const heapCode = [
    "def push(h, x):",
    "    h.append(x)",
    "    i = len(h) - 1",
    "    while i and h[i] < h[(i - 1) // 2]:",
    "        swap(h, i, (i - 1) // 2)  # smaller rises",
    "        i = (i - 1) // 2",
    "def pop(h):",
    "    top, h[0] = h[0], h[-1]  # last item to the root",
    "    h.pop()",
    "    sift_down(h, 0)  # larger sinks",
    "    return top",
];

/** Moves the item at `i` up while it is smaller than its parent; calls `step` for each swap. */
function siftUp(h: number[], i: number, step: (a: number, b: number) => void) {
    while (i > 0) {
        const p = (i - 1) >> 1;
        if (h[i]! >= h[p]!) break;
        [h[i], h[p]] = [h[p]!, h[i]!];
        step(i, p);
        i = p;
    }
    return i;
}

/** Moves the item at `i` down while a child is smaller; calls `step` for each swap. */
function siftDown(h: number[], i: number, step: (a: number, b: number) => void) {
    for (;;) {
        const l = 2 * i + 1;
        const r = l + 1;
        let m = i;
        if (l < h.length && h[l]! < h[m]!) m = l;
        if (r < h.length && h[r]! < h[m]!) m = r;
        if (m === i) return i;
        [h[i], h[m]] = [h[m]!, h[i]!];
        step(i, m);
        i = m;
    }
}

const heapPicture = (h: number[], cur: number[], moved: number[]): Pick<Frame, "graph" | "cells"> => ({
    cells: h.map(String),
    graph: {
        nodes: treeNodes(h.map(String), (i) => (cur.includes(i) ? "cur" : moved.includes(i) ? "front" : undefined)),
        edges: treeEdges(h.map(String), () => undefined),
        h: 200,
    },
});

function simulateHeap(input: DemoInput): Frame[] {
    const xs = input.items.map(Number);
    const pops = input.param ?? 0;
    const h: number[] = [];
    const popped: number[] = [];
    let swaps = 0;
    let shifts = 0;
    const sorted: number[] = [];
    const work = (): Work => ({
        title: "WORK SO FAR",
        bars: [{ label: "Heap: swaps", value: swaps, of: Math.max(swaps, shifts, 1), tone: "good" }, { label: "Sorted list: items shifted", value: shifts, of: Math.max(swaps, shifts, 1), tone: "bad" }],
        note: "A sorted list keeps everything in order, so each insert shifts every larger item along. A heap only keeps the smallest on top: an item climbs at most one level per comparison, and a tree of n items is only about log n levels tall.",
    });
    const read = (): Reading[] => [{ label: "SMALLEST (ROOT)", value: h.length ? h[0]! : "–" }, { label: "POPPED", value: popped.join(" ") || "none" }];
    const frame = (say: string, lines: number[], cur: number[], moved: number[], ptr: Pointer[], result?: string): Frame => ({ ...heapPicture(h, cur, moved), pointers: ptr, lines, read: read(), work: work(), say, result });
    const out: Frame[] = [frame("A min-heap keeps its smallest item at the top, in an array: the children of place i are 2i+1 and 2i+2. Push the numbers one at a time, then pop the smallest.", [1], [], [], [])];
    for (const x of xs) {
        shifts += sorted.filter((y) => y > x).length;
        sorted.push(x);
        sorted.sort((a, b) => a - b);
        h.push(x);
        let at = h.length - 1;
        out.push(frame(`Push ${x}: add it at the end, the next free place (${at}).`, [2, 3], [at], [], [{ at, label: "i", tone: "r" }]));
        const trail: number[] = [];
        siftUp(h, at, (a, b) => {
            swaps++;
            trail.push(b);
            at = b;
            out.push(frame(`${h[b]} is smaller than its parent ${h[a]}, so swap them: it rises one level.`, [4, 5, 6], [b], [a], [{ at: b, label: "i", tone: "r" }]));
        });
        if (trail.length === 0) out.push(frame(`${x} is not smaller than its parent, so it stays. A push costs one comparison here.`, [4], [at], [], [{ at, label: "i", tone: "r" }]));
    }
    for (let k = 0; k < pops && h.length; k++) {
        const top = h[0]!;
        popped.push(top);
        const last = h.pop()!;
        if (h.length) h[0] = last;
        out.push(frame(`Pop: the smallest is ${top}. Move the last item (${last}) to the root, so the array has no hole.`, [7, 8, 9], h.length ? [0] : [], [], h.length ? [{ at: 0, label: "i", tone: "r" }] : []));
        let at = 0;
        siftDown(h, 0, (a, b) => {
            swaps++;
            at = b;
            out.push(frame(`${h[b]} is smaller than ${h[a]}, one of the root's children, so swap them: the larger item sinks one level.`, [10], [b], [a], [{ at: b, label: "i", tone: "r" }]));
        });
        void at;
    }
    out.push(frame(`Popped ${popped.join(", ")}: the smallest ${popped.length} in order, and each took about log n swaps, never a full sort.`, [11], [], [], [], popped.join(" ")));
    return out;
}

const topkCode = [
    "def top_k(stream, k):",
    "    h = []  # a min-heap of the k largest so far",
    "    for x in stream:",
    "        if len(h) < k:",
    "            heappush(h, x)",
    "        elif x > h[0]:  # beats the smallest of the k",
    "            heapreplace(h, x)",
    "    return h",
];

function simulateTopK(input: DemoInput): Frame[] {
    const xs = input.items.map(Number);
    const k = input.param ?? 1;
    const n = xs.length;
    const h: number[] = [];
    let ops = 0;
    const work = (): Work => ({
        title: "THE HEAP NEVER GROWS PAST k",
        bars: [{ label: "Heap size", value: h.length, of: n, tone: "good", ratio: true }, { label: "Sort all of them", value: Math.min(n, ops + 0 || 1) && n, of: n, tone: "bad", ratio: true }],
        note: `Sorting keeps all ${n} items. The heap keeps only the ${k} largest seen so far, with the smallest of them on top: a new item either beats that one and replaces it, or is thrown away at once. Each item costs at most log k.`,
    });
    const frame = (i: number, say: string, lines: number[], cur: number[], moved: number[], result?: string): Frame => ({
        pointers: i >= 0 ? [{ at: i, label: "x", tone: "r" }] : [],
        cells: xs.map(String),
        dim: i >= 0 ? xs.map((_, j) => j).filter((j) => j > i) : [],
        graph: { nodes: treeNodes(h.map(String), (j) => (cur.includes(j) ? "cur" : moved.includes(j) ? "front" : undefined)), edges: treeEdges(h.map(String), () => undefined), h: 170 },
        lines,
        read: [{ label: "THE k LARGEST", value: [...h].sort((a, b) => b - a).join(" ") || "none" }, { label: "SMALLEST OF THEM", value: h.length ? h[0]! : "–" }] as Reading[],
        work: work(),
        say,
        result,
    });
    const out: Frame[] = [frame(-1, `Keep the ${k} largest numbers of a stream. A min-heap of size ${k} holds them, and its root is the smallest of the ${k}: the one a newcomer has to beat.`, [2], [], [])];
    for (let i = 0; i < n; i++) {
        const x = xs[i]!;
        if (h.length < k) {
            h.push(x);
            ops++;
            const at = siftUp(h, h.length - 1, () => {});
            out.push(frame(i, `The heap has room (${h.length - 1} of ${k}), so push ${x}.`, [3, 4, 5], [at], []));
        } else if (x > h[0]!) {
            const gone = h[0]!;
            h[0] = x;
            ops++;
            const at = siftDown(h, 0, () => {});
            out.push(frame(i, `${x} beats the smallest of the ${k}, ${gone}, so ${gone} leaves and ${x} takes its place and sinks to where it belongs.`, [6, 7], [at], []));
        } else {
            out.push(frame(i, `${x} does not beat ${h[0]}, the smallest of the ${k} largest, so it cannot be one of them. Throw it away: one comparison.`, [6], [], []));
        }
    }
    out.push(frame(-1, `The stream ended. The heap holds the ${k} largest: ${[...h].sort((a, b) => b - a).join(", ")}.`, [8], [], [], [...h].sort((a, b) => b - a).join(" ")));
    return out;
}

// ---------------------------------------------------------------- trie

type TrieNode = { id: number; ch: string; depth: number; kids: Map<string, number>; end: boolean; parent: number };

/** The finished trie of all the words (to fix every node's place), and where each prefix lives. */
function layoutTrie(words: string[]) {
    const nodes: TrieNode[] = [{ id: 0, ch: "·", depth: 0, kids: new Map(), end: false, parent: -1 }];
    for (const w of words) {
        let at = 0;
        for (const ch of w) {
            let nx = nodes[at]!.kids.get(ch);
            if (nx === undefined) {
                nx = nodes.length;
                nodes.push({ id: nx, ch, depth: nodes[at]!.depth + 1, kids: new Map(), end: false, parent: at });
                nodes[at]!.kids.set(ch, nx);
            }
            at = nx;
        }
    }
    const x = new Map<number, number>();
    let leaf = 0;
    const place = (id: number): number => {
        const kids = [...nodes[id]!.kids.values()].sort((a, b) => nodes[a]!.ch.localeCompare(nodes[b]!.ch));
        if (kids.length === 0) {
            x.set(id, leaf++);
            return x.get(id)!;
        }
        const xs = kids.map(place);
        x.set(id, (xs[0]! + xs[xs.length - 1]!) / 2);
        return x.get(id)!;
    };
    place(0);
    const depth = Math.max(...nodes.map((n) => n.depth), 1);
    return { nodes, pos: (id: number) => ({ x: leaf > 1 ? x.get(id)! / (leaf - 1) : 0.5, y: nodes[id]!.depth / depth }), leaves: leaf, depth };
}

const trieCode = [
    "class Trie:",
    "    def __init__(self):",
    "        self.root = {}  # letter -> child",
    "    def insert(self, word):",
    "        node = self.root",
    "        for ch in word:",
    "            node = node.setdefault(ch, {})  # reuse or create",
    "        node[\"$\"] = True  # a word ends here",
    "    def search(self, word):",
    "        node = self.root",
    "        for ch in word:",
    "            if ch not in node:",
    "                return False",
    "            node = node[ch]",
    "        return \"$\" in node",
];

function simulateTrie(input: DemoInput, query: boolean): Frame[] {
    const all = input.items;
    const words = query ? all.slice(0, -1) : all;
    const q = query ? all[all.length - 1]! : "";
    const lay = layoutTrie(all);
    const alive = new Set<number>([0]);
    const ends = new Set<number>();
    const trie = new Map<string, number>([["", 0]]);
    let cur = -1;
    let hit: number[] = [];
    let typed = 0;
    const graph = (): NonNullable<Frame["graph"]> => ({
        w: Math.max(380, lay.leaves * 62),
        h: Math.max(190, lay.depth * 62 + 40),
        nodes: lay.nodes.filter((n) => alive.has(n.id)).map((n): GNode => ({ id: n.id, ...lay.pos(n.id), label: n.ch, tone: n.id === cur ? "cur" : hit.includes(n.id) ? "hit" : ends.has(n.id) ? "done" : undefined, sub: ends.has(n.id) ? "end" : undefined })),
        edges: lay.nodes.filter((n) => n.parent >= 0 && alive.has(n.id)).map((n): GEdge => ({ a: n.parent, b: n.id, tone: hit.includes(n.id) ? "hit" : n.id === cur ? "new" : undefined })),
    });
    const frame = (say: string, lines: number[], read: Reading[], result?: string): Frame => ({
        pointers: [],
        graph: graph(),
        lines,
        read,
        work: {
            title: query ? "ONE STEP PER LETTER OF THE QUESTION" : "SHARED PREFIXES ARE STORED ONCE",
            bars: query
                ? [{ label: "Letters of the word asked", value: Math.min(typed, q.length), of: Math.max(q.length, 1), tone: "good", ratio: true }, { label: "Words stored", value: words.length, of: Math.max(words.length, 1), tone: "bad", ratio: true }]
                : [{ label: "Nodes in the trie", value: alive.size - 1, of: Math.max(words.join("").length, 1), tone: "good", ratio: true }, { label: "Letters typed", value: typed, of: Math.max(words.join("").length, 1), tone: "bad", ratio: true }],
            note: query
                ? "Looking a word up walks one node per letter, so it takes as long as the word is, however many words are stored. A list of words would be compared one by one."
                : "Words that start the same share the same nodes, so a trie stores each common prefix once. Inserting a word takes one step per letter, whatever else is stored.",
        } as Work,
        say,
        result,
    });
    const out: Frame[] = [frame(query ? `The trie already holds ${words.join(", ")}. Now look up “${q}”: follow one edge per letter.` : "A trie is a tree of letters. Each word is a path from the root, and a mark on the last node says a word ends there. Insert the words one by one.", [query ? 9 : 3], [{ label: "WORDS", value: words.join(" ") }])];
    // build the stored words silently for the lookup, or step through them for the insert
    const stored = (w: string, show: boolean) => {
        let at = 0;
        let prefix = "";
        for (const ch of w) {
            prefix += ch;
            const next = lay.nodes[at]!.kids.get(ch)!;
            const fresh = !alive.has(next);
            alive.add(next);
            trie.set(prefix, next);
            at = next;
            if (show) {
                typed++;
                cur = at;
                out.push(frame(fresh ? `“${w}”: there is no ${ch} under “${prefix.slice(0, -1) || "the root"}” yet, so create it.` : `“${w}”: “${prefix}” is already in the trie, so reuse that node: no new storage.`, [5, 6], [{ label: "WORD", value: w }, { label: "NODES", value: alive.size - 1 }]));
            }
        }
        ends.add(at);
        if (show) out.push(frame(`The last letter of “${w}”: mark the node as a word end.`, [7], [{ label: "WORD", value: w }, { label: "NODES", value: alive.size - 1 }]));
    };
    for (const w of words) stored(w, !query);
    if (!query) {
        cur = -1;
        const letters = words.join("").length;
        out.push(frame(`${words.length} words, ${letters} letters, but only ${alive.size - 1} nodes: the shared beginnings were stored once.`, [8], [{ label: "NODES", value: alive.size - 1 }, { label: "LETTERS TYPED", value: letters }], `${alive.size - 1} nodes`));
        return out;
    }
    let at = 0;
    let prefix = "";
    for (const ch of q) {
        typed++;
        const next = lay.nodes[at]!.kids.get(ch);
        if (next === undefined || !trie.has(prefix + ch) || !alive.has(next)) {
            cur = -1;
            out.push(frame(`There is no ${ch} under “${prefix || "the root"}”, so “${q}” is not stored, and the search stops after ${typed} step${typed === 1 ? "" : "s"}.`, [11, 12, 13], [{ label: "PREFIX FOUND", value: prefix || "none" }], "not found"));
            return out;
        }
        at = next;
        prefix += ch;
        hit = [...hit, at];
        cur = at;
        out.push(frame(`Follow ${ch}: “${prefix}” is in the trie.`, [10, 11, 14], [{ label: "PREFIX FOUND", value: prefix }]));
    }
    cur = -1;
    const isWord = ends.has(at);
    out.push(frame(isWord ? `Every letter was found and the last node is marked as a word end: “${q}” is stored.` : `Every letter was found, but the last node is not a word end: “${q}” is only a prefix of a stored word.`, [15], [{ label: "PREFIX FOUND", value: q }], isWord ? "found" : "prefix only"));
    return out;
}

const wordsField = (n: number) => (text: string) => {
    const items = text.toLowerCase().split(/[\s,]+/).filter(Boolean);
    return items.length >= 2 && items.length <= n && items.every((w) => /^[a-z]{1,5}$/.test(w)) ? { items } : null;
};

export const HEAP_DEMOS: Record<string, Demo> = {
    "heap-sift": {
        title: "Push and pop a min-heap",
        code: heapCode,
        initial: { items: ["5", "3", "8", "1", "9", "2", "7"], param: 3 },
        fields: { text: "Numbers to push, separated by commas", param: { label: "Pops", min: 0, max: 7 } },
        parse: (text, param) => {
            const items = numbers(text);
            const k = Number(param);
            return items.length >= 2 && items.length <= 7 && smallInts(items) && Number.isInteger(k) && param.trim() !== "" && k >= 0 && k <= items.length ? { items, param: k } : null;
        },
        simulate: simulateHeap,
    },
    "heap-topk": {
        title: "The k largest of a stream",
        code: topkCode,
        initial: { items: ["4", "9", "2", "7", "5", "8", "1", "6"], param: 3 },
        fields: { text: "A stream of numbers, separated by commas", param: { label: "k", min: 1, max: 5 } },
        parse: (text, param) => {
            const items = numbers(text);
            const k = Number(param);
            return items.length >= 2 && items.length <= MAX_CELLS && smallInts(items) && Number.isInteger(k) && param.trim() !== "" && k >= 1 && k <= 5 && k < items.length ? { items, param: k } : null;
        },
        simulate: simulateTopK,
    },
};

export const TRIE_DEMOS: Record<string, Demo> = {
    "trie-insert": {
        title: "Insert words into a trie",
        code: trieCode,
        initial: { items: ["car", "card", "care", "cat", "dog"] },
        noRow: true,
        fields: { text: "Words, separated by commas" },
        parse: wordsField(6),
        simulate: (input) => simulateTrie(input, false),
    },
    "trie-search": {
        title: "Look a word up",
        code: trieCode,
        initial: { items: ["car", "card", "care", "cat", "dog", "car"] },
        noRow: true,
        fields: { text: "Words to store, then the word to look up last" },
        parse: wordsField(7),
        simulate: (input) => simulateTrie(input, true),
    },
};
