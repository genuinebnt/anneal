/** A placeholder while a page loads: a heading bar and `lines` text bars. It announces itself as loading. */
export function Skeleton({ lines = 4 }: { lines?: number }) {
    return (
        <div className="sk" role="status" aria-label="Loading">
            <i className="h" />
            {Array.from({ length: lines }, (_, i) => (
                <i key={i} />
            ))}
        </div>
    );
}
