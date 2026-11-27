const EVERY_NAME: &str = "every name";

pub fn keys(name: &str) -> Vec<String> {
    let first: String = name.chars().take(1).collect();
    vec![format!("first {first}"), EVERY_NAME.to_string()]
}

pub fn near(
    typed: &str,
    slips: usize,
    list: &mut dyn FnMut(&str) -> Vec<usize>,
    compare: &mut dyn FnMut(usize) -> Option<usize>,
) -> Vec<(usize, usize)> {
    let first: String = typed.chars().take(1).collect();
    let candidates = if slips == 0 {
        list(&format!("first {first}"))
    } else {
        list(EVERY_NAME)
    };
    let mut close: Vec<(usize, usize)> = candidates
        .into_iter()
        .filter_map(|n| compare(n).map(|d| (n, d)))
        .collect();
    close.sort_by_key(|&(n, d)| (d, n));
    close
}
