use solution::*;

fn index(docs: &[(u32, &str)]) -> Index {
    let mut i = Index::new();
    for (id, t) in docs {
        i.add(*id, t);
    }
    i
}

#[test]
fn an_index_built_from_a_list() {
    let i = index(&[(4, "needle in a haystack")]);
    check!("found", i.search("needle"), vec![4]);
}

#[test]
fn finds_substrings() {
    check!(r#"not just words"#, { let i = index(&[(1, "hello world"), (2, "yellow"), (3, "nothing")]); (i.search("llo"), i.search("low"), i.search("world")) }, (vec![1, 2], vec![2], vec![1]));
}

#[test]
fn results_are_sorted() {
    check!(r#"ids ascending"#, { let i = index(&[(9, "foo bar"), (2, "bar foo"), (5, "foobar")]); i.search("foo") }, vec![2, 5, 9]);
}

#[test]
fn trigrams_present_is_not_enough() {
    check!(r#"abc xbcd contains abc and bcd, not abcd"#, { let i = index(&[(1, "abc xbcd"), (2, "abcd")]); (i.candidates("abcd"), i.search("abcd")) }, (Some(vec![1, 2]), vec![2]));
}

#[test]
fn short_queries_scan() {
    check!(r#"1 and 2 chars"#, { let i = index(&[(1, "ab"), (2, "xyz"), (3, "a")]); (i.candidates("ab"), i.search("ab"), i.search("a"), i.search("")) }, (None, vec![1], vec![1, 3], vec![1, 2, 3]));
}

#[test]
fn case_sensitive() {
    check!(r#"A is not a"#, { let i = index(&[(1, "Hello")]); (i.search("Hello"), i.search("hello")) }, (vec![1], vec![]));
}

#[test]
fn replacing_a_document() {
    check!(r#"old trigrams stop matching"#, { let mut i = index(&[(1, "alpha beta")]); i.add(1, "gamma delta"); (i.search("alpha"), i.candidates("alpha"), i.search("delta")) }, (vec![], Some(vec![]), vec![1]));
}
