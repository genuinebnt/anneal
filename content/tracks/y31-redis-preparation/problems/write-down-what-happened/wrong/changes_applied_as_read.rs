use std::collections::HashMap;

/// A source of random numbers. `below(n)` is in `0..n`.
pub trait Dice {
    fn below(&mut self, n: usize) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Seat {
    Free,
    /// Held for a while by someone who has not paid.
    Held { who: String, until: u64 },
    Sold { who: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Hold any free seat for `ms` milliseconds.
    Hold { who: String, ms: u64 },
    /// Buy the seat held.
    Buy { who: String },
    /// Give the seat back, held or sold.
    Release { who: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// The command was done, about this seat.
    Done(usize),
    Refused,
}

/// One change to the hall.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Held { seat: usize, who: String, until: u64 },
    Sold { seat: usize },
    Freed { seat: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Begin,
    Change(Effect),
    End,
}

/// The log does not describe a history of this hall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLog {
    /// The index of the entry that is wrong.
    pub at: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hall {
    seats: Vec<Seat>,
    /// Which seat each person has.
    owners: HashMap<String, usize>,
}

impl Hall {
    pub fn new(size: usize) -> Hall {
        Hall {
            seats: vec![Seat::Free; size],
            owners: HashMap::new(),
        }
    }

    pub fn seats(&self) -> &[Seat] {
        &self.seats
    }

    /// Applies one change if it fits the hall.
    fn apply(&mut self, effect: &Effect) -> bool {
        match effect {
            Effect::Held { seat, who, until } => {
                if self.seats.get(*seat) != Some(&Seat::Free) || self.owners.contains_key(who) {
                    return false;
                }
                self.seats[*seat] = Seat::Held {
                    who: who.clone(),
                    until: *until,
                };
                self.owners.insert(who.clone(), *seat);
            }
            Effect::Sold { seat } => {
                let Some(Seat::Held { who, .. }) = self.seats.get(*seat) else {
                    return false;
                };
                self.seats[*seat] = Seat::Sold { who: who.clone() };
            }
            Effect::Freed { seat } => {
                let who = match self.seats.get(*seat) {
                    Some(Seat::Held { who, .. } | Seat::Sold { who }) => who.clone(),
                    _ => return false,
                };
                self.seats[*seat] = Seat::Free;
                self.owners.remove(&who);
            }
        }
        true
    }

    fn change(&mut self, effect: Effect, effects: &mut Vec<Effect>) {
        let fits = self.apply(&effect);
        debug_assert!(fits, "the hall only makes changes that fit");
        effects.push(effect);
    }

    pub fn run(
        &mut self,
        commands: &[Command],
        now: u64,
        dice: &mut dyn Dice,
    ) -> (Vec<Reply>, Vec<Entry>) {
        let mut effects = Vec::new();
        for seat in 0..self.seats.len() {
            if matches!(self.seats[seat], Seat::Held { until, .. } if until <= now) {
                self.change(Effect::Freed { seat }, &mut effects);
            }
        }
        let replies = commands
            .iter()
            .map(|c| self.one(c, now, dice, &mut effects))
            .collect();
        let wrap = effects.len() > 1;
        let mut log = Vec::new();
        if wrap {
            log.push(Entry::Begin);
        }
        log.extend(effects.into_iter().map(Entry::Change));
        if wrap {
            log.push(Entry::End);
        }
        (replies, log)
    }

    fn one(
        &mut self,
        command: &Command,
        now: u64,
        dice: &mut dyn Dice,
        effects: &mut Vec<Effect>,
    ) -> Reply {
        match command {
            Command::Hold { who, ms } => {
                if self.owners.contains_key(who) {
                    return Reply::Refused;
                }
                let free: Vec<usize> = (0..self.seats.len())
                    .filter(|&s| self.seats[s] == Seat::Free)
                    .collect();
                if free.is_empty() {
                    return Reply::Refused;
                }
                let seat = free[dice.below(free.len())];
                let until = now.saturating_add(*ms);
                self.change(
                    Effect::Held {
                        seat,
                        who: who.clone(),
                        until,
                    },
                    effects,
                );
                Reply::Done(seat)
            }
            Command::Buy { who } => match self.owners.get(who).copied() {
                Some(seat) if matches!(self.seats[seat], Seat::Held { .. }) => {
                    self.change(Effect::Sold { seat }, effects);
                    Reply::Done(seat)
                }
                _ => Reply::Refused,
            },
            Command::Release { who } => match self.owners.get(who).copied() {
                Some(seat) => {
                    self.change(Effect::Freed { seat }, effects);
                    Reply::Done(seat)
                }
                None => Reply::Refused,
            },
        }
    }

    pub fn replay(size: usize, log: &[Entry]) -> Result<(Hall, usize), BadLog> {
        let mut hall = Hall::new(size);
        let mut used = 0;
        // the changes of the unit being read
        let mut open: Option<Vec<(usize, &Effect)>> = None;
        for (at, entry) in log.iter().enumerate() {
            let ready = match entry {
                Entry::Begin => {
                    if open.replace(Vec::new()).is_some() {
                        return Err(BadLog { at });
                    }
                    continue;
                }
                Entry::End => open.take().ok_or(BadLog { at })?,
                Entry::Change(effect) => match &mut open {
                    Some(_) => {
                        if !hall.apply(effect) {
                            return Err(BadLog { at });
                        }
                        continue;
                    }
                    None => vec![(at, effect)],
                },
            };
            for (at, effect) in ready {
                if !hall.apply(effect) {
                    return Err(BadLog { at });
                }
            }
            used = at + 1;
        }
        Ok((hall, used))
    }
}
