use solution::*;

#[test]
fn fresh_allocator() {
    check!(r#"one big free block"#, { let b = Buddy::new(4); (b.free_units(), b.largest_free()) }, (16, 16));
}

#[test]
fn first_allocations_are_lowest() {
    check!(r#"1, 1, 2, 4 units in 8"#, { let mut b = Buddy::new(3); [b.alloc(1), b.alloc(1), b.alloc(2), b.alloc(4)] }, [Some(0), Some(1), Some(2), Some(4)]);
}

#[test]
fn rounds_up_to_power_of_two() {
    check!(r#"5 units takes 8"#, { let mut b = Buddy::new(4); let a = b.alloc(5); (a, b.free_units()) }, (Some(0), 8));
}

#[test]
fn exhaustion() {
    check!(r#"everything allocated"#, { let mut b = Buddy::new(2); let all = b.alloc(4); (all, b.alloc(1), b.free_units()) }, (Some(0), None, 0));
}

#[test]
fn free_coalesces_back() {
    check!(r#"alloc 1, free it, alloc the whole range"#, { let mut b = Buddy::new(3); let a = b.alloc(1).unwrap(); b.free(a, 1); (b.largest_free(), b.alloc(8)) }, (8, Some(0)));
}
