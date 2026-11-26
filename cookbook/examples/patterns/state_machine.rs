//! RECIPE 47 (patterns): an enum state machine where illegal transitions are impossible to write silently.
//! LEVEL: intermediate
//! Use in: anything with a lifecycle (job/task/connection states); modelling "can this move from A to B?".   Run: cargo run --example state_machine
//! COMMON USES: order/payment/job/connection lifecycles: make illegal transitions unrepresentable and testable.
#![allow(dead_code)]
//!
//! Pattern: states are enum variants that CARRY the data valid in that state; `transition(self, event)` consumes the
//! old state and returns the new one (or an error). An exhaustive `match` on `(state, event)` makes every legal move
//! explicit and everything else an error. Terminal states can never be left because no arm leaves them.
//! Domain here: parcel delivery (not tasks!). Compare with the typestate approach in typestate_builder.
#[derive(Debug, Clone, PartialEq)]
enum Parcel {
    Created,
    Shipped { carrier: String },
    Delivered { signed_by: String },
    Returned { reason: String },
}
#[derive(Debug)]
enum Event {
    Ship(String),
    Deliver(String),
    Return(String),
}
#[derive(Debug, PartialEq)]
struct BadTransition {
    from: String,
    event: String,
}

impl Parcel {
    fn is_terminal(&self) -> bool {
        matches!(self, Parcel::Delivered { .. } | Parcel::Returned { .. })
    }
    fn on(self, ev: Event) -> Result<Parcel, BadTransition> {
        use Event::*;
        use Parcel::*;
        match (self, ev) {
            (Created, Ship(carrier)) => Ok(Shipped { carrier }),
            (Shipped { .. }, Deliver(signed_by)) => Ok(Delivered { signed_by }),
            (Shipped { .. }, Return(reason)) => Ok(Returned { reason }),
            (state, ev) => Err(BadTransition {
                from: format!("{state:?}"),
                event: format!("{ev:?}"),
            }),
        }
    }
}

fn main() {
    let p = Parcel::Created;
    let p = p.on(Event::Ship("ups".into())).unwrap();
    println!("{p:?}");
    let p = p.on(Event::Deliver("ana".into())).unwrap();
    println!("{p:?} terminal={}", p.is_terminal());
    println!("{:?}", p.clone().on(Event::Return("late".into())));
    println!("{:?}", Parcel::Created.on(Event::Deliver("x".into())));

    // Test the whole transition table by brute force: terminal states must reject every event.
    let events = || {
        vec![
            Event::Ship("c".into()),
            Event::Deliver("s".into()),
            Event::Return("r".into()),
        ]
    };
    for terminal in [
        Parcel::Delivered {
            signed_by: "a".into(),
        },
        Parcel::Returned { reason: "b".into() },
    ] {
        for ev in events() {
            assert!(
                terminal.clone().on(ev).is_err(),
                "terminal state {terminal:?} was left!"
            );
        }
    }
    println!("invariant holds: terminal states cannot be left");
}
