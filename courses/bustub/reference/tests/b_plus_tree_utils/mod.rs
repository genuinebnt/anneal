//! Helpers shared by the B+ tree tests: BusTub's `b_plus_tree_utils.h` helpers that need only the tree's public API, and a checker
//! (`check_shape`) that verifies the rules every B+ tree keeps from outside, through the tree's observers (`depth`, `leaf_sizes`,
//! `leaf_tombstones`), a scan and the number of pages a lookup latches. Not part of any stage: nothing here for you to write.
#![allow(dead_code)]

use std::collections::BTreeSet;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::common::config::PageId;
use bustub::common::rid::Rid;
use bustub::storage::index::b_plus_tree::BPlusTree;
use bustub::storage::index::generic_key::{GenericComparator, GenericKey};

pub type Key = GenericKey<8>;
pub type Cmp = GenericComparator<8>;
pub type Tree<'a, const T: usize = 0> = BPlusTree<'a, Key, Rid, Cmp, T>;

pub fn index_key(key: i64) -> Key {
    let mut k = Key::default();
    k.set_from_integer(key);
    k
}

/// BusTub's tests make the rid out of the key: `rid.Set(key >> 32, key & 0xFFFFFFFF)`.
pub fn rid_of(key: i64) -> Rid {
    Rid::new(PageId((key >> 32) as i32), (key & 0xFFFF_FFFF) as u32)
}

pub fn new_tree<'a>(bpm: &'a BufferPoolManager, leaf_max_size: u32, internal_max_size: u32) -> Tree<'a> {
    new_tree_t::<0>(bpm, leaf_max_size, internal_max_size)
}

/// A tree whose leaves have a tombstone buffer of `T` keys (BusTub's `NumTombs`).
pub fn new_tree_t<'a, const T: usize>(bpm: &'a BufferPoolManager, leaf_max_size: u32, internal_max_size: u32) -> Tree<'a, T> {
    let header_page_id = bpm.new_page();
    BPlusTree::new("foo_pk", header_page_id, bpm, GenericComparator::<8>, leaf_max_size, internal_max_size)
}

/// Inserts `key` with the rid BusTub's tests use. Returns what the tree answered.
pub fn insert<const T: usize>(tree: &Tree<T>, key: i64) -> bool {
    tree.insert(&index_key(key), &rid_of(key))
}

pub fn remove<const T: usize>(tree: &Tree<T>, key: i64) {
    tree.remove(&index_key(key));
}

pub fn get<const T: usize>(tree: &Tree<T>, key: i64) -> Vec<Rid> {
    tree.get_value(&index_key(key))
}

/// All keys in order, by following the iterator.
pub fn keys_by_scan<const T: usize>(tree: &Tree<T>) -> Vec<i64> {
    tree.begin().map(|(k, _)| k.get_as_integer()).collect()
}

/// BusTub's `TreeValuesMatch`: every inserted key has exactly one value, every deleted key has none.
pub fn tree_values_match<const T: usize>(tree: &Tree<T>, inserted: &[i64], deleted: &[i64]) -> bool {
    inserted.iter().all(|&k| get(tree, k).len() == 1) && deleted.iter().all(|&k| get(tree, k).is_empty())
}

/// The first key of each leaf, left to right, for a tree without tombstones: leaves hold consecutive runs of the sorted keys.
pub fn leaf_first_keys(tree: &Tree<0>) -> Vec<i64> {
    let keys = keys_by_scan(tree);
    let mut at = 0;
    let mut firsts = Vec::new();
    for size in tree.leaf_sizes() {
        firsts.push(keys[at]);
        at += size;
    }
    firsts
}

/// How many leaves a tree with `leaves` leaves and these maxima may have at most `levels` internal levels above them, and the fewest.
fn leaf_range(internal_max: u32, levels: u32) -> (u128, u128) {
    let min_fanout = internal_max.div_ceil(2) as u128;
    let fewest = if levels == 0 { 1 } else { 2 * min_fanout.pow(levels - 1) };
    let most = (internal_max as u128).pow(levels);
    (fewest, most)
}

