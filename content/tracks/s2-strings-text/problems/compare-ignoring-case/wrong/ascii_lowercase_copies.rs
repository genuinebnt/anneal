// Wrong: lowercases copies of both sides, which allocates on every comparison.
pub fn is_command(input: &str, command: &str) -> bool {
    input.trim().to_ascii_lowercase() == command.to_ascii_lowercase()
}

pub fn count_word(text: &str, word: &str) -> usize {
    let word = word.to_ascii_lowercase();
    text.split_whitespace()
        .filter(|w| w.to_ascii_lowercase() == word)
        .count()
}
