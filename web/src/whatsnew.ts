// What changed, newest first, in the owner's words. Add an entry whenever something visible ships: a toast offers the list once after an update,
// and Settings keeps it. `id` is what the toast remembers having shown.

export interface Entry {
    id: string;
    date: string;
    title: string;
    items: string[];
}

export const WHATS_NEW: Entry[] = [
    {
        id: "2026-10-29",
        date: "29 Oct 2026",
        title: "Review, counting, safety and a lot of small things",
        items: [
            "Review: pick any review from the list, sort it, review ahead, J and K to move, U to undo, a session strip underneath. The timer only runs when you start it.",
            "Settings (account menu): count premium problems or not (off by default), theme with System, accent, width, the numbers bar, timer chime, backup and restore.",
            "Premium problems are always listed. Completing one counts toward your streak whatever the setting; the totals and bars follow the setting.",
            "The streak follows your plan: a rest or practice day neither breaks it nor adds to it.",
            "Log options are On my own, With help, Attempted, Instant. Keys 1 to 4 log, U undoes, Reset progress archives first and can be restored.",
            "After Attempted or With help the toast can ask why (idea, implementation, edge cases, complexity), and offers the next problem.",
            "Nothing is ever deleted without an archive: resets, deleted solutions and plans and discarded drafts are kept in data_archive. Backups are taken before every deploy; Settings downloads one too.",
            "The ⌘K box logs problems (type: log two sum), `?` lists every shortcut, and the calendar moves by month with the arrow keys.",
            "A link to your own notes per problem, a phone-friendly quick log at /log, free alternatives beside a premium problem, a click on a heatmap day, weak spots in review ahead, and the Mock interview's Start button on top.",
            "Back from LeetCode, a slim bar asks how it went (it never logs by itself). On Mondays the home page sums up last week. The Mock interview shows a trend over your rounds.",
            "Settings can mark problems you solved before anneal (paste their numbers; no streak or review is touched; undo the batch) and make a calendar link for your phone with review days and solve days.",
        ],
    },
];

export const LATEST = WHATS_NEW[0]!.id;
