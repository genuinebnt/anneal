// Small pieces shared by the animation files (ideaDemos*.ts): reading what was typed and laying out a tree.
import type { GEdge, GNode } from "./ideaDemos.ts";

export const MAX_CELLS = 10;
export const numbers = (text: string) => text.split(",").map((x) => x.trim()).filter(Boolean);
export const smallInts = (xs: string[]) => xs.every((x) => /^-?\d{1,3}$/.test(x));

/** A binary tree written level by level (`_` is an empty place). Place `i` has children `2i+1` and `2i+2`; a place under an empty one is empty. */
export function treeShape(items: string[]) {
    const exists = items.map((v, i) => v !== "_" && (i === 0 || exists_(items, (i - 1) >> 1)));
    return exists;
}
function exists_(items: string[], i: number): boolean {
    return items[i] !== "_" && (i === 0 || exists_(items, (i - 1) >> 1));
}

export const levelOf = (i: number) => Math.floor(Math.log2(i + 1));

/** Positions (0..1) of a tree of `n` places, drawn level by level. */
export function treeNodes(items: string[], tone: (i: number) => GNode["tone"], sub?: (i: number) => string | undefined): GNode[] {
    const ex = treeShape(items);
    const levels = levelOf(items.length - 1) + 1;
    return items.flatMap((v, i) => {
        if (!ex[i]) return [];
        const L = levelOf(i);
        const k = i + 1 - 2 ** L;
        return [{ id: i, x: (k + 0.5) / 2 ** L, y: levels > 1 ? L / (levels - 1) : 0, label: v, tone: tone(i), sub: sub?.(i) }];
    });
}

export function treeEdges(items: string[], tone: (child: number) => GEdge["tone"]): GEdge[] {
    const ex = treeShape(items);
    return items.flatMap((_, i) => (i > 0 && ex[i] ? [{ a: (i - 1) >> 1, b: i, tone: tone(i) }] : []));
}

/** Reads a level-order tree: numbers and `_`, the root present, 1 to 7 places. */
export function parseTree(text: string): string[] | null {
    const items = numbers(text).map((x) => x.replace(/^null$/i, "_"));
    if (items.length < 1 || items.length > 7 || items[0] === "_") return null;
    return items.every((x) => x === "_" || /^-?\d{1,3}$/.test(x)) ? items : null;
}
