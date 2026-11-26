pub trait Player {
    fn new(name: &str, score: u32) -> Self
    where
        Self: Sized;
    fn name(&self) -> String;
    fn score(&self) -> u32;

    /// True if `self` ranks above `other`: a higher score, or the same score and a name that sorts first.
    fn beats(&self, other: &dyn Player) -> bool {
        self.score() > other.score()
            || (self.score() == other.score() && self.name() < other.name())
    }
}

pub struct Human {
    pub name: String,
    pub score: u32,
}

/// A team's name is its members joined with "+".
pub struct Team {
    pub members: Vec<String>,
    pub score: u32,
}

impl Player for Human {
    fn new(name: &str, score: u32) -> Self {
        Human {
            name: name.to_string(),
            score,
        }
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn score(&self) -> u32 {
        self.score
    }
}

impl Player for Team {
    fn new(name: &str, score: u32) -> Self {
        Team {
            members: name.split('+').map(String::from).collect(),
            score,
        }
    }

    fn name(&self) -> String {
        self.members.join("+")
    }

    fn score(&self) -> u32 {
        self.score
    }
}

/// The player nobody beats; the first of them on an exact tie. None if there are no players.
pub fn winner(players: &[Box<dyn Player>]) -> Option<&dyn Player> {
    let mut best: Option<&dyn Player> = None;
    for p in players {
        if best.map_or(true, |b| p.beats(b)) {
            best = Some(p.as_ref());
        }
    }
    best
}