/// Checks, from outside, every rule a B+ tree without tombstones keeps (Err names the first broken one):
/// - the leaves hold exactly the live keys: their sizes add up to the number of keys;
/// - a leaf at rest holds at most `leaf_max - 1` pairs, and every leaf of a tree with more than one leaf holds at least `leaf_max / 2`;
/// - the depth fits the number of leaves for the fan-out limits: between `2 * ceil(max/2)^(h-1)` and `max^h` leaves under `h` internal levels;
/// - **every leaf is at the same depth**: every lookup latches the header and exactly one page per level (`depth() + 1` read latches);
/// - with `with_scan`, a scan returns the keys in strictly increasing order and they are exactly the live keys.
pub fn check_shape(tree: &Tree<0>, live: &BTreeSet<i64>, leaf_max: u32, internal_max: u32, with_scan: bool) -> Result<(), String> {
    let sizes = tree.leaf_sizes();
    let depth = tree.depth();
    if live.is_empty() {
        return if depth == 0 && sizes.is_empty() && tree.is_empty() && !tree.get_root_page_id().is_valid() {
            Ok(())
        } else {
            Err(format!("an empty tree must have depth 0, no leaves and an invalid root; got depth {depth}, leaves {sizes:?}"))
        };
    }
    if tree.is_empty() || depth == 0 {
        return Err(format!("{} keys but the tree says it is empty (depth {depth})", live.len()));
    }
    if sizes.iter().sum::<usize>() != live.len() {
        return Err(format!("the leaves hold {:?} = {} pairs but {} keys were inserted", sizes, sizes.iter().sum::<usize>(), live.len()));
    }
    if let Some(too_big) = sizes.iter().find(|&&s| s as u32 >= leaf_max) {
        return Err(format!("a leaf holds {too_big} pairs; at rest a leaf holds at most leaf_max_size - 1 = {}", leaf_max - 1));
    }
    if sizes.len() > 1 {
        if let Some(too_small) = sizes.iter().find(|&&s| (s as u32) < leaf_max / 2) {
            return Err(format!("a leaf holds {too_small} pairs, fewer than the minimum {} (leaf sizes {sizes:?})", leaf_max / 2));
        }
    }
    let leaves = sizes.len() as u128;
    let internal_levels = depth as u32 - 1;
    let (fewest, most) = leaf_range(internal_max, internal_levels);
    if leaves < fewest || leaves > most {
        return Err(format!("{leaves} leaves under {internal_levels} internal levels with at most {internal_max} children each: that needs {fewest} to {most} leaves"));
    }
    // equal depth: every lookup latches the header page and one page per level
    for &k in live.iter().step_by((live.len() / 40).max(1)) {
        let before = tree.bpm.get_reads();
        let found = get(tree, k);
        let latched = tree.bpm.get_reads() - before;
        if found.len() != 1 {
            return Err(format!("key {k} has {} values", found.len()));
        }
        if latched != depth + 1 {
            return Err(format!("a lookup of key {k} latched {latched} pages; with depth {depth} every lookup should latch {} (header plus one page per level): leaves are not all at one depth", depth + 1));
        }
    }
    if with_scan {
        let scanned = keys_by_scan(tree);
        if !scanned.windows(2).all(|w| w[0] < w[1]) {
            return Err("a scan did not return strictly increasing keys".into());
        }
        if scanned != live.iter().copied().collect::<Vec<_>>() {
            return Err(format!("a scan returned {} keys, not the {} live ones", scanned.len(), live.len()));
        }
    }
    Ok(())
}

