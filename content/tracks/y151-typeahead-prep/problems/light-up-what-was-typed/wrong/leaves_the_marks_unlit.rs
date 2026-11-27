/// What a lowercase letter is replaced with in a key. `None` means: keep it. This table is deliberately small.
pub fn plain(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' | 'ń' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'ł' => "l",
        'š' | 'ś' => "s",
        'ž' | 'ź' | 'ż' => "z",
        _ => return None,
    })
}

fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// Adds one character of the text to a key that may still end in a space.
fn push(key: &mut String, c: char) {
    if c.is_whitespace() {
        if !key.is_empty() && !key.ends_with(' ') {
            key.push(' ');
        }
    } else if !c.is_control() {
        for low in c.to_lowercase().filter(|&l| !is_mark(l)) {
            match plain(low) {
                Some(letters) => key.push_str(letters),
                None => key.push(low),
            }
        }
    }
}

/// The form of `text` that is used for matching.
pub fn search_key(text: &str) -> String {
    let mut key = String::new();
    for c in text.chars() {
        push(&mut key, c);
    }
    if key.ends_with(' ') {
        key.pop();
    }
    key
}

/// How many bytes at the start of `shown` to light up for `typed`.
pub fn typed_end(shown: &str, typed: &str) -> Option<usize> {
    let want = search_key(typed);
    if !search_key(shown).starts_with(&want) {
        return None;
    }
    if want.is_empty() {
        return Some(0);
    }
    // the key of a start of `shown` is what has been pushed so far, without a trailing space
    let mut key = String::new();
    let mut chars = shown.char_indices();
    for (at, c) in chars.by_ref() {
        push(&mut key, c);
        if key.trim_end_matches(' ').len() >= want.len() {
            let mut end = at + c.len_utf8();
            end += shown[end..]
                .chars()
                .take_while(|_| false)
                .map(char::len_utf8)
                .sum::<usize>();
            return Some(end);
        }
    }
    None
}
