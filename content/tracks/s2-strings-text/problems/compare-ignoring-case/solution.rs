pub fn is_command(input: &str, command: &str) -> bool {
    input.trim().eq_ignore_ascii_case(command)
}

pub fn count_word(text: &str, word: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.eq_ignore_ascii_case(word))
        .count()
}
