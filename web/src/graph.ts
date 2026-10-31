// The DSA graph, built from the catalog and your progress (GET /api/dsa): patterns, techniques, problems and the companies that ask most,
// with what links them. Plain functions with no React, so they can be tested with `node --test`.
import type { DsaOverview, DsaProblem } from "./api";

export type Kind = "pat" | "tec" | "pro" | "co";
export type EdgeKind = "uses" | "tag" | "asks";
export type State = "sol" | "due" | "att" | "new";

export interface GNode {
    id: number;
    key: string;
    kind: Kind;
    label: string;
    /** Index of the pattern it belongs to, or -1 for a company. */
    pat: number;
    r: number;
    state?: State;
    /** For a pattern or technique: the share of its problems in scope that are solved, 0 to 1. */
    done?: number;
    slug?: string;
    code?: string;
    x: number;
    y: number;
    vx: number;
    vy: number;
}

export interface Graph {
    nodes: GNode[];
    edges: [number, number, EdgeKind][];
    adj: Map<number, Set<number>>;
}

const stateOf = (p: DsaProblem, today: string): State => (p.state.due && p.state.due <= today && p.state.last_grade ? "due" : p.state.solved ? "sol" : p.state.last_grade ? "att" : "new");

/** Shared LeetCode topic tags between two problems. */
export const sharedTags = (a: DsaProblem, b: DsaProblem) => a.tags.filter((t) => b.tags.includes(t));

/** The graph of the problems in `scope`, their techniques and patterns, and the companies that ask the most of them. */
export function buildGraph(o: DsaOverview, scope: DsaProblem[], companies = 10): Graph {
    const nodes: GNode[] = [];
    const edges: [number, number, EdgeKind][] = [];
    const add = (n: Omit<GNode, "id" | "x" | "y" | "vx" | "vy">) => {
        const id = nodes.length;
        nodes.push({ ...n, id, x: 0, y: 0, vx: 0, vy: 0 });
        return id;
    };
    const patIdx = new Map(o.patterns.map((p, i) => [p.code, i] as const));
    const patByName = new Map(o.patterns.map((p, i) => [p.name, i] as const));
    const patNode = new Map<number, number>();
    o.patterns.forEach((p, i) => patNode.set(i, add({ key: `pat:${p.code}`, kind: "pat", label: p.name, pat: i, r: 17, code: p.code })));
    const tecNode = new Map<string, number>();
    for (const t of o.techniques) {
        if (!scope.some((p) => p.technique === t.id)) continue;
        const pi = patByName.get(t.pattern) ?? 0;
        const id = add({ key: `tec:${t.id}`, kind: "tec", label: t.name, pat: pi, r: 9, code: o.patterns[pi]?.code });
        tecNode.set(t.id, id);
        edges.push([patNode.get(pi)!, id, "uses"]);
    }
    const proNode = new Map<string, number>();
    for (const p of scope) {
        const pi = patIdx.get(p.pattern) ?? 0;
        const st = stateOf(p, o.today);
        const id = add({ key: `pro:${p.id}`, kind: "pro", label: p.title, pat: pi, r: st === "new" ? 5 : 6, state: st, slug: p.slug });
        proNode.set(p.id, id);
        edges.push([tecNode.get(p.technique) ?? patNode.get(pi)!, id, "uses"]);
    }
    // how much of each technique and pattern is solved
    const share = (ps: DsaProblem[]) => (ps.length ? ps.filter((p) => p.state.solved).length / ps.length : 0);
    for (const [tid, id] of tecNode) nodes[id]!.done = share(scope.filter((p) => p.technique === tid));
    for (const [pi, id] of patNode) nodes[id]!.done = share(scope.filter((p) => (patIdx.get(p.pattern) ?? 0) === pi));
    // a problem links to the problem of another technique it shares the most tags with, when that is at least three
    for (const p of scope) {
        let best: DsaProblem | undefined;
        let n = 2;
        for (const q of scope) {
            if (q.id === p.id || q.technique === p.technique) continue;
            const k = sharedTags(p, q).length;
            if (k > n) {
                n = k;
                best = q;
            }
        }
        if (best && p.id < best.id) edges.push([proNode.get(p.id)!, proNode.get(best.id)!, "tag"]);
    }
    // the companies that ask the most of these problems, each linked to the patterns it asks five or more of
    const asked = new Map<string, Map<number, number>>();
    for (const p of scope) {
        for (const c of p.companies) {
            const m = asked.get(c.name) ?? new Map<number, number>();
            const pi = patIdx.get(p.pattern) ?? 0;
            m.set(pi, (m.get(pi) ?? 0) + 1);
            asked.set(c.name, m);
        }
    }
    const top = [...asked.entries()].map(([name, m]) => [name, m, [...m.values()].reduce((a, b) => a + b, 0)] as const).sort((a, b) => b[2] - a[2]).slice(0, companies);
    for (const [name, m] of top) {
        const id = add({ key: `co:${name}`, kind: "co", label: name, pat: -1, r: 11 });
        for (const [pi, k] of m) if (k >= 5) edges.push([id, patNode.get(pi)!, "asks"]);
    }
    const adj = new Map<number, Set<number>>(nodes.map((n) => [n.id, new Set<number>()]));
    for (const [u, v] of edges) {
        adj.get(u)!.add(v);
        adj.get(v)!.add(u);
    }
    return { nodes, edges, adj };
}

