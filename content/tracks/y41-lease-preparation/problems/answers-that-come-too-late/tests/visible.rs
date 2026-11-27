use solution::*;
use Input::{Ask, GiveUp, Reply, TimeUp};
use Output::{Answered, Busy, Failed, Send, SetTimer};

/// The two outputs of one new attempt.
#[allow(dead_code)]
fn tried(attempt: u64, request: u64, wait_ms: u64) -> Vec<Output> {
    vec![Send { attempt, request }, SetTimer { attempt, wait_ms }]
}

#[test]
fn a_question_is_sent_and_answered() {
    let mut caller = Caller::new(100, 1000, 3);
    check!("ask", caller.handle(Ask { request: 7 }), tried(1, 7, 100));
    check!(
        "reply",
        caller.handle(Reply { attempt: 1, answer: 42 }),
        vec![Answered { request: 7, answer: 42 }]
    );
    check!("its timer goes off later", caller.handle(TimeUp { attempt: 1 }), vec![]);
}

#[test]
fn silence_means_asking_again_with_more_patience() {
    let mut caller = Caller::new(100, 300, 4);
    caller.handle(Ask { request: 7 });
    check!("second", caller.handle(TimeUp { attempt: 1 }), tried(2, 7, 200));
    check!("third, capped", caller.handle(TimeUp { attempt: 2 }), tried(3, 7, 300));
    check!("fourth", caller.handle(TimeUp { attempt: 3 }), tried(4, 7, 300));
    check!("no fifth", caller.handle(TimeUp { attempt: 4 }), vec![Failed { request: 7 }]);
    check!("too late now", caller.handle(Reply { attempt: 4, answer: 1 }), vec![]);
}

#[test]
fn a_slow_answer_to_an_earlier_try_still_counts() {
    let mut caller = Caller::new(100, 1000, 3);
    caller.handle(Ask { request: 7 });
    caller.handle(TimeUp { attempt: 1 });
    check!(
        "answer to try 1",
        caller.handle(Reply { attempt: 1, answer: 5 }),
        vec![Answered { request: 7, answer: 5 }]
    );
    check!("answer to try 2", caller.handle(Reply { attempt: 2, answer: 5 }), vec![]);
}

#[test]
fn one_question_at_a_time() {
    let mut caller = Caller::new(100, 1000, 3);
    caller.handle(Ask { request: 7 });
    check!("busy", caller.handle(Ask { request: 8 }), vec![Busy { request: 8 }]);
    check!(
        "7 is still open",
        caller.handle(Reply { attempt: 1, answer: 1 }),
        vec![Answered { request: 7, answer: 1 }]
    );
    check!("now 8 goes", caller.handle(Ask { request: 8 }), tried(2, 8, 100));
}

#[test]
fn yesterdays_answer_is_not_todays() {
    let mut caller = Caller::new(100, 1000, 3);
    caller.handle(Ask { request: 7 });
    check!("dropped", caller.handle(GiveUp), vec![]);
    check!("next question", caller.handle(Ask { request: 8 }), tried(2, 8, 100));
    check!("old answer", caller.handle(Reply { attempt: 1, answer: 99 }), vec![]);
    check!("old timer", caller.handle(TimeUp { attempt: 1 }), vec![]);
}

#[test]
fn a_timer_of_an_earlier_try_changes_nothing() {
    let mut caller = Caller::new(10, 1000, 5);
    caller.handle(Ask { request: 1 });
    caller.handle(TimeUp { attempt: 1 });
    check!("the same timer again", caller.handle(TimeUp { attempt: 1 }), vec![]);
    check!("the current one", caller.handle(TimeUp { attempt: 2 }), tried(3, 1, 40));
}
