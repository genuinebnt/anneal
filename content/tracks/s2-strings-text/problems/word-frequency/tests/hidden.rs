use solution::*;

#[test]
fn invalid_utf8() {
    check!(r#"b"ok \xff""#, word_frequencies(&b"ok \xff"[..]).is_err(), true);
}

#[test]
fn punctuation_only() {
    check!(r#""-- !! ...""#, word_frequencies("-- !! ...".as_bytes()).unwrap().len(), 0);
}

#[test]
fn inner_punctuation_kept() {
    check!(r#""don't Don't""#, word_frequencies("don't Don't".as_bytes()).unwrap(), vec![("don't".to_string(), 2)]);
}

#[test]
fn unicode_lowercase() {
    check!(r#""École école ÉCOLE""#, word_frequencies("École école ÉCOLE".as_bytes()).unwrap(), vec![("école".to_string(), 3)]);
}

#[test]
fn unicode_punctuation() {
    check!(r#""«mot» ¡hola!""#, word_frequencies("«mot» ¡hola!".as_bytes()).unwrap(), vec![("hola".to_string(), 1), ("mot".to_string(), 1)]);
}

#[test]
fn digits_are_words() {
    check!(r#""42 42 x""#, word_frequencies("42 42 x".as_bytes()).unwrap(), vec![("42".to_string(), 2), ("x".to_string(), 1)]);
}

#[test]
fn crlf_and_blank_lines() {
    check!(r#""a\r\n\r\nb a\n""#, word_frequencies("a\r\n\r\nb a\n".as_bytes()).unwrap(), vec![("a".to_string(), 2), ("b".to_string(), 1)]);
}

#[test]
fn hyphen_inside_kept() {
    check!(r#""well-known -well- known""#, word_frequencies("well-known -well- known".as_bytes()).unwrap(), vec![("known".to_string(), 1), ("well".to_string(), 1), ("well-known".to_string(), 1)]);
}

#[test]
fn invalid_utf8_later_line() {
    check!(r#"b"fine\nbad \xc3\x28""#, word_frequencies(&b"fine\nbad \xc3\x28"[..]).is_err(), true);
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(2207);
    // Each piece and the word it counts as (None: it doesn't count).
    let pieces = [("Cat", Some("cat")), ("cat", Some("cat")), ("dog", Some("dog")), ("DOG!", Some("dog")), ("(cat)", Some("cat")),
                  ("--", None), ("bird.", Some("bird")), ("Émile", Some("émile")), ("b", Some("b"))];
    for _ in 0..300 {
        let n = rng.below(12);
        let mut text = String::new();
        let mut counts = std::collections::BTreeMap::new();
        for _ in 0..n {
            let (piece, word) = *rng.pick(&pieces);
            text.push_str(piece);
            text.push_str(*rng.pick(&[" ", "\n", "  "]));
            if let Some(w) = word {
                *counts.entry(w.to_string()).or_insert(0u32) += 1;
            }
        }
        let mut want: Vec<(String, u32)> = counts.into_iter().collect();
        want.sort_by(|a, b| b.1.cmp(&a.1));
        check!(format!("input = {text:?}"), word_frequencies(text.as_bytes()).unwrap(), want);
    }
}

#[test]
fn scale_50k_distinct() {
    let mut text = String::new();
    for _ in 0..4 {
        for i in 0..50_000 {
            text.push_str(&format!("w{i:05} "));
        }
        text.push('\n');
    }
    let out = word_frequencies(text.as_bytes()).unwrap();
    check!("w00000 … w49999, each 4 times (200000 words)", (out.len(), out[0].clone(), out[49_999].clone()), (50_000, ("w00000".to_string(), 4), ("w49999".to_string(), 4)));
}

#[test]
fn tab_separates_words() {
    check!(r#""a\tb a""#, word_frequencies("a\tb a".as_bytes()).unwrap(), vec![("a".to_string(), 2), ("b".to_string(), 1)]);
}
