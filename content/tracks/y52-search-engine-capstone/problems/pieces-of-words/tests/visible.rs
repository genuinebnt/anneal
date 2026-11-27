use solution::*;

fn index(batches: &[&[&str]]) -> Index {
    let mut index = Index::new();
    for batch in batches {
        index.add_batch(batch);
    }
    index
}

fn hits(candidates: usize, pages: &[u32]) -> PatternHits {
    PatternHits {
        candidates,
        pages: pages.to_vec(),
    }
}

#[test]
fn inside_words_and_in_any_case() {
    let i = index(&[&["Broadcast news", "weather forecast"], &["TYPECAST", "cats"]]);
    check!("*cast*", i.find_pattern("*cast*"), hits(3, &[0, 1, 2]));
    check!("CAST", i.find_pattern("CAST"), hits(3, &[0, 1, 2]));
}

#[test]
fn pieces_in_order() {
    let i = index(&[&["memory ordering", "ordering memory"]]);
    check!("mem*ring: both have the windows", i.find_pattern("mem*ring"), hits(2, &[0]));
}

#[test]
fn windows_can_be_there_without_the_piece() {
    // abc and bcd are both in "abc xbcd", but "abcd" is not
    let i = index(&[&["abc xbcd", "abcd"]]);
    check!("abcd", i.find_pattern("abcd"), hits(2, &[1]));
}

#[test]
fn short_pieces_make_every_page_a_candidate() {
    let i = index(&[&["ab", "xy"], &["cab"]]);
    check!("*ab*", i.find_pattern("*ab*"), hits(3, &[0, 2]));
    check!("**", i.find_pattern("**"), hits(3, &[0, 1, 2]));
}

#[test]
fn across_words_and_nothing_found() {
    let i = index(&[&["end. Start"]]);
    check!("d. st", i.find_pattern("d. st"), hits(1, &[0]));
    check!("zzz", i.find_pattern("zzz"), hits(0, &[]));
}
