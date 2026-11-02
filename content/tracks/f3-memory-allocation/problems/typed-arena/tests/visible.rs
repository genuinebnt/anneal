use solution::*;

#[test]
fn alloc_and_read() {
    check!(r#"three values"#, { let a = Arena::new(); let x = a.alloc(1); let y = a.alloc(2); let z = a.alloc(3); (*x, *y, *z, a.len()) }, (1, 2, 3, 3));
}

#[test]
fn mutate_through_the_reference() {
    check!(r#"write after alloc"#, { let a = Arena::new(); let x = a.alloc(String::from("a")); x.push('b'); x.clone() }, "ab".to_string());
}

#[test]
fn empty() {
    check!(r#"fresh arena"#, { let a: Arena<u8> = Arena::new(); (a.len(), a.is_empty(), a.chunk_count()) }, (0, true, 0));
}

#[test]
fn first_chunk_holds_four() {
    check!(r#"4 values then a 5th"#, { let a = Arena::new(); for i in 0..4 { a.alloc(i); } let c4 = a.chunk_count(); a.alloc(4); (c4, a.chunk_count()) }, (1, 2));
}

#[test]
fn many_references_alive_together() {
    check!(r#"10k refs held across later allocations"#, { let a = Arena::new(); let refs: Vec<&mut u64> = (0..10_000u64).map(|i| a.alloc(i)).collect(); refs.iter().map(|r| **r).sum::<u64>() }, 49_995_000);
}