/** The shortest path between two nodes, by links; empty when there is none. */
export function shortestPath(g: Graph, a: number, b: number): number[] {
    const prev = new Map<number, number | null>([[a, null]]);
    const q = [a];
    while (q.length) {
        const c = q.shift()!;
        if (c === b) break;
        for (const m of g.adj.get(c) ?? []) {
            if (!prev.has(m)) {
                prev.set(m, c);
                q.push(m);
            }
        }
    }
    if (!prev.has(b)) return [];
    const path: number[] = [];
    for (let c: number | null | undefined = b; c !== null && c !== undefined; c = prev.get(c)) path.unshift(c);
    return path;
}

/** A small seeded random number generator, so the same graph gets the same vectors and layout. */
export function rng(seed: number) {
    let s = seed % 2147483647 || 1;
    return () => (s = (s * 16807) % 2147483647) / 2147483647;
}

/** FastRP graph vectors: a sparse random projection of each node, then averaged with its neighbours a few times. Nodes whose
 *  surroundings look alike end up with alike vectors. No model, only the links. */
export function fastRP(g: Graph, dim = 64, iterations = 3, seed = 7): Float32Array[] {
    const rand = rng(seed);
    const n = g.nodes.length;
    let cur: Float32Array[] = g.nodes.map(() => {
        const v = new Float32Array(dim);
        for (let i = 0; i < dim; i++) {
            const r = rand();
            v[i] = r < 1 / 6 ? -1 : r > 5 / 6 ? 1 : 0;
        }
        return v;
    });
    const sum = g.nodes.map(() => new Float32Array(dim));
    const weights = [0, 1, 1, 0.5];
    for (let it = 1; it <= iterations; it++) {
        const next = g.nodes.map(() => new Float32Array(dim));
        for (let u = 0; u < n; u++) {
            const nb = g.adj.get(u)!;
            if (nb.size === 0) continue;
            for (const v of nb) for (let i = 0; i < dim; i++) next[u]![i]! += cur[v]![i]! / nb.size;
        }
        cur = next.map(normalise);
        const w = weights[it] ?? 0.5;
        for (let u = 0; u < n; u++) for (let i = 0; i < dim; i++) sum[u]![i]! += w * cur[u]![i]!;
    }
    return sum.map(normalise);
}

function normalise(v: Float32Array): Float32Array {
    let s = 0;
    for (const x of v) s += x * x;
    const k = s > 0 ? 1 / Math.sqrt(s) : 0;
    return v.map((x) => x * k);
}

export const cosine = (a: Float32Array, b: Float32Array) => {
    let s = 0;
    for (let i = 0; i < a.length; i++) s += a[i]! * b[i]!;
    return s;
};
