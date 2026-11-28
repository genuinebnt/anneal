use solution::*;
use Instr::*;

#[allow(dead_code)]
fn routine(name: &str, takes: usize, code: &[Instr]) -> Routine {
    // The line of instruction i is i + 1.
    Routine {
        name: name.into(),
        takes,
        code: code
            .iter()
            .enumerate()
            .map(|(i, &x)| (x, i as u32 + 1))
            .collect(),
    }
}

#[allow(dead_code)]
const ROOMY: Limits = Limits {
    calls: 1_000_000,
    values: usize::MAX,
    steps: 10_000_000,
};

#[allow(dead_code)]
fn crash(cause: Cause, trace: &[(&str, u32)]) -> Result<i64, Crash> {
    Err(Crash {
        cause,
        trace: trace.iter().map(|(n, l)| (n.to_string(), *l)).collect(),
    })
}

/// sum(n) = 0 if n == 0, else n + sum(n - 1)
#[allow(dead_code)]
fn sum() -> Routine {
    routine(
        "sum",
        1,
        &[
            Load(0),
            JumpIfZero(9),
            Load(0),
            Load(0),
            Push(1),
            Sub,
            Call(0),
            Add,
            Return,
            Push(0),
            Return,
        ],
    )
}

#[test]
fn every_fault_of_one_routine() {
    let cases: &[(&[Instr], Cause, u32)] = &[
        (&[Pop], Cause::Underflow, 1),
        (&[Push(1), Add], Cause::Underflow, 2),
        (&[Return], Cause::Underflow, 1),
        (&[JumpIfZero(0)], Cause::Underflow, 1),
        (&[Store(0)], Cause::Underflow, 1),
        (&[Load(0)], Cause::BadSlot, 1),
        (&[Push(1), Store(0)], Cause::BadSlot, 2),
        (&[Push(1), Load(usize::MAX)], Cause::BadSlot, 2),
        (&[Push(1), Push(2), Store(1)], Cause::BadSlot, 3),
        (&[Jump(5)], Cause::BadAddress, 1),
        (&[Push(1), Pop], Cause::BadAddress, 2),
        (&[Call(9)], Cause::BadRoutine, 1),
        (&[Push(i64::MAX), Push(1), Add], Cause::Overflow, 3),
        (&[Push(i64::MIN), Push(1), Sub], Cause::Overflow, 3),
        (&[Push(i64::MIN), Push(-1), Mul], Cause::Overflow, 3),
        (&[Fail(3)], Cause::Failed(3), 1),
    ];
    for &(code, cause, line) in cases {
        let got = run(&[routine("r", 0, code)], 0, &[], ROOMY);
        check!(format!("{code:?}"), got, crash(cause, &[("r", line)]));
    }
}

#[test]
fn an_empty_routine_and_a_missing_entry() {
    check!(
        "empty",
        run(&[routine("e", 0, &[])], 0, &[], ROOMY),
        crash(Cause::BadAddress, &[("e", 0)])
    );
    check!(
        "no entry",
        run(&[], 0, &[], ROOMY),
        crash(Cause::BadRoutine, &[])
    );
    check!(
        "entry without arguments",
        run(&[sum()], 0, &[], ROOMY),
        crash(Cause::Underflow, &[])
    );
    let none = Limits { calls: 0, ..ROOMY };
    check!(
        "no calls allowed",
        run(&[sum()], 0, &[1], none),
        crash(Cause::TooManyCalls, &[])
    );
}

#[test]
fn extra_arguments_stay_below() {
    let take_one = routine("one", 1, &[Load(0), Return]);
    check!(
        "the last argument",
        run(&[take_one], 0, &[1, 2, 3], ROOMY),
        Ok(3)
    );
    let reach = routine("reach", 1, &[Pop, Pop]);
    check!(
        "not the others",
        run(&[reach], 0, &[1, 2, 3], ROOMY),
        crash(Cause::Underflow, &[("reach", 2)])
    );
}

#[test]
fn a_call_cannot_borrow_arguments_from_further_down() {
    let main = routine("main", 0, &[Push(0), Push(1), Push(2), Call(1), Return]);
    let middle = routine("middle", 2, &[Pop, Call(2), Return]);
    let greedy = routine("greedy", 2, &[Push(0), Return]);
    check!(
        "one value, two wanted",
        run(&[main, middle, greedy], 0, &[], ROOMY),
        crash(Cause::Underflow, &[("middle", 2), ("main", 4)])
    );
}

#[test]
fn what_a_callee_leaves_behind_disappears() {
    let main = routine("main", 0, &[Push(5), Push(1), Call(1), Add, Return]);
    let messy = routine("messy", 1, &[Push(8), Push(9), Push(100), Return]);
    check!("5 + 100", run(&[main, messy], 0, &[], ROOMY), Ok(105));
}

#[test]
fn locals_are_slots() {
    // Counts n down to 0 while adding it to a second slot.
    let code = [
        Push(0),
        Load(0),
        JumpIfZero(12),
        Load(1),
        Load(0),
        Add,
        Store(1),
        Load(0),
        Push(1),
        Sub,
        Store(0),
        Jump(1),
        Load(1),
        Return,
    ];
    check!(
        "1 + .. + 1000",
        run(&[routine("loop", 1, &code)], 0, &[1000], ROOMY),
        Ok(500500)
    );
}

