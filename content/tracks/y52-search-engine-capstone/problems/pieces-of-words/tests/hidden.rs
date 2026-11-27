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
fn pieces_may_not_share_bytes() {
    let i = index(&[&["ababa", "abaaba"]]);
    check!("aba*aba", i.find_pattern("aba*aba"), hits(2, &[1]));
}

#[test]
fn windows_are_bytes_not_characters() {
    // "é" is two bytes, so "éa" has a window and "é" alone has none
    let i = index(&[&["xéay", "é a", "plain"]]);
    check!("éa", i.find_pattern("éa"), hits(1, &[0]));
    check!("é", i.find_pattern("é"), hits(3, &[0, 1]));
    let j = index(&[&["🦀", "crab"]]);
    check!("one four-byte character", j.find_pattern("🦀"), hits(1, &[0]));
}

#[test]
fn a_capital_sigma_at_the_end_of_a_piece() {
    let i = index(&[&["ΟΔΟΣΤΡΩΜΑ", "ΟΔΟΣ"]]);
    check!("*ΟΔΟΣ*", i.find_pattern("*ΟΔΟΣ*").pages, vec![0, 1]);
    check!("στρ", i.find_pattern("στρ").pages, vec![0]);
    check!("*οδοσ*, typed in small letters", i.find_pattern("*οδοσ*").pages, vec![0, 1]);
}

#[test]
fn a_piece_used_twice() {
    let i = index(&[&["one fish", "fish and fish"]]);
    check!("fish*fish", i.find_pattern("fish*fish"), hits(2, &[1]));
}

#[test]
fn a_window_missing_from_one_batch_only() {
    let i = index(&[&["needle"], &["hay", "hay"], &["pins and needles"]]);
    check!("needle", i.find_pattern("needle"), hits(2, &[0, 3]));
    check!("empty index", Index::new().find_pattern("abc"), hits(0, &[]));
    check!("empty index, empty pattern", Index::new().find_pattern(""), hits(0, &[]));
}

#[test]
fn a_short_piece_beside_a_long_one() {
    let i = index(&[&["ab", "zq needle", "needle zq", "needle"]]);
    check!("zq*needle", i.find_pattern("zq*needle"), hits(3, &[1]));
}

#[test]
fn what_the_earlier_steps_did_still_works() {
    let i = index(&[&["memory ordering"], &["ordering"]]);
    check!("phrase", i.phrase(&["memory", "ordering"]), vec![0]);
    check!("postings", i.postings("ordering").len(), 2);
    // each word of each batch: 1 posting, page, 1 position, gap
    check!("packed bytes: only the word lists", i.packed_bytes(), 12);
}

#[test]
fn matches_reading_every_page() {
    let mut rng = anneal_prelude::Rng::new(5206);
    let alphabet = "abAB .é";
    for _ in 0..250 {
        let mut index = Index::new();
        let mut pages: Vec<String> = Vec::new();
        for _ in 0..rng.below(4) {
            let batch: Vec<String> = (0..rng.below(5))
                .map(|_| {
                    let len = rng.below(14);
                    rng.string(len, alphabet)
                })
                .collect();
            let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
            index.add_batch(&refs);
            pages.extend(batch);
        }
        for _ in 0..15 {
            let pattern = if rng.below(3) > 0 && !pages.is_empty() {
                // pieces cut from one page
                let page: Vec<char> = rng.pick(&pages).chars().collect();
                let mut cut = Vec::new();
                for _ in 0..1 + rng.below(3) {
                    let a = rng.below(page.len() + 1);
                    let b = (a + rng.below(6)).min(page.len());
                    cut.push(page[a..b].iter().collect::<String>());
                }
                cut.join("*")
            } else {
                let len = rng.below(7);
                rng.string(len, "abAé *")
            };
            let folded = fold(&pattern);
            let pieces: Vec<&str> = folded.split('*').filter(|p| !p.is_empty()).collect();
            let mut want = hits(0, &[]);
            for (n, page) in pages.iter().enumerate() {
                let text = fold(page);
                let candidate = pieces.iter().all(|p| p.as_bytes().windows(3).all(|w| text.as_bytes().windows(3).any(|x| x == w)));
                if !candidate {
                    continue;
                }
                want.candidates += 1;
                // reach[i]: the first i bytes of the pattern, with a star at both ends, fit what has been read
                let whole = format!("*{folded}*").into_bytes();
                let mut reach = vec![false; whole.len() + 1];
                reach[0] = true;
                for i in 0..whole.len() {
                    reach[i + 1] = reach[i] && whole[i] == b'*';
                }
                for byte in text.bytes() {
                    let mut next = vec![false; whole.len() + 1];
                    for (i, &p) in whole.iter().enumerate() {
                        next[i + 1] = if p == b'*' { next[i] || reach[i + 1] } else { p == byte && reach[i] };
                    }
                    reach = next;
                }
                if reach[whole.len()] {
                    want.pages.push(n as u32);
                }
            }
            check!(format!("pages {pages:?}, pattern {pattern:?}"), index.find_pattern(&pattern), want);
        }
    }
}

#[test]
fn many_pages_and_a_rare_pattern() {
    let mut index = Index::new();
    for batch in 0..4 {
        let pages: Vec<String> = (0..200)
            .map(|i| {
                let mut page = format!("an ordinary page, filed under general matters, {}", i % 10);
                if batch == 3 && i == 77 {
                    page.push_str(" quokka sighting");
                }
                page
            })
            .collect();
        let refs: Vec<&str> = pages.iter().map(String::as_str).collect();
        index.add_batch(&refs);
    }
    for pattern in ["*quokka*", "ordinary*quokka", "general*sighting"] {
        check!(pattern, index.find_pattern(pattern), hits(1, &[677]));
    }
    check!("unknown", index.find_pattern("*xylophone*"), hits(0, &[]));
}
