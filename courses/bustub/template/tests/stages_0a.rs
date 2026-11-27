//! Tests for module 0a: a persistent trie and a thread-safe store over it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use std::collections::BTreeMap;

use proptest::prelude::*;

use bustub::primer::trie::{Trie, TrieNode};
use bustub::primer::trie_store::TrieStore;

fn t() -> Trie {
    Trie::new()
}

// ---- 0a-01: get ---------------------------------------------------------------------------------------------------------------------------
//
// Stage 1 comes before `put`, so these tests build their tries by hand from the (given) public fields of `TrieNode`.

fn value_node<T: std::any::Any + Send + Sync>(children: Vec<(char, Arc<TrieNode>)>, value: T) -> Arc<TrieNode> {
    Arc::new(TrieNode { children: children.into_iter().collect::<BTreeMap<_, _>>(), value: Some(Arc::new(value)) })
}

fn plain_node(children: Vec<(char, Arc<TrieNode>)>) -> Arc<TrieNode> {
    Arc::new(TrieNode { children: children.into_iter().collect::<BTreeMap<_, _>>(), value: None })
}

/// A trie with the single key "test" holding `233u32`: root, t, e, s, t(value).
fn test_trie() -> Trie {
    let leaf = value_node(vec![], 233u32);
    let s = plain_node(vec![('t', leaf)]);
    let e = plain_node(vec![('s', s)]);
    let t = plain_node(vec![('e', e)]);
    Trie::from_root(Some(plain_node(vec![('t', t)])))
}

#[test]
fn s0a_01_an_empty_trie_has_no_values() {
    let trie = t();
    assert!(trie.get::<u32>("").is_none(), "an empty trie has no values: expected `trie.get::<u32>(\"\").is_none()`");
    assert!(trie.get::<u32>("anything").is_none(), "an empty trie has no values: expected `trie.get::<u32>(\"anything\").is_none()`");
    assert!(trie.root().is_none(), "an empty trie has no values: expected `trie.root().is_none()`");
}

#[test]
fn s0a_01_get_finds_the_value_at_the_end_of_the_key() {
    assert_eq!(test_trie().get::<u32>("test"), Some(&233), "get finds the value at the end of the key");
}

#[test]
fn s0a_01_a_prefix_that_holds_no_value_is_not_found() {
    let trie = test_trie();
    assert!(trie.get::<u32>("te").is_none(), "te only leads to test");
    assert!(trie.get::<u32>("tests").is_none(), "the key is longer than any stored");
    assert!(trie.get::<u32>("").is_none(), "a prefix that holds no value is not found: expected `trie.get::<u32>(\"\").is_none()`");
    assert!(trie.get::<u32>("tx").is_none(), "a prefix that holds no value is not found: expected `trie.get::<u32>(\"tx\").is_none()`");
}

#[test]
fn s0a_01_the_requested_type_must_match_the_stored_one() {
    let trie = test_trie();
    assert!(trie.get::<String>("test").is_none(), "the requested type must match the stored one: expected `trie.get::<String>(\"test\").is_none()`");
    assert!(trie.get::<u64>("test").is_none(), "the requested type must match the stored one: expected `trie.get::<u64>(\"test\").is_none()`");
    assert_eq!(trie.get::<u32>("test"), Some(&233), "the requested type must match the stored one");
}

#[test]
fn s0a_01_the_empty_key_lives_in_the_root() {
    let trie = Trie::from_root(Some(value_node(vec![('a', value_node(vec![], 1u32))], String::from("empty-key"))));
    assert_eq!(trie.get::<String>(""), Some(&String::from("empty-key")), "the empty key lives in the root");
    assert_eq!(trie.get::<u32>("a"), Some(&1), "the empty key lives in the root");
}

#[test]
fn s0a_01_a_node_can_have_a_value_and_children_and_both_are_found() {
    let trie = Trie::from_root(Some(plain_node(vec![('a', value_node(vec![('b', value_node(vec![], 2u32))], 1u32))])));
    assert_eq!(trie.get::<u32>("a"), Some(&1), "a node can have a value and children and both are found");
    assert_eq!(trie.get::<u32>("ab"), Some(&2), "a node can have a value and children and both are found");
}

#[test]
fn s0a_01_get_shared_gives_an_owner_of_the_value() {
    let trie = Trie::from_root(Some(plain_node(vec![('k', value_node(vec![], String::from("v")))])));
    let shared = trie.get_shared::<String>("k").unwrap();
    drop(trie);
    assert_eq!(*shared, "v", "the value outlives the trie it came from");
    assert!(test_trie().get_shared::<String>("test").is_none(), "get shared gives an owner of the value: expected `test_trie().get_shared::<String>(\"test\").is_none()`");
}

// ---- 0a-02: put -------------------------------------------------------------------------------------------------------------------------

#[test]
fn s0a_02_put_builds_one_node_per_character() {
    let trie = t().put("test", 233u32);
    let mut node = trie.root().unwrap();
    for c in ['t', 'e', 's', 't'] {
        assert_eq!(node.children.len(), 1, "put builds one node per character");
        assert!(!node.is_value_node(), "put builds one node per character: expected `!node.is_value_node()`");
        node = node.children.get(&c).unwrap();
    }
    assert!(node.children.is_empty(), "put builds one node per character: expected `node.children.is_empty()`");
    assert!(node.is_value_node(), "put builds one node per character: expected `node.is_value_node()`");
}

