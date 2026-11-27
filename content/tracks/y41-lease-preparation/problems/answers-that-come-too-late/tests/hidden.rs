use solution::*;
use Input::{Ask, GiveUp, Reply, TimeUp};
use Output::{Answered, Busy, Failed, Send, SetTimer};

#[allow(dead_code)]
fn tried(attempt: u64, request: u64, wait_ms: u64) -> Vec<Output> {
    vec![Send { attempt, request }, SetTimer { attempt, wait_ms }]
}

#[test]
#[should_panic]
fn no_attempts_at_all_is_refused() {
    Caller::new(10, 10, 0);
}

#[test]
fn one_attempt_only() {
    let mut caller = Caller::new(10, 10, 1);
    caller.handle(Ask { request: 3 });
    check!("fails at once", caller.handle(TimeUp { attempt: 1 }), vec![Failed { request: 3 }]);
    check!("and takes the next", caller.handle(Ask { request: 4 }), tried(2, 4, 10));
}

#[test]
fn the_first_wait_is_capped_too() {
    let mut caller = Caller::new(500, 200, 2);
    check!("first", caller.handle(Ask { request: 1 }), tried(1, 1, 200));
    check!("second", caller.handle(TimeUp { attempt: 1 }), tried(2, 1, 200));
}

#[test]
fn the_waiting_starts_again_for_a_new_question() {
    let mut caller = Caller::new(10, 1000, 4);
    caller.handle(Ask { request: 1 });
    caller.handle(TimeUp { attempt: 1 });
    caller.handle(TimeUp { attempt: 2 });
    caller.handle(Reply { attempt: 3, answer: 0 });
    check!("fresh patience", caller.handle(Ask { request: 2 }), tried(4, 2, 10));
}

#[test]
fn numbers_that_were_never_sent() {
    let mut caller = Caller::new(10, 1000, 4);
    check!("reply while idle", caller.handle(Reply { attempt: 1, answer: 0 }), vec![]);
    caller.handle(Ask { request: 1 });
    for attempt in [0, 2, 3, u64::MAX] {
        check!(format!("reply {attempt}"), caller.handle(Reply { attempt, answer: 0 }), vec![]);
        check!(format!("timer {attempt}"), caller.handle(TimeUp { attempt }), vec![]);
    }
}

#[test]
fn waits_do_not_overflow() {
    let mut caller = Caller::new(u64::MAX / 2 + 5, u64::MAX, 3);
    caller.handle(Ask { request: 1 });
    check!("doubled past the end", caller.handle(TimeUp { attempt: 1 }), tried(2, 1, u64::MAX));
}

#[test]
fn a_hundred_attempts_keep_the_cap() {
    let mut caller = Caller::new(3, 5000, 100);
    caller.handle(Ask { request: 1 });
    let mut wait = 3u64;
    for attempt in 1..100 {
        wait = (wait * 2).min(5000);
        check!(format!("after attempt {attempt}"), caller.handle(TimeUp { attempt }), tried(attempt + 1, 1, wait));
    }
    check!("the hundredth was the last", caller.handle(TimeUp { attempt: 100 }), vec![Failed { request: 1 }]);
}

#[test]
fn a_zero_wait_stays_zero() {
    let mut caller = Caller::new(0, 100, 3);
    check!("first", caller.handle(Ask { request: 1 }), tried(1, 1, 0));
    check!("second", caller.handle(TimeUp { attempt: 1 }), tried(2, 1, 0));
}

#[test]
fn matches_a_model_that_remembers_everything() {
    let mut rng = anneal_prelude::Rng::new(41_201);
    for round in 0..300 {
        let (first, cap, max) = (rng.int(0, 20) as u64, rng.int(0, 60) as u64, rng.int(1, 4) as u32);
        let mut caller = Caller::new(first, cap, max);
        // the model keeps, for every message ever sent, which asking it belonged to
        let mut sent: Vec<usize> = Vec::new();
        let mut askings: Vec<u64> = Vec::new();
        let mut open: Option<usize> = None;
        let mut waits: Vec<u64> = Vec::new();
        for op in 0..60 {
            let any = |rng: &mut anneal_prelude::Rng, n: usize| rng.below(n + 2) as u64;
            let input = match rng.below(8) {
                0 | 1 => Ask { request: rng.below(5) as u64 },
                2 | 3 => Reply { attempt: any(&mut rng, sent.len()), answer: op },
                4 => TimeUp { attempt: sent.len() as u64 },
                5 | 6 => TimeUp { attempt: any(&mut rng, sent.len()) },
                _ => GiveUp,
            };
            let of_open = |attempt: u64| {
                attempt >= 1 && attempt as usize <= sent.len() && open == Some(sent[attempt as usize - 1])
            };
            let mut want = Vec::new();
            match input {
                Ask { request } if open.is_some() => want.push(Busy { request }),
                Ask { request } => {
                    askings.push(request);
                    open = Some(askings.len() - 1);
                    sent.push(askings.len() - 1);
                    waits = vec![first.min(cap)];
                    want = tried(sent.len() as u64, request, waits[0]);
                }
                Reply { attempt, answer } if of_open(attempt) => {
                    want.push(Answered { request: askings[open.unwrap()], answer });
                    open = None;
                }
                TimeUp { attempt } if of_open(attempt) && attempt as usize == sent.len() => {
                    let which = open.unwrap();
                    if waits.len() == max as usize {
                        want.push(Failed { request: askings[which] });
                        open = None;
                    } else {
                        let wait = (waits[waits.len() - 1] * 2).min(cap);
                        waits.push(wait);
                        sent.push(which);
                        want = tried(sent.len() as u64, askings[which], wait);
                    }
                }
                GiveUp => open = None,
                _ => {}
            }
            check!(format!("round {round}, op {op}: {input:?}"), caller.handle(input), want);
        }
    }
}