#[test]
fn the_call_limit_is_exact() {
    let limits = |calls| Limits { calls, ..ROOMY };
    check!("11 active", run(&[sum()], 0, &[10], limits(11)), Ok(55));
    let mut trace = vec![("sum", 7); 10];
    check!(
        "10 allowed",
        run(&[sum()], 0, &[10], limits(10)),
        crash(Cause::TooManyCalls, &trace)
    );
    trace.truncate(3);
    check!(
        "3 allowed",
        run(&[sum()], 0, &[10], limits(3)),
        crash(Cause::TooManyCalls, &trace)
    );
}

#[test]
fn the_value_limit_is_exact() {
    let two = routine("two", 0, &[Push(1), Push(2), Add, Return]);
    check!(
        "2 fit",
        run(
            std::slice::from_ref(&two),
            0,
            &[],
            Limits { values: 2, ..ROOMY }
        ),
        Ok(3)
    );
    check!(
        "1 fits",
        run(&[two], 0, &[], Limits { values: 1, ..ROOMY }),
        crash(Cause::TooManyValues, &[("two", 2)])
    );
    check!(
        "arguments count",
        run(&[sum()], 0, &[1, 2, 3], Limits { values: 2, ..ROOMY }),
        crash(Cause::TooManyValues, &[])
    );
    let main = routine("main", 0, &[Push(1), Push(2), Call(1), Return]);
    let first = routine("first", 2, &[Load(0), Return]);
    check!(
        "a full stack",
        run(
            &[main.clone(), first],
            0,
            &[],
            Limits { values: 3, ..ROOMY }
        ),
        Ok(1)
    );
}

#[test]
fn the_step_limit_is_exact() {
    let code = [Push(1), Push(2), Add, Return];
    let limits = |steps| Limits { steps, ..ROOMY };
    check!(
        "4 steps",
        run(&[routine("r", 0, &code)], 0, &[], limits(4)),
        Ok(3)
    );
    check!(
        "3 steps",
        run(&[routine("r", 0, &code)], 0, &[], limits(3)),
        crash(Cause::OutOfSteps, &[("r", 4)])
    );
    let spin = routine("spin", 0, &[Jump(0)]);
    check!(
        "forever",
        run(&[spin], 0, &[], limits(1000)),
        crash(Cause::OutOfSteps, &[("spin", 1)])
    );
}

#[test]
fn a_hundred_thousand_deep() {
    check!(
        "sum(100 000)",
        run(&[sum()], 0, &[100_000], ROOMY),
        Ok(5_000_050_000)
    );
    let forever = routine("f", 0, &[Call(0)]);
    let got = run(
        &[forever],
        0,
        &[],
        Limits {
            calls: 100_000,
            ..ROOMY
        },
    )
    .map_err(|c| (c.cause, c.trace.len()));
    check!(
        "endless recursion",
        got,
        Err((Cause::TooManyCalls, 100_000))
    );
}

// Random trees of calls: each routine adds, subtracts or multiplies the results of routines after it.

#[test]
fn random_call_trees() {
    let mut rng = anneal_prelude::Rng::new(6901);
    for _ in 0..800 {
        let count = 2 + rng.below(6);
        let mut routines = Vec::new();
        let mut values = vec![0i64; count];
        for i in (0..count).rev() {
            let start = rng.int(-9, 9);
            let mut code = vec![Push(start)];
            let mut value = start;
            for _ in 0..rng.below(4) {
                if i + 1 == count {
                    break;
                }
                let j = i + 1 + rng.below(count - i - 1);
                let (op, result) = match rng.below(3) {
                    0 => (Add, value.checked_add(values[j])),
                    1 => (Sub, value.checked_sub(values[j])),
                    _ => (Mul, value.checked_mul(values[j])),
                };
                let Some(result) = result.filter(|r| r.abs() < 1_000_000) else {
                    continue;
                };
                value = result;
                // A scratch value under the result checks that a call disturbs nothing.
                code.extend([Call(j), op]);
            }
            code.push(Return);
            values[i] = value;
            routines.push(routine(&format!("r{i}"), 0, &code));
        }
        routines.reverse();
        let text = format!("{routines:?}");
        check!(text, run(&routines, 0, &[], ROOMY), Ok(values[0]));
    }
}

#[test]
fn missing_arguments_are_found_before_the_call_limit() {
    let main = routine("main", 0, &[Call(1), Return]);
    let one = Limits { calls: 1, ..ROOMY };
    check!(
        "a call with no arguments, at the call limit",
        run(&[main, sum()], 0, &[], one),
        crash(Cause::Underflow, &[("main", 1)])
    );
    let none = Limits { calls: 0, ..ROOMY };
    check!(
        "the entry with no arguments, and no calls allowed",
        run(&[sum()], 0, &[], none),
        crash(Cause::Underflow, &[])
    );
}
