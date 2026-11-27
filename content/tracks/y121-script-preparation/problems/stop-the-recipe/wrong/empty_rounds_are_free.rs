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

/// How a step ended, when it did not stop everything.
#[derive(Clone, Copy)]
enum Flow {
    Done,
    NextRound,
    EndRepeat,
    Return,
}

struct Machine<'a> {
    procedures: &'a HashMap<String, Vec<Step>>,
    log: String,
    running: Vec<&'a str>,
    fuel: u64,
}

impl<'a> Machine<'a> {
    fn fail(&self, cause: Cause) -> Failure {
        let trace = self.running.iter().rev().map(|s| s.to_string()).collect();
        Failure { cause, trace }
    }

    fn spend(&mut self) -> Result<(), Failure> {
        self.fuel = self
            .fuel
            .checked_sub(1)
            .ok_or_else(|| self.fail(Cause::OutOfTime))?;
        Ok(())
    }

    fn call(&mut self, name: &str) -> Result<(), Failure> {
        let Some((name, steps)) = self.procedures.get_key_value(name) else {
            return Err(self.fail(Cause::UnknownProcedure(name.to_string())));
        };
        if self.running.len() == MAX_RUNNING {
            return Err(self.fail(Cause::TooDeep));
        }
        self.running.push(name);
        let flow = self.list(steps, None)?;
        if matches!(flow, Flow::NextRound | Flow::EndRepeat) {
            return Err(self.fail(Cause::NoRepeat));
        }
        self.running.pop();
        Ok(())
    }

    fn list(&mut self, steps: &'a [Step], round: Option<u32>) -> Result<Flow, Failure> {
        for step in steps {
            match self.step(step, round)? {
                Flow::Done => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Done)
    }

    fn step(&mut self, step: &'a Step, round: Option<u32>) -> Result<Flow, Failure> {
        self.spend()?;
        match step {
            Step::Log(c) => self.log.push(*c),
            Step::Repeat(rounds, steps) => {
                for round in 1..=*rounds {
                    match self.list(steps, Some(round))? {
                        Flow::Done | Flow::NextRound => {}
                        Flow::EndRepeat => break,
                        Flow::Return => return Ok(Flow::Return),
                    }
                }
            }
            Step::InRound(wanted, inner) => {
                let Some(current) = round else {
                    return Err(self.fail(Cause::NoRepeat));
                };
                if current == *wanted {
                    return self.step(inner, round);
                }
            }
            Step::NextRound => return Ok(Flow::NextRound),
            Step::EndRepeat => return Ok(Flow::EndRepeat),
            Step::Run(name) => self.call(name)?,
            Step::Return => return Ok(Flow::Return),
            Step::Alarm(code) => return Err(self.fail(Cause::Alarm(*code))),
        }
        Ok(Flow::Done)
    }
}

/// Runs the procedure called `main`, spending at most `fuel` units of time.
pub fn run(procedures: &HashMap<String, Vec<Step>>, fuel: u64) -> Report {
    let mut machine = Machine {
        procedures,
        log: String::new(),
        running: Vec::new(),
        fuel,
    };
    let failure = machine.call("main").err();
    Report {
        log: machine.log,
        failure,
    }
}