#[test]
fn s0a_02_put_replaces_a_value_even_with_another_type() {
    let trie = t().put("test", 233u32).put("test", 23333333u32);
    assert_eq!(trie.get::<u32>("test"), Some(&23333333), "put replaces a value even with another type");
    let trie = trie.put("test", String::from("23333333"));
    assert_eq!(trie.get::<String>("test"), Some(&String::from("23333333")), "put replaces a value even with another type");
    assert!(trie.get::<u32>("test").is_none(), "put replaces a value even with another type: expected `trie.get::<u32>(\"test\").is_none()`");
}

#[test]
fn s0a_02_keys_that_are_prefixes_of_each_other_share_a_path() {
    let trie = t().put("111", 111u32).put("11", 11u32).put("1111", 1111u32).put("11", 22u32);
    assert_eq!(trie.get::<u32>("11"), Some(&22), "keys that are prefixes of each other share a path");
    assert_eq!(trie.get::<u32>("111"), Some(&111), "keys that are prefixes of each other share a path");
    assert_eq!(trie.get::<u32>("1111"), Some(&1111), "keys that are prefixes of each other share a path");
    assert!(trie.get::<u32>("1").is_none(), "keys that are prefixes of each other share a path: expected `trie.get::<u32>(\"1\").is_none()`");
}

#[test]
fn s0a_02_putting_never_changes_the_old_trie() {
    let one = t().put("test", 2333u32);
    let two = one.put("te", 23u32);
    let three = two.put("tes", 233u32);
    assert!(one.get::<u32>("te").is_none(), "putting never changes the old trie: expected `one.get::<u32>(\"te\").is_none()`");
    assert!(two.get::<u32>("tes").is_none(), "putting never changes the old trie: expected `two.get::<u32>(\"tes\").is_none()`");
    assert_eq!(three.get::<u32>("te"), Some(&23), "putting never changes the old trie");
    assert_eq!(three.get::<u32>("tes"), Some(&233), "putting never changes the old trie");
    assert_eq!(three.get::<u32>("test"), Some(&2333), "putting never changes the old trie");
}

#[test]
fn s0a_02_overwriting_keeps_the_other_versions_values() {
    let base = t().put("test", 2333u32).put("te", 23u32).put("tes", 233u32);
    let a = base.put("te", String::from("23"));
    let b = base.put("tes", String::from("233"));
    let c = base.put("test", String::from("2333"));
    assert_eq!(base.get::<u32>("te"), Some(&23), "overwriting keeps the other versions values");
    assert_eq!(a.get::<String>("te").map(String::as_str), Some("23"), "overwriting keeps the other versions values");
    assert_eq!(a.get::<u32>("tes"), Some(&233), "overwriting keeps the other versions values");
    assert_eq!(b.get::<String>("tes").map(String::as_str), Some("233"), "overwriting keeps the other versions values");
    assert_eq!(b.get::<u32>("test"), Some(&2333), "overwriting keeps the other versions values");
    assert_eq!(c.get::<String>("test").map(String::as_str), Some("2333"), "overwriting keeps the other versions values");
}

#[test]
fn s0a_02_only_the_path_is_copied_everything_else_is_shared() {
    let base = t().put("ab", 1u32).put("ac", 2u32).put("xy", 3u32);
    let next = base.put("ab", 10u32);
    let (old_root, new_root) = (base.root().unwrap(), next.root().unwrap());
    assert!(!Arc::ptr_eq(old_root, new_root), "the root is on the path");
    assert!(Arc::ptr_eq(&old_root.children[&'x'], &new_root.children[&'x']), "the x branch is shared");
    assert!(Arc::ptr_eq(&old_root.children[&'a'].children[&'c'], &new_root.children[&'a'].children[&'c']), "the sibling leaf is shared");
    assert!(!Arc::ptr_eq(&old_root.children[&'a'], &new_root.children[&'a']), "only the path is copied everything else is shared: expected `!Arc::ptr_eq(&old_root.children[&'a'], &new_root.children[&'a'])`");
}

#[test]
fn s0a_02_values_are_not_copied_and_need_not_be_clonable() {
    struct NoClone(u32);
    let trie = t().put("tes", Box::new(NoClone(233))).put("te", Box::new(NoClone(23))).put("test", Box::new(NoClone(2333)));
    assert_eq!(trie.get::<Box<NoClone>>("te").unwrap().0, 23, "values are not copied and need not be clonable");
    assert_eq!(trie.get::<Box<NoClone>>("test").unwrap().0, 2333, "values are not copied and need not be clonable");
    let before = trie.get::<Box<NoClone>>("test").unwrap() as *const _;
    let trie = trie.put("tes", Box::new(NoClone(0)));
    let after = trie.get::<Box<NoClone>>("test").unwrap() as *const _;
    assert_eq!(before, after, "the value of an untouched key keeps its address");
}

// ---- 0a-02: remove ----------------------------------------------------------------------------------------------------------------------

#[test]
fn s0a_02_remove_deletes_only_the_given_key() {
    let trie = t().put("test", 2333u32).put("te", 23u32).put("tes", 233u32).remove("tes");
    assert!(trie.get::<u32>("tes").is_none(), "remove deletes only the given key: expected `trie.get::<u32>(\"tes\").is_none()`");
    assert_eq!(trie.get::<u32>("te"), Some(&23), "remove deletes only the given key");
    assert_eq!(trie.get::<u32>("test"), Some(&2333), "remove deletes only the given key");
}

