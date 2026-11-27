use solution::*;

const POOL: [char; 24] = [
    'a', 'e', 's', 'S', 'É', 'é', 'ß', 'æ', 'Ł', 'İ', ' ', ' ', '\t', '\n', '\u{1}', '\u{7f}', '\u{300}', '\u{301}',
    '\u{36f}', '\u{370}', '東', 'Σ', '\u{a0}', 'z',
];

fn random_text(rng: &mut anneal_prelude::Rng, max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(&POOL)).collect()
}

/// The statement, read literally: try every start of `shown`.
fn slow_end(shown: &str, typed: &str) -> Option<usize> {
    let want = search_key(typed);
    if !search_key(shown).starts_with(&want) {
        return None;
    }
    if want.is_empty() {
        return Some(0);
    }
    let cuts = shown.char_indices().map(|(i, _)| i).chain([shown.len()]);
    for mut end in cuts {
        if search_key(&shown[..end]).starts_with(&want) {
            for c in shown[end..].chars() {
                if !('\u{300}'..='\u{36f}').contains(&c) {
                    break;
                }
                end += c.len_utf8();
            }
            return Some(end);
        }
    }
    None
}

#[test]
fn a_dotted_capital_i_loses_its_dot() {
    check!("İ", search_key("İSTANBUL"), "istanbul".to_string());
    check!("lit", typed_end("İstanbul", "ist"), Some("İst".len()));
}

#[test]
fn many_characters_give_a_key_that_is_its_own_key() {
    let mut bad = None;
    // every character of the first blocks, then a spread over the rest
    let low = '\0'..'\u{3000}';
    let rest = ('\u{3000}'..=char::MAX).step_by(101);
    for c in low.chain(rest) {
        let once = search_key(&format!("a{c}b"));
        if search_key(&once) != once {
            bad = Some(c);
            break;
        }
    }
    check!("first character whose key changes again", bad, None);
}

#[test]
fn control_characters_vanish_and_whitespace_runs_join() {
    check!("control inside a word", search_key("a\u{1}b\u{0}"), "ab".to_string());
    check!("control between spaces", search_key("a \u{1} b"), "a b".to_string());
    check!("controls that are whitespace", search_key("a\r\n\u{b}b"), "a b".to_string());
    check!("only marks", search_key("\u{301}\u{302}"), String::new());
    check!("the first character after the marks stays", search_key("a\u{370}"), "a\u{371}".to_string());
}

#[test]
fn half_of_a_double_letter_lights_the_whole() {
    check!("s of ß", typed_end("ß!", "s"), Some(2));
    check!("stras", typed_end("Straße", "stras"), Some("Straß".len()));
    check!("a of æ", typed_end("Æble", "a"), Some(2));
}

#[test]
fn a_space_is_matched_only_when_a_word_follows() {
    check!("trailing space typed", typed_end("New York", "new "), Some(3));
    check!("space then letter", typed_end("New   York", "new y"), Some("New   Y".len()));
    check!("leading space shown", typed_end("  New", "n"), Some(3));
    check!("typed longer than shown", typed_end("New", "new y"), None);
}

#[test]
fn other_scripts_are_cut_between_characters() {
    check!("one character", typed_end("東京タワー", "東京"), Some("東京".len()));
    check!("capitals", typed_end("ΣΟΦΙΑ", "σοφ"), Some("ΣΟΦ".len()));
}

#[test]
fn agrees_with_trying_every_start() {
    let mut rng = anneal_prelude::Rng::new(15101);
    for _ in 0..3000 {
        let shown = random_text(&mut rng, 10);
        let typed = if rng.bool() {
            random_text(&mut rng, 4)
        } else {
            let n = rng.below(shown.chars().count() + 1);
            shown.chars().take(n).collect()
        };
        check!(format!("{shown:?} / {typed:?}"), typed_end(&shown, &typed), slow_end(&shown, &typed));
    }
}

#[test]
fn an_empty_name_matches_only_an_empty_text() {
    check!("both empty", typed_end("", ""), Some(0));
    check!("something typed", typed_end("", "a"), None);
    check!("only a mark typed", typed_end("abc", "\u{301}"), Some(0));
}
