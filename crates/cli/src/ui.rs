//! The learner's screens: boxes, the stage banner, the celebrations, the roadmap strip, the logo and the guide. Pure functions that
//! return text (colour comes from `term`, so it is off in pipes and under `NO_COLOR`), which keeps them testable.

use crate::term::{bold, dim, green};

/// The width of `s` as drawn: ANSI colour codes take no room, every other character takes one column.
pub fn visible_width(s: &str) -> usize {
    let mut n = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip an escape sequence: ESC [ ... final byte in @..~
            for d in chars.by_ref() {
                if ('@'..='~').contains(&d) && d != '[' {
                    break;
                }
            }
        } else {
            n += 1;
        }
    }
    n
}

/// Cuts `s` to `max` columns, ending in `…` when it was longer (plain text only).
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A rounded box around `lines`, with an optional title in the top edge. As wide as its widest line (and the title), at least `min`.
pub fn boxed(title: Option<&str>, lines: &[String], min: usize) -> String {
    let title_w = title.map_or(0, |t| visible_width(t) + 3);
    let inner = lines.iter().map(|l| visible_width(l)).max().unwrap_or(0).max(title_w).max(min);
    let mut out = String::new();
    match title {
        Some(t) => out.push_str(&format!("╭─ {t} {}╮\n", "─".repeat(inner + 2 - title_w))),
        None => out.push_str(&format!("╭{}╮\n", "─".repeat(inner + 2))),
    }
    for l in lines {
        out.push_str(&format!("│ {l}{} │\n", " ".repeat(inner - visible_width(l))));
    }
    out.push_str(&format!("╰{}╯", "─".repeat(inner + 2)));
    out
}

/// `▰▰▰▱▱▱`: `done` of `total` in `width` cells.
pub fn meter(done: usize, total: usize, width: usize) -> String {
    // proportional, but a started meter shows something and an unfinished one is never full
    let filled = if total == 0 {
        0
    } else if done >= total {
        width
    } else {
        (done * width / total).max(usize::from(done > 0)).min(width.saturating_sub(1))
    };
    format!("{}{}", "▰".repeat(filled), "▱".repeat(width - filled))
}

/// What the banner needs to know about a stage.
pub struct StageInfo<'a> {
    pub module: &'a str,
    pub module_title: &'a str,
    pub id: &'a str,
    pub title: &'a str,
    pub difficulty: &'a str,
    pub kind: &'a str,
    /// 1-based place of the stage among the stages of its module, and how many there are.
    pub at: usize,
    pub of: usize,
    pub course_done: usize,
    pub course_total: usize,
}

/// The box at the top of a stage: where it is, how hard it is, how far along you are.
pub fn stage_banner(s: &StageInfo) -> String {
    let title = format!("{} · {}", s.module, s.module_title);
    let kind = if s.kind == "challenge" { "  challenge" } else { "" };
    let lines = vec![
        format!("{}  {}", bold(s.id), bold(s.title)),
        format!("{}{}", dim(s.difficulty), dim(kind)),
        format!("{} {}", meter(s.at - 1, s.of, 12), dim(&format!("stage {} of {} in this module · {} of {} overall", s.at, s.of, s.course_done, s.course_total))),
    ];
    boxed(Some(&title), &lines, 44)
}

/// What the completion box says about the stage that has just passed.
pub struct StageDone<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub course_done: usize,
    pub course_total: usize,
    /// How long the passing test run took.
    pub took_ms: u64,
    /// Stages passed in the last 24 hours (this one included).
    pub last_day: usize,
    pub module_done: usize,
    pub module_total: usize,
}

/// A stage has just passed.
pub fn stage_complete(d: &StageDone) -> String {
    let StageDone { id, title, course_done, course_total, took_ms, last_day, module_done, module_total } = *d;
    let secs = took_ms / 1000;
    let took = if secs >= 60 { format!("{}m {:02}s", secs / 60, secs % 60) } else { format!("{secs}s") };
    let lines = vec![
        green(&format!("★ {title}")),
        format!("{} {}", meter(module_done, module_total, 12), dim(&format!("{module_done} of {module_total} in this module"))),
        dim(&format!("{course_done} of {course_total} stages · tests took {took} · {last_day} passed in the last 24h")),
    ];
    format!("{}\n{}", green(&format!("✓ STAGE {id} COMPLETE")), boxed(None, &lines, 40))
}