#[test]
fn s0a_02_removing_everything_leaves_an_empty_trie_with_no_root() {
    let mut trie = t().put("test", 2333u32).put("te", 23u32).put("tes", 233u32).put("", 123u32);
    for k in ["", "te", "tes", "test"] {
        trie = trie.remove(k);
    }
    assert!(trie.root().is_none(), "removing everything leaves an empty trie with no root: expected `trie.root().is_none()`");
    for k in ["", "te", "tes", "test"] {
        assert!(trie.get::<u32>(k).is_none(), "removing everything leaves an empty trie with no root: expected `trie.get::<u32>(k).is_none()`");
    }
}

#[test]
fn s0a_02_nodes_with_no_value_and_no_children_are_pruned() {
    let trie = t().put("test", 2333u32).put("te", 23u32).put("tes", 233u32).remove("tes").remove("test");
    let te = &trie.root().unwrap().children[&'t'].children[&'e'];
    assert!(te.children.is_empty(), "s and t were pruned");
    assert!(te.is_value_node(), "nodes with no value and no children are pruned: expected `te.is_value_node()`");
}

#[test]
fn s0a_02_a_node_that_still_leads_somewhere_stays() {
    let trie = t().put("test", 1u32).put("te", 2u32).remove("te");
    assert!(trie.get::<u32>("te").is_none(), "a node that still leads somewhere stays: expected `trie.get::<u32>(\"te\").is_none()`");
    assert_eq!(trie.get::<u32>("test"), Some(&1), "a node that still leads somewhere stays");
    assert!(!trie.root().unwrap().children[&'t'].children[&'e'].is_value_node(), "a node that still leads somewhere stays: expected `!trie.root().unwrap().children[&'t'].children[&'e'].is_value_node()`");
}

#[test]
fn s0a_02_removing_a_missing_key_changes_nothing() {
    let trie = t().put("test", 1u32);
    for k in ["tes", "tests", "x", ""] {
        let same = trie.remove(k);
        assert_eq!(same.get::<u32>("test"), Some(&1), "{k}");
    }
    assert!(t().remove("x").root().is_none(), "removing a missing key changes nothing: expected `t().remove(\"x\").root().is_none()`");
}

#[test]
fn s0a_02_removing_never_changes_the_old_trie() {
    let trie3 = t().put("test", 2333u32).put("te", 23u32).put("tes", 233u32);
    let trie4 = trie3.remove("te");
    let trie5 = trie3.remove("tes");
    let trie6 = trie3.remove("test");
    assert_eq!(trie3.get::<u32>("te"), Some(&23), "removing never changes the old trie");
    assert!(trie4.get::<u32>("te").is_none(), "removing never changes the old trie: expected `trie4.get::<u32>(\"te\").is_none()`");
    assert_eq!(trie4.get::<u32>("tes"), Some(&233), "removing never changes the old trie");
    assert!(trie5.get::<u32>("tes").is_none(), "removing never changes the old trie: expected `trie5.get::<u32>(\"tes\").is_none()`");
    assert_eq!(trie5.get::<u32>("test"), Some(&2333), "removing never changes the old trie");
    assert!(trie6.get::<u32>("test").is_none(), "removing never changes the old trie: expected `trie6.get::<u32>(\"test\").is_none()`");
    assert_eq!(trie6.get::<u32>("te"), Some(&23), "removing never changes the old trie");
}

// ---- 0a-03: the store ------------------------------------------------------------------------------------------------------------------

#[test]
fn s0a_03_put_get_remove() {
    let store = TrieStore::new();
    assert!(store.get::<u32>("233").is_none(), "put get remove: expected `store.get::<u32>(\"233\").is_none()`");
    store.put("233", 2333u32);
    assert_eq!(*store.get::<u32>("233").unwrap(), 2333, "put get remove");
    store.remove("233");
    assert!(store.get::<u32>("233").is_none(), "put get remove: expected `store.get::<u32>(\"233\").is_none()`");
}

#[test]
fn s0a_03_a_guard_stays_valid_after_the_key_is_removed() {
    let store = TrieStore::new();
    store.put("233", String::from("2333"));
    let guard = store.get::<String>("233").unwrap();
    store.remove("233");
    assert!(store.get::<String>("233").is_none(), "a guard stays valid after the key is removed: expected `store.get::<String>(\"233\").is_none()`");
    assert_eq!(*guard, "2333", "a guard stays valid after the key is removed");
}

#[test]
fn s0a_03_a_guard_sees_the_version_it_was_taken_from() {
    let store = TrieStore::new();
    store.put("k", 1u32);
    let old = store.get::<u32>("k").unwrap();
    store.put("k", 2u32);
    assert_eq!(*old, 1, "a guard sees the version it was taken from");
    assert_eq!(*store.get::<u32>("k").unwrap(), 2, "a guard sees the version it was taken from");
}

#[test]
fn s0a_03_values_need_not_be_clonable() {
    struct NoClone(u32);
    let store = TrieStore::new();
    store.put("tes", Box::new(NoClone(233)));
    store.put("te", Box::new(NoClone(23)));
    assert_eq!(store.get::<Box<NoClone>>("te").unwrap().0, 23, "values need not be clonable");
    store.remove("te");
    assert!(store.get::<Box<NoClone>>("te").is_none(), "values need not be clonable: expected `store.get::<Box<NoClone>>(\"te\").is_none()`");
}

