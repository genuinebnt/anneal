use solution::*;

#[test]
fn empty_interner() {
    check!(r#"nothing interned"#, { let i = Interner::new(); (i.len(), i.is_empty(), i.get("")) }, (0, true, None));
}

#[test]
fn empty_string() {
    check!(r#"the empty string is a string"#, { let mut i = Interner::new(); let e = i.intern(""); let a = i.intern("a"); (i.resolve(e).len(), e == a, i.intern("") == e, i.len()) }, (0, false, true, 2));
}

#[test]
fn stored_exactly() {
    check!(r#"no trimming or case folding"#, { let mut i = Interner::new(); let a = i.intern("Foo"); let b = i.intern("foo"); let c = i.intern(" foo"); let d = i.intern("foo "); (a == b, b == c, c == d, i.len(), i.resolve(c).to_string()) }, (false, false, false, 4, " foo".to_string()));
}

#[test]
fn unicode() {
    check!(r#"multi-byte strings"#, { let mut i = Interner::new(); let a = i.intern("héllo"); let b = i.intern("日本語"); let c = i.intern("héllo"); (a == c, i.resolve(b).to_string(), i.len()) }, (true, "日本語".to_string(), 2));
}

#[test]
fn symbol_is_four_bytes() {
    check!(r#"Symbol layout"#, (std::mem::size_of::<Symbol>(), std::mem::size_of::<Option<Symbol>>() >= 4), (4, true));
}

#[test]
fn symbols_are_copy_and_hashable() {
    check!(r#"use as map keys"#, { let mut i = Interner::new(); let a = i.intern("k"); let mut m = std::collections::HashMap::new(); m.insert(a, 1); let b = a; (m[&b], m.contains_key(&i.intern("k"))) }, (1, true));
}

#[test]
fn owned_and_borrowed_inputs() {
    check!(r#"a String's slice and a literal"#, { let mut i = Interner::new(); let owned = String::from("same"); let a = i.intern(&owned); drop(owned); let b = i.intern("same"); (a == b, i.resolve(a).to_string()) }, (true, "same".to_string()));
}

#[test]
fn random_vs_model() {
    use std::collections::HashMap;
    let mut rng = anneal_prelude::Rng::new(6501);
    let mut i = Interner::new();
    let mut model: HashMap<String, u32> = HashMap::new();
    for _ in 0..30_000 {
        let len = rng.below(6) as usize;
        let s: String = (0..len).map(|_| (b'a' + rng.below(4) as u8) as char).collect();
        let want = {
            let n = model.len() as u32;
            *model.entry(s.clone()).or_insert(n)
        };
        let got = i.intern(&s);
        check!("symbol equals the first-seen index of the string", got.0, want);
        check!("resolve round-trips", i.resolve(got).to_string(), s);
    }
    check!("len equals the number of distinct strings", i.len(), model.len());
}

#[test]
fn many_long_strings_are_stored_once() {
    let mut i = Interner::new();
    let words: Vec<String> = (0..5_000).map(|n| format!("identifier_number_{n:08}")).collect();
    let first: Vec<Symbol> = words.iter().map(|w| i.intern(w)).collect();
    let second: Vec<Symbol> = words.iter().map(|w| i.intern(w)).collect();
    check!("second pass hits every time", first == second, true);
    check!("len after two passes", i.len(), 5_000);
}