/// A module's last stage has just passed.
pub fn module_complete(code: &str, title: &str, stages: usize) -> String {
    let lines = vec![
        String::new(),
        bold(&format!("MODULE {code} COMPLETE")),
        title.to_owned(),
        dim(&format!("{stages} stages passed")),
        String::new(),
    ];
    let boxed = boxed(Some("✦ ✦ ✦"), &lines, 40);
    let centred: Vec<String> = boxed.lines().map(str::to_owned).collect();
    centred.join("\n")
}

/// The last stage of the course.
pub fn course_complete(title: &str) -> String {
    [
        "          ___________",
        "         '._==_==_=_.'",
        "         .-\\:      /-.",
        "        | (|:.     |) |",
        "         '-|:.     |-'",
        "           \\::.    /",
        "            '::. .'",
        "              ) (",
        "            _.' '._",
        "           '-------'",
    ]
    .iter()
    .map(|l| green(l))
    .collect::<Vec<_>>()
    .join("\n")
        + &format!("\n\n{}\n{}", bold(&format!("You finished {title}.")), dim("Every stage passes. Rebuild a module from a blank file next, and explain it aloud."))
}

/// One node of the roadmap: `●` done, `◐` begun, `○` not yet.
fn node(done: usize, total: usize) -> &'static str {
    if total > 0 && done == total {
        "●"
    } else if done > 0 {
        "◐"
    } else {
        "○"
    }
}