#[test]
fn s0a_03_four_writers_and_four_readers() {
    let store = Arc::new(TrieStore::new());
    let per_thread = 2_000u32;
    let key = |n: u32| format!("{n:05}");
    let writers: Vec<_> = (0..4u32)
        .map(|tid| {
            let store = store.clone();
            std::thread::spawn(move || {
                for i in 0..per_thread {
                    store.put(&key(i * 4 + tid), format!("value-{:08}", i * 4 + tid));
                }
                for i in 0..per_thread {
                    store.remove(&key(i * 4 + tid));
                }
                for i in 0..per_thread {
                    store.put(&key(i * 4 + tid), format!("new-value-{:08}", i * 4 + tid));
                }
            })
        })
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..4u32)
        .map(|tid| {
            let (store, stop) = (store.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut i = 0;
                while !stop.load(Ordering::Relaxed) {
                    store.get::<String>(&key(i * 4 + tid));
                    i = (i + 1) % per_thread;
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        r.join().unwrap();
    }
    for i in 0..per_thread * 4 {
        let guard = store.get::<String>(&key(i)).unwrap_or_else(|| panic!("key {i} was lost: two writers started from the same version"));
        assert_eq!(*guard, format!("new-value-{i:08}"), "four writers and four readers");
    }
}

#[test]
fn s0a_03_a_reader_never_waits_for_a_writer_even_one_stuck_in_a_drop() {
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::Duration;
    // A value whose drop blocks until it is released. Overwriting it makes the writer drop the old version, and the drop blocks: a
    // store that does that while holding the lock readers need would keep every reader waiting.
    struct Blocker {
        started: Mutex<mpsc::Sender<()>>,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl Drop for Blocker {
        fn drop(&mut self) {
            let _ = self.started.lock().unwrap().send(());
            let _ = self.release.lock().unwrap().recv_timeout(Duration::from_secs(10));
        }
    }
    let store = Arc::new(TrieStore::new());
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    store.put("a", 1u32);
    store.put("slow", Blocker { started: Mutex::new(started_tx), release: Mutex::new(release_rx) });
    let writer = {
        let store = store.clone();
        std::thread::spawn(move || store.put("slow", 0u32))
    };
    started_rx.recv_timeout(Duration::from_secs(10)).expect("overwriting the value drops it once no version holds it any more");
    // the writer is now stuck inside the drop of the old value
    let (tx, rx) = mpsc::channel();
    {
        let store = store.clone();
        std::thread::spawn(move || {
            let _ = tx.send(store.get::<u32>("a").map(|g| *g));
        });
    }
    let got = rx.recv_timeout(Duration::from_secs(2));
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    assert_eq!(got, Ok(Some(1)), "a reader must not wait for a writer, not even one stuck cleaning up the old version: drop it outside the lock readers take");
}

#[test]
fn s0a_03_readers_are_not_blocked_by_a_writer_building_a_big_trie() {
    let store = Arc::new(TrieStore::new());
    for i in 0..1000u32 {
        store.put(&format!("{i:04}"), i);
    }
    let writer = {
        let store = store.clone();
        std::thread::spawn(move || {
            for i in 1000..6000u32 {
                store.put(&format!("{i:04}"), i);
            }
        })
    };
    // while the writer runs, readers keep finding what is already there
    while !writer.is_finished() {
        for i in (0..1000u32).step_by(97) {
            assert_eq!(*store.get::<u32>(&format!("{i:04}")).unwrap(), i, "readers are not blocked by a writer building a big trie");
        }
    }
    writer.join().unwrap();
}

// ---- 0a-04: BusTub's trie tests ---------------------------------------------------------------------------------------------------------

#[test]
fn s0a_04_mixed_test() {
    let mut trie = Trie::new();
    for i in 0..23333u32 {
        trie = trie.put(&format!("{i:05}"), format!("value-{i:08}"));
    }
    let full = trie.clone();
    for i in (0..23333u32).step_by(2) {
        trie = trie.put(&format!("{i:05}"), format!("new-value-{i:08}"));
    }
    let overridden = trie.clone();
    for i in (0..23333u32).step_by(3) {
        trie = trie.remove(&format!("{i:05}"));
    }
    let fin = trie;
    for i in 0..23333u32 {
        let key = format!("{i:05}");
        assert_eq!(full.get::<String>(&key), Some(&format!("value-{i:08}")), "mixed test");
        let want = if i % 2 == 0 { format!("new-value-{i:08}") } else { format!("value-{i:08}") };
        assert_eq!(overridden.get::<String>(&key), Some(&want), "mixed test");
        if i % 3 == 0 {
            assert!(fin.get::<String>(&key).is_none(), "mixed test: expected `fin.get::<String>(&key).is_none()`");
        } else {
            assert_eq!(fin.get::<String>(&key), Some(&want), "mixed test");
        }
    }
}

#[test]
fn s0a_04_copy_on_write_tests_with_the_empty_key() {
    let trie3 = Trie::new().put("test", 2333u32).put("te", 23u32).put("", 233u32);
    let trie4 = trie3.put("te", String::from("23"));
    let trie5 = trie3.put("", String::from("233"));
    let trie6 = trie3.put("test", String::from("2333"));
    assert_eq!(trie3.get::<u32>("te"), Some(&23), "copy on write tests with the empty key");
    assert_eq!(trie3.get::<u32>(""), Some(&233), "copy on write tests with the empty key");
    assert_eq!(trie3.get::<u32>("test"), Some(&2333), "copy on write tests with the empty key");
    assert_eq!(trie4.get::<String>("te").map(String::as_str), Some("23"), "copy on write tests with the empty key");
    assert_eq!(trie4.get::<u32>(""), Some(&233), "copy on write tests with the empty key");
    assert_eq!(trie5.get::<String>("").map(String::as_str), Some("233"), "copy on write tests with the empty key");
    assert_eq!(trie5.get::<u32>("test"), Some(&2333), "copy on write tests with the empty key");
    assert_eq!(trie6.get::<String>("test").map(String::as_str), Some("2333"), "copy on write tests with the empty key");
    assert_eq!(trie6.get::<u32>(""), Some(&233), "copy on write tests with the empty key");
}

#[test]
fn s0a_04_trie_store_mixed_test() {
    let store = TrieStore::new();
    for i in 0..23333u32 {
        store.put(&format!("{i:05}"), format!("value-{i:08}"));
    }
    for i in (0..23333u32).step_by(2) {
        store.put(&format!("{i:05}"), format!("new-value-{i:08}"));
    }
    for i in (0..23333u32).step_by(3) {
        store.remove(&format!("{i:05}"));
    }
    for i in 0..23333u32 {
        let key = format!("{i:05}");
        let got = store.get::<String>(&key);
        if i % 3 == 0 {
            assert!(got.is_none(), "trie store mixed test: expected `got.is_none()`");
        } else if i % 2 == 0 {
            assert_eq!(*got.unwrap(), format!("new-value-{i:08}"), "trie store mixed test");
        } else {
            assert_eq!(*got.unwrap(), format!("value-{i:08}"), "trie store mixed test");
        }
    }
}

#[test]
fn s0a_04_pointer_stability_and_noncopyable_values() {
    let trie = Trie::new().put("test", 2333u32);
    let before = trie.get::<u32>("test").unwrap() as *const u32;
    let trie = trie.put("tes", 233u32).put("te", 23u32);
    let after = trie.get::<u32>("test").unwrap() as *const u32;
    assert_eq!(before, after, "pointer stability and noncopyable values");
    let boxed = Trie::new().put("tes", Box::new(233u32)).put("te", Box::new(23u32)).put("test", Box::new(2333u32));
    assert_eq!(**boxed.get::<Box<u32>>("te").unwrap(), 23, "pointer stability and noncopyable values");
    let boxed = boxed.remove("te").remove("tes").remove("test");
    assert!(boxed.get::<Box<u32>>("te").is_none(), "pointer stability and noncopyable values: expected `boxed.get::<Box<u32>>(\"te\").is_none()`");
}

// ---- properties: the trie against a map, with every old version kept -----------------------------------------------------------------

fn pconfig() -> ProptestConfig {
    ProptestConfig { cases: 64, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

/// Keys over a two-letter alphabet, so that many keys are prefixes of others.
fn key_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec!['a', 'b']), 0..5).prop_map(|cs| cs.into_iter().collect())
}

/// All 31 keys of length 0 to 4 over {a, b}: the universe the properties look at.
fn universe() -> Vec<String> {
    let mut all = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..4 {
        layer = layer.iter().flat_map(|k| ['a', 'b'].map(|c| format!("{k}{c}"))).collect();
        all.extend(layer.clone());
    }
    all
}

/// A mutable node used only to build a trie by hand from a map (stage 1 has no `put` yet).
#[derive(Default)]
struct Draft {
    children: BTreeMap<char, Draft>,
    value: Option<u32>,
}

impl Draft {
    fn freeze(self) -> Arc<TrieNode> {
        Arc::new(TrieNode { children: self.children.into_iter().map(|(c, d)| (c, d.freeze())).collect(), value: self.value.map(|v| Arc::new(v) as _) })
    }
}

fn built_by_hand(model: &BTreeMap<String, u32>) -> Trie {
    if model.is_empty() {
        return Trie::new();
    }
    let mut root = Draft::default();
    for (k, v) in model {
        let mut n = &mut root;
        for c in k.chars() {
            n = n.children.entry(c).or_default();
        }
        n.value = Some(*v);
    }
    Trie::from_root(Some(root.freeze()))
}

fn same_as_model(trie: &Trie, model: &BTreeMap<String, u32>) -> Result<(), TestCaseError> {
    for k in universe() {
        prop_assert_eq!(trie.get::<u32>(&k).copied(), model.get(&k).copied(), "key {:?}", k);
        prop_assert!(trie.get::<String>(&k).is_none(), "wrong type for {:?} must be None", k);
        prop_assert_eq!(trie.get_shared::<u32>(&k).map(|a| *a), model.get(&k).copied());
    }
    Ok(())
}

/// Every node has a value or a child (a trie leaves no empty branches behind), and the trie has a root exactly when it holds a key.
fn pruned(node: &TrieNode) -> bool {
    (node.is_value_node() || !node.children.is_empty()) && node.children.values().all(|c| pruned(c))
}

proptest! {
    #![proptest_config(pconfig())]

    /// A trie built by hand from any set of keys answers `get` for every key of the universe as the map does: present keys with their
    /// value, everything else (prefixes, extensions, other types) with nothing.
    #[test]
    fn s0a_01_get_agrees_with_a_map_for_every_key(entries in prop::collection::vec((key_strategy(), any::<u32>()), 0..12)) {
        let model: BTreeMap<String, u32> = entries.into_iter().collect();
        same_as_model(&built_by_hand(&model), &model)?;
    }

    /// Random puts and removes, keeping **every version**: at each step the new version equals the map, **every older version still
    /// equals the map it was**, the new version shares with the old one every node off the path of the key, and no empty branch is left.
    #[test]
    fn s0a_02_every_version_stays_what_it_was_and_shares_all_it_can(ops in prop::collection::vec((any::<bool>(), key_strategy(), any::<u32>()), 1..40)) {
        let mut versions: Vec<(Trie, BTreeMap<String, u32>)> = vec![(Trie::new(), BTreeMap::new())];
        for (put, key, value) in ops {
            let (old, mut model) = versions.last().unwrap().clone();
            let new = if put { model.insert(key.clone(), value); old.put(&key, value) } else { model.remove(&key); old.remove(&key) };
            same_as_model(&new, &model)?;
            prop_assert_eq!(new.root().is_some(), !model.is_empty(), "a trie has a root exactly when it holds a key");
            if let Some(root) = new.root() {
                prop_assert!(pruned(root), "an empty branch was left behind after {:?} {:?}", put, key);
            }
            // sharing: along the key's path nodes are new; every child hanging off the path is the same node as before
            let (mut a, mut b) = (old.root().cloned(), new.root().cloned());
            for c in key.chars() {
                let (Some(x), Some(y)) = (a.clone(), b.clone()) else { break };
                for (k, child) in &y.children {
                    if *k != c {
                        let before = x.children.get(k);
                        prop_assert!(before.is_some_and(|o| Arc::ptr_eq(o, child)), "child {:?} off the path of {:?} was copied", k, key);
                    }
                }
                a = x.children.get(&c).cloned();
                b = y.children.get(&c).cloned();
            }
            versions.push((new, model));
        }
        for (i, (trie, model)) in versions.iter().enumerate() {
            same_as_model(trie, model).map_err(|e| TestCaseError::fail(format!("version {i} changed afterwards: {e}")))?;
        }
    }

    /// The store, used from one thread, is the map; a guard taken before later writes still holds the value it was taken with.
    #[test]
    fn s0a_03_the_store_behaves_like_a_map_and_guards_keep_their_values(ops in prop::collection::vec((0u8..3, key_strategy(), any::<u32>()), 1..40)) {
        let store = TrieStore::new();
        let mut model: BTreeMap<String, u32> = BTreeMap::new();
        let mut guards = vec![];
        for (op, key, value) in ops {
            match op {
                0 => { store.put(&key, value); model.insert(key, value); }
                1 => { store.remove(&key); model.remove(&key); }
                _ => if let Some(g) = store.get::<u32>(&key) { guards.push((*g, g)); },
            }
            for k in universe() {
                prop_assert_eq!(store.get::<u32>(&k).map(|g| *g), model.get(&k).copied());
            }
        }
        for (seen, guard) in guards {
            prop_assert_eq!(*guard, seen, "a guard must keep the value it found");
        }
    }
}

#[test]
fn s0a_04_a_reader_never_sees_a_value_go_backwards_while_a_writer_counts_up() {
    let store = Arc::new(TrieStore::new());
    store.put("counter", 0u64);
    store.put("other", 7u32);
    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (store, done) = (store.clone(), done.clone());
            std::thread::spawn(move || {
                let mut last = 0u64;
                let mut reads = 0u64;
                while !done.load(Ordering::SeqCst) || reads < 100 {
                    let v = *store.get::<u64>("counter").expect("the counter is always there");
                    assert!(v >= last, "the counter went from {last} back to {v}");
                    assert_eq!(*store.get::<u32>("other").unwrap(), 7, "an unrelated key never changes");
                    last = v;
                    reads += 1;
                }
                last
            })
        })
        .collect();
    for n in 1..=3000u64 {
        store.put("counter", n);
    }
    done.store(true, Ordering::SeqCst);
    for r in readers {
        assert!(r.join().unwrap() <= 3000);
    }
    assert_eq!(*store.get::<u64>("counter").unwrap(), 3000);
}