/// `check_shape` for a tree whose leaves have a tombstone buffer of `T` keys. A leaf holds its live pairs and its tombstoned pairs, both
/// counted in its size. Checks: every leaf's keys are strictly increasing; each leaf's tombstone buffer has at most `T` keys, no repeats,
/// every one of them physically in that leaf; the live keys are exactly the physical keys minus the tombstoned ones (and a scan, with
/// `with_scan`, returns just those, in order); leaf sizes and depth obey the same limits as without tombstones; every lookup latches
/// `depth() + 1` pages.
pub fn check_shape_t<const T: usize>(
    tree: &Tree<T>,
    live: &BTreeSet<i64>,
    leaf_max: u32,
    internal_max: u32,
    with_scan: bool,
) -> Result<(), String> {
    let keys: Vec<Vec<i64>> = tree.leaf_keys().into_iter().map(|l| l.iter().map(|k| k.get_as_integer()).collect()).collect();
    let tombs: Vec<Vec<i64>> = tree.leaf_tombstones().into_iter().map(|l| l.iter().map(|k| k.get_as_integer()).collect()).collect();
    let depth = tree.depth();
    if keys.len() != tombs.len() || keys.len() != tree.leaf_sizes().len() {
        return Err("the observers disagree about how many leaves there are".into());
    }
    let mut physical_live = BTreeSet::new();
    let mut tombstoned = 0;
    for (i, (ks, ts)) in keys.iter().zip(&tombs).enumerate() {
        if !ks.windows(2).all(|w| w[0] < w[1]) {
            return Err(format!("leaf {i}: keys {ks:?} are not strictly increasing"));
        }
        if ts.len() > T {
            return Err(format!("leaf {i}: {} tombstones but the buffer holds {T}", ts.len()));
        }
        let distinct: BTreeSet<_> = ts.iter().collect();
        if distinct.len() != ts.len() {
            return Err(format!("leaf {i}: a key is tombstoned twice: {ts:?}"));
        }
        if let Some(t) = ts.iter().find(|t| !ks.contains(t)) {
            return Err(format!("leaf {i}: tombstone {t} is not a key of that leaf ({ks:?})"));
        }
        if ks.len() as u32 >= leaf_max {
            return Err(format!("leaf {i} holds {} pairs; at rest at most {}", ks.len(), leaf_max - 1));
        }
        if keys.len() > 1 && (ks.len() as u32) < leaf_max / 2 {
            return Err(format!("leaf {i} holds {} pairs, fewer than the minimum {}", ks.len(), leaf_max / 2));
        }
        tombstoned += ts.len();
        physical_live.extend(ks.iter().filter(|k| !ts.contains(k)).copied());
    }
    if &physical_live != live {
        return Err(format!(
            "the leaves hold {} live keys but {} were expected (tombstones in the leaves: {tombstoned})",
            physical_live.len(),
            live.len()
        ));
    }
    if live.is_empty() && tombstoned == 0 {
        return if depth == 0 { Ok(()) } else { Err(format!("nothing is stored but the depth is {depth}")) };
    }
    if depth == 0 {
        return Err("keys are stored but the tree says depth 0".into());
    }
    let (fewest, most) = leaf_range(internal_max, depth as u32 - 1);
    if (keys.len() as u128) < fewest || (keys.len() as u128) > most {
        return Err(format!(
            "{} leaves under {} internal levels with at most {internal_max} children each: that needs {fewest} to {most} leaves",
            keys.len(),
            depth - 1
        ));
    }
    for &k in live.iter().step_by((live.len() / 40).max(1)) {
        let before = tree.bpm.get_reads();
        let found = get(tree, k);
        let latched = tree.bpm.get_reads() - before;
        if found.len() != 1 {
            return Err(format!("live key {k} has {} values", found.len()));
        }
        if latched != depth + 1 {
            return Err(format!("a lookup of key {k} latched {latched} pages; with depth {depth} it should latch {}", depth + 1));
        }
    }
    if with_scan {
        let scanned = keys_by_scan(tree);
        if scanned != live.iter().copied().collect::<Vec<_>>() {
            return Err(format!("a scan returned {} keys, not the {} live ones", scanned.len(), live.len()));
        }
    }
    Ok(())
}

/// The leaves of a tree as text, left to right: `[0,1][2,3~2,3][4,5~4]` is three leaves; the keys before the `~` are the pairs stored
/// in the leaf, the keys after it its tombstone buffer, oldest first.
pub fn leaves_t<const T: usize>(tree: &Tree<T>) -> String {
    let keys = tree.leaf_keys();
    let tombs = tree.leaf_tombstones();
    keys.iter()
        .zip(&tombs)
        .map(|(ks, ts)| {
            let k: Vec<String> = ks.iter().map(|k| k.get_as_integer().to_string()).collect();
            let t: Vec<String> = ts.iter().map(|k| k.get_as_integer().to_string()).collect();
            if t.is_empty() {
                format!("[{}]", k.join(","))
            } else {
                format!("[{}~{}]", k.join(","), t.join(","))
            }
        })
        .collect()
}
