use std::fmt::Write;

#[derive(Debug, PartialEq)]
pub struct Player {
    pub name: String,
    pub score: u32,
}

/// Appends `line` and a newline to any text sink, taken by value.
fn record<W: Write>(mut log: W, line: &str) {
    writeln!(log, "{line}").expect("writing to a String can't fail");
}

/// One round of a game. It borrows the players and the log from the caller for `'a`.
pub struct Round<'a> {
    players: &'a mut [Player],
    log: &'a mut String,
}

impl<'a> Round<'a> {
    pub fn new(players: &'a mut [Player], log: &'a mut String) -> Self {
        Round { players, log }
    }

    /// Moves up to `points` from player `from` to player `to` (never more than `from` has), logs
    /// "<from's name> -> <to's name>: <moved>", and returns how many points moved.
    pub fn transfer(&mut self, from: usize, to: usize, points: u32) -> u32 {
        let moved = points.min(self.players[from].score);
        self.players[from].score -= moved;
        self.players[to].score += moved;
        let (a, b) = (&self.players[from], &self.players[to]);
        record(
            &mut *self.log,
            &format!("{} -> {}: {moved}", a.name, b.name),
        );
        moved
    }

    /// Renames player `i` and logs "<old> is now <new>".
    pub fn rename(&mut self, i: usize, new: &str) {
        let p = &mut self.players[i];
        let old = std::mem::replace(&mut p.name, new.to_string());
        record(&mut *self.log, &format!("{old} is now {}", p.name));
    }

    /// Ends the round: logs "round over" and hands the players back for the rest of `'a`.
    pub fn finish(self) -> &'a mut [Player] {
        record(self.log, "round over");
        self.players
    }
}