// @@ challenge 0a-c1 begin
mod ch_0a_c1 {
    use proptest::prelude::*;

    use bustub::primer::prefix_table::PrefixTable;

    fn table() -> PrefixTable<char> {
        let mut t = PrefixTable::new();
        t.insert("", 'A');
        t.insert("10", 'B');
        t.insert("1011", 'C');
        t
    }

    #[test]
    fn s0a_c1_the_longest_matching_prefix_wins() {
        let t = table();
        assert_eq!(t.lookup("10110"), Some(("1011", &'C')));
        assert_eq!(t.lookup("1001"), Some(("10", &'B')));
        assert_eq!(t.lookup("0"), Some(("", &'A')));
        assert_eq!(t.lookup("1011"), Some(("1011", &'C')), "an address equal to a prefix matches it");
    }

    #[test]
    fn s0a_c1_without_a_default_entry_some_addresses_match_nothing() {
        let mut t = PrefixTable::new();
        t.insert("11", 1);
        assert_eq!(t.lookup("10"), None);
        assert_eq!(t.lookup("1"), None, "a prefix longer than the address cannot match");
        assert_eq!(t.lookup("110"), Some(("11", &1)));
    }

    #[test]
    fn s0a_c1_replacing_and_removing() {
        let mut t = table();
        assert_eq!(t.insert("10", 'Z'), Some('B'));
        assert_eq!(t.lookup("100"), Some(("10", &'Z')));
        assert_eq!(t.remove("1011"), Some('C'));
        assert_eq!(t.lookup("10110"), Some(("10", &'Z')), "falls back to the next longest");
        assert_eq!(t.remove("1011"), None);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn s0a_c1_a_prefix_only_matches_at_the_start_of_the_address() {
        let mut t = PrefixTable::new();
        t.insert("01", 'x');
        assert_eq!(t.lookup("101"), None, "01 occurs inside the address, not at its start");
        assert_eq!(t.lookup("011"), Some(("01", &'x')));
        assert_eq!(t.len(), 1);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against a scan over every entry for the longest one that is a prefix.
        #[test]
        fn s0a_c1_property_lookup_equals_a_scan(prefixes in proptest::collection::vec("[01]{0,5}", 0..10), addr in "[01]{0,8}") {
            let mut t = PrefixTable::new();
            for (i, p) in prefixes.iter().enumerate() { t.insert(p, i); }
            let mut want: Option<(&str, usize)> = None;
            let mut last: std::collections::BTreeMap<&str, usize> = Default::default();
            for (i, p) in prefixes.iter().enumerate() { last.insert(p.as_str(), i); }
            for (p, &i) in &last { if addr.starts_with(p) && want.is_none_or(|(w, _)| p.len() > w.len()) { want = Some((p, i)); } }
            prop_assert_eq!(t.lookup(&addr).map(|(p, v)| (p, *v)), want);
        }
    }
}
// @@ challenge 0a-c1 end

// @@ challenge 0a-c2 begin
mod ch_0a_c2 {
    use proptest::prelude::*;

