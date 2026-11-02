use solution::*;

#[test]
fn too_big() {
    check!(r#"larger than the region"#, { let mut b = Buddy::new(3); (b.alloc(9), b.alloc(8), b.alloc(usize::MAX)) }, (None, Some(0), None));
}

#[test]
fn zero_counts_as_one() {
    check!(r#"alloc(0)"#, { let mut b = Buddy::new(2); (b.alloc(0), b.alloc(0), b.free_units()) }, (Some(0), Some(1), 2));
}

#[test]
fn partial_coalescing() {
    check!(r#"free only one of two buddies"#, { let mut b = Buddy::new(2); let a = b.alloc(1).unwrap(); let c = b.alloc(1).unwrap(); b.free(a, 1); (b.largest_free(), b.free_units(), c) }, (2, 3, 1));
}

#[test]
fn buddy_of_a_right_hand_block() {
    check!(r#"free the right half first, then the left"#, { let mut b = Buddy::new(2); let l = b.alloc(2).unwrap(); let r = b.alloc(2).unwrap(); b.free(r, 2); let mid = b.largest_free(); b.free(l, 2); (r, mid, b.largest_free()) }, (2, 2, 4));
}

#[test]
fn cascading_merge() {
    check!(r#"merge climbs several orders"#, { let mut b = Buddy::new(4); let v: Vec<usize> = (0..16).map(|_| b.alloc(1).unwrap()).collect(); for o in v { b.free(o, 1); } (b.largest_free(), b.alloc(16)) }, (16, Some(0)));
}

#[test]
fn fragmentation_is_visible() {
    check!(r#"free every other unit"#, { let mut b = Buddy::new(3); let v: Vec<usize> = (0..8).map(|_| b.alloc(1).unwrap()).collect(); for o in v.iter().step_by(2) { b.free(*o, 1); } (b.free_units(), b.largest_free(), b.alloc(2)) }, (4, 1, None));
}

#[test]
fn lowest_address_first() {
    check!(r#"two free blocks of the same order"#, { let mut b = Buddy::new(3); let v: Vec<usize> = (0..4).map(|_| b.alloc(2).unwrap()).collect(); b.free(v[2], 2); b.free(v[0], 2); (b.alloc(2), b.alloc(2), b.alloc(2)) }, (Some(0), Some(4), None));
}

#[test]
fn alignment() {
    check!(r#"an order-k block is k-aligned"#, { let mut b = Buddy::new(6); let a = b.alloc(1).unwrap(); let c = b.alloc(8).unwrap(); let d = b.alloc(4).unwrap(); let e = b.alloc(16).unwrap(); (a % 1, c % 8, d % 4, e % 16) }, (0, 0, 0, 0));
}

#[test]
fn random_vs_a_unit_bitmap() {
    let mut rng = anneal_prelude::Rng::new(6503);
    let order = 9u32;
    let total = 1usize << order;
    let mut b = Buddy::new(order);
    let mut used = vec![false; total];
    let mut live: Vec<(usize, usize)> = Vec::new(); // (offset, requested size)
    for _ in 0..20_000 {
        if live.is_empty() || rng.below(100) < 55 {
            let size = 1 + rng.below(40) as usize;
            let block = size.next_power_of_two();
            let got = b.alloc(size);
            // the model: a block exists iff some aligned run of `block` free units exists
            let exists = (0..total).step_by(block).any(|s| used[s..s + block].iter().all(|u| !u));
            check!("alloc succeeds iff an aligned free block of that size exists", got.is_some(), exists);
            if let Some(off) = got {
                check!("the block is aligned to its size", off % block, 0);
                check!("the block does not overlap live ones", used[off..off + block].iter().any(|u| *u), false);
                for u in &mut used[off..off + block] {
                    *u = true;
                }
                live.push((off, size));
            }
        } else {
            let (off, size) = live.swap_remove(rng.below(live.len()) as usize);
            let block = size.next_power_of_two();
            b.free(off, size);
            for u in &mut used[off..off + block] {
                *u = false;
            }
        }
        let free = used.iter().filter(|u| !**u).count();
        check!("free_units matches the bitmap", b.free_units(), free);
    }
    for (off, size) in live {
        b.free(off, size);
    }
    check!("everything freed coalesces into one block", (b.free_units(), b.largest_free()), (total, total));
}
