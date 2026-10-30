/** The small picture beside a row of "which one": the cells, and where the window or pointers sit. Names are `IDEA_GLYPHS` in `crates/content/src/dsa.rs`. */
const SPEC: Record<string, { n: number; w: [number, number][]; w2?: [number, number][]; dots?: number[]; bars?: number[] }> = {
    fixed: { n: 10, w: [[3, 6]] },
    longest: { n: 10, w: [[1, 8]] },
    shortest: { n: 10, w: [[4, 6]] },
    counting: { n: 10, w: [[1, 7]], w2: [[1, 4]] },
    frequency: { n: 10, w: [[2, 8]], dots: [3, 5, 7] },
    deque: { n: 10, w: [[3, 6]], bars: [9, 7, 6, 8, 4, 3, 5, 2, 6, 3] },
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
            {(g.dots ?? []).map((i) => <circle key={i} className="id-gd" cx={i * step + c / 2} cy={10.5} r={2.2} />)}
            {g.w.map(([a, b]) => <rect key={`w${a}`} className="id-gw" x={a * step - 1.5} y={2} width={(b - a) * step} height={g.bars ? 19 : 17} rx={4} />)}
            {(g.w2 ?? []).map(([a, b]) => <rect key={`v${a}`} className="id-gw2" x={a * step - 1.5} y={0.5} width={(b - a) * step} height={21} rx={4} />)}
        </svg>
    );
}