    use bustub::primer::trie::Trie;
    use bustub::primer::trie_extras::keys_with_prefix;
    use std::collections::BTreeSet;

    fn build(keys: &[&str]) -> Trie {
        keys.iter().fold(Trie::new(), |t, k| t.put(k, k.len() as u32))
    }

    #[test]
    fn s0a_c2_keys_under_a_prefix_come_out_sorted() {
        let t = build(&["abc", "b", "a", "ab"]);
        assert_eq!(keys_with_prefix(&t, "a"), vec!["a", "ab", "abc"]);
        assert_eq!(keys_with_prefix(&t, "ab"), vec!["ab", "abc"]);
        assert_eq!(keys_with_prefix(&t, ""), vec!["a", "ab", "abc", "b"]);
    }

    #[test]
    fn s0a_c2_a_prefix_that_leads_nowhere_lists_nothing() {
        let t = build(&["abc"]);
        assert_eq!(keys_with_prefix(&t, "x"), Vec::<String>::new());
        assert_eq!(keys_with_prefix(&t, "abcd"), Vec::<String>::new());
        assert_eq!(keys_with_prefix(&Trie::new(), ""), Vec::<String>::new());
    }

    #[test]
    fn s0a_c2_an_interior_node_without_a_value_is_not_a_key() {
        let t = build(&["abc"]);
        assert_eq!(keys_with_prefix(&t, "a"), vec!["abc"]);
        assert_eq!(keys_with_prefix(&t, "ab"), vec!["abc"]);
    }

