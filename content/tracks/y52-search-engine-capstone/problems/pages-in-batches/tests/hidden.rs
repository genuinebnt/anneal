use solution::*;

fn index(batches: &[&[&str]]) -> Index {
    let mut index = Index::new();
    for batch in batches {
        index.add_batch(batch);
    }
    index
}

#[test]
fn pages_without_words_still_count() {
    let i = index(&[&["", "..."], &["a"], &[""]]);
    check!("counts", (i.len(), i.batches()), (4, 3));
    check!("lengths", (i.page_len(1), i.page_len(2), i.page_len(3)), (Some(0), Some(1), Some(0)));
    check!("a", i.phrase(&["a"]), vec![2]);
}

#[test]
fn a_phrase_stays_on_one_page() {
    let i = index(&[&["one two", "three four"]]);
    check!("two three", i.phrase(&["two", "three"]), Vec::<u32>::new());
}

#[test]
fn a_repeated_word_in_a_phrase() {
    let i = index(&[&["ha ha ha", "ha", "ha ho ha"]]);
    check!("ha ha", i.phrase(&["ha", "ha"]), vec![0]);
    check!("ha ha ha ha", i.phrase(&["ha"; 4]), Vec::<u32>::new());
}

#[test]
fn only_a_later_start_fits() {
    let i = index(&[&["a b a c a b d"]]);
    check!("a b d", i.phrase(&["a", "b", "d"]), vec![0]);
    check!("a b c", i.phrase(&["a", "b", "c"]), Vec::<u32>::new());
}

#[test]
fn an_unknown_word_anywhere() {
    let i = index(&[&["a b"]]);
    check!("first, last", (i.phrase(&["z", "b"]), i.phrase(&["a", "z"])), (vec![], vec![]));
    check!("a huge page number", i.page_len(u32::MAX), None);
}

#[test]
fn packed_size_counts_each_batch_separately() {
    // batch 1: "a" on pages 0 and 1 -> 1+3+3 = 7 bytes; batch 2: "a" on page 2 -> 4 bytes, "b" on page 2 -> 4
    let i = index(&[&["a", "a"], &["a b"]]);
    check!("bytes", i.packed_bytes(), 15);
    let mut many = Index::new();
    for _ in 0..200 {
        many.add_batch(&["w"]);
    }
    // a page number of 128 or more takes two bytes
    check!("200 batches", many.packed_bytes(), 128 * 4 + 72 * 5);
}

fn words(rng: &mut anneal_prelude::Rng, most: usize) -> Vec<String> {
    let vocab = ["a", "b", "c", "Café", "D4"];
    (0..rng.below(most)).map(|_| rng.pick(&vocab).to_string()).collect()
}

#[test]
fn matches_reading_every_page() {
    let mut rng = anneal_prelude::Rng::new(5203);
    for _ in 0..150 {
        let mut index = Index::new();
        let mut pages: Vec<Vec<String>> = Vec::new();
        let mut bytes = 0;
        let mut batches = 0;
        for _ in 0..rng.below(5) {
            let batch: Vec<String> = (0..rng.below(4)).map(|_| words(&mut rng, 9).join(", ")).collect();
            let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
            index.add_batch(&refs);
            let first = pages.len();
            pages.extend(batch.iter().map(|p| tokenize(p)));
            batches += usize::from(!batch.is_empty());
            let mut seen: Vec<&String> = pages[first..].iter().flatten().collect();
            seen.sort();
            seen.dedup();
            for w in seen {
                let list: Vec<Posting> = (first..pages.len())
                    .filter_map(|p| {
                        let positions: Vec<u32> = (0..pages[p].len()).filter(|&i| pages[p][i] == *w).map(|i| i as u32).collect();
                        (!positions.is_empty()).then(|| Posting { page: p as u32, positions })
                    })
                    .collect();
                bytes += encode(&list).len();
            }
        }
        check!(format!("{pages:?}: counts"), (index.len() as usize, index.batches(), index.packed_bytes()), (pages.len(), batches, bytes));
        for (p, page) in pages.iter().enumerate() {
            check!(format!("{pages:?}: length of {p}"), index.page_len(p as u32), Some(page.len() as u32));
        }
        for _ in 0..10 {
            let ask: Vec<String> = words(&mut rng, 4).iter().map(|w| w.to_lowercase()).collect();
            let ask: Vec<&str> = ask.iter().map(String::as_str).collect();
            let want: Vec<u32> = (0..pages.len())
                .filter(|&p| !ask.is_empty() && pages[p].windows(ask.len().max(1)).any(|w| w.iter().map(String::as_str).eq(ask.iter().copied())))
                .map(|p| p as u32)
                .collect();
            check!(format!("{pages:?}: phrase {ask:?}"), index.phrase(&ask), want);
        }
        for w in ["a", "café", "d4", "missing"] {
            let got: Vec<(u32, usize)> = index.postings(w).iter().map(|p| (p.page, p.positions.len())).collect();
            let want: Vec<(u32, usize)> = (0..pages.len())
                .map(|p| (p as u32, pages[p].iter().filter(|x| *x == w).count()))
                .filter(|&(_, n)| n > 0)
                .collect();
            check!(format!("{pages:?}: postings {w}"), got, want);
        }
    }
}

#[test]
fn phrases_over_words_that_are_everywhere() {
    let mut index = Index::new();
    for batch in 0..4 {
        let pages: Vec<&str> = (0..300).map(|i| if (batch, i) == (3, 299) { "to the guide of rust" } else { "the guide to rust" }).collect();
        index.add_batch(&pages);
    }
    check!("the guide", index.phrase(&["the", "guide"]).len(), 1_200);
    check!("rust the", index.phrase(&["rust", "the"]), Vec::<u32>::new());
    check!("guide of rust", index.phrase(&["guide", "of", "rust"]), vec![1_199]);
}
