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
fn nothing_to_run() {
    check!(
        "no main",
        go(vec![], 10),
        stopped("", Cause::UnknownProcedure("main".into()), &[])
    );
    check!("empty main", go(vec![("main", vec![])], 0), fine(""));
}

#[test]
fn a_repeat_of_the_caller_does_not_count() {
    use build::*;
    for step in [Step::NextRound, Step::EndRepeat, in_round(1, log('x'))] {
        let list = vec![
            ("main", vec![rep(3, vec![call("p")])]),
            ("p", vec![log('p'), step.clone()]),
        ];
        check!(
            format!("{step:?} in a called procedure"),
            go(list, 100),
            stopped("p", Cause::NoRepeat, &["p", "main"])
        );
        let list = vec![("main", vec![log('m'), step.clone()])];
        check!(
            format!("{step:?} in main"),
            go(list, 100),
            stopped("m", Cause::NoRepeat, &["main"])
        );
    }
}

#[test]
fn the_round_is_the_nearest_repeats() {
    use build::*;
    let list = vec![(
        "main",
        vec![rep(
            3,
            vec![rep(2, vec![in_round(2, log('i'))]), in_round(3, log('O'))],
        )],
    )];
    check!("rounds", go(list, 100), fine("iiiO"));
    let list = vec![(
        "main",
        vec![rep(
            3,
            vec![
                in_round(2, in_round(2, in_round(2, Step::EndRepeat))),
                log('.'),
            ],
        )],
    )];
    check!("deep inside InRound", go(list, 100), fine("."));
    let list = vec![
        ("main", vec![rep(2, vec![call("p"), in_round(2, log('s'))])]),
        ("p", vec![rep(5, vec![in_round(5, log('f'))])]),
    ];
    check!("a call in between", go(list, 100), fine("ffs"));
}

#[test]
fn what_is_not_reached_is_not_wrong() {
    use build::*;
    let list = vec![(
        "main",
        vec![
            rep(0, vec![call("nobody")]),
            Step::Return,
            call("nobody"),
            Step::NextRound,
        ],
    )];
    check!("unreached", go(list, 100), fine(""));
    let list = vec![(
        "main",
        vec![rep(2, vec![in_round(9, call("nobody"))]), log('k')],
    )];
    check!("round never comes", go(list, 100), fine("k"));
}

#[test]
fn unknown_procedures() {
    use build::*;
    let list = vec![("main", vec![call("a")]), ("a", vec![log('a'), call("zz")])];
    check!(
        "zz",
        go(list, 100),
        stopped("a", Cause::UnknownProcedure("zz".into()), &["a", "main"])
    );
}

#[test]
fn returned_procedures_leave_the_trace() {
    use build::*;
    let list = vec![
        ("main", vec![call("a"), call("a"), Step::Alarm(1)]),
        ("a", vec![rep(3, vec![call("b"), Step::Return])]),
        ("b", vec![Step::Return, Step::Alarm(2)]),
    ];
    check!(
        "all returned",
        go(list, 1000),
        stopped("", Cause::Alarm(1), &["main"])
    );
}

#[test]
fn fifty_may_run_and_no_more() {
    use build::*;
    fn chain(n: usize) -> Vec<(String, Vec<Step>)> {
        let mut list = vec![("main".to_string(), vec![call("p1")])];
        for i in 1..n {
            let steps = if i + 1 < n {
                vec![call(&format!("p{}", i + 1))]
            } else {
                vec![Step::Log('!')]
            };
            list.push((format!("p{i}"), steps));
        }
        list
    }
    for (n, ok) in [(50, true), (51, false)] {
        let owned = chain(n);
        let list: Vec<(&str, Vec<Step>)> =
            owned.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
        let got = go(list, 1000);
        check!(
            format!("{n} procedures"),
            (
                got.1.is_none(),
                got.1.map(|f| (f.0, f.1.len(), f.1[0].clone()))
            ),
            (
                ok,
                if ok {
                    None
                } else {
                    Some((Cause::TooDeep, 50, "p49".to_string()))
                }
            )
        );
    }
}