    #[test]
    fn s0a_c2_old_versions_keep_their_keys() {
        let v1 = build(&["a", "b"]);
        let v2 = v1.put("c", 1u32).remove("a");
        assert_eq!(keys_with_prefix(&v1, ""), vec!["a", "b"]);
        assert_eq!(keys_with_prefix(&v2, ""), vec!["b", "c"]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals filtering the sorted key set by `starts_with`.
        #[test]
        fn s0a_c2_property_prefix_listing_equals_a_filter(keys in proptest::collection::btree_set("[ab]{0,4}", 0..12), prefix in "[ab]{0,3}") {
            let mut t = Trie::new();
            for k in &keys { t = t.put(k, 1u32); }
            let want: Vec<String> = keys.iter().filter(|k| k.starts_with(&prefix)).cloned().collect();
            prop_assert_eq!(keys_with_prefix(&t, &prefix), want);
            let all: BTreeSet<String> = keys_with_prefix(&t, "").into_iter().collect();
            prop_assert_eq!(all, keys);
        }
    }
}
// @@ challenge 0a-c2 end

// @@ challenge 0a-c3 begin
mod ch_0a_c3 {
    use proptest::prelude::*;

    use bustub::primer::trie::Trie;
    use bustub::primer::trie_extras::{node_count, shared_nodes};

    fn build(keys: &[&str]) -> Trie {
        keys.iter().fold(Trie::new(), |t, k| t.put(k, 1u32))
    }

    #[test]
    fn s0a_c3_counting_nodes() {
        assert_eq!(node_count(&Trie::new()), 0);
        assert_eq!(node_count(&build(&["abc"])), 4, "the root and three letters");
        assert_eq!(node_count(&build(&["abc", "abd", "x"])), 1 + 4 + 1);
    }

    #[test]
    fn s0a_c3_a_trie_shares_everything_with_itself_and_nothing_with_an_empty_one() {
        let t = build(&["a", "ab", "b"]);
        assert_eq!(shared_nodes(&t, &t), node_count(&t));
        assert_eq!(shared_nodes(&t, &Trie::new()), 0);
    }

    #[test]
    fn s0a_c3_one_put_copies_only_the_path() {
        let keys: Vec<String> = (0..60).map(|i| format!("{:03}{}", i, i % 7)).collect();
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let old = build(&refs);
        let new = old.put("0123456", 9u32);
        let fresh = node_count(&new) - shared_nodes(&old, &new);
        assert!(fresh <= "0123456".len() + 1, "{fresh} nodes are new but the path has {}: put copied more than the path", "0123456".len() + 1);
        assert!(shared_nodes(&old, &new) > 50, "almost everything must be shared");
    }

    #[test]
    fn s0a_c3_removing_a_key_also_copies_only_the_path() {
        let old = build(&["alpha", "alps", "beta", "gamma", "delta", "alphabet"]);
        let new = old.remove("alps");
        assert!(node_count(&new) - shared_nodes(&old, &new) <= "alps".len() + 1);
    }

