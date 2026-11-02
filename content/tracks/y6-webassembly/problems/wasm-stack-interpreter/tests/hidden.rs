use solution::*;

#[test]
fn large_and_negative_constants() {
    check!(r#"sign extension and 5-byte LEBs"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x80, 0x7F, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0xFF, 0xFF, 0xFF, 0xFF, 0x07, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x80, 0x80, 0x80, 0x80, 0x78, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0xE4, 0x00, 0x0B] }], 0, &[], 100000)), (Ok(vec![-128]), Ok(vec![i32::MAX]), Ok(vec![i32::MIN]), Ok(vec![100])));
}

#[test]
fn arithmetic_wraps() {
    check!(r#"MAX + 1 and 65536 * 65536"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0xFF, 0xFF, 0xFF, 0xFF, 0x07, 0x41, 0x01, 0x6A, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x80, 0x80, 0x04, 0x41, 0x80, 0x80, 0x04, 0x6C, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x00, 0x41, 0x01, 0x6B, 0x0B] }], 0, &[], 100000)), (Ok(vec![i32::MIN]), Ok(vec![0]), Ok(vec![-1])));
}

#[test]
fn division_and_remainder() {
    check!(r#"signed, truncating"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x79, 0x41, 0x02, 0x6D, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x79, 0x41, 0x03, 0x6F, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x80, 0x80, 0x80, 0x80, 0x78, 0x41, 0x7F, 0x6F, 0x0B] }], 0, &[], 100000)), (Ok(vec![-3]), Ok(vec![-1]), Ok(vec![0])));
}

#[test]
fn division_traps() {
    check!(r#"zero divisor and MIN / -1"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x01, 0x41, 0x00, 0x6D, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x01, 0x41, 0x00, 0x6F, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x80, 0x80, 0x80, 0x80, 0x78, 0x41, 0x7F, 0x6D, 0x0B] }], 0, &[], 100000)), (Err(Trap::DivideByZero), Err(Trap::DivideByZero), Err(Trap::IntegerOverflow)));
}

#[test]
fn shifts_take_the_count_mod_32() {
    check!(r#"1 << 33, -8 >> 1, -1 >>> 28"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x01, 0x41, 0x21, 0x74, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x78, 0x41, 0x01, 0x75, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x7F, 0x41, 0x1C, 0x76, 0x0B] }], 0, &[], 100000)), (Ok(vec![2]), Ok(vec![-4]), Ok(vec![15])));
}

#[test]
fn signed_and_unsigned_compare() {
    check!(r#"-1 vs 1"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x7F, 0x41, 0x01, 0x48, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x7F, 0x41, 0x01, 0x49, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x7F, 0x41, 0x01, 0x4B, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x05, 0x41, 0x05, 0x4F, 0x0B] }], 0, &[], 100000)), (Ok(vec![1]), Ok(vec![0]), Ok(vec![1]), Ok(vec![1])));
}

#[test]
fn select_and_drop() {
    check!(r#"c != 0 picks the first"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x0A, 0x41, 0x14, 0x41, 0x01, 0x1B, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x0A, 0x41, 0x14, 0x41, 0x00, 0x1B, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x01, 0x41, 0x02, 0x1A, 0x0B] }], 0, &[], 100000)), (Ok(vec![10]), Ok(vec![20]), Ok(vec![1])));
}

#[test]
fn tee_keeps_the_value_and_locals_start_at_zero() {
    check!(r#"local.tee and an unset local"#, (run(&[Func { params: 0, results: 1, locals: 1, code: vec![0x41, 0x05, 0x22, 0x00, 0x20, 0x00, 0x6A, 0x0B] }], 0, &[], 100000), run(&[Func { params: 1, results: 1, locals: 2, code: vec![0x20, 0x02, 0x0B] }], 0, &[9], 100000)), (Ok(vec![10]), Ok(vec![0])));
}

#[test]
fn br_carries_a_value_and_cuts_the_stack() {
    check!(r#"values below the block survive, extra ones do not"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x02, 0x7F, 0x41, 0x07, 0x0C, 0x00, 0x41, 0x09, 0x0B, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0xE4, 0x00, 0x02, 0x7F, 0x41, 0x01, 0x41, 0x02, 0x0C, 0x00, 0x0B, 0x6A, 0x0B] }], 0, &[], 100000)), (Ok(vec![7]), Ok(vec![102])));
}

#[test]
fn br_out_of_two_blocks() {
    check!(r#"br 1 skips the outer end"#, run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x02, 0x40, 0x02, 0x40, 0x0C, 0x01, 0x0B, 0x00, 0x0B, 0x0B] }], 0, &[], 100000), Ok(vec![]));
}

#[test]
fn br_if_taken_and_not_taken() {
    check!(r#"same block, two conditions"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x02, 0x7F, 0x41, 0x09, 0x41, 0x01, 0x0D, 0x00, 0x1A, 0x41, 0x04, 0x0B, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x02, 0x7F, 0x41, 0x09, 0x41, 0x00, 0x0D, 0x00, 0x1A, 0x41, 0x04, 0x0B, 0x0B] }], 0, &[], 100000)), (Ok(vec![9]), Ok(vec![4])));
}

#[test]
fn return_from_inside_a_block() {
    check!(r#"return keeps the top result"#, run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x02, 0x40, 0x41, 0x03, 0x0F, 0x0B, 0x41, 0x04, 0x0B] }], 0, &[], 100000), Ok(vec![3]));
}

#[test]
fn if_without_else() {
    check!(r#"false skips, true runs"#, (run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x00, 0x04, 0x40, 0x00, 0x0B, 0x41, 0x08, 0x0B] }], 0, &[], 100000), run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x01, 0x04, 0x40, 0x00, 0x0B, 0x41, 0x08, 0x0B] }], 0, &[], 100000)), (Ok(vec![8]), Err(Trap::Unreachable)));
}

#[test]
fn call_passes_arguments_in_order() {
    check!(r#"10 - 3 through call 1"#, run(&[Func { params: 0, results: 1, locals: 0, code: vec![0x41, 0x0A, 0x41, 0x03, 0x10, 0x01, 0x0B] }, Func { params: 2, results: 1, locals: 0, code: vec![0x20, 0x00, 0x20, 0x01, 0x6B, 0x0B] }], 0, &[], 100000), Ok(vec![7]));
}

#[test]
fn several_results() {
    check!(r#"returned bottom to top"#, run(&[Func { params: 0, results: 2, locals: 0, code: vec![0x41, 0x01, 0x41, 0x02, 0x0B] }], 0, &[], 100000), Ok(vec![1, 2]));
}

#[test]
fn call_depth_is_capped_at_100() {
    check!(r#"99 recursions are fine, 100 are not"#, (run(&[Func { params: 1, results: 1, locals: 0, code: vec![0x20, 0x00, 0x45, 0x04, 0x7F, 0x41, 0x00, 0x05, 0x20, 0x00, 0x41, 0x01, 0x6B, 0x10, 0x00, 0x0B, 0x0B] }], 0, &[99], 100000), run(&[Func { params: 1, results: 1, locals: 0, code: vec![0x20, 0x00, 0x45, 0x04, 0x7F, 0x41, 0x00, 0x05, 0x20, 0x00, 0x41, 0x01, 0x6B, 0x10, 0x00, 0x0B, 0x0B] }], 0, &[100], 100000), run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x10, 0x00, 0x0B] }], 0, &[], 1000000)), (Ok(vec![0]), Err(Trap::StackOverflow), Err(Trap::StackOverflow)));
}

#[test]
fn fuel_is_exact() {
    check!(r#"const, drop, end is three units"#, (run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x41, 0x01, 0x1A, 0x0B] }], 0, &[], 3), run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x41, 0x01, 0x1A, 0x0B] }], 0, &[], 2), run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x0B] }], 0, &[], 0)), (Ok(vec![]), Err(Trap::OutOfFuel), Err(Trap::OutOfFuel)));
}

#[test]
fn missing_functions() {
    check!(r#"entry and call targets"#, (run(&[], 0, &[], 10), run(&[Func { params: 0, results: 0, locals: 0, code: vec![0x10, 0x05, 0x0B] }], 0, &[], 100000)), (Err(Trap::BadFunction(0)), Err(Trap::BadFunction(5))));
}

fn func(params: usize, results: usize, locals: usize, code: &[u8]) -> Func {
    Func { params, results, locals, code: code.to_vec() }
}

fn sleb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if (v == 0 && b & 0x40 == 0) || (v == -1 && b & 0x40 != 0) {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

#[test]
fn malformed_code_is_a_trap_not_a_panic() {
    let cases: Vec<(&str, Func)> = vec![
        ("stack underflow", func(0, 1, 0, &[0x6A, 0x0B])),
        ("unknown opcode", func(0, 0, 0, &[0xFF, 0x0B])),
        ("no final end", func(0, 1, 0, &[0x41, 0x01])),
        ("bytes after the final end", func(0, 0, 0, &[0x0B, 0x01])),
        ("block never closed", func(0, 0, 0, &[0x02, 0x40, 0x0B])),
        ("else outside an if", func(0, 0, 0, &[0x02, 0x40, 0x05, 0x0B, 0x0B])),
        ("bad local index", func(0, 0, 0, &[0x20, 0x00, 0x1A, 0x0B])),
        ("bad label depth", func(0, 0, 0, &[0x0C, 0x05, 0x0B])),
        ("truncated immediate", func(0, 0, 0, &[0x41, 0x80])),
        ("bad block type", func(0, 0, 0, &[0x02, 0x7E, 0x0B, 0x0B])),
    ];
    for (what, f) in cases {
        check!(what, run(&[f], 0, &[], 1000), Err(Trap::Malformed));
    }
    check!("wrong argument count", run(&[func(2, 0, 0, &[0x0B])], 0, &[1], 1000), Err(Trap::Malformed));
}

#[test]
fn random_straight_line_arithmetic_matches_a_model() {
    let mut rng = anneal_prelude::Rng::new(4242);
    for round in 0..400 {
        let mut code: Vec<u8> = Vec::new();
        let mut model: Vec<i32> = Vec::new();
        for _ in 0..40 {
            if model.len() < 2 || rng.below(3) == 0 {
                let v = rng.below(4_000_001) as i64 - 2_000_000;
                code.push(0x41);
                sleb(v, &mut code);
                model.push(v as i32);
            } else {
                let b = model.pop().unwrap();
                let a = model.pop().unwrap();
                let (op, r) = match rng.below(6) {
                    0 => (0x6A, a.wrapping_add(b)),
                    1 => (0x6B, a.wrapping_sub(b)),
                    2 => (0x6C, a.wrapping_mul(b)),
                    3 => (0x71, a & b),
                    4 => (0x72, a | b),
                    _ => (0x73, a ^ b),
                };
                code.push(op);
                model.push(r);
            }
        }
        code.push(0x0B);
        let got = run(&[func(0, model.len(), 0, &code)], 0, &[], 100_000);
        check!(format!("round {round}"), got, Ok(model));
    }
}

#[test]
fn random_loops_match_a_model() {
    // sum of i*i for i in 1..=n, with a loop whose body is built from the same pieces every time
    let prog: Vec<u8> = vec![
        0x02, 0x40, 0x03, 0x40, 0x20, 0x00, 0x45, 0x0D, 0x01, // block loop: if n == 0 break
        0x20, 0x01, 0x20, 0x00, 0x20, 0x00, 0x6C, 0x6A, 0x21, 0x01, // acc += n * n
        0x20, 0x00, 0x41, 0x01, 0x6B, 0x21, 0x00, 0x0C, 0x00, 0x0B, 0x0B, // n -= 1; repeat
        0x20, 0x01, 0x0B,
    ];
    let mut rng = anneal_prelude::Rng::new(9);
    for _ in 0..100 {
        let n = rng.below(300) as i32;
        let want: i32 = (1..=n).fold(0i32, |a, i| a.wrapping_add(i.wrapping_mul(i)));
        check!(format!("n = {n}"), run(&[func(1, 1, 1, &prog)], 0, &[n], 1_000_000), Ok(vec![want]));
    }
}
