/** A walkthrough step's lines, `10-18` or `12`, as the first and the last. */
export function lineRange(spec: string): [number, number] {
    const [a, b] = spec.split("-").map((x) => Number(x.trim()));
    return [a!, b ?? a!];
}
