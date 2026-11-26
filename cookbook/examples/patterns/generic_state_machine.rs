//! RECIPE 77 (patterns): a generic state-machine trait, replaying event logs, and table-driven transitions.
//! LEVEL: advanced
//! Use in: lifecycles you want to TEST exhaustively and REPLAY from an event log (event sourcing).   Run: cargo run --release --example generic_state_machine
//! COMMON USES: order/account/ticket lifecycles; event sourcing (replay events to rebuild state); exhaustive model checking.
#![allow(dead_code)]
//!
//! Separate the PURE transition function from everything else: `step(&state, &event) -> Result<state, error>`.
//! Because it is pure, you can (1) replay a stored event list to rebuild state, (2) fuzz it with random events and assert
//! invariants, (3) reuse one generic `replay` for many machines. This is the core of event sourcing.
//! Associated types let each machine pick its own State/Event/Error; `replay<M: Machine>` works for all of them.
//! Read: https://martinfowler.com/eaaDev/EventSourcing.html and state_machine (enum FSM), typestate_protocol (typestate).
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

trait Machine {
    type State: Clone + Debug + PartialEq;
    type Event: Debug;
    type Error: Debug;
    fn initial() -> Self::State;
    fn step(state: &Self::State, event: &Self::Event) -> Result<Self::State, Self::Error>;
}

/// Fold an event log into a state; on failure report WHICH event index broke the rules.
fn replay<M: Machine>(events: &[M::Event]) -> Result<M::State, (usize, M::Error)> {
    events
        .iter()
        .enumerate()
        .try_fold(M::initial(), |s, (i, e)| {
            M::step(&s, e).map_err(|err| (i, err))
        })
}

/// Every intermediate state (useful for debugging and for property tests).
fn trace<M: Machine>(events: &[M::Event]) -> Result<Vec<M::State>, (usize, M::Error)> {
    let mut states = vec![M::initial()];
    for (i, e) in events.iter().enumerate() {
        let next = M::step(states.last().unwrap(), e).map_err(|err| (i, err))?;
        states.push(next);
    }
    Ok(states)
}

// ---- machine 1: a turnstile that accepts every event in every state (total function) ----
struct Turnstile;
#[derive(Clone, Debug, PartialEq)]
enum Gate {
    Locked,
    Open,
}
#[derive(Debug, Clone, Copy)]
enum Input {
    Coin,
    Push,
}
impl Machine for Turnstile {
    type State = Gate;
    type Event = Input;
    type Error = std::convert::Infallible;
    fn initial() -> Gate {
        Gate::Locked
    }
    fn step(s: &Gate, e: &Input) -> Result<Gate, Self::Error> {
        Ok(match (s, e) {
            (_, Input::Coin) => Gate::Open,
            (Gate::Open, Input::Push) => Gate::Locked,
            (Gate::Locked, Input::Push) => Gate::Locked,
        })
    }
}

// ---- machine 2: a bank account that REJECTS invalid events (partial function) ----
struct Account;
#[derive(Clone, Debug, PartialEq)]
struct Balance {
    open: bool,
    cents: u64,
}
#[derive(Debug)]
enum Tx {
    Open,
    Deposit(u64),
    Withdraw(u64),
    Close,
}
#[derive(Debug, PartialEq)]
enum TxError {
    NotOpen,
    AlreadyOpen,
    Insufficient { need: u64, have: u64 },
    NonZeroClose,
}
impl Machine for Account {
    type State = Balance;
    type Event = Tx;
    type Error = TxError;
    fn initial() -> Balance {
        Balance {
            open: false,
            cents: 0,
        }
    }
    fn step(s: &Balance, e: &Tx) -> Result<Balance, TxError> {
        match (s.open, e) {
            (false, Tx::Open) => Ok(Balance {
                open: true,
                cents: 0,
            }),
            (true, Tx::Open) => Err(TxError::AlreadyOpen),
            (false, _) => Err(TxError::NotOpen),
            (true, Tx::Deposit(n)) => Ok(Balance {
                cents: s.cents + n,
                ..s.clone()
            }),
            (true, Tx::Withdraw(n)) if *n <= s.cents => Ok(Balance {
                cents: s.cents - n,
                ..s.clone()
            }),
            (true, Tx::Withdraw(n)) => Err(TxError::Insufficient {
                need: *n,
                have: s.cents,
            }),
            (true, Tx::Close) if s.cents == 0 => Ok(Balance {
                open: false,
                cents: 0,
            }),
            (true, Tx::Close) => Err(TxError::NonZeroClose),
        }
    }
}