/// The modules in a row, `● 0a ─ ● 1a ─ ◐ 1b ─ ○ 1c`, wrapped to `width`. `current` is shown in bold.
pub fn roadmap(modules: &[(String, usize, usize)], current: Option<&str>, width: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let (mut line, mut used) = (String::new(), 0);
    for (i, (code, done, total)) in modules.iter().enumerate() {
        let plain = format!("{} {code}", node(*done, *total));
        let cell = if Some(code.as_str()) == current { bold(&plain) } else if done == total && *total > 0 { green(&plain) } else { plain.clone() };
        let w = visible_width(&plain) + if i == 0 { 0 } else { 3 };
        if used > 0 && used + w > width {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if used > 0 {
            line.push_str(&dim(" ─ "));
        }
        line.push_str(&cell);
        used += visible_width(&plain) + if used > 0 { 3 } else { 0 };
    }
    lines.push(line);
    lines.join("\n")
}

/// The word "anneal" in three rows.
pub fn logo() -> String {
    ["┌─┐┌┐┌┌┐┌┌─┐┌─┐┬  ", "├─┤│││││││├┤ ├─┤│  ", "┴ ┴┘└┘┘└┘└─┘┴ ┴┴─┘"].iter().map(|l| format!("  {}", bold(l))).collect::<Vec<_>>().join("\n")
}

/// The one-screen guide: the loop, and the commands around it.
pub fn guide() -> String {
    let row = |cmd: &str, what: &str| format!("    {}  {}", bold(&format!("{cmd:<30}")), dim(what));
    let mut out = vec![logo(), format!("  {}", dim("a database, built one tested stage at a time")), String::new(), bold("  THE LOOP")];
    out.push(row("anneal course show", "read the stage: the task, what is tested"));
    out.push(row("anneal course tests", "the test cases you have to pass"));
    out.push(dim("    edit the stubs marked with the stage id in src/"));
    out.push(row("anneal course test", "run them (--watch: again on every save)"));
    out.push(row("anneal course show --hint 1", "stuck? a nudge (counts as helped)"));
    out.push(row("anneal course next", "the next stage, once this one passes"));
    out.push(String::new());
    out.push(bold("  AROUND IT"));
    out.push(row("anneal course status", "the map: every module and stage"));
    out.push(row("anneal course adopt", "take our solution of a stage"));
    out.push(row("anneal course update", "new stages and tests, your code untouched"));
    out.push(row("anneal self-update", "the newest anneal"));
    out.push(row("anneal course doctor", "check your tools and setup"));
    out.push(String::new());
    out.push(dim("  git commit -am work && git push    saves your progress; the hook runs the tests"));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_take_no_room() {
        assert_eq!(visible_width("abc"), 3);
        assert_eq!(visible_width("\x1b[32mabc\x1b[0m"), 3);
        assert_eq!(visible_width("✓ ok"), 4);
    }

    #[test]
    fn a_box_has_straight_edges_whatever_the_lines() {
        let b = boxed(Some("1a · Disk"), &["short".to_owned(), "\x1b[1ma longer line\x1b[0m".to_owned()], 10);
        let widths: Vec<usize> = b.lines().map(visible_width).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}\n{b}");
        // a title wider than the lines widens the box instead of cutting the title
        let wide = boxed(Some("a very long title indeed"), &["x".to_owned()], 0);
        let widths: Vec<usize> = wide.lines().map(visible_width).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]) && wide.contains("a very long title indeed"), "{wide}");
        assert_eq!(boxed(None, &[], 0).lines().count(), 2);
    }

    #[test]
    fn the_meter_shows_progress_without_lying_at_the_ends() {
        assert_eq!(meter(0, 6, 6), "▱▱▱▱▱▱");
        assert_eq!(meter(6, 6, 6), "▰▰▰▰▰▰");
        assert_eq!(meter(1, 100, 10), "▰▱▱▱▱▱▱▱▱▱"); // a little done is not "nothing done"
        assert_eq!(meter(0, 0, 4), "▱▱▱▱");
        assert_eq!(meter(99, 100, 6), "▰▰▰▰▰▱"); // never full until it is
    }

    #[test]
    fn the_roadmap_marks_each_module_and_wraps() {
        let mods: Vec<(String, usize, usize)> = vec![("0a".into(), 4, 4), ("1a".into(), 1, 6), ("1b".into(), 0, 5), ("1c".into(), 0, 5)];
        let one = roadmap(&mods, Some("1a"), 80);
        assert_eq!(one.lines().count(), 1);
        assert!(one.contains("● 0a") && one.contains("◐ 1a") && one.contains("○ 1b"), "{one}");
        let wrapped = roadmap(&mods, None, 14);
        assert!(wrapped.lines().count() > 1 && wrapped.lines().all(|l| visible_width(l) <= 14), "{wrapped}");
    }

    #[test]
    fn the_banner_and_celebrations_name_the_stage() {
        let info = StageInfo { module: "1a", module_title: "Disk manager", id: "1a-01", title: "Pages that survive", difficulty: "Medium · 30 minutes to 1 hour", kind: "build", at: 1, of: 6, course_done: 0, course_total: 164 };
        let b = stage_banner(&info);
        assert!(b.contains("1a-01") && b.contains("Pages that survive") && b.contains("stage 1 of 6"), "{b}");
        let c = stage_complete(&StageDone { id: "1a-01", title: "Pages that survive", course_done: 1, course_total: 164, took_ms: 83_000, last_day: 2, module_done: 1, module_total: 6 });
        assert!(c.contains("STAGE 1a-01 COMPLETE") && c.contains("1m 23s") && c.contains("2 passed in the last 24h"), "{c}");
        assert!(module_complete("1a", "Disk manager", 6).contains("MODULE 1a COMPLETE"));
        assert!(course_complete("BusTub").contains("You finished BusTub"));
    }

    /// `cargo test -p anneal-cli preview -- --ignored --nocapture` prints every screen as a learner sees it.
    #[test]
    #[ignore]
    fn preview() {
        let info = StageInfo { module: "2c", module_title: "B+ tree: insert, delete and scans", id: "2c-05", title: "Latching and the fast path", difficulty: "Medium · 30 minutes to 1 hour", kind: "build", at: 5, of: 6, course_done: 63, course_total: 164 };
        println!("{}\n", stage_banner(&info));
        println!("{}\n", stage_complete(&StageDone { id: "2c-05", title: "Latching and the fast path", course_done: 64, course_total: 164, took_ms: 83_000, last_day: 3, module_done: 5, module_total: 6 }));
        println!("{}\n", module_complete("2c", "B+ tree: insert, delete and scans", 6));
        println!("{}\n", course_complete("Build a DBMS: BusTub in Rust"));
        let mods: Vec<(String, usize, usize)> = ["0a", "0b", "0c", "0d", "r", "1a", "1b", "1c", "1d", "1e", "2a", "2b", "2c", "2d", "3a", "3b", "3c", "3d", "3e", "3f", "3g", "3h", "4a", "4b", "4c"].iter().enumerate().map(|(i, c)| ((*c).to_owned(), if i < 12 { 6 } else if i == 12 { 4 } else { 0 }, 6)).collect();
        println!("{}\n", roadmap(&mods, Some("2c"), 80));
        println!("{}\n", guide());
    }

    #[test]
    fn the_guide_lists_the_loop_and_only_real_commands() {
        let g = guide();
        for cmd in ["course show", "course tests", "course test", "course next", "course status", "course adopt", "course update", "self-update", "course doctor"] {
            assert!(g.contains(cmd), "{cmd}");
        }
        assert!(clip("abcdefghij", 5) == "abcd…" && clip("abc", 5) == "abc");
    }
}
