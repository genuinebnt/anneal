use solution::*;
use Stmt::{Break, Continue};

#[allow(dead_code)]
fn emit(c: char) -> Stmt {
    Stmt::Emit(c)
}
#[allow(dead_code)]
fn seq(list: Vec<Stmt>) -> Stmt {
    Stmt::Seq(list)
}
#[allow(dead_code)]
fn when(flag: u8, a: Stmt, b: Stmt) -> Stmt {
    Stmt::If(flag, Box::new(a), Box::new(b))
}
#[allow(dead_code)]
fn while_(flag: u8, body: Stmt) -> Stmt {
    Stmt::While(flag, Box::new(body))
}
#[allow(dead_code)]
fn nothing() -> Stmt {
    Stmt::Seq(vec![])
}

/// Runs a compiled program. `answers` replies to the flag tests in turn; the run stops when they run out.
/// The result lists emitted characters, and a digit for each flag asked.
#[allow(dead_code)]
fn run(program: &Program, answers: &[bool]) -> String {
    let mut out = String::new();
    let mut at = program.start;
    let mut asked = 0;
    while out.len() < 200 {
        match program.ops.get(at) {
            None => return out + " <link outside ops>",
            Some(Op::Halt) => return out + ".",
            Some(&Op::Emit(c, next)) => {
                out.push(c);
                at = next;
            }
            Some(&Op::Branch { flag, yes, no }) => {
                out.push(char::from(b'0' + flag));
                let Some(&answer) = answers.get(asked) else {
                    return out;
                };
                asked += 1;
                at = if answer { yes } else { no };
            }
        }
    }
    out
}

#[allow(dead_code)]
fn compiled(statement: &Stmt, answers: &[bool]) -> Result<String, CompileError> {
    compile(statement).map(|p| run(&p, answers))
}

use anneal_prelude::Rng;

#[test]
fn loops_with_nothing_in_them() {
    check!("an empty body", compiled(&while_(1, nothing()), &[true, true, false]), Ok("111.".to_string()));
    check!("only continue", compiled(&while_(1, Continue), &[true, true, false]), Ok("111.".to_string()));
    let p = compile(&seq(vec![while_(1, Break), emit('z')])).unwrap();
    check!("only break", run(&p, &[true]), "1z.".to_string());
}

#[test]
fn jumps_go_to_the_nearest_loop() {
    let inner = while_(2, seq(vec![emit('b'), when(3, Break, Continue)]));
    let s = seq(vec![while_(1, seq(vec![emit('a'), inner, emit('c')])), emit('z')]);
    check!("break leaves the inner loop only", compiled(&s, &[true, true, true, false]), Ok("1a2b3c1z.".to_string()));
    check!("continue re-tests the inner loop", compiled(&s, &[true, true, false, false, false]), Ok("1a2b32c1z.".to_string()));
}

#[test]
fn a_jump_after_an_inner_loop_belongs_to_the_outer_one() {
    let s = seq(vec![while_(1, seq(vec![while_(2, emit('i')), Break, emit('x')])), emit('z')]);
    check!("break after the inner loop", compiled(&s, &[true, false]), Ok("12z.".to_string()));
    check!("unreachable code still gets its operation", compile(&s).map(|p| p.ops.len()), Ok(6));
}

#[test]
fn a_hidden_bad_jump_is_still_refused() {
    let s = seq(vec![emit('a'), when(1, nothing(), when(2, Break, nothing())), emit('b')]);
    check!("deep inside ifs", compile(&s).err(), Some(CompileError::JumpOutsideLoop));
}

/// The meaning of a statement, run directly on the tree.
#[allow(dead_code)]
fn walk(s: &Stmt, answers: &[bool], asked: &mut usize, out: &mut String) -> Flow {
    if out.len() >= 200 {
        return Flow::Stop;
    }
    let mut ask = |flag: u8, out: &mut String| {
        out.push(char::from(b'0' + flag));
        let a = answers.get(*asked).copied();
        *asked += 1;
        a
    };
    match s {
        Stmt::Emit(c) => {
            out.push(*c);
            Flow::Next
        }
        Stmt::Seq(list) => {
            for s in list {
                match walk(s, answers, asked, out) {
                    Flow::Next => {}
                    other => return other,
                }
            }
            Flow::Next
        }
        Stmt::If(flag, a, b) => match ask(*flag, out) {
            None => Flow::Stop,
            Some(true) => walk(a, answers, asked, out),
            Some(false) => walk(b, answers, asked, out),
        },
        Stmt::While(flag, body) => loop {
            if out.len() >= 200 {
                return Flow::Stop;
            }
            out.push(char::from(b'0' + *flag));
            let a = answers.get(*asked).copied();
            *asked += 1;
            match a {
                None => return Flow::Stop,
                Some(false) => return Flow::Next,
                Some(true) => match walk(body, answers, asked, out) {
                    Flow::Stop => return Flow::Stop,
                    Flow::Break => return Flow::Next,
                    Flow::Next | Flow::Continue => {}
                },
            }
        },
        Stmt::Break => Flow::Break,
        Stmt::Continue => Flow::Continue,
    }
}