    #[test]
    fn s0a_c3_two_versions_of_a_trie_share_what_neither_changed() {
        let base = build(&["a", "b", "c", "d"]);
        let left = base.put("a1", 1u32);
        let right = base.put("b1", 1u32);
        assert_eq!(shared_nodes(&left, &right), node_count(&base) - 3, "only the root and the two changed paths differ: c and d are shared");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: for any trie and key, a put or remove makes at most `len + 1` new nodes.
        #[test]
        fn s0a_c3_property_changes_copy_only_one_path(keys in proptest::collection::btree_set("[abc]{1,5}", 0..12), key in "[abc]{1,5}") {
            let mut t = Trie::new();
            for k in &keys { t = t.put(k, 1u32); }
            let put = t.put(&key, 2u32);
            prop_assert!(node_count(&put) - shared_nodes(&t, &put) <= key.chars().count() + 1);
            let rem = t.remove(&key);
            prop_assert!(node_count(&rem).saturating_sub(shared_nodes(&t, &rem)) <= key.chars().count() + 1);
            prop_assert!(shared_nodes(&t, &put) <= node_count(&t).min(node_count(&put)));
        }
    }
}
// @@ challenge 0a-c3 end

// @@ challenge 0a-c4 begin
mod ch_0a_c4 {
    use proptest::prelude::*;

    use bustub::primer::shared_list::PList;

    #[test]
    fn s0a_c4_pushing_does_not_change_the_old_version() {
        let a = PList::new();
        let b = a.push(1);
        assert_eq!(a.to_vec(), Vec::<i64>::new());
        assert_eq!(b.to_vec(), vec![1]);
    }

    #[test]
    fn s0a_c4_branching_from_one_version() {
        let b = PList::new().push(1);
        let c = b.push(2);
        let d = b.push(3);
        assert_eq!((b.to_vec(), c.to_vec(), d.to_vec()), (vec![1], vec![2, 1], vec![3, 1]));
    }

    #[test]
    fn s0a_c4_a_version_dropped_and_recreated_does_not_disturb_the_others() {
        let base = PList::new().push(1).push(2);
        {
            let _tmp = base.push(99);
        }
        let after = base.push(7);
        assert_eq!(base.to_vec(), vec![2, 1]);
        assert_eq!(after.to_vec(), vec![7, 2, 1]);
    }

    #[test]
    fn s0a_c4_lengths_are_fixed_at_creation() {
        let a = PList::new().push(5);
        let b = a.push(6);
        assert_eq!((a.len(), b.len()), (1, 2));
        assert!(PList::new().is_empty());
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a tree of versions, each equal to the vector model of its history, whatever is pushed onto whom.
        #[test]
        fn s0a_c4_property_every_version_keeps_its_contents(ops in proptest::collection::vec((0usize..8, any::<i64>()), 0..30)) {
            let mut versions: Vec<(PList, Vec<i64>)> = vec![(PList::new(), vec![])];
            for (from, v) in ops {
                let (src, model) = versions[from % versions.len()].clone();
                let new = src.push(v);
                let mut m = vec![v];
                m.extend(model);
                versions.push((new, m));
                for (p, m) in &versions {
                    prop_assert_eq!(&p.to_vec(), m);
                    prop_assert_eq!(p.len(), m.len());
                }
            }
        }
    }
}
// @@ challenge 0a-c4 end

// @@ challenge 0a-c5 begin
mod ch_0a_c5 {
    use proptest::prelude::*;

    use bustub::primer::trie::Trie;
    use bustub::primer::trie_extras::keys_matching;

    fn build(keys: &[&str]) -> Trie {
        keys.iter().fold(Trie::new(), |t, k| t.put(k, 1u32))
    }

    #[test]
    fn s0a_c5_a_question_mark_matches_exactly_one_character() {
        let t = build(&["abc", "abd", "aec", "xyz"]);
        assert_eq!(keys_matching(&t, "a?c"), vec!["abc", "aec"]);
        assert_eq!(keys_matching(&t, "ab?"), vec!["abc", "abd"]);
        assert_eq!(keys_matching(&t, "???"), vec!["abc", "abd", "aec", "xyz"]);
    }

    #[test]
    fn s0a_c5_the_length_must_match() {
        let t = build(&["a", "ab", "abc"]);
        assert_eq!(keys_matching(&t, "a?"), vec!["ab"]);
        assert_eq!(keys_matching(&t, "?"), vec!["a"]);
        assert_eq!(keys_matching(&t, "????"), Vec::<String>::new());
    }

    #[test]
    fn s0a_c5_literals_only_and_the_empty_pattern() {
        let t = build(&["", "a", "ab"]);
        assert_eq!(keys_matching(&t, "ab"), vec!["ab"]);
        assert_eq!(keys_matching(&t, ""), vec![""], "the empty key matches the empty pattern");
        assert_eq!(keys_matching(&Trie::new(), "?"), Vec::<String>::new());
    }

    #[test]
    fn s0a_c5_results_come_back_in_key_order() {
        let t = build(&["ac", "ab", "bb", "aa"]);
        assert_eq!(keys_matching(&t, "a?"), vec!["aa", "ab", "ac"]);
        assert_eq!(keys_matching(&t, "?b"), vec!["ab", "bb"]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: equals filtering the key set by a position-by-position match.
        #[test]
        fn s0a_c5_property_matching_equals_a_filter(keys in proptest::collection::btree_set("[abc]{0,4}", 0..12), pattern in "[abc?]{0,4}") {
            let mut t = Trie::new();
            for k in &keys { t = t.put(k, 1u32); }
            let m = |k: &str| k.chars().count() == pattern.chars().count() && k.chars().zip(pattern.chars()).all(|(a, p)| p == '?' || a == p);
            let want: Vec<String> = keys.iter().filter(|k| m(k)).cloned().collect();
            prop_assert_eq!(keys_matching(&t, &pattern), want);
        }
    }
}
// @@ challenge 0a-c5 end
