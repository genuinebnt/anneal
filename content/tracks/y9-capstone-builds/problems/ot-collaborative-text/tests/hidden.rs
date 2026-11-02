use solution::*;

#[test]
fn delete_vs_insert_mirrors_insert_vs_delete() {
    check!(r#"arguments swapped"#, (transform(&Op::Delete { pos: 2 }, &Op::Insert { pos: 2, ch: 'x', site: 1 }), transform(&Op::Delete { pos: 2 }, &Op::Insert { pos: 5, ch: 'x', site: 1 })), ((Op::Delete { pos: 3 }, Op::Insert { pos: 2, ch: 'x', site: 1 }), (Op::Delete { pos: 2 }, Op::Insert { pos: 4, ch: 'x', site: 1 })));
}

#[test]
fn nop_is_neutral() {
    check!(r#"both sides"#, (transform(&Op::Nop, &Op::Insert { pos: 1, ch: 'a', site: 1 }), transform(&Op::Delete { pos: 0 }, &Op::Nop), transform(&Op::Nop, &Op::Nop)), ((Op::Nop, Op::Insert { pos: 1, ch: 'a', site: 1 }), (Op::Delete { pos: 0 }, Op::Nop), (Op::Nop, Op::Nop)));
}

#[test]
fn insert_at_the_deleted_position() {
    check!(r#"boundary: <= moves the delete"#, transform(&Op::Insert { pos: 3, ch: 'x', site: 1 }, &Op::Delete { pos: 3 }), (Op::Insert { pos: 3, ch: 'x', site: 1 }, Op::Delete { pos: 4 }));
}

#[test]
fn insert_right_after_the_deleted_character() {
    check!(r#"boundary: > moves the insert"#, transform(&Op::Insert { pos: 4, ch: 'x', site: 1 }, &Op::Delete { pos: 3 }), (Op::Insert { pos: 3, ch: 'x', site: 1 }, Op::Delete { pos: 3 }));
}

#[test]
fn two_inserts_at_the_start_and_end() {
    check!(r#"extreme positions"#, (transform(&Op::Insert { pos: 0, ch: 'a', site: 2 }, &Op::Insert { pos: 0, ch: 'b', site: 1 }), transform(&Op::Insert { pos: 9, ch: 'a', site: 1 }, &Op::Insert { pos: 9, ch: 'b', site: 2 })), ((Op::Insert { pos: 1, ch: 'a', site: 2 }, Op::Insert { pos: 0, ch: 'b', site: 1 }), (Op::Insert { pos: 9, ch: 'a', site: 1 }, Op::Insert { pos: 10, ch: 'b', site: 2 })));
}

#[test]
fn lists_of_ops() {
    check!(r#"two edits each"#, { let s = "hello"; let a = vec![Op::Insert { pos: 0, ch: '>', site: 1 }, Op::Delete { pos: 3 }]; let b = vec![Op::Insert { pos: 5, ch: '!', site: 2 }, Op::Delete { pos: 0 }]; let (a2, b2) = transform_lists(&a, &b); (apply_all(&apply_all(s, &a), &b2), apply_all(&apply_all(s, &b), &a2), a2.len(), b2.len()) }, (">elo!".to_string(), ">elo!".to_string(), 2, 2));
}

#[test]
fn empty_lists() {
    check!(r#"nothing to transform"#, { let a = vec![Op::Insert { pos: 0, ch: 'x', site: 1 }]; let (a2, b2) = transform_lists(&a, &[]); (a2 == a, b2.len()) }, (true, 0));
}

#[test]
fn both_delete_everything() {
    check!(r#"same deletions become Nops"#, { let s = "abc"; let a = vec![Op::Delete { pos: 0 }, Op::Delete { pos: 0 }, Op::Delete { pos: 0 }]; let (a2, b2) = transform_lists(&a, &a); (apply_all(&apply_all(s, &a), &b2), apply_all(&apply_all(s, &a), &a2) == "", b2.iter().all(|o| *o == Op::Nop)) }, ("".to_string(), true, true));
}

fn random_op(rng: &mut anneal_prelude::Rng, text: &str, site: u32) -> Op {
    let len = text.chars().count();
    if len > 0 && rng.below(3) == 0 {
        Op::Delete { pos: rng.below(len) }
    } else {
        Op::Insert { pos: rng.below(len + 1), ch: (b'a' + rng.below(26) as u8) as char, site }
    }
}

fn random_seq(rng: &mut anneal_prelude::Rng, start: &str, site: u32, n: usize) -> Vec<Op> {
    let mut text = start.to_string();
    let mut ops = Vec::new();
    for _ in 0..n {
        let op = random_op(rng, &text, site);
        text = apply(&text, &op);
        ops.push(op);
    }
    ops
}

#[test]
fn single_ops_always_converge() {
    let mut rng = anneal_prelude::Rng::new(2024);
    for round in 0..3000 {
        let len = rng.below(8);
        let s: String = (0..len).map(|_| (b'a' + rng.below(26) as u8) as char).collect();
        let a = random_op(&mut rng, &s, 1);
        let b = random_op(&mut rng, &s, 2);
        let (a2, b2) = transform(&a, &b);
        let left = apply(&apply(&s, &a), &b2);
        let right = apply(&apply(&s, &b), &a2);
        check!(format!("round {round}: {s:?} a={a:?} b={b:?}"), left, right);
    }
}

#[test]
fn sequences_always_converge() {
    let mut rng = anneal_prelude::Rng::new(99);
    for round in 0..1500 {
        let len = rng.below(10);
        let s: String = (0..len).map(|_| (b'A' + rng.below(26) as u8) as char).collect();
        let (na, nb) = (rng.below(6), rng.below(6));
        let a = random_seq(&mut rng, &s, 1, na);
        let b = random_seq(&mut rng, &s, 2, nb);
        let (a2, b2) = transform_lists(&a, &b);
        check!(format!("round {round}: lengths are kept"), (a2.len(), b2.len()), (a.len(), b.len()));
        let left = apply_all(&apply_all(&s, &a), &b2);
        let right = apply_all(&apply_all(&s, &b), &a2);
        check!(format!("round {round}: {s:?} a={a:?} b={b:?}"), left, right);
    }
}

#[test]
fn identical_characters_stress_the_tie_breaks() {
    // all characters equal, so only positions and sites decide: any mishandled tie shows up as a different length or order of edits
    let mut rng = anneal_prelude::Rng::new(5);
    for round in 0..800 {
        let len = rng.below(6);
        let s: String = (0..len).map(|_| 'z').collect();
        let (na, nb) = (rng.below(5), rng.below(5));
        let a = random_seq(&mut rng, &s, 1, na);
        let b = random_seq(&mut rng, &s, 2, nb);
        let (a_for_b, b_for_a) = transform_lists(&a, &b);
        let at_a = apply_all(&apply_all(&s, &a), &b_for_a);
        let at_b = apply_all(&apply_all(&s, &b), &a_for_b);
        check!(format!("round {round}: replicas agree"), at_a, at_b);
    }
}
