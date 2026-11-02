// Filter rows shared by the left panels of the DSA and Rust pages (styles: .d-opt and .d-tog in dsa.css).

export function Opt({ label, n, on, onClick, sw, hint }: { label: string; n: number; on: boolean; onClick: () => void; sw?: string; hint?: string }) {
  return (
    <button className={`d-opt${on ? " on" : ""}${n === 0 && !on ? " zero" : ""}`} onClick={onClick} aria-pressed={on}>
      {sw ? <span className="sw" style={{ background: sw }} /> : <span className="bx" />}
      <span className="d">
        {label}
        {hint && <small>{hint}</small>}
      </span>
      <span className="c">{n}</span>
    </button>
  );
}

export function Tog({ label, hint, on, onClick }: { label: string; hint?: string; on: boolean; onClick: () => void }) {
  return (
    <div className="d-tog">
      <span>
        {label}
        {hint && <small>{hint}</small>}
      </span>
      <button className={`d-sw${on ? " on" : ""}`} aria-pressed={on} aria-label={label} onClick={onClick} />
    </div>
  );
}
