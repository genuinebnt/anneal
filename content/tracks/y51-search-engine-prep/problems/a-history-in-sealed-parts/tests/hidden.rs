use solution::*;

fn batch(n: usize) -> Vec<(u32, &'static str)> {
    vec![(1, "x"); n]
}

#[test]
fn exactly_twice_as_large_is_left_alone() {
    let mut h = History::new();
    h.append(&batch(6));
    h.append(&batch(3));
    check!("6 then 3", h.part_sizes(), vec![6, 3]);
    h.append(&batch(1));
    check!("then 1", h.part_sizes(), vec![6, 3, 1]);
}

#[test]
fn one_batch_can_set_off_several_joins() {
    let mut h = History::new();
    for n in [8, 4, 2] {
        h.append(&batch(n));
    }
    check!("8 4 2", h.part_sizes(), vec![8, 4, 2]);
    h.append(&batch(2));
    check!("one more batch of 2", h.part_sizes(), vec![16]);
}

#[test]
fn a_large_batch_joins_small_older_parts() {
    let mut h = History::new();
    h.append(&[(1, "old")]);
    h.append(&[(2, "new"), (1, "newer"), (2, "newest")]);
    check!("sizes", h.part_sizes(), vec![4]);
    check!("order is kept", (h.message(0), h.message(2)), (Some((1, "old")), Some((1, "newer"))));
    check!("by author 1", h.by_author(1), vec![0, 2]);
}

#[test]
fn lengths_are_counted_in_characters() {
    let mut h = History::new();
    h.append(&[(1, "é🦀"), (1, "")]);
    check!("mean of 2 and 0", h.mean_length(), Some(1.0));
}

#[test]
fn a_snapshot_can_go_its_own_way() {
    let mut h = History::new();
    h.append(&batch(2));
    let mut copy = h.snapshot();
    copy.append(&[(5, "only in the copy")]);
    copy.append(&batch(1));
    h.append(&[(6, "only in the original")]);
    check!("original", (h.len(), h.message(2)), (3, Some((6, "only in the original"))));
    check!("copy", (copy.len(), copy.message(2)), (4, Some((5, "only in the copy"))));
    check!("copy authors", copy.by_author(6), Vec::<usize>::new());
}

#[test]
fn matches_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(5104);
    let texts = ["", "a", "bb", "héllo", "a longer message"];
    for _ in 0..120 {
        let mut h = History::new();
        let mut all: Vec<(u32, &str)> = Vec::new();
        let mut sizes: Vec<usize> = Vec::new();
        let mut shots: Vec<(History, usize)> = Vec::new();
        for _ in 0..rng.below(30) {
            let n = if rng.below(8) == 0 { rng.below(20) } else { rng.below(4) };
            let next: Vec<(u32, &str)> = (0..n).map(|_| (rng.below(3) as u32, *rng.pick(&texts))).collect();
            h.append(&next);
            all.extend(&next);
            if n > 0 {
                sizes.push(n);
                while sizes.len() >= 2 && sizes[sizes.len() - 2] < 2 * sizes[sizes.len() - 1] {
                    let last = sizes.pop().unwrap_or(0);
                    if let Some(t) = sizes.last_mut() {
                        *t += last;
                    }
                }
            }
            check!(format!("sizes after {} messages", all.len()), h.part_sizes(), sizes.clone());
            if rng.below(4) == 0 {
                shots.push((h.snapshot(), all.len()));
            }
        }
        shots.push((h, all.len()));
        for (view, len) in &shots {
            let want = &all[..*len];
            check!("len", view.len(), want.len());
            for i in 0..=want.len() {
                check!(format!("message {i} of {want:?}"), view.message(i), want.get(i).copied());
            }
            for a in 0..3 {
                let w: Vec<usize> = (0..want.len()).filter(|&i| want[i].0 == a).collect();
                check!(format!("by_author {a} of {want:?}"), view.by_author(a), w);
            }
            let chars: usize = want.iter().map(|m| m.1.chars().count()).sum();
            let mean = (!want.is_empty()).then(|| chars as f64 / want.len() as f64);
            check!(format!("mean of {want:?}"), view.mean_length(), mean);
        }
    }
}

#[test]
fn a_long_history_with_a_snapshot_along_the_way() {
    let mut h = History::new();
    let mut shots = Vec::new();
    for i in 0..20_000u32 {
        h.append(&[(i % 5, "hello")]);
        if i % 1_000 == 999 {
            let (shot, used) = anneal_prelude::allocs(|| h.snapshot());
            check!(format!("bytes asked for by snapshot() at {} messages: {} (at most 4096)", i + 1, used.bytes), used.bytes <= 4_096, true);
            shots.push((shot, i as usize + 1));
            check!("part count stays small", h.part_sizes().len() <= 15, true);
            check!("middle message", h.message(i as usize / 2), Some(((i / 2) % 5, "hello")));
        }
    }
    for (shot, len) in &shots {
        check!("an old view", (shot.len(), shot.message(*len)), (*len, None));
    }
    check!("len and mean", (h.len(), h.mean_length()), (20_000, Some(5.0)));
}

#[test]
fn a_snapshot_of_nothing() {
    let mut h = History::new();
    let nothing = h.snapshot();
    h.append(&batch(3));
    check!("the old view", (nothing.len(), nothing.message(0), nothing.mean_length()), (0, None, None));
    check!("huge message number", h.message(usize::MAX), None);
}
