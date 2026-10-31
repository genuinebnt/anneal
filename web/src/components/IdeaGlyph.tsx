/** The small picture beside a row of "which one": the cells, and where the window or pointers sit. Names are `IDEA_GLYPHS` in `crates/content/src/dsa.rs`. */
const SPEC: Record<string, { n: number; w: [number, number][]; w2?: [number, number][]; dots?: number[]; bars?: number[]; ptr?: number[] }> = {
    fixed: { n: 10, w: [[3, 6]] },
    longest: { n: 10, w: [[1, 8]] },
    shortest: { n: 10, w: [[4, 6]] },
    counting: { n: 10, w: [[1, 7]], w2: [[1, 4]] },
    frequency: { n: 10, w: [[2, 8]], dots: [3, 5, 7] },
    deque: { n: 10, w: [[3, 6]], bars: [9, 7, 6, 8, 4, 3, 5, 2, 6, 3] },
    opposite: { n: 10, w: [[0, 10]], ptr: [0, 9] },
    samedir: { n: 10, w: [], w2: [[0, 4]], ptr: [3, 7] },
    ksum: { n: 10, w: [], dots: [1], ptr: [3, 9] },
    merge: { n: 10, w: [[0, 5]], w2: [[5, 10]], ptr: [2, 7] },
    greedy: { n: 10, w: [], dots: [0, 9], ptr: [0, 9] },
    peaks: { n: 10, w: [], bars: [3, 1, 4, 2, 6, 2, 3, 5, 2, 1], ptr: [2, 9] },
    halving: { n: 10, w: [[0, 10]], w2: [[5, 10]], ptr: [4] },
    boundary: { n: 10, w: [], dots: [5, 6, 7, 8, 9], ptr: [5] },
    answer: { n: 10, w: [], bars: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], ptr: [6] },
    rotated: { n: 10, w: [], bars: [6, 7, 8, 9, 10, 1, 2, 3, 4, 5], ptr: [5] },
    partition: { n: 10, w: [[0, 4]], w2: [[4, 10]], dots: [3, 4] },
    mountain: { n: 10, w: [], bars: [1, 3, 5, 7, 9, 8, 6, 4, 3, 1], ptr: [4] },
    nested: { n: 10, w: [[0, 10], [2, 8]], w2: [[4, 6]] },
    cancel: { n: 10, w: [], w2: [[3, 5]], dots: [3, 4], ptr: [5] },
    monotone: { n: 10, w: [], bars: [9, 7, 5, 3, 6, 4, 2, 8, 5, 3], ptr: [4] },
    contrib: { n: 10, w: [[2, 7]], bars: [4, 6, 2, 7, 3, 5, 1, 6, 4, 3] },
    runstate: { n: 10, w: [], dots: [0, 1, 2, 3, 4], bars: [3, 3, 2, 2, 1], ptr: [4] },
    evaluate: { n: 10, w: [], dots: [0, 2, 4, 6, 8], ptr: [9] },
    reverse: { n: 10, w: [], w2: [[0, 5]], dots: [0, 1, 2, 3, 4], ptr: [4, 5] },
    cycle: { n: 10, w: [[3, 10]], dots: [3, 6, 9], ptr: [3, 9] },
    gap: { n: 10, w: [], w2: [[2, 7]], ptr: [2, 7] },
    splice: { n: 10, w: [], w2: [[2, 5]], dots: [3], ptr: [2, 5] },
    clone: { n: 10, w: [[0, 5]], w2: [[5, 10]], dots: [1, 6] },
    lru: { n: 10, w: [], dots: [0, 1, 2, 3, 4, 5], ptr: [0, 5] },
    kmerge: { n: 10, w: [], bars: [2, 4, 3, 6, 5, 7, 6, 8, 7, 9], ptr: [0] },
    lookup: { n: 10, w: [], w2: [[0, 7]], dots: [2], ptr: [7] },
    tally: { n: 10, w: [], bars: [4, 2, 3, 1, 2, 1, 3, 1, 2, 1] },
    groups: { n: 10, w: [[0, 3]], w2: [[4, 7]], dots: [1, 5, 8] },
    runsum: { n: 10, w: [], w2: [[3, 8]], bars: [1, 2, 3, 3, 4, 5, 5, 6, 7, 8], ptr: [2, 7] },
    buckets: { n: 10, w: [[3, 4], [7, 8]], bars: [2, 7, 3, 9, 1, 5, 2, 8, 4, 1] },
    streak: { n: 10, w: [[2, 7]], dots: [2, 3, 4, 5, 6] },
    vote: { n: 10, w: [], w2: [[0, 10]], dots: [0, 1, 3, 4, 6, 7, 9] },
    marks: { n: 10, w: [], dots: [2, 6], ptr: [2, 6] },
    descend: { n: 10, w: [[0, 10]], w2: [[0, 5]], dots: [0, 2, 4], ptr: [4] },
    bubble: { n: 10, w: [], bars: [2, 3, 5, 8, 9, 6, 4, 3, 2, 1], ptr: [4] },
    carry: { n: 10, w: [[0, 6]], dots: [0, 1, 2, 3, 4, 5], ptr: [5] },
    bstwalk: { n: 10, w: [[0, 10]], w2: [[5, 10]], dots: [7], ptr: [7] },
    inorderw: { n: 10, w: [], bars: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], dots: [4], ptr: [4] },
    levels: { n: 10, w: [[0, 1]], w2: [[1, 3]], dots: [3, 4, 5, 6] },
    edgelist: { n: 10, w: [], dots: [1, 3, 5, 7, 9], ptr: [0, 8] },
    rebuild: { n: 10, w: [[0, 4]], w2: [[5, 10]], dots: [4] },
};

export function IdeaGlyph({ kind }: { kind: string }) {
    const g = SPEC[kind];
    if (!g) return null;
    const c = 9;
    const gap = 1.6;
    const step = c + gap;
    return (
        <svg className="id-glyph" viewBox={`0 0 ${g.n * step} 22`} aria-hidden="true">
            {Array.from({ length: g.n }, (_, i) => {
                const h = g.bars ? (g.bars[i] ?? 1) * 1.6 : c;
                return <rect key={i} className="id-gc" x={i * step} y={g.bars ? 20 - h : 6} width={c} height={h} rx={g.bars ? 2 : 2.5} style={{ animationDelay: `${i * 28}ms` }} />;
            })}
            {(g.ptr ?? []).map((i) => <path key={`p${i}`} className="id-gp" d={`M${i * step + c / 2 - 3} 22 L${i * step + c / 2} 17 L${i * step + c / 2 + 3} 22Z`} />)}
            {(g.dots ?? []).map((i) => <circle key={i} className="id-gd" cx={i * step + c / 2} cy={10.5} r={2.2} />)}
            {g.w.map(([a, b]) => <rect key={`w${a}`} className="id-gw" x={a * step - 1.5} y={2} width={(b - a) * step} height={g.bars ? 19 : 17} rx={4} />)}
            {(g.w2 ?? []).map(([a, b]) => <rect key={`v${a}`} className="id-gw2" x={a * step - 1.5} y={0.5} width={(b - a) * step} height={21} rx={4} />)}
        </svg>
    );
}
