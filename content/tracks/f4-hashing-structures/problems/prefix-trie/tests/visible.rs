use solution::*;

fn of(words: &[&str]) -> Dictionary {
    let mut d = Dictionary::new();
    for w in words {
        d.insert(w);
    }
    d
}

#[test]
fn core_words_not_prefixes() {
    let d = of(&["apple"]);
    check!("(apple, app, apples)", (d.contains("apple"), d.contains("app"), d.contains("apples")), (true, false, false));
}

#[test]
fn core_insert_and_remove_report_changes() {
    let mut d = Dictionary::new();
    check!("insert twice", (d.insert("tea"), d.insert("tea"), d.len()), (true, false, 1));
    check!("remove twice", (d.remove("tea"), d.remove("tea"), d.len()), (true, false, 0));
}

#[test]
fn core_count_by_prefix() {
    let d = of(&["tea", "ten", "to", "inn"]);
    check!("(\"\", t, te, tea, x)", ["", "t", "te", "tea", "x"].map(|p| d.count_prefix(p)), [4, 3, 2, 1, 0]);
}

#[test]
fn part2_completions_in_order() {
    let d = of(&["car", "cat", "cart", "dog", "ca"]);
    check!("complete(ca, 3)", d.complete("ca", 3), vec!["ca", "car", "cart"]);
    check!("complete(ca, 10)", d.complete("ca", 10), vec!["ca", "car", "cart", "cat"]);
    check!("complete(z, 10)", d.complete("z", 10), Vec::<String>::new());
}

#[test]
fn part3_longest_beginning() {
    let d = of(&["/api", "/api/users"]);
    check!("deeper path", d.longest_stored_prefix("/api/users/7"), Some("/api/users"));
    check!("shorter", d.longest_stored_prefix("/api/user"), Some("/api"));
    check!("none", d.longest_stored_prefix("/ap"), None);
}

#[test]
fn part4_prefix_count_after_removal() {
    let mut d = of(&["to", "tea"]);
    check!("both", d.prefix_count(), 4);
    d.remove("tea");
    check!("without tea", d.prefix_count(), 2);
}