#[test]
fn endless_work_runs_out_of_time() {
    use build::*;
    let list = vec![("main", vec![rep(u32::MAX, vec![])])];
    check!(
        "empty rounds",
        go(list, 10_000),
        stopped("", Cause::OutOfTime, &["main"])
    );
    let list = vec![(
        "main",
        vec![rep(u32::MAX, vec![rep(u32::MAX, vec![Step::NextRound])])],
    )];
    check!(
        "nested",
        go(list, 10_000),
        stopped("", Cause::OutOfTime, &["main"])
    );
    let list = vec![
        ("main", vec![call("p")]),
        ("p", vec![rep(u32::MAX, vec![Step::Log('.')])]),
    ];
    let got = go(list, 1000);
    check!(
        "logging",
        (got.0.len() > 300, got.0.len() <= 500, got.1),
        (
            true,
            true,
            Some((Cause::OutOfTime, vec!["p".to_string(), "main".to_string()]))
        )
    );
}

#[test]
fn fuel_is_counted_exactly() {
    use build::*;
    // The repeat step 1, then two rounds of 1 + 2 steps each: 7 in all.
    let list = || vec![("main", vec![rep(2, vec![log('a'), log('b')])])];
    check!("7 units", go(list(), 7), fine("abab"));
    check!(
        "6 units",
        go(list(), 6),
        stopped("aba", Cause::OutOfTime, &["main"])
    );
}

// Random scripts of repeats, rounds and exits in one procedure, compared with the same script unrolled by hand:
// each round is a copy of the body, and an exit cuts the copies short.

#[allow(dead_code)]
fn random_steps(rng: &mut anneal_prelude::Rng, depth: usize, inside: bool) -> Vec<Step> {
    (0..rng.below(5))
        .map(|_| match rng.below(if inside { 6 } else { 3 }) {
            0 | 1 if depth > 0 => {
                Step::Repeat(rng.below(4) as u32, random_steps(rng, depth - 1, true))
            }
            3 => Step::InRound(1 + rng.below(3) as u32, Box::new(Step::NextRound)),
            4 => Step::InRound(1 + rng.below(3) as u32, Box::new(Step::EndRepeat)),
            5 => Step::InRound(1 + rng.below(3) as u32, Box::new(Step::Return)),
            _ => Step::Log(*rng.pick(&['a', 'b', 'c', 'd'])),
        })
        .collect()
}

/// Returns false once a `Return` has been met. `jump` is set by the other two exits.
#[allow(dead_code)]
fn unrolled(steps: &[Step], round: u32, log: &mut String, jump: &mut u8) {
    for step in steps {
        if *jump != 0 {
            return;
        }
        match step {
            Step::Log(c) => log.push(*c),
            Step::Repeat(n, body) => {
                for r in 1..=*n {
                    unrolled(body, r, log, jump);
                    if *jump == 1 {
                        *jump = 0;
                        continue;
                    }
                    if *jump == 2 {
                        *jump = 0;
                        break;
                    }
                    if *jump == 3 {
                        return;
                    }
                }
            }
            Step::InRound(r, inner) if *r == round => {
                *jump = match **inner {
                    Step::NextRound => 1,
                    Step::EndRepeat => 2,
                    _ => 3,
                };
            }
            _ => {}
        }
    }
}

#[test]
fn random_scripts() {
    let mut rng = anneal_prelude::Rng::new(6501);
    for _ in 0..2000 {
        let mut steps = random_steps(&mut rng, 4, false);
        steps.push(Step::Log('E'));
        let mut want = String::new();
        unrolled(&steps, 0, &mut want, &mut 0);
        let text = format!("{steps:?}");
        check!(text, go(vec![("main", steps)], 100_000), fine(&want));
    }
}

#[test]
fn an_unknown_name_is_found_before_the_depth() {
    use build::*;
    // main and p1 to p49 run: 50 at once. p49 then names a procedure that does not exist.
    let mut owned = vec![("main".to_string(), vec![call("p1")])];
    for i in 1..50 {
        let next = if i < 49 { format!("p{}", i + 1) } else { "missing".to_string() };
        owned.push((format!("p{i}"), vec![call(&next)]));
    }
    let list: Vec<(&str, Vec<Step>)> = owned.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
    let got = go(list, 1000);
    check!(
        "50 running, then Run(\"missing\")",
        got.1.map(|f| (f.0, f.1.len(), f.1[0].clone())),
        Some((Cause::UnknownProcedure("missing".to_string()), 50, "p49".to_string()))
    );
}