#[allow(dead_code)]
enum Flow {
    Next,
    Break,
    Continue,
    Stop,
}

#[allow(dead_code)]
fn random(rng: &mut Rng, depth: u32, in_loop: bool, count: &mut usize) -> Stmt {
    match rng.below(if depth == 0 { 3 } else { 6 }) {
        1 if in_loop => Break,
        2 if in_loop => Continue,
        0..=2 => {
            *count += 1;
            emit(*rng.pick(&['a', 'b', 'c', 'd']))
        }
        3 => {
            let n = rng.below(4);
            seq((0..n).map(|_| random(rng, depth - 1, in_loop, count)).collect())
        }
        4 => {
            *count += 1;
            let flag = rng.below(10) as u8;
            let a = random(rng, depth - 1, in_loop, count);
            let b = random(rng, depth - 1, in_loop, count);
            when(flag, a, b)
        }
        _ => {
            *count += 1;
            let flag = rng.below(10) as u8;
            while_(flag, random(rng, depth - 1, true, count))
        }
    }
}

#[test]
fn random_programs_behave_like_their_source() {
    let mut rng = Rng::new(1503);
    for _ in 0..1500 {
        let mut count = 0;
        let s = random(&mut rng, 4, false, &mut count);
        let p = compile(&s).expect("every jump is inside a loop");
        check!(format!("number of operations for {s:?}"), p.ops.len(), count + 1);
        check!(format!("only ops[0] is Halt for {s:?}"), p.ops.iter().filter(|&&o| o == Op::Halt).count(), 1);
        check!(format!("ops[0] for {s:?}"), p.ops[0], Op::Halt);
        for _ in 0..4 {
            let n = rng.below(14);
            let answers: Vec<bool> = (0..n).map(|_| rng.bool()).collect();
            let mut want = String::new();
            let mut asked = 0;
            if let Flow::Next = walk(&s, &answers, &mut asked, &mut want) {
                want.push('.');
            }
            check!(format!("{s:?} with answers {answers:?}"), run(&p, &answers), want);
        }
    }
}

#[test]
fn a_very_long_sequence_of_statements() {
    let list: Vec<Stmt> = (0..300_000).map(|i| if i % 2 == 0 { emit('a') } else { while_(1, nothing()) }).collect();
    let p = compile(&seq(list)).unwrap();
    check!("300 000 statements side by side", p.ops.len(), 300_001);
    check!("the first few", run(&p, &[false, false]), "a1a1a1".to_string());
}

#[test]
fn both_sides_of_an_if_may_jump() {
    let s = seq(vec![while_(1, when(2, Break, Continue)), emit('z')]);
    check!("continue then break", compiled(&s, &[true, false, true, true]), Ok("1212z.".to_string()));
    check!("only the loop test, the if and the emit", compile(&s).map(|p| p.ops.len()), Ok(4));
}

#[test]
fn two_loops_one_after_the_other() {
    let s = seq(vec![while_(1, emit('a')), while_(2, seq(vec![emit('b'), Break])), emit('c')]);
    check!("both run", compiled(&s, &[true, false, true]), Ok("1a12bc.".to_string()));
}

#[test]
fn an_endless_loop_stops_at_the_limit() {
    let p = compile(&while_(7, nothing())).unwrap();
    let mut asked = Vec::new();
    let got = execute(&p, &mut |f| {
        asked.push(f);
        true
    }, 1000);
    check!("out of steps, after asking exactly 1000 times", (got, asked.len(), asked.iter().all(|&f| f == 7)), (Err(OutOfSteps), 1000, true));
}

#[test]
fn halt_is_free_and_the_limit_is_exact() {
    let halted = Program { ops: vec![Op::Halt], start: 0 };
    check!("nothing to do needs no steps", execute(&halted, &mut |_| true, 0), Ok(String::new()));
    let by_hand = Program { ops: vec![Op::Halt, Op::Emit('x', 0), Op::Branch { flag: 2, yes: 1, no: 0 }], start: 2 };
    check!("a branch and an emit are two steps", execute(&by_hand, &mut |f| f == 2, 2), Ok("x".to_string()));
    check!("one is too few", execute(&by_hand, &mut |_| true, 1), Err(OutOfSteps));
    check!("the largest limit", execute(&by_hand, &mut |_| false, u64::MAX), Ok(String::new()));
}