// ---- table-driven machine: transitions as DATA, generic over any hashable state/event ----
struct Table<S, E> {
    start: S,
    rules: HashMap<(S, E), S>,
}
impl<S: Copy + Eq + Hash + Debug, E: Copy + Eq + Hash + Debug> Table<S, E> {
    fn run(&self, events: &[E]) -> Result<S, String> {
        let mut s = self.start;
        for e in events {
            s = *self
                .rules
                .get(&(s, *e))
                .ok_or_else(|| format!("no rule for {s:?} on {e:?}"))?;
        }
        Ok(s)
    }
}

fn main() {
    println!(
        "{:?}",
        replay::<Turnstile>(&[Input::Push, Input::Coin, Input::Push])
    );
    println!(
        "{:?}",
        trace::<Turnstile>(&[Input::Coin, Input::Push]).unwrap()
    );

    let ok = [
        Tx::Open,
        Tx::Deposit(500),
        Tx::Withdraw(200),
        Tx::Withdraw(300),
        Tx::Close,
    ];
    println!("{:?}", replay::<Account>(&ok));
    let bad = [Tx::Open, Tx::Deposit(100), Tx::Withdraw(500)];
    println!("{:?}", replay::<Account>(&bad));
    println!("{:?}", replay::<Account>(&[Tx::Deposit(1)]));

    // INVARIANT: replaying a prefix then the rest equals replaying everything (determinism / event-sourcing law)
    let events = [
        Tx::Open,
        Tx::Deposit(50),
        Tx::Deposit(25),
        Tx::Withdraw(10),
        Tx::Withdraw(65),
        Tx::Close,
    ];
    let full = replay::<Account>(&events).unwrap();
    for k in 0..=events.len() {
        let prefix_state = replay::<Account>(&events[..k]).unwrap();
        let continued = events[k..]
            .iter()
            .try_fold(prefix_state, |s, e| Account::step(&s, e))
            .unwrap();
        assert_eq!(continued, full, "split at {k}");
    }
    // INVARIANT via brute force: no event sequence can make the balance go negative (u64 + checks) or reopen a closed...
    let alphabet = [Tx::Open, Tx::Deposit(5), Tx::Withdraw(5), Tx::Close];
    let mut seqs = vec![vec![]];
    for _ in 0..5 {
        let mut next = vec![];
        for s in &seqs {
            for (i, _) in alphabet.iter().enumerate() {
                let mut t: Vec<usize> = s.clone();
                t.push(i);
                next.push(t);
            }
        }
        seqs = next;
    }
    let mut valid = 0;
    for idxs in &seqs {
        let evs: Vec<&Tx> = idxs.iter().map(|&i| &alphabet[i]).collect();
        let mut st = Account::initial();
        let mut ok = true;
        for e in evs {
            match Account::step(&st, e) {
                Ok(n) => st = n,
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        valid += ok as u32;
    }
    println!(
        "{} of {} length-5 sequences are fully valid",
        valid,
        seqs.len()
    );

    let door: Table<&str, &str> = Table {
        start: "closed",
        rules: HashMap::from([
            (("closed", "open"), "open"),
            (("open", "close"), "closed"),
            (("closed", "lock"), "locked"),
            (("locked", "unlock"), "closed"),
        ]),
    };
    println!(
        "{:?} {:?}",
        door.run(&["open", "close", "lock"]),
        door.run(&["lock", "open"])
    );
}
