use solution::*;

const VOCAB: [&str; 6] = ["cast", "Broadcast", "memory", "order", "café", "pin"];

type Batches = Vec<Vec<String>>;

fn build(batches: &Batches) -> Index {
    let mut index = Index::new();
    for batch in batches {
        let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
        index.add_batch(&refs);
    }
    index
}

fn random_batch(rng: &mut anneal_prelude::Rng) -> Vec<String> {
    (0..1 + rng.below(4))
        .map(|_| {
            let n = rng.below(6);
            (0..n).map(|_| *rng.pick(&VOCAB)).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// Every answer the index can give that a merge must leave alone.
fn answers(i: &Index) -> String {
    let mut out = format!("{} pages;", i.len());
    for page in 0..=i.len() {
        out += &format!("{:?}", i.page_len(page));
    }
    for word in ["cast", "broadcast", "memory", "order", "café", "pin", "none"] {
        out += &format!("{:?}", i.postings(word));
    }
    out += &format!("{:?}{:?}", i.phrase(&["memory", "order"]), i.phrase(&["pin", "pin"]));
    for pattern in ["*cast*", "road*st", "ry or", "café", "in", "zzz", ""] {
        out += &format!("{:?}", i.find_pattern(pattern));
    }
    for query in ["memory OR cast", "pin NOT order", "\"memory order\" OR *adc*", "NOT NOT café", "NOT pin"] {
        // {:?} of an f64 is exact, so this compares every bit of every score
        out += &format!("{:?}{:?}", i.search(query, 4), i.search(query, 1_000));
    }
    out
}

#[test]
fn a_word_that_only_a_later_batch_has() {
    let mut i = build(&vec![vec!["a".into()], vec!["b".into()], vec!["late word".into()]]);
    i.merge_all();
    check!("late", (i.phrase(&["late", "word"]), i.find_pattern("late wo").pages), (vec![2], vec![2]));
}

#[test]
fn page_numbers_over_127_survive() {
    let batches: Batches = (0..300).map(|n| vec![format!("w{}", n % 3)]).collect();
    let mut i = build(&batches);
    let before = i.postings("w1");
    check!("merge the middle", i.merge(100, 150), true);
    check!("batches", i.batches(), 151);
    check!("postings", i.postings("w1") == before, true);
    i.merge_all();
    check!("postings after all", i.postings("w1") == before, true);
}

#[test]
fn the_last_two() {
    let mut i = build(&vec![vec!["a".into()], vec!["b".into()], vec!["c".into()]]);
    check!("merge(1, 2)", (i.merge(1, 2), i.batches()), (true, 2));
    check!("merge(1, 2) again", (i.merge(1, 2), i.batches()), (false, 2));
}

#[test]
fn pages_with_no_words() {
    let mut i = build(&vec![vec!["".into()], vec!["--".into()]]);
    i.merge_all();
    check!("lengths", (i.len(), i.page_len(0), i.page_len(1), i.packed_bytes()), (2, Some(0), Some(0), 0));
}

#[test]
fn packed_bytes_shrink_to_one_list_per_word() {
    let batches: Batches = (0..50).map(|_| vec!["same words again".to_string()]).collect();
    let mut i = build(&batches);
    check!("50 batches", i.packed_bytes(), 50 * 3 * 4);
    i.merge_all();
    // per word: 1 byte of count, then page, count and position for each of 50 pages
    check!("one batch", i.packed_bytes(), 3 * (1 + 50 * 3));
}

#[test]
fn random_histories_match_a_fresh_build_with_the_same_grouping() {
    let mut rng = anneal_prelude::Rng::new(5209);
    for _ in 0..60 {
        let mut groups: Batches = (0..rng.below(6)).map(|_| random_batch(&mut rng)).collect();
        let mut index = build(&groups);
        let mut log = format!("start {groups:?}");
        for _ in 0..8 {
            if rng.below(3) == 0 {
                let batch = random_batch(&mut rng);
                let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
                index.add_batch(&refs);
                log += &format!("; add {batch:?}");
                groups.push(batch);
            } else {
                let first = rng.below(groups.len() + 1);
                let count = rng.below(5);
                let fits = count >= 2 && first + count <= groups.len();
                let before = answers(&index);
                check!(format!("{log}; merge({first}, {count})"), index.merge(first, count), fits);
                log += &format!("; merge({first}, {count})");
                check!(format!("{log}: answers unchanged"), answers(&index) == before, true);
                if fits {
                    let joined: Vec<String> = groups[first..first + count].concat();
                    groups.splice(first..first + count, [joined]);
                }
            }
            let fresh = build(&groups);
            check!(format!("{log}: batches and bytes"), (index.batches(), index.packed_bytes()), (groups.len(), fresh.packed_bytes()));
            check!(format!("{log}: same answers as a fresh build"), answers(&index) == answers(&fresh), true);
        }
    }
}

#[test]
fn merge_all_equals_adding_everything_at_once() {
    let mut rng = anneal_prelude::Rng::new(5210);
    for _ in 0..40 {
        let groups: Batches = (0..rng.below(12)).map(|_| random_batch(&mut rng)).collect();
        let mut index = build(&groups);
        index.merge_all();
        let all: Vec<String> = groups.concat();
        let one = build(&vec![all.clone()]);
        check!(format!("{groups:?}"), (index.batches(), index.packed_bytes()), (one.batches(), one.packed_bytes()));
        check!(format!("{groups:?}: answers"), answers(&index) == answers(&one), true);
    }
}

#[test]
fn merging_a_thousand_batches_handles_each_page_a_few_times() {
    let page = "alpha beta gamma delta epsilon zeta";
    let mut index = Index::new();
    let mut text_bytes = 0;
    for n in 0..1_000 {
        let numbered = format!("{page} n{n}");
        index.add_batch(&[numbered.as_str(), page]);
        text_bytes += (numbered.len() + page.len()) as u64;
    }
    let (_, used) = anneal_prelude::allocs(|| index.merge_all());
    let each = used.bytes / text_bytes;
    check!(format!("bytes asked for by merge_all, per byte of page text: {each} (at most 2000)"), each <= 2_000, true);
    check!("one batch of 2000 pages", (index.batches(), index.len()), (1, 2_000));
    check!("a late page", index.phrase(&["zeta", "n999"]), vec![1_998]);
    check!("postings", index.postings("gamma").len(), 2_000);
}
