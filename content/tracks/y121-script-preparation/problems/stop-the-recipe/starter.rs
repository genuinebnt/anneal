use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Step {
    /// Writes one character to the log.
    Log(char),
    /// Runs the steps, then runs them again, up to this many rounds.
    Repeat(u32, Vec<Step>),
    /// Runs the step only in this round (from 1) of the nearest repeat.
    InRound(u32, Box<Step>),
    /// Ends this round of the nearest repeat.
    NextRound,
    /// Ends the nearest repeat.
    EndRepeat,
    /// Runs another procedure.
    Run(String),
    /// Ends this procedure.
    Return,
    /// Stops everything with this code.
    Alarm(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    Alarm(u32),
    UnknownProcedure(String),
    NoRepeat,
    TooDeep,
    OutOfTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub cause: Cause,
    /// The procedures that were running, the innermost first.
    pub trace: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub log: String,
    pub failure: Option<Failure>,
}

/// Procedures that may be running at once, the first one included.
pub const MAX_RUNNING: usize = 50;

/// Runs the procedure called `main`, spending at most `fuel` units of time.
pub fn run(procedures: &HashMap<String, Vec<Step>>, fuel: u64) -> Report {
    todo!()
}
