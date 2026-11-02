use solution::*;

fn index(docs: &[(u32, &str)]) -> Index {
    let mut i = Index::new();
    for (id, t) in docs {
        i.add(*id, t);
    }
    i
}

fn ids(r: &[(u32, f64)]) -> Vec<u32> {
    r.iter().map(|x| x.0).collect()
}


#[test]
fn an_index_built_from_a_list() {
    let i = index(&[(4, "needle in a haystack")]);
    check!("found", ids(&i.search("needle", 3)), vec![4]);
}

#[test]
fn tokenizing() {
    check!(r#"lowercase alphanumeric runs"#, tokenize("Hello, World! It's 3D-printing"), vec!["hello", "world", "it", "s", "3d", "printing"].into_iter().map(String::from).collect::<Vec<_>>());
}

#[test]
fn the_score_of_a_single_document() {
    check!(r#"N = 1, n = 1, len = avglen"#, { let i = index(&[(1, "rust")]); let r = i.search("rust", 5); (r.len(), r[0].0, (r[0].1 - 0.28768207245178096).abs() < 1e-9) }, (1, 1, true));
}

#[test]
fn more_occurrences_rank_higher() {
    check!(r#"tf = 3 beats tf = 1"#, { let i = index(&[(1, "rust is fast"), (2, "rust rust rust everywhere"), (3, "python is slow")]); ids(&i.search("rust", 10)) }, vec![2, 1]);
}

#[test]
fn only_matching_documents() {
    check!(r#"no zero scores"#, { let i = index(&[(1, "alpha"), (2, "beta"), (3, "alpha beta")]); ids(&i.search("alpha", 10)) }, vec![1, 3]);
}

#[test]
fn k_limits_the_results() {
    check!(r#"top 1"#, { let i = index(&[(1, "a b"), (2, "a"), (3, "a c d")]); ids(&i.search("a", 1)) }, vec![2]);
}

#[test]
fn rare_terms_matter_more() {
    check!(r#"idf: the rare word wins"#, { let i = index(&[(1, "the common word"), (2, "the rare zebra"), (3, "the common thing"), (4, "the common stuff")]); ids(&i.search("common zebra", 4))[0] }, 2);
}
