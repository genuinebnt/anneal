use solution::*;
use std::collections::HashMap;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn log(c: char) -> Step {
        Step::Log(c)
    }
    pub fn rep(n: u32, s: Vec<Step>) -> Step {
        Step::Repeat(n, s)
    }
    pub fn in_round(r: u32, s: Step) -> Step {
        Step::InRound(r, Box::new(s))
    }
    pub fn call(n: &str) -> Step {
        Step::Run(n.into())
    }
}

#[allow(dead_code)]
fn go(list: Vec<(&str, Vec<Step>)>, fuel: u64) -> (String, Option<(Cause, Vec<String>)>) {
    let procedures: HashMap<String, Vec<Step>> =
        list.into_iter().map(|(n, s)| (n.to_string(), s)).collect();
    let thread = std::thread::Builder::new().stack_size(64 << 20);
    let handle = thread.spawn(move || run(&procedures, fuel)).expect("spawn");
    let report = handle.join().expect("run panicked");
    (report.log, report.failure.map(|f| (f.cause, f.trace)))
}

#[allow(dead_code)]
fn fine(log: &str) -> (String, Option<(Cause, Vec<String>)>) {
    (log.to_string(), None)
}

#[allow(dead_code)]
fn stopped(log: &str, cause: Cause, trace: &[&str]) -> (String, Option<(Cause, Vec<String>)>) {
    (
        log.to_string(),
        Some((cause, trace.iter().map(|s| s.to_string()).collect())),
    )
}

#[test]
fn repeats_and_calls() {
    use build::*;
    let list = vec![
        ("main", vec![rep(3, vec![call("beep")]), log('.')]),
        ("beep", vec![log('b')]),
    ];
    check!("three beeps", go(list, 100), fine("bbb."));
}

#[test]
fn leaving_a_round() {
    use build::*;
    let list = vec![(
        "main",
        vec![rep(
            3,
            vec![log('a'), in_round(2, Step::NextRound), log('b')],
        )],
    )];
    check!("skip b in round 2", go(list, 100), fine("abaab"));
}

#[test]
fn leaving_the_nearest_repeat_only() {
    use build::*;
    let list = vec![(
        "main",
        vec![rep(
            2,
            vec![
                log('<'),
                rep(9, vec![log('x'), in_round(2, Step::EndRepeat)]),
                log('>'),
            ],
        )],
    )];
    check!("inner ends in round 2", go(list, 100), fine("<xx><xx>"));
}

#[test]
fn return_leaves_one_procedure() {
    use build::*;
    let list = vec![
        ("main", vec![call("p"), log('m')]),
        (
            "p",
            vec![rep(5, vec![rep(5, vec![log('p'), Step::Return])]), log('!')],
        ),
    ];
    check!("return from two repeats", go(list, 100), fine("pm"));
}

#[test]
fn an_alarm_names_who_was_running() {
    use build::*;
    let list = vec![
        ("main", vec![call("a"), call("b")]),
        ("a", vec![log('a')]),
        ("b", vec![rep(2, vec![call("c")])]),
        ("c", vec![log('c'), Step::Alarm(7)]),
    ];
    check!(
        "alarm in c",
        go(list, 100),
        stopped("ac", Cause::Alarm(7), &["c", "b", "main"])
    );
}
