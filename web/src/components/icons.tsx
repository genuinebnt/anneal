// The icons of the four ways a problem can go, drawn once so they look the same everywhere (they replace the characters ✓ ½ ! ⚡, which differ by font).
import type { Grade } from "../api";

const PATH = {
    good: <path d="M3.4 8.6l3.1 3.1 6.1-6.6" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />,
    hard: (
        <>
            <circle cx="8" cy="8" r="5.3" fill="none" stroke="currentColor" strokeWidth="1.6" />
            <path d="M8 2.7a5.3 5.3 0 0 0 0 10.6z" fill="currentColor" />
        </>
    ),
    again: <path d="M8 3.4v5.6M8 12.3v.3" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />,
    easy: <path d="M9.2 1.8L4.3 9h3.3l-.7 5.2L11.9 7H8.5z" fill="currentColor" />,
} as const;

/** The icon for a way of logging: on my own, with help, attempted, instant. */
export function GradeGlyph({ grade }: { grade: Grade }) {
    return (
        <svg className="icon" viewBox="0 0 16 16" aria-hidden="true">
            {PATH[grade]}
        </svg>
    );
}
