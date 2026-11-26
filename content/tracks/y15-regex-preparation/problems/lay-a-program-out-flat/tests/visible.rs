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

#[test]
fn a_sequence_runs_in_order() {
    let s = seq(vec![emit('a'), emit('b'), emit('c')]);
    check!("a; b; c", compiled(&s, &[]), Ok("abc.".to_string()));
    check!("an empty sequence", compiled(&nothing(), &[]), Ok(".".to_string()));
}

#[test]
fn if_takes_one_side_and_carries_on() {
    let s = seq(vec![when(1, emit('y'), emit('n')), emit('z')]);
    check!("flag set", compiled(&s, &[true]), Ok("1yz.".to_string()));
    check!("flag clear", compiled(&s, &[false]), Ok("1nz.".to_string()));
}

#[test]
fn while_tests_again_after_the_body() {
    let s = seq(vec![while_(2, emit('a')), emit('z')]);
    check!("twice", compiled(&s, &[true, true, false]), Ok("2a2a2z.".to_string()));
    check!("never", compiled(&s, &[false]), Ok("2z.".to_string()));
}

#[test]
fn break_and_continue() {
    let body = seq(vec![emit('a'), when(2, Break, nothing()), when(3, Continue, nothing()), emit('b')]);
    let s = seq(vec![while_(1, body), emit('z')]);
    check!("break out", compiled(&s, &[true, true]), Ok("1a2z.".to_string()));
    check!("continue goes to the test", compiled(&s, &[true, false, true, false]), Ok("1a231z.".to_string()));
}

#[test]
fn one_operation_per_emit_if_and_while() {
    let s = while_(1, seq(vec![emit('a'), when(2, Break, Continue), nothing()]));
    let p = compile(&s).unwrap();
    check!("while + emit + if, and the Halt", p.ops.len(), 4);
    check!("ops[0] is Halt", p.ops[0], Op::Halt);
}

#[test]
fn a_jump_needs_a_loop() {
    check!("break alone", compile(&Break).err(), Some(CompileError::JumpOutsideLoop));
    check!("continue after a loop", compile(&seq(vec![while_(1, nothing()), Continue])).err(), Some(CompileError::JumpOutsideLoop));
}

#[test]
fn execute_returns_what_was_emitted() {
    let p = compile(&seq(vec![while_(1, emit('a')), emit('z')])).unwrap();
    let mut answers = [true, true, false].into_iter();
    check!("two rounds", execute(&p, &mut |_| answers.next().unwrap_or(false), 100), Ok("aaz".to_string()));
}

#[test]
fn a_run_that_hits_the_limit_has_no_output() {
    let p = compile(&seq(vec![emit('a'), emit('b'), emit('c')])).unwrap();
    check!("three steps are enough", execute(&p, &mut |_| false, 3), Ok("abc".to_string()));
    check!("two are not", execute(&p, &mut |_| false, 2), Err(OutOfSteps));
}
