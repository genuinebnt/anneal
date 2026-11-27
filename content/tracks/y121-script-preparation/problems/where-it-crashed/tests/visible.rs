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
fn arithmetic() {
    let main = routine("main", 0, &[Push(10), Push(3), Sub, Push(2), Mul, Return]);
    check!("(10 - 3) * 2", run(&[main], 0, &[], ROOMY), Ok(14));
}

#[test]
fn a_recursive_routine() {
    check!("sum(10)", run(&[sum()], 0, &[10], ROOMY), Ok(55));
}

#[test]
fn arguments_come_in_order() {
    let main = routine(
        "main",
        0,
        &[Push(100), Push(9), Push(2), Call(1), Add, Return],
    );
    let minus = routine("minus", 2, &[Load(0), Load(1), Sub, Return]);
    check!("100 + (9 - 2)", run(&[main, minus], 0, &[], ROOMY), Ok(107));
}

#[test]
fn the_trace_says_who_called_whom() {
    let main = routine("main", 0, &[Push(1), Call(1), Return]);
    let check = routine("check", 1, &[Load(0), Pop, Call(2), Return]);
    let boom = routine("boom", 0, &[Fail(42)]);
    check!(
        "boom",
        run(&[main, check, boom], 0, &[], ROOMY),
        crash(Cause::Failed(42), &[("boom", 1), ("check", 3), ("main", 2)])
    );
}

#[test]
fn a_callers_values_are_out_of_reach() {
    let main = routine("main", 0, &[Push(7), Call(1), Return]);
    let thief = routine("thief", 0, &[Pop, Push(0), Return]);
    check!(
        "pop below slot 0",
        run(&[main, thief], 0, &[], ROOMY),
        crash(Cause::Underflow, &[("thief", 1), ("main", 2)])
    );
}
