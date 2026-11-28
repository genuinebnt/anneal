use solution::*;

#[test]
fn prefix_is_not_equal() {
    check!(r#""quitter", "quit""#, is_command("quitter", "quit"), false);
}

#[test]
fn none() {
    check!(r#""", "a""#, count_word("", "a"), 0);
}

#[test]
fn empty_command() {
    check!(r#""   ", """#, is_command("   ", ""), true);
}

#[test]
fn command_not_trimmed() {
    check!(r#""QUIT", "quit ""#, is_command("QUIT", "quit "), false);
}

#[test]
fn inner_space_kept() {
    check!(r#""qu it", "quit""#, is_command("qu it", "quit"), false);
}

#[test]
fn ascii_only_command() {
    check!(r#""É", "é""#, is_command("É", "é"), false);
}

#[test]
fn ascii_only_count() {
    check!(r#""ÉCOLE école École", "école""#, count_word("ÉCOLE école École", "école"), 1);
}

#[test]
fn punctuation_counts() {
    check!(r#""the, the. the", "the""#, count_word("the, the. the", "the"), 1);
}

#[test]
fn empty_word() {
    check!(r#""a b", """#, count_word("a b", ""), 0);
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(2204);
    let words = ["ab", "AB", "aB", "ba", "a", "abc", "Ab"];
    for _ in 0..300 {
        let n = rng.below(6);
        let mut text = String::new();
        for _ in 0..n {
            text.push_str(*rng.pick(&words));
            text.push_str(*rng.pick(&[" ", "  ", "\n", "\t"]));
        }
        let word = *rng.pick(&words);
        let fold = |s: &str| -> Vec<u8> { s.bytes().map(|b| if b.is_ascii_uppercase() { b + 32 } else { b }).collect() };
        let want = text.split_whitespace().filter(|w| fold(w) == fold(word)).count();
        let padded = format!(" {word}\t");
        check!(format!("text = {text:?}, word = {word:?}"), (count_word(&text, word), is_command(&padded, &word.to_uppercase())), (want, true));
    }
}

#[test]
fn scale_200k_words() {
    let text = "Go gO go stop ".repeat(50_000);
    check!("text = \"Go gO go stop …\" (200000 words), word = \"GO\"", count_word(&text, "GO"), 150_000);
}

#[test]
fn no_allocations() {
    let (got, seen) = anneal_prelude::allocs(|| (is_command("  QUIT \n", "quit"), count_word("The cat saw the THE", "the")));
    check!(r#"is_command("  QUIT \n", "quit") and count_word("The cat saw the THE", "the"): (results, heap allocations)"#, (got, seen.count), ((true, 3), 0));
}
