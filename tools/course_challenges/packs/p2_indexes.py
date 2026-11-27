from _c import C
M2A, M2B, M2C, M2D = "08-typed-pages", "09-extendible-hash", "10-b-plus-tree", "11-b-plus-tree-tombstones"
CH = []

CH.append(C("2a-c3", M2A, "92-challenge-a-slotted-page", "build", "Challenge: a slotted page", "medium", "stages_2a::s2a_c3",
  ["storing variable-length records in a fixed-size page","reclaiming fragmented space without losing a record"],
  ["slotted-pages","slot-allocation-and-invariants","model-based-testing"],
  "`SlottedPage` in `src/storage/page/slotted_page.rs`: a fixed-size page that stores variable-length byte records. `insert` returns a slot number, `get` reads a record by slot, `delete` frees it. A record costs its length plus **4 bytes** of slot entry; the page has `size` bytes in all. Space freed by deletes must be reusable even when it is scattered.",
  "Heap pages hold rows of different lengths, and deletes leave holes. A page that cannot reuse its holes fills up with garbage while claiming to be full. The design question is how the bytes are laid out; the contract is that an insert succeeds whenever the *total* free space is enough.",
  ["`new(size)`; `insert(record)` returns the lowest unused slot number holding the record, or `None` if `record.len() + 4 > free_space()`.","`get(slot)` is the record, or `None` for a free or unknown slot; `delete(slot)` frees it (false if it was not live).","`free_space()` is `size` minus the sum of `len + 4` over live records; `len()` the number of live records."],
  ["`free_space() + sum(live len + 4) == size`.","Every live record reads back exactly the bytes inserted, whatever was inserted and deleted since.","Slot numbers of live records are unique and stable: a record keeps its slot until it is deleted."],
  ["`insert` succeeds iff the record fits in the *total* free space, however fragmented.","Deleting a record and inserting one of the same length succeeds.","Delete then insert the same bytes returns the same (lowest free) slot."],
  ["size 20: insert 4 bytes -> slot 0 (free 12); insert 4 -> slot 1 (free 4); insert 1 -> None","delete(0); insert 4 -> slot 0"],
  ["Insert, get, delete and the space accounting.","Fragmentation: freed holes are reusable.","Slot reuse is lowest first.","A property against a model of slots and a byte budget."],
  src=("src/storage/page/slotted_page.rs", '''
//! A fixed-size page of variable-length records.

pub struct SlottedPage {
    // @begin 2a-c3
    size: usize,
    slots: Vec<Option<Vec<u8>>>,
    //~ _page: (),
    // @end
}

const SLOT_COST: usize = 4;

impl SlottedPage {
    pub fn new(size: usize) -> SlottedPage {
        // @begin 2a-c3
        SlottedPage { size, slots: Vec::new() }
        //~ todo!("2a-c3: an empty page of `size` bytes")
        // @end
    }

    pub fn free_space(&self) -> usize {
        // @begin 2a-c3
        self.size - self.slots.iter().flatten().map(|r| r.len() + SLOT_COST).sum::<usize>()
        //~ todo!("2a-c3: the bytes not used by records and their slot entries")
        // @end
    }

    pub fn insert(&mut self, record: &[u8]) -> Option<usize> {
        // @begin 2a-c3
        if record.len() + SLOT_COST > self.free_space() {
            return None;
        }
        let slot = match self.slots.iter().position(|s| s.is_none()) {
            Some(i) => i,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.slots[slot] = Some(record.to_vec());
        Some(slot)
        //~ todo!("2a-c3: the lowest free slot, if the record fits in the total free space")
        // @end
    }

    pub fn get(&self, slot: usize) -> Option<&[u8]> {
        // @begin 2a-c3
        self.slots.get(slot)?.as_deref()
        //~ todo!("2a-c3: the record in the slot")
        // @end
    }

    pub fn delete(&mut self, slot: usize) -> bool {
        // @begin 2a-c3
        match self.slots.get_mut(slot) {
            Some(s @ Some(_)) => {
                *s = None;
                true
            }
            _ => false,
        }
        //~ todo!("2a-c3: free the slot")
        // @end
    }

    pub fn len(&self) -> usize {
        // @begin 2a-c3
        self.slots.iter().flatten().count()
        //~ todo!("2a-c3: how many records")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
'''),
  test=("tests/stages_2a.rs", '''
use bustub::storage::page::slotted_page::SlottedPage;

#[test]
fn s2a_c3_records_come_back_and_space_is_accounted() {
    let mut p = SlottedPage::new(20);
    assert_eq!(p.insert(b"abcd"), Some(0));
    assert_eq!(p.free_space(), 12);
    assert_eq!(p.insert(b"wxyz"), Some(1));
    assert_eq!(p.free_space(), 4);
    assert_eq!(p.insert(b"q"), None, "1 byte + 4 does not fit in 4... it needs 5");
    assert_eq!((p.get(0), p.get(1), p.get(2)), (Some(&b"abcd"[..]), Some(&b"wxyz"[..]), None));
}

#[test]
fn s2a_c3_a_deleted_record_gives_its_space_and_its_slot_back() {
    let mut p = SlottedPage::new(20);
    p.insert(b"abcd");
    p.insert(b"wxyz");
    assert!(p.delete(0));
    assert!(!p.delete(0), "already free");
    assert_eq!(p.free_space(), 12);
    assert_eq!(p.insert(b"1234"), Some(0), "the lowest free slot is reused");
    assert_eq!(p.get(0), Some(&b"1234"[..]));
}

#[test]
fn s2a_c3_scattered_free_space_is_usable_for_one_bigger_record() {
    let mut p = SlottedPage::new(100);
    let slots: Vec<_> = (0..4).map(|i| p.insert(&[i as u8; 20]).unwrap()).collect(); // 4 * 24 = 96 bytes
    assert_eq!(p.insert(&[9; 5]), None);
    p.delete(slots[0]);
    p.delete(slots[2]);
    assert_eq!(p.free_space(), 52);
    assert!(p.insert(&[7; 48]).is_some(), "48 + 4 = 52: two holes of 24 make room for one record of 48");
    assert_eq!(p.get(slots[1]), Some(&[1u8; 20][..]));
    assert_eq!(p.get(slots[3]), Some(&[3u8; 20][..]), "the records that stayed are intact");
}

#[test]
fn s2a_c3_empty_records_cost_only_their_slot_entry() {
    let mut p = SlottedPage::new(8);
    assert_eq!(p.insert(b""), Some(0));
    assert_eq!(p.insert(b""), Some(1));
    assert_eq!(p.insert(b""), None);
    assert_eq!(p.get(0), Some(&b""[..]));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: against a model of slots with a byte budget: lowest free slot, fit by total free space, records intact.
    #[test]
    fn s2a_c3_property_a_page_matches_a_model(size in 8usize..120, ops in proptest::collection::vec((any::<bool>(), 0usize..24, any::<u8>()), 0..60)) {
        let mut p = SlottedPage::new(size);
        let mut m: Vec<Option<Vec<u8>>> = Vec::new();
        for (ins, n, byte) in ops {
            if ins {
                let rec = vec![byte; n];
                let used: usize = m.iter().flatten().map(|r| r.len() + 4).sum();
                let want = if n + 4 > size - used { None } else {
                    let slot = m.iter().position(|s| s.is_none()).unwrap_or_else(|| { m.push(None); m.len() - 1 });
                    m[slot] = Some(rec.clone());
                    Some(slot)
                };
                prop_assert_eq!(p.insert(&rec), want);
            } else {
                let slot = n % (m.len() + 1);
                let want = m.get_mut(slot).is_some_and(|s| s.take().is_some());
                prop_assert_eq!(p.delete(slot), want);
            }
            let used: usize = m.iter().flatten().map(|r| r.len() + 4).sum();
            prop_assert_eq!(p.free_space(), size - used);
            for (i, s) in m.iter().enumerate() {
                prop_assert_eq!(p.get(i), s.as_deref());
            }
        }
    }
}
''')))

CH.append(C("2a-c4", M2A, "93-challenge-prefix-compressed-keys", "build", "Challenge: prefix-compressed keys", "medium", "stages_2a::s2a_c4",
  ["storing sorted keys by what differs from the previous key","decoding untrusted bytes without panicking"],
  ["bytes-endianness-and-views","errors-as-values-with-result","property-testing-and-fuzzing"],
  "`encode_keys` and `decode_keys` in `src/storage/page/key_block.rs`: a block of **sorted** byte-string keys stored as, for each key, how many leading bytes it shares with the previous key, and then the rest. Decoding must reverse it exactly and reject malformed bytes without panicking.",
  "Index keys are often long and alike (`/users/42/orders/1`, `/users/42/orders/2`), and an inner page that fits twice the keys is a shallower tree. Prefix compression is the standard answer. The real work is the decoder: it reads bytes from a disk, and a corrupt page must be an error, not a crash.",
  ["`encode_keys(keys)`: `keys` are sorted, each at most 255 bytes. The encoding is a count (1 byte) then, for each key, `shared` (1 byte, the length of the common prefix with the previous key; 0 for the first), `rest_len` (1 byte), and the rest.","`decode_keys(bytes)` returns the keys, or `None` if the bytes are not exactly a well-formed block (truncated, `shared` longer than the previous key, trailing bytes)."],
  ["`decode_keys(encode_keys(k)) == Some(k)` for every valid sorted `k`.","The encoded length is `1 + sum(2 + (len(key) - shared_with_previous))`.","Decoding never panics, on any bytes."],
  ["Keys that share long prefixes encode smaller than keys that do not.","Any prefix of a valid block (cut short) is rejected.","Decoding then encoding any accepted bytes gives the same bytes."],
  ["[\"apple\", \"apply\", \"banana\"] -> 3 | 0,5,apple | 4,1,y | 0,6,banana"],
  ["Exact bytes for a small block.","Round trips; the size formula.","Malformed input: truncation, bad `shared`, trailing bytes.","A property over random bytes: no panic and re-encoding agrees."],
  src=("src/storage/page/key_block.rs", '''
//! A block of sorted keys with the shared prefix of each key left out.

/// The keys must be sorted and at most 255 bytes each, and there are at most 255 of them.
pub fn encode_keys(keys: &[Vec<u8>]) -> Vec<u8> {
    // @begin 2a-c4
    let mut out = vec![keys.len() as u8];
    let mut prev: &[u8] = &[];
    for k in keys {
        let shared = prev.iter().zip(k).take_while(|(a, b)| a == b).count();
        out.push(shared as u8);
        out.push((k.len() - shared) as u8);
        out.extend_from_slice(&k[shared..]);
        prev = k;
    }
    out
    //~ todo!("2a-c4: the count, then (shared, rest length, rest) for each key")
    // @end
}

/// The keys of a block, or `None` if the bytes are not exactly one well-formed block.
pub fn decode_keys(bytes: &[u8]) -> Option<Vec<Vec<u8>>> {
    // @begin 2a-c4
    let (&count, mut rest) = bytes.split_first()?;
    let mut keys: Vec<Vec<u8>> = Vec::new();
    for _ in 0..count {
        let (&shared, r) = rest.split_first()?;
        let (&len, r) = r.split_first()?;
        let (len, shared) = (len as usize, shared as usize);
        if r.len() < len {
            return None;
        }
        let prev: &[u8] = keys.last().map_or(&[], |k| k.as_slice());
        if shared > prev.len() {
            return None;
        }
        let mut key = prev[..shared].to_vec();
        key.extend_from_slice(&r[..len]);
        keys.push(key);
        rest = &r[len..];
    }
    if rest.is_empty() {
        Some(keys)
    } else {
        None
    }
    //~ todo!("2a-c4: rebuild each key from the previous key's prefix; refuse anything malformed")
    // @end
}
'''),
  test=("tests/stages_2a.rs", '''
use bustub::storage::page::key_block::{decode_keys, encode_keys};

fn k(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[test]
fn s2a_c4_the_exact_bytes_of_a_small_block() {
    let block = encode_keys(&[k("apple"), k("apply"), k("banana")]);
    let mut want = vec![3, 0, 5];
    want.extend_from_slice(b"apple");
    want.extend_from_slice(&[4, 1]);
    want.extend_from_slice(b"y");
    want.extend_from_slice(&[0, 6]);
    want.extend_from_slice(b"banana");
    assert_eq!(block, want);
}

#[test]
fn s2a_c4_blocks_round_trip_including_empty_ones_and_equal_keys() {
    for keys in [vec![], vec![k("")], vec![k("a"), k("a"), k("ab")], vec![k("x"); 5]] {
        assert_eq!(decode_keys(&encode_keys(&keys)), Some(keys.clone()), "{keys:?}");
    }
}

#[test]
fn s2a_c4_alike_keys_take_less_room_than_unlike_ones() {
    let alike: Vec<_> = (0..20).map(|i| k(&format!("/users/42/orders/{i:03}"))).collect();
    let plain: usize = alike.iter().map(|x| x.len()).sum();
    assert!(encode_keys(&alike).len() < plain / 2, "20 keys sharing 16 bytes should be well under half their plain size");
}

#[test]
fn s2a_c4_malformed_blocks_are_rejected_not_panicked_on() {
    assert_eq!(decode_keys(&[]), None, "no count");
    assert_eq!(decode_keys(&[2, 0, 1, b'a']), None, "the second key is missing");
    assert_eq!(decode_keys(&[1, 3, 1, b'a']), None, "shared is longer than the (empty) previous key");
    assert_eq!(decode_keys(&[1, 0, 5, b'a']), None, "truncated rest");
    assert_eq!(decode_keys(&[1, 0, 1, b'a', 9]), None, "trailing byte");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: round trip and the size formula, for any sorted keys.
    #[test]
    fn s2a_c4_property_round_trip_and_size(mut keys in proptest::collection::vec(proptest::collection::vec(0u8..4, 0..12), 0..20)) {
        keys.sort();
        let block = encode_keys(&keys);
        prop_assert_eq!(decode_keys(&block), Some(keys.clone()));
        let mut want = 1;
        let mut prev: &[u8] = &[];
        for key in &keys {
            let shared = prev.iter().zip(key).take_while(|(a, b)| a == b).count();
            want += 2 + key.len() - shared;
            prev = key;
        }
        prop_assert_eq!(block.len(), want);
        for cut in 0..block.len() {
            prop_assert_eq!(decode_keys(&block[..cut]), None, "a block cut at {} must be rejected", cut);
        }
    }

    /// Property: decoding any bytes never panics, and what it accepts re-encodes to the same bytes.
    #[test]
    fn s2a_c4_property_arbitrary_bytes_decode_or_fail_cleanly(bytes in proptest::collection::vec(any::<u8>(), 0..40)) {
        if let Some(keys) = decode_keys(&bytes) {
            prop_assert_eq!(encode_keys(&keys), bytes);
        }
    }
}
''')))

CH.append(C("2b-c3", M2B, "92-challenge-consistent-hashing", "build", "Challenge: consistent hashing", "medium", "stages_2b::s2b_c3",
  ["placing keys on nodes so that adding or removing a node moves few keys","virtual nodes for balance"],
  ["hash-functions-and-murmur3","extendible-hashing","model-based-testing"],
  "`HashRing` in `src/container/hash/hash_ring.rs`: a ring that maps keys to named nodes. Each node is placed on the ring at `vnodes` points (virtual nodes); a key belongs to the node at the first point at or after the key's own point, wrapping around. Adding or removing a node must move only the keys that have to move.",
  "`hash(key) % n` sends almost every key to a different node when `n` changes. Distributed caches, sharded databases and object stores need the opposite: when a node joins, it takes a slice from each neighbour and nothing else moves. This is the core of that idea, in a form small enough to test exhaustively.",
  ["`add_node(name)` places `vnodes` points at `hash(name, i)` for `i in 0..vnodes`; false if the node is already there.","`remove_node(name)` removes its points; false if unknown.","`node_for(key)` is the node owning the first point at or after `hash(key)` (wrapping), `None` on an empty ring.","`hash64` is given, so that results are deterministic."],
  ["Every key maps to a node that is currently on the ring.","The same ring and key always give the same node.","With one node, every key maps to it."],
  ["Adding a node moves keys only **to** the new node; no key moves between two old nodes.","Removing a node moves only the keys that were on it.","Adding a node and removing it again restores every key's owner.","With enough virtual nodes the keys spread within a factor of about 2 of an even split."],
  ["ring {A, B}: node_for(k) is A or B; add C: each key is still on its old node or on C","remove C: every key is back where it was"],
  ["Mapping, wrapping, empty ring.","Keys that move when a node is added or removed.","Balance with many virtual nodes."],
  src=("src/container/hash/hash_ring.rs", '''
//! A consistent-hashing ring with virtual nodes.

use std::collections::BTreeMap;

/// A deterministic 64-bit mix of a key and a salt (so that tests do not depend on a hasher's seed).
pub fn hash64(key: u64, salt: u64) -> u64 {
    let mut x = key ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn name_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

pub struct HashRing {
    // @begin 2b-c3
    vnodes: u32,
    /// point on the ring -> node name
    points: BTreeMap<u64, String>,
    nodes: std::collections::BTreeSet<String>,
    //~ _ring: (),
    // @end
}

impl HashRing {
    pub fn new(vnodes: u32) -> HashRing {
        // @begin 2b-c3
        HashRing { vnodes: vnodes.max(1), points: BTreeMap::new(), nodes: Default::default() }
        //~ todo!("2b-c3: an empty ring")
        // @end
    }

    pub fn add_node(&mut self, name: &str) -> bool {
        // @begin 2b-c3
        if !self.nodes.insert(name.to_owned()) {
            return false;
        }
        for i in 0..self.vnodes {
            self.points.insert(hash64(name_hash(name), i as u64), name.to_owned());
        }
        true
        //~ todo!("2b-c3: put the node on the ring at `vnodes` points")
        // @end
    }

    pub fn remove_node(&mut self, name: &str) -> bool {
        // @begin 2b-c3
        if !self.nodes.remove(name) {
            return false;
        }
        self.points.retain(|_, n| n != name);
        true
        //~ todo!("2b-c3: take the node's points off the ring")
        // @end
    }

    pub fn node_for(&self, key: u64) -> Option<&str> {
        // @begin 2b-c3
        let h = hash64(key, 0xA5A5);
        self.points.range(h..).next().or_else(|| self.points.iter().next()).map(|(_, n)| n.as_str())
        //~ todo!("2b-c3: the node at the first point at or after the key's point, wrapping")
        // @end
    }

    pub fn node_count(&self) -> usize {
        // @begin 2b-c3
        self.nodes.len()
        //~ todo!("2b-c3: how many nodes")
        // @end
    }
}
'''),
  test=("tests/stages_2b.rs", '''
use bustub::container::hash::hash_ring::HashRing;
use std::collections::HashMap;

fn owners(r: &HashRing, n: u64) -> Vec<String> {
    (0..n).map(|k| r.node_for(k).unwrap().to_owned()).collect()
}

#[test]
fn s2b_c3_an_empty_ring_owns_nothing_and_one_node_owns_everything() {
    let mut r = HashRing::new(8);
    assert_eq!(r.node_for(5), None);
    assert!(r.add_node("a"));
    assert!(!r.add_node("a"));
    assert!((0..200).all(|k| r.node_for(k) == Some("a")));
    assert_eq!(r.node_count(), 1);
}

#[test]
fn s2b_c3_adding_a_node_moves_keys_only_to_the_new_node() {
    let mut r = HashRing::new(32);
    for n in ["a", "b", "c"] {
        r.add_node(n);
    }
    let before = owners(&r, 2000);
    r.add_node("d");
    let after = owners(&r, 2000);
    let moved = before.iter().zip(&after).filter(|(b, a)| b != a).count();
    assert!(before.iter().zip(&after).all(|(b, a)| b == a || a == "d"), "a key moved between two old nodes");
    assert!(moved > 0 && moved < 1000, "the new node takes a slice, not everything: {moved} of 2000 moved");
}

#[test]
fn s2b_c3_removing_a_node_moves_only_its_keys_and_adding_it_back_restores_them() {
    let mut r = HashRing::new(32);
    for n in ["a", "b", "c", "d"] {
        r.add_node(n);
    }
    let before = owners(&r, 2000);
    assert!(r.remove_node("c"));
    assert!(!r.remove_node("c"));
    let during = owners(&r, 2000);
    for (b, d) in before.iter().zip(&during) {
        assert!(b == d || b == "c", "key moved although its node stayed");
        assert_ne!(d, "c");
    }
    r.add_node("c");
    assert_eq!(owners(&r, 2000), before, "the ring is a function of its nodes, not of the order they were added in");
}

#[test]
fn s2b_c3_many_virtual_nodes_spread_the_keys() {
    let mut r = HashRing::new(100);
    for n in ["a", "b", "c", "d"] {
        r.add_node(n);
    }
    let mut count: HashMap<String, usize> = HashMap::new();
    for o in owners(&r, 10_000) {
        *count.entry(o).or_default() += 1;
    }
    assert_eq!(count.len(), 4);
    assert!(count.values().all(|&c| c > 1250 && c < 5000), "uneven spread: {count:?}");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: for any sequence of joins and leaves, only the keys of the node that changed move, and every key has an owner on the ring.
    #[test]
    fn s2b_c3_property_membership_changes_move_only_their_keys(ops in proptest::collection::vec((any::<bool>(), 0u8..5), 1..15)) {
        let mut r = HashRing::new(16);
        let mut members: Vec<String> = Vec::new();
        for (add, n) in ops {
            let name = format!("n{n}");
            let before: Vec<Option<String>> = (0..300).map(|k| r.node_for(k).map(str::to_owned)).collect();
            let changed = if add { let c = r.add_node(&name); if c { members.push(name.clone()); } c } else { let c = r.remove_node(&name); if c { members.retain(|m| m != &name); } c };
            let after: Vec<Option<String>> = (0..300).map(|k| r.node_for(k).map(str::to_owned)).collect();
            prop_assert_eq!(r.node_count(), members.len());
            for (b, a) in before.iter().zip(&after) {
                if let Some(a) = a { prop_assert!(members.contains(a)); }
                if !changed { prop_assert_eq!(b, a); }
                else if add { if let (Some(b), Some(a)) = (b, a) { prop_assert!(a == b || a == &name, "moved from {} to {} on adding {}", b, a, name); } }
                else if let Some(b) = b { if b != &name { prop_assert_eq!(Some(b), a.as_ref(), "a key of a node that stayed moved"); } }
            }
        }
    }
}
''')))

CH.append(C("2b-c4", M2B, "93-challenge-cuckoo-hashing", "build", "Challenge: cuckoo hashing", "medium", "stages_2b::s2b_c4",
  ["a hash table where every key has exactly two possible homes","displacing keys and growing when displacement cycles"],
  ["extendible-hashing","hash-functions-and-murmur3","model-based-testing"],
  "`CuckooSet` in `src/container/hash/cuckoo_set.rs`: a hash set with **two tables** and two hash functions; a key is always in slot `h1(key)` of table 0 or slot `h2(key)` of table 1, so a lookup looks in at most two places. An insert into an occupied slot kicks the old key out to its other home, and so on; if that goes on too long the table grows and everything is placed again.",
  "Chaining and open addressing can degrade to long probes on a bad day. Cuckoo hashing gives a hard bound of two probes for every lookup, which is why it appears in network hardware and in-memory stores, and the price is a more careful insert. The invariant is crisp, which makes it a good test target.",
  ["`insert(key)` returns false if the key is already present; otherwise places it, displacing keys as needed, and growing when displacement exceeds `max_kicks`.","`contains(key)` looks only at the key's two homes; `remove(key)` clears it.","`homes(key)` returns the two (table, slot) positions for the table's current size; `position(key)` says where the key actually is."],
  ["Every stored key is at one of its two homes, `homes(key)`.","No key is stored twice; the number of keys is `len()`.","The load factor never exceeds one half after an insert."],
  ["`contains` is true exactly for the keys inserted and not removed.","Growing never loses a key.","Every lookup inspects at most two slots."],
  ["insert 1..1000 -> all contained, every key at one of its two homes","remove a key -> not contained, the others still are"],
  ["Insert, contains, remove; duplicates.","Many inserts force displacement and growth.","Every key stays at one of its homes.","A property against a `HashSet`."],
  src=("src/container/hash/cuckoo_set.rs", '''
//! A cuckoo hash set: two tables, two hash functions, two possible homes for every key.

fn mix(mut x: u64, seed: u64) -> u64 {
    x ^= seed;
    x = (x ^ (x >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    x = (x ^ (x >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

pub struct CuckooSet {
    // @begin 2b-c4
    tables: [Vec<Option<u64>>; 2],
    len: usize,
    max_kicks: usize,
    //~ _cuckoo: (),
    // @end
}

impl CuckooSet {
    pub fn new() -> CuckooSet {
        // @begin 2b-c4
        CuckooSet { tables: [vec![None; 8], vec![None; 8]], len: 0, max_kicks: 32 }
        //~ todo!("2b-c4: two small tables")
        // @end
    }

    pub fn len(&self) -> usize {
        // @begin 2b-c4
        self.len
        //~ todo!("2b-c4: how many keys")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The two homes of `key` as (table, slot), for the current table size.
    pub fn homes(&self, key: u64) -> [(usize, usize); 2] {
        // @begin 2b-c4
        let n = self.tables[0].len() as u64;
        [(0, (mix(key, 0x1234_5678) % n) as usize), (1, (mix(key, 0x9abc_def0) % n) as usize)]
        //~ todo!("2b-c4: slot h1(key) of table 0 and h2(key) of table 1")
        // @end
    }

    /// Where `key` is stored right now.
    pub fn position(&self, key: u64) -> Option<(usize, usize)> {
        // @begin 2b-c4
        self.homes(key).into_iter().find(|&(t, s)| self.tables[t][s] == Some(key))
        //~ todo!("2b-c4: look only at the two homes")
        // @end
    }

    pub fn contains(&self, key: u64) -> bool {
        self.position(key).is_some()
    }

    pub fn insert(&mut self, key: u64) -> bool {
        // @begin 2b-c4
        if self.contains(key) {
            return false;
        }
        self.len += 1;
        if let Err(loose) = self.place(key) {
            let mut keys = self.stored();
            keys.push(loose);
            self.rehash(keys);
        }
        if self.len > self.tables[0].len() {
            let keys = self.stored();
            self.rehash(keys);
        }
        true
        //~ todo!("2b-c4: place the key, displacing others; grow and place everything again if that takes too long or the tables are half full")
        // @end
    }

    // @begin 2b-c4
    fn stored(&self) -> Vec<u64> {
        self.tables.iter().flatten().flatten().copied().collect()
    }

    /// Walks the displacement chain; `Err(k)` names the key that was left without a home after `max_kicks` displacements.
    fn place(&mut self, key: u64) -> Result<(), u64> {
        let mut cur = key;
        let mut t = 0;
        for _ in 0..self.max_kicks {
            let s = self.homes(cur)[t].1;
            match self.tables[t][s].replace(cur) {
                None => return Ok(()),
                Some(old) => {
                    cur = old;
                    t = 1 - t;
                }
            }
        }
        Err(cur)
    }

    /// Doubles the tables and places every key again, doubling further if a placement fails.
    fn rehash(&mut self, mut keys: Vec<u64>) {
        loop {
            let n = self.tables[0].len() * 2;
            self.tables = [vec![None; n], vec![None; n]];
            let mut failed = None;
            for (i, &k) in keys.iter().enumerate() {
                if let Err(loose) = self.place(k) {
                    failed = Some((i, loose));
                    break;
                }
            }
            match failed {
                None => return,
                Some((i, loose)) => {
                    let mut all = self.stored();
                    all.push(loose);
                    all.extend_from_slice(&keys[i + 1..]);
                    keys = all;
                }
            }
        }
    }
    //~ // TODO(2b-c4): helpers of your own (displacement walk, growing)
    // @end

    pub fn remove(&mut self, key: u64) -> bool {
        // @begin 2b-c4
        match self.position(key) {
            Some((t, s)) => {
                self.tables[t][s] = None;
                self.len -= 1;
                true
            }
            None => false,
        }
        //~ todo!("2b-c4: clear the key's slot")
        // @end
    }
}

impl Default for CuckooSet {
    fn default() -> Self {
        CuckooSet::new()
    }
}
'''),
  test=("tests/stages_2b.rs", '''
use bustub::container::hash::cuckoo_set::CuckooSet;
use std::collections::HashSet;

#[test]
fn s2b_c4_inserted_keys_are_found_and_sit_at_one_of_their_two_homes() {
    let mut s = CuckooSet::new();
    for k in 0..1000u64 {
        assert!(s.insert(k * 7919));
    }
    assert_eq!(s.len(), 1000);
    for k in 0..1000u64 {
        let key = k * 7919;
        assert!(s.contains(key));
        let pos = s.position(key).unwrap();
        assert!(s.homes(key).contains(&pos), "key {key} is not at one of its homes");
    }
    assert!(!s.contains(1));
}

#[test]
fn s2b_c4_duplicates_and_removals() {
    let mut s = CuckooSet::new();
    assert!(s.insert(5));
    assert!(!s.insert(5));
    assert!(s.remove(5));
    assert!(!s.remove(5));
    assert!(s.is_empty() && !s.contains(5));
}

#[test]
fn s2b_c4_growing_never_loses_a_key() {
    let mut s = CuckooSet::new();
    let keys: Vec<u64> = (0..3000).map(|i| i * 2_654_435_761).collect();
    for (n, &k) in keys.iter().enumerate() {
        s.insert(k);
        if n % 211 == 0 {
            assert!(keys[..=n].iter().all(|&x| s.contains(x)), "a key went missing after insert {n}");
        }
    }
}

#[test]
fn s2b_c4_keys_that_collide_in_a_small_table_are_displaced_not_dropped() {
    let mut s = CuckooSet::new();
    for k in 0..40u64 {
        s.insert(k);
    }
    assert_eq!(s.len(), 40);
    assert!((0..40).all(|k| s.contains(k)));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: against a `HashSet`, and every key always at one of its homes.
    #[test]
    fn s2b_c4_property_a_cuckoo_set_is_a_set(ops in proptest::collection::vec((any::<bool>(), 0u64..120), 0..200)) {
        let mut s = CuckooSet::new();
        let mut m = HashSet::new();
        for (ins, k) in ops {
            if ins { prop_assert_eq!(s.insert(k), m.insert(k)); } else { prop_assert_eq!(s.remove(k), m.remove(&k)); }
            prop_assert_eq!(s.len(), m.len());
        }
        for k in 0..120u64 {
            prop_assert_eq!(s.contains(k), m.contains(&k));
            if let Some(p) = s.position(k) { prop_assert!(s.homes(k).contains(&p)); }
        }
    }
}
''')))

CH.append(C("2b-c5", M2B, "94-challenge-when-to-shrink", "debug", "Challenge: when to shrink", "easy", "stages_2b::s2b_c5",
  ["finding a wrong quantifier in a structural precondition","stating when a directory may be halved"],
  ["extendible-hashing","checking-invariants"],
  "`src/container/hash/directory_shrink.rs` decides whether an extendible hash directory can be halved and builds the halved directory. It looks right, and it allows a shrink that would send keys to the wrong bucket. Find the bug and fix it.",
  "Shrinking is the mirror image of doubling and the less-tested half of the structure. The rule is one word of logic: the directory can be halved only when **no** bucket needs the full depth. Getting 'any' and 'all' mixed up produces a table that loses keys only after a delete, much later.",
  ["`can_shrink(local_depths, global_depth)` is true exactly when the global depth is above 0 and **every** bucket's local depth is below it.","`shrink_slots(slots)` takes a directory whose two halves are identical (the upper half mirrors the lower) and returns the lower half, or `None` if the halves differ or the length is odd."],
  ["A directory that can shrink has two identical halves.","Shrinking by one never makes a local depth exceed the new global depth."],
  ["Adding a bucket at full depth makes `can_shrink` false.","If `can_shrink` is true, `shrink_slots` of a doubled directory returns the original."],
  ["depths [1,1,0], global 2 -> true","depths [2,1], global 2 -> false","global 0 -> false"],
  ["The rule for typical and edge cases.","A property: `can_shrink` equals 'global above 0 and all depths below it'.","`shrink_slots` is the inverse of doubling."],
  src=("src/container/hash/directory_shrink.rs", '''
//! When an extendible hash directory may be halved.

/// True when the directory can be halved: the global depth is above 0 and no bucket uses all of it.
pub fn can_shrink(local_depths: &[u32], global_depth: u32) -> bool {
    // @begin 2b-c5
    global_depth > 0 && local_depths.iter().all(|&d| d < global_depth)
    //~ global_depth > 0 && local_depths.iter().any(|&d| d < global_depth)
    // @end
}

/// The directory after halving: `slots` must be two identical halves; returns the first half, or `None` otherwise.
pub fn shrink_slots(slots: &[usize]) -> Option<Vec<usize>> {
    if slots.len() % 2 != 0 || slots.is_empty() {
        return None;
    }
    let (lo, hi) = slots.split_at(slots.len() / 2);
    if lo == hi {
        Some(lo.to_vec())
    } else {
        None
    }
}
'''),
  test=("tests/stages_2b.rs", '''
use bustub::container::hash::directory_shrink::{can_shrink, shrink_slots};

#[test]
fn s2b_c5_a_directory_shrinks_only_when_no_bucket_uses_the_full_depth() {
    assert!(can_shrink(&[1, 1, 0], 2));
    assert!(!can_shrink(&[2, 1], 2), "one bucket at full depth needs the whole directory");
    assert!(!can_shrink(&[1, 1, 2], 2));
}

#[test]
fn s2b_c5_a_depth_zero_directory_cannot_shrink() {
    assert!(!can_shrink(&[0], 0));
    assert!(!can_shrink(&[], 0));
}

#[test]
fn s2b_c5_all_buckets_shallow_is_enough() {
    assert!(can_shrink(&[0, 0, 0, 0], 3));
    assert!(can_shrink(&[2, 2, 2], 3));
}

#[test]
fn s2b_c5_shrink_slots_halves_a_mirrored_directory_and_refuses_others() {
    assert_eq!(shrink_slots(&[0, 1, 0, 1]), Some(vec![0, 1]));
    assert_eq!(shrink_slots(&[0, 1, 2, 3]), None);
    assert_eq!(shrink_slots(&[0, 1, 0]), None);
    assert_eq!(shrink_slots(&[]), None);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: the rule is exactly 'the global depth is above 0 and every local depth is below it'.
    #[test]
    fn s2b_c5_property_the_shrink_rule(depths in proptest::collection::vec(0u32..5, 0..8), global in 0u32..5) {
        prop_assert_eq!(can_shrink(&depths, global), global > 0 && depths.iter().all(|&d| d < global));
    }

    /// Property: doubling a directory and shrinking it gives it back.
    #[test]
    fn s2b_c5_property_shrink_undoes_doubling(slots in proptest::collection::vec(0usize..6, 1..10)) {
        let mut doubled = slots.clone();
        doubled.extend(&slots);
        prop_assert_eq!(shrink_slots(&doubled), Some(slots));
    }
}
''')))

CH.append(C("2c-c3", M2C, "92-challenge-prefix-scans", "build", "Challenge: prefix scans", "easy", "stages_2c::s2c_c3",
  ["turning a prefix match into a half-open key range","the one case where no upper bound exists"],
  ["range-scans-and-the-leaf-chain","routing-with-separator-keys"],
  "`prefix_end(prefix)` in `src/storage/index/prefix_range.rs`: the smallest byte string that is greater than **every** string that starts with `prefix`, or `None` if there is none (the prefix is empty or all `0xFF`). With it, `LIKE 'abc%'` becomes the range `[\"abc\", prefix_end(\"abc\"))` on an ordered index.",
  "A B+ tree answers ranges, not patterns. Turning a string prefix into a range is how `LIKE 'abc%'`, a path prefix or a tuple prefix of a composite key becomes an index scan. The calculation is a few lines and has one famous edge: `0xFF` bytes carry.",
  ["`prefix_end(p)` strips trailing `0xFF` bytes and adds one to the last remaining byte; `None` when nothing remains.","Byte strings compare lexicographically."],
  ["`prefix <= prefix_end(prefix)` (when it exists) and they are not equal.","`prefix_end` is never itself a string with that prefix."],
  ["A key `k` starts with `p` exactly when `p <= k` and (`prefix_end(p)` is `None` or `k < prefix_end(p)`).","`prefix_end` is the *smallest* such bound: nothing strictly between the prefixed strings and it starts with `p`.","A longer prefix gives a bound no larger than a shorter one's."],
  ["\"abc\" -> \"abd\"","[0x61, 0xFF] -> [0x62]","[0xFF, 0xFF] -> None","[] -> None"],
  ["Ordinary prefixes, carries and the unbounded cases.","A property: membership in the range equals `starts_with`."],
  src=("src/storage/index/prefix_range.rs", '''
//! The upper bound of a prefix scan.

/// The smallest byte string greater than every string that starts with `prefix`; `None` if there is no such string.
pub fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    // @begin 2c-c3
    let mut end = prefix.to_vec();
    while let Some(&last) = end.last() {
        if last == 0xFF {
            end.pop();
        } else {
            *end.last_mut().unwrap() += 1;
            return Some(end);
        }
    }
    None
    //~ todo!("2c-c3: drop trailing 0xFF bytes, add one to the last byte left")
    // @end
}
'''),
  test=("tests/stages_2c.rs", '''
use bustub::storage::index::prefix_range::prefix_end;

#[test]
fn s2c_c3_the_next_string_after_a_prefix() {
    assert_eq!(prefix_end(b"abc"), Some(b"abd".to_vec()));
    assert_eq!(prefix_end(&[0x61]), Some(vec![0x62]));
}

#[test]
fn s2c_c3_trailing_ff_bytes_carry() {
    assert_eq!(prefix_end(&[0x61, 0xFF]), Some(vec![0x62]));
    assert_eq!(prefix_end(&[0x61, 0xFF, 0xFF]), Some(vec![0x62]));
    assert_eq!(prefix_end(&[0x00, 0xFF]), Some(vec![0x01]));
}

#[test]
fn s2c_c3_no_bound_when_every_byte_is_ff_or_there_are_none() {
    assert_eq!(prefix_end(&[0xFF, 0xFF]), None);
    assert_eq!(prefix_end(&[]), None);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a key is in `[prefix, prefix_end)` exactly when it starts with the prefix.
    #[test]
    fn s2c_c3_property_the_range_is_the_prefix(prefix in proptest::collection::vec(prop_oneof![Just(0u8), Just(1u8), Just(254u8), Just(255u8)], 0..4), key in proptest::collection::vec(prop_oneof![Just(0u8), Just(1u8), Just(254u8), Just(255u8)], 0..6)) {
        let in_range = key >= prefix && prefix_end(&prefix).is_none_or(|e| key < e);
        prop_assert_eq!(in_range, key.starts_with(&prefix), "prefix {:?} key {:?} end {:?}", prefix, key, prefix_end(&prefix));
    }

    /// Property: the bound is the smallest possible: appending anything to the prefix stays below it.
    #[test]
    fn s2c_c3_property_the_bound_is_not_too_large(prefix in proptest::collection::vec(any::<u8>(), 1..5)) {
        if let Some(e) = prefix_end(&prefix) {
            let mut longest = prefix.clone();
            longest.extend([0xFF, 0xFF, 0xFF]);
            prop_assert!(longest < e);
            prop_assert!(!e.starts_with(&prefix));
        }
    }
}
''')))

CH.append(C("2c-c4", M2C, "93-challenge-merging-sorted-runs", "build", "Challenge: merging sorted runs", "medium", "stages_2c::s2c_c4",
  ["merging many sorted inputs lazily with a heap","keeping equal keys in the order of their inputs"],
  ["ordered-sets-as-priority-queues","range-scans-and-the-leaf-chain","iterators-and-closures"],
  "`KMerge` in `src/storage/index/kmerge.rs`: an iterator that merges any number of sorted runs into one sorted stream, lazily (it never builds the whole output), yielding `(key, run_index)`. Among equal keys, the lower run index comes first.",
  "Merging sorted inputs is under a range scan over several indexes, under the merge half of an external sort, under a compaction in an LSM tree and under a `UNION ALL ... ORDER BY`. A heap makes it `O(log k)` per item, and the stability rule is what makes results deterministic.",
  ["`KMerge::new(runs)` takes `Vec<Vec<i64>>`, each run sorted ascending.","`next()` yields the smallest remaining key and the index of the run it came from; ties go to the lower run index, and within a run in order.","It does not copy all runs into one vector and sort."],
  ["The output is sorted by (key, run index).","Every input element is yielded exactly once.","Each run's elements come out in their original order."],
  ["The output equals concatenating all runs and stable-sorting by key.","Adding an empty run changes nothing.","Merging the output of two merges equals merging all four runs."],
  ["[[1,4],[2,4]] -> (1,0) (2,1) (4,0) (4,1)"],
  ["Small merges and ties.","Empty runs and no runs.","A property against a stable sort."],
  src=("src/storage/index/kmerge.rs", '''
//! A lazy k-way merge of sorted runs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct KMerge {
    // @begin 2c-c4
    runs: Vec<std::vec::IntoIter<i64>>,
    heap: BinaryHeap<Reverse<(i64, usize)>>,
    //~ _merge: (),
    // @end
}

impl KMerge {
    pub fn new(runs: Vec<Vec<i64>>) -> KMerge {
        // @begin 2c-c4
        let mut iters: Vec<_> = runs.into_iter().map(|r| r.into_iter()).collect();
        let mut heap = BinaryHeap::new();
        for (i, it) in iters.iter_mut().enumerate() {
            if let Some(k) = it.next() {
                heap.push(Reverse((k, i)));
            }
        }
        KMerge { runs: iters, heap }
        //~ todo!("2c-c4: a heap holding the first key of each run")
        // @end
    }
}

impl Iterator for KMerge {
    type Item = (i64, usize);

    fn next(&mut self) -> Option<(i64, usize)> {
        // @begin 2c-c4
        let Reverse((key, run)) = self.heap.pop()?;
        if let Some(next) = self.runs[run].next() {
            self.heap.push(Reverse((next, run)));
        }
        Some((key, run))
        //~ todo!("2c-c4: pop the smallest, refill from the run it came from")
        // @end
    }
}
'''),
  test=("tests/stages_2c.rs", '''
use bustub::storage::index::kmerge::KMerge;

#[test]
fn s2c_c4_two_runs_with_a_tie_go_in_run_order() {
    let out: Vec<_> = KMerge::new(vec![vec![1, 4], vec![2, 4]]).collect();
    assert_eq!(out, vec![(1, 0), (2, 1), (4, 0), (4, 1)]);
}

#[test]
fn s2c_c4_empty_runs_and_no_runs() {
    assert_eq!(KMerge::new(vec![]).count(), 0);
    assert_eq!(KMerge::new(vec![vec![], vec![]]).count(), 0);
    let out: Vec<_> = KMerge::new(vec![vec![], vec![3], vec![]]).collect();
    assert_eq!(out, vec![(3, 1)]);
}

#[test]
fn s2c_c4_runs_of_very_different_lengths() {
    let big: Vec<i64> = (0..1000).map(|i| i * 2).collect();
    let out: Vec<_> = KMerge::new(vec![big, vec![1, 3, 5], vec![-5]]).collect();
    assert_eq!(out.len(), 1004);
    assert!(out.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(out[0], (-5, 2));
}

#[test]
fn s2c_c4_it_is_lazy() {
    // 100 runs of a million elements each would not fit in memory as one vector, but taking three items must be cheap
    struct Count;
    let _ = Count;
    let runs: Vec<Vec<i64>> = (0..100).map(|r| (0..2000).map(|i| i * 100 + r).collect()).collect();
    let first3: Vec<_> = KMerge::new(runs).take(3).collect();
    assert_eq!(first3, vec![(0, 0), (1, 1), (2, 2)]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: the merge is a stable sort of the concatenation, by key then run.
    #[test]
    fn s2c_c4_property_a_merge_is_a_stable_sort(mut runs in proptest::collection::vec(proptest::collection::vec(-8i64..8, 0..10), 0..6)) {
        for r in &mut runs { r.sort(); }
        let mut want: Vec<(i64, usize)> = runs.iter().enumerate().flat_map(|(i, r)| r.iter().map(move |&k| (k, i))).collect();
        want.sort_by_key(|&(k, _)| k); // stable: equal keys stay in run order
        prop_assert_eq!(KMerge::new(runs).collect::<Vec<_>>(), want);
    }
}
''')))

CH.append(C("2c-c5", M2C, "94-challenge-borrow-or-merge", "debug", "Challenge: borrow or merge", "easy", "stages_2c::s2c_c5",
  ["finding an order-of-checks bug in a rebalancing decision"],
  ["splitting-and-promoting","routing-with-separator-keys","checking-invariants"],
  "`after_delete` in `src/storage/index/rebalance.rs` decides what to do with a B+ tree node that has fallen below its minimum: borrow a key from a sibling, or merge with one. It looks right, and it merges when borrowing would have been enough. Find the bug and fix it.",
  "Merging is the more expensive and more disruptive repair: it removes a node and a separator from the parent, and may cascade upwards. A tree that merges whenever it *can* instead of only when it *must* is correct, slow, and shaped differently from the one the tests describe. The fix is in the order of two checks.",
  ["Given the node's key count, the minimum and maximum, and the key counts of its left and right siblings (if any):","Nothing if the node has at least `min` keys.","Otherwise borrow from the **left** sibling if it has more than `min`, else from the **right** if it has more than `min`.","Otherwise merge with the left sibling if it exists and the two fit in `max`, else with the right if it exists and they fit; `Underfull` if none applies (the root's case)."],
  ["A merge result never exceeds `max` keys.","A borrow leaves the sibling with at least `min` keys.","The decision never merges while a borrow is possible."],
  ["Adding keys to a sibling can only change a merge into a borrow, never the reverse.","With no siblings and an underfull node the answer is `Underfull`."],
  ["min 2, max 4, node 1, left 3 -> BorrowLeft","node 1, left 2, right 3 -> BorrowRight","node 1, left 2, right 2 -> MergeLeft"],
  ["Each branch.","Borrow preferred over merge, left preferred over right.","A property over all small counts."],
  src=("src/storage/index/rebalance.rs", '''
//! What to do with a node that has too few keys after a delete.

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Nothing,
    BorrowLeft,
    BorrowRight,
    MergeLeft,
    MergeRight,
    /// No sibling can help (the node is the only child).
    Underfull,
}

pub fn after_delete(node: usize, min: usize, max: usize, left: Option<usize>, right: Option<usize>) -> Action {
    if node >= min {
        return Action::Nothing;
    }
    // @begin 2c-c5
    if left.is_some_and(|l| l > min) {
        return Action::BorrowLeft;
    }
    if right.is_some_and(|r| r > min) {
        return Action::BorrowRight;
    }
    if left.is_some_and(|l| l + node <= max) {
        return Action::MergeLeft;
    }
    if right.is_some_and(|r| r + node <= max) {
        return Action::MergeRight;
    }
    //~ if left.is_some_and(|l| l + node <= max) {
    //~     return Action::MergeLeft;
    //~ }
    //~ if right.is_some_and(|r| r + node <= max) {
    //~     return Action::MergeRight;
    //~ }
    //~ if left.is_some_and(|l| l > min) {
    //~     return Action::BorrowLeft;
    //~ }
    //~ if right.is_some_and(|r| r > min) {
    //~     return Action::BorrowRight;
    //~ }
    // @end
    Action::Underfull
}
'''),
  test=("tests/stages_2c.rs", '''
use bustub::storage::index::rebalance::{after_delete, Action};

#[test]
fn s2c_c5_a_node_with_enough_keys_needs_nothing() {
    assert_eq!(after_delete(2, 2, 4, Some(2), Some(4)), Action::Nothing);
    assert_eq!(after_delete(4, 2, 4, None, None), Action::Nothing);
}

#[test]
fn s2c_c5_borrowing_is_preferred_to_merging() {
    assert_eq!(after_delete(1, 2, 4, Some(3), Some(2)), Action::BorrowLeft, "the left sibling has a key to spare, and merging would also fit");
    assert_eq!(after_delete(1, 2, 4, Some(2), Some(3)), Action::BorrowRight);
}

#[test]
fn s2c_c5_the_left_sibling_is_tried_first() {
    assert_eq!(after_delete(1, 2, 4, Some(3), Some(4)), Action::BorrowLeft);
    assert_eq!(after_delete(1, 2, 4, None, Some(3)), Action::BorrowRight);
}

#[test]
fn s2c_c5_merging_only_when_no_sibling_can_spare_a_key() {
    assert_eq!(after_delete(1, 2, 4, Some(2), Some(2)), Action::MergeLeft);
    assert_eq!(after_delete(1, 2, 4, None, Some(2)), Action::MergeRight);
}

#[test]
fn s2c_c5_a_node_alone_stays_underfull() {
    assert_eq!(after_delete(1, 2, 4, None, None), Action::Underfull);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: never merge while a borrow is possible, a merge always fits, and a borrow leaves the sibling above the minimum.
    #[test]
    fn s2c_c5_property_the_decision_obeys_the_rules(min in 1usize..4, extra in 0usize..4, node in 0usize..6, left in proptest::option::of(0usize..9), right in proptest::option::of(0usize..9)) {
        let max = 2 * min + extra;
        let a = after_delete(node, min, max, left, right);
        let can_borrow = left.is_some_and(|l| l > min) || right.is_some_and(|r| r > min);
        if node >= min { prop_assert_eq!(a, Action::Nothing); return Ok(()); }
        match a {
            Action::BorrowLeft => prop_assert!(left.unwrap() > min),
            Action::BorrowRight => { prop_assert!(right.unwrap() > min); prop_assert!(left.is_none_or(|l| l <= min), "the left sibling could have been used"); }
            Action::MergeLeft | Action::MergeRight => {
                prop_assert!(!can_borrow, "merged although a borrow was possible");
                let sib = if a == Action::MergeLeft { left.unwrap() } else { right.unwrap() };
                prop_assert!(sib + node <= max);
            }
            Action::Underfull => prop_assert!(!can_borrow && left.is_none_or(|l| l + node > max) && right.is_none_or(|r| r + node > max)),
            Action::Nothing => prop_assert!(false),
        }
    }
}
''')))

CH.append(C("2d-c2", M2D, "91-challenge-a-tombstone-leaf", "build", "Challenge: a tombstone leaf", "medium", "stages_2d::s2d_c2",
  ["deleting by marking, and reviving a marked slot","a compaction rule driven by the share of dead entries"],
  ["range-scans-and-the-leaf-chain","slot-allocation-and-invariants","model-based-testing"],
  "`TombstoneLeaf` in `src/storage/index/tombstone_leaf.rs`: one leaf as a sorted list of slots where `remove` only **marks** a slot dead (a tombstone) and `insert` of a key that has a tombstone **revives** it in place. `should_compact()` says when more than half the slots are tombstones, and `compact()` drops them.",
  "Tombstones make deletes cheap and concurrency-friendly, and their cost is paid by every scan that has to step over them. The two details that go wrong are inserting a key whose tombstone is still there (a duplicate slot, so the key appears twice or is never found) and compacting without preserving order.",
  ["`insert(key, value)` returns the old live value if the key was live; revives a tombstone in place (no new slot); otherwise adds a slot in order.","`remove(key)` marks the slot dead and returns the value it had, `None` if the key was not live.","`get(key)` is the live value or `None`; `live_len()`, `slots()` (total including tombstones), `tombstones()`.","`should_compact()` is `tombstones * 2 > slots`; `compact()` removes every tombstone slot."],
  ["Keys in the slots are strictly increasing: a key has at most one slot.","`slots() == live_len() + tombstones()`.","`get` never returns a removed key's value."],
  ["Removing a key and inserting it again does not grow `slots()`.","`compact()` changes no `get` answer and no ordering of live keys.","After `compact()`, `tombstones() == 0` and `should_compact()` is false."],
  ["insert 1,2,3; remove 2 -> slots 3, live 2, tombstones 1; insert 2 again -> slots 3, live 3"],
  ["Insert, remove, revive; ordering.","The compaction rule and compaction.","A property against a `BTreeMap`, with the slot invariants."],
  src=("src/storage/index/tombstone_leaf.rs", '''
//! A leaf whose deleted entries stay behind as tombstones until it is compacted.

pub struct TombstoneLeaf {
    // @begin 2d-c2
    /// (key, value) in key order; a value of `None` is a tombstone.
    slots: Vec<(i64, Option<u64>)>,
    //~ _leaf: (),
    // @end
}

impl TombstoneLeaf {
    pub fn new() -> TombstoneLeaf {
        // @begin 2d-c2
        TombstoneLeaf { slots: Vec::new() }
        //~ todo!("2d-c2: an empty leaf")
        // @end
    }

    /// Stores `value` under `key`; returns the value it replaced if the key was live.
    pub fn insert(&mut self, key: i64, value: u64) -> Option<u64> {
        // @begin 2d-c2
        match self.slots.binary_search_by_key(&key, |s| s.0) {
            Ok(i) => self.slots[i].1.replace(value),
            Err(i) => {
                self.slots.insert(i, (key, Some(value)));
                None
            }
        }
        //~ todo!("2d-c2: put the key in order, reusing its slot if it has one (live or dead)")
        // @end
    }

    /// Marks the key dead; returns its value if it was live.
    pub fn remove(&mut self, key: i64) -> Option<u64> {
        // @begin 2d-c2
        let i = self.slots.binary_search_by_key(&key, |s| s.0).ok()?;
        self.slots[i].1.take()
        //~ todo!("2d-c2: leave a tombstone")
        // @end
    }

    pub fn get(&self, key: i64) -> Option<u64> {
        // @begin 2d-c2
        let i = self.slots.binary_search_by_key(&key, |s| s.0).ok()?;
        self.slots[i].1
        //~ todo!("2d-c2: the live value")
        // @end
    }

    pub fn slots(&self) -> usize {
        // @begin 2d-c2
        self.slots.len()
        //~ todo!("2d-c2: all slots, live or dead")
        // @end
    }

    pub fn tombstones(&self) -> usize {
        // @begin 2d-c2
        self.slots.iter().filter(|s| s.1.is_none()).count()
        //~ todo!("2d-c2: dead slots")
        // @end
    }

    pub fn live_len(&self) -> usize {
        self.slots() - self.tombstones()
    }

    pub fn should_compact(&self) -> bool {
        self.tombstones() * 2 > self.slots()
    }

    /// Drops every tombstone, keeping the live keys in order.
    pub fn compact(&mut self) {
        // @begin 2d-c2
        self.slots.retain(|s| s.1.is_some());
        //~ todo!("2d-c2: remove the dead slots")
        // @end
    }

    /// The live keys in order.
    pub fn keys(&self) -> Vec<i64> {
        // @begin 2d-c2
        self.slots.iter().filter(|s| s.1.is_some()).map(|s| s.0).collect()
        //~ todo!("2d-c2: the keys of the live slots")
        // @end
    }
}

impl Default for TombstoneLeaf {
    fn default() -> Self {
        TombstoneLeaf::new()
    }
}
'''),
  test=("tests/stages_2d.rs", '''
use bustub::storage::index::tombstone_leaf::TombstoneLeaf;
use std::collections::BTreeMap;

#[test]
fn s2d_c2_removing_leaves_a_tombstone_and_inserting_revives_it_in_place() {
    let mut l = TombstoneLeaf::new();
    for k in [3, 1, 2] {
        assert_eq!(l.insert(k, k as u64 * 10), None);
    }
    assert_eq!(l.remove(2), Some(20));
    assert_eq!((l.slots(), l.live_len(), l.tombstones()), (3, 2, 1));
    assert_eq!(l.get(2), None);
    assert_eq!(l.insert(2, 99), None, "the key was dead, so there is no old value");
    assert_eq!((l.slots(), l.live_len(), l.tombstones()), (3, 3, 0), "revived in its own slot, no new slot");
    assert_eq!(l.get(2), Some(99));
    assert_eq!(l.keys(), vec![1, 2, 3]);
}

#[test]
fn s2d_c2_inserting_a_live_key_replaces_and_returns_the_old_value() {
    let mut l = TombstoneLeaf::new();
    l.insert(5, 1);
    assert_eq!(l.insert(5, 2), Some(1));
    assert_eq!((l.get(5), l.slots()), (Some(2), 1));
}

#[test]
fn s2d_c2_removing_a_missing_or_dead_key_says_none() {
    let mut l = TombstoneLeaf::new();
    l.insert(1, 1);
    assert_eq!(l.remove(9), None);
    assert_eq!(l.remove(1), Some(1));
    assert_eq!(l.remove(1), None);
}

#[test]
fn s2d_c2_compaction_rule_and_effect() {
    let mut l = TombstoneLeaf::new();
    for k in 0..6 {
        l.insert(k, k as u64);
    }
    for k in 0..3 {
        l.remove(k);
    }
    assert!(!l.should_compact(), "3 of 6 is exactly half, not more than half");
    l.remove(3);
    assert!(l.should_compact());
    l.compact();
    assert_eq!((l.slots(), l.tombstones(), l.should_compact()), (2, 0, false));
    assert_eq!(l.keys(), vec![4, 5]);
    assert_eq!(l.get(4), Some(4));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: against a `BTreeMap`, with a key never occupying two slots, and compaction changing no answer.
    #[test]
    fn s2d_c2_property_a_leaf_matches_a_map(ops in proptest::collection::vec((0u8..4, 0i64..8, any::<u64>()), 0..80)) {
        let mut l = TombstoneLeaf::new();
        let mut m: BTreeMap<i64, u64> = BTreeMap::new();
        let mut ever: std::collections::BTreeSet<i64> = Default::default();
        for (op, k, v) in ops {
            match op {
                0 | 1 => { prop_assert_eq!(l.insert(k, v), m.insert(k, v)); ever.insert(k); }
                2 => prop_assert_eq!(l.remove(k), m.remove(&k)),
                _ => { l.compact(); ever = m.keys().copied().collect(); }
            }
            prop_assert_eq!(l.live_len(), m.len());
            prop_assert_eq!(l.slots(), ever.len(), "a key must never occupy two slots");
            prop_assert_eq!(l.keys(), m.keys().copied().collect::<Vec<_>>());
            for key in 0..8 { prop_assert_eq!(l.get(key), m.get(&key).copied()); }
        }
    }
}
''')))

CH.append(C("2d-c3", M2D, "92-challenge-counting-live-keys", "build", "Challenge: counting live keys", "medium", "stages_2d::s2d_c3",
  ["a Fenwick tree for prefix sums with point updates","finding the leaf that holds the k-th live key"],
  ["range-scans-and-the-leaf-chain","model-based-testing"],
  "`LiveCounts` in `src/storage/index/live_counts.rs`: per-leaf counts of live keys with `O(log n)` operations: `add(leaf, delta)` when a key is inserted or tombstoned, `prefix(i)` (live keys in leaves before `i`), `range(l, r)`, and `find(k)` (which leaf holds the k-th live key, counting from 0, and how far into that leaf).",
  "`OFFSET 100000` and `COUNT(*) WHERE key BETWEEN a AND b` on a tree full of tombstones should not walk every leaf. If each inner entry (or a Fenwick tree beside the leaves) knows how many live keys lie below it, both become a logarithmic descent. This is that structure, free of the tree around it.",
  ["`new(counts)` starts from the live count of each leaf.","`add(leaf, delta)` changes a leaf's count (delta may be negative; a count never goes below 0).","`prefix(i)` is the sum of counts of leaves `0..i`; `range(l, r)` of leaves `l..r`; `total()`.","`find(k)` is `Some((leaf, offset))` such that `prefix(leaf) + offset == k` and `offset < count(leaf)`, or `None` if `k >= total()`."],
  ["`prefix(i + 1) - prefix(i) == count(i)`.","`prefix(n) == total()`.","`find` and `prefix` are consistent: `find(prefix(i) + j) == Some((i, j))` when `j < count(i)`."],
  ["After `add(i, d)`, `prefix(j)` changes by `d` exactly for `j > i`.","`find` is monotone: a larger `k` never lands in an earlier leaf.","Empty leaves are never returned by `find`."],
  ["counts [2,0,3]: prefix(3) = 5; find(2) = (2, 0); find(5) = None","add(1, 2) -> counts [2,2,3]; find(2) = (1, 0)"],
  ["Counts, prefixes and ranges.","Finding the k-th live key, skipping empty leaves.","A property against recomputing sums from scratch."],
  src=("src/storage/index/live_counts.rs", '''
//! Live-key counts per leaf, with fast prefix sums and a k-th-key search (a Fenwick tree).

pub struct LiveCounts {
    // @begin 2d-c3
    /// 1-based Fenwick tree over the leaf counts.
    tree: Vec<i64>,
    counts: Vec<i64>,
    //~ _counts: (),
    // @end
}

impl LiveCounts {
    pub fn new(counts: &[usize]) -> LiveCounts {
        // @begin 2d-c3
        let mut c = LiveCounts { tree: vec![0; counts.len() + 1], counts: vec![0; counts.len()] };
        for (i, &n) in counts.iter().enumerate() {
            c.add(i, n as i64);
        }
        c
        //~ todo!("2d-c3: build the structure from the counts")
        // @end
    }

    pub fn leaves(&self) -> usize {
        // @begin 2d-c3
        self.counts.len()
        //~ todo!("2d-c3: how many leaves")
        // @end
    }

    pub fn count(&self, leaf: usize) -> usize {
        // @begin 2d-c3
        self.counts[leaf] as usize
        //~ todo!("2d-c3: the live keys of one leaf")
        // @end
    }

    /// Changes the leaf's count by `delta`; a count never goes below 0.
    pub fn add(&mut self, leaf: usize, delta: i64) {
        // @begin 2d-c3
        let delta = delta.max(-self.counts[leaf]);
        self.counts[leaf] += delta;
        let mut i = leaf + 1;
        while i < self.tree.len() {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
        //~ todo!("2d-c3: update the count and whatever makes prefix sums fast")
        // @end
    }

    /// Live keys in leaves `0..i`.
    pub fn prefix(&self, i: usize) -> usize {
        // @begin 2d-c3
        let (mut i, mut sum) = (i.min(self.counts.len()), 0i64);
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum as usize
        //~ todo!("2d-c3: sum of the counts before leaf i, in O(log n)")
        // @end
    }

    pub fn range(&self, l: usize, r: usize) -> usize {
        if r <= l {
            0
        } else {
            self.prefix(r) - self.prefix(l)
        }
    }

    pub fn total(&self) -> usize {
        self.prefix(self.leaves())
    }

    /// The leaf holding the `k`-th live key (from 0) and the key's offset among that leaf's live keys.
    pub fn find(&self, k: usize) -> Option<(usize, usize)> {
        // @begin 2d-c3
        if k >= self.total() {
            return None;
        }
        let mut pos = 0usize;
        let mut rem = k as i64;
        let mut step = self.tree.len().next_power_of_two();
        while step > 0 {
            let next = pos + step;
            if next < self.tree.len() && self.tree[next] <= rem {
                pos = next;
                rem -= self.tree[next];
            }
            step >>= 1;
        }
        Some((pos, rem as usize))
        //~ todo!("2d-c3: descend the tree to the leaf where the running sum passes k")
        // @end
    }
}
'''),
  test=("tests/stages_2d.rs", '''
use bustub::storage::index::live_counts::LiveCounts;

#[test]
fn s2d_c3_prefix_sums_and_ranges() {
    let c = LiveCounts::new(&[2, 0, 3, 1]);
    assert_eq!((c.prefix(0), c.prefix(1), c.prefix(2), c.prefix(3), c.prefix(4)), (0, 2, 2, 5, 6));
    assert_eq!((c.range(1, 3), c.range(2, 2), c.range(3, 1), c.total()), (3, 0, 0, 6));
    assert_eq!(c.prefix(100), 6, "past the end is the total");
}

#[test]
fn s2d_c3_find_the_leaf_of_the_kth_live_key_skipping_empty_leaves() {
    let c = LiveCounts::new(&[2, 0, 3]);
    assert_eq!(c.find(0), Some((0, 0)));
    assert_eq!(c.find(1), Some((0, 1)));
    assert_eq!(c.find(2), Some((2, 0)), "leaf 1 is empty and is skipped");
    assert_eq!(c.find(4), Some((2, 2)));
    assert_eq!(c.find(5), None);
}

#[test]
fn s2d_c3_updates_change_later_prefixes_only() {
    let mut c = LiveCounts::new(&[2, 0, 3]);
    c.add(1, 2);
    assert_eq!((c.prefix(1), c.prefix(2), c.prefix(3)), (2, 4, 7));
    assert_eq!(c.find(2), Some((1, 0)));
    c.add(0, -5);
    assert_eq!(c.count(0), 0, "a count never goes below zero");
    assert_eq!(c.total(), 5);
}

#[test]
fn s2d_c3_no_leaves_and_all_empty_leaves() {
    let c = LiveCounts::new(&[]);
    assert_eq!((c.total(), c.find(0), c.leaves()), (0, None, 0));
    let c = LiveCounts::new(&[0, 0, 0]);
    assert_eq!(c.find(0), None);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: against sums recomputed from scratch, after any updates; `find` is the inverse of `prefix`.
    #[test]
    fn s2d_c3_property_counts_match_recomputed_sums(init in proptest::collection::vec(0usize..5, 0..12), ops in proptest::collection::vec((0usize..12, -4i64..5), 0..40)) {
        let mut c = LiveCounts::new(&init);
        let mut m: Vec<i64> = init.iter().map(|&x| x as i64).collect();
        for (i, d) in ops {
            if m.is_empty() { break; }
            let i = i % m.len();
            c.add(i, d);
            m[i] = (m[i] + d).max(0);
            for j in 0..=m.len() {
                prop_assert_eq!(c.prefix(j) as i64, m[..j].iter().sum::<i64>());
            }
            let total: i64 = m.iter().sum();
            for k in 0..total as usize + 1 {
                let want = { let mut rem = k as i64; m.iter().position(|&n| { if rem < n { true } else { rem -= n; false } }).map(|leaf| (leaf, (k as i64 - m[..leaf].iter().sum::<i64>()) as usize)) };
                prop_assert_eq!(c.find(k), want, "find({})", k);
            }
        }
    }
}
''')))

CH.append(C("2d-c4", M2D, "93-challenge-the-twice-inserted-key", "debug", "Challenge: the twice-inserted key", "easy", "stages_2d::s2d_c4",
  ["finding a bug where reviving a deleted key creates a second entry"],
  ["checking-invariants","property-testing-and-fuzzing"],
  "`src/storage/index/dead_slots.rs` is a sorted list of keys in which `remove` leaves a dead slot behind. It looks right, and after a key is removed and inserted again it is sometimes reported missing and sometimes twice. Find the bug and fix it.",
  "Tombstones keep the key in place, so a later insert of the same key has to find that slot, not add another one beside it. The bug is invisible in a test that only inserts and removes different keys, and it shows up in the counts: `len` says one thing and a scan says another.",
  ["`insert(key)` returns true if the key was not live; a dead slot of the same key is revived, not duplicated.","`remove(key)` marks the slot dead; false if the key was not live.","`contains(key)`, `len()` (live keys) and `keys()` (live keys in order)."],
  ["The slots hold strictly increasing keys: a key has at most one slot.","`len()` equals the number of live slots and the length of `keys()`."],
  ["`insert(k)` then `remove(k)` then `insert(k)` leaves one live `k`.","`keys()` is always sorted and duplicate-free."],
  ["insert 5; remove 5; insert 5 -> contains 5, len 1, keys [5]"],
  ["Revive after remove, repeatedly.","Ordering and counts.","A property against a `BTreeSet`."],
  src=("src/storage/index/dead_slots.rs", '''
//! A sorted list of keys with dead slots.

pub struct DeadSlots {
    /// (key, live), sorted by key.
    slots: Vec<(i64, bool)>,
}

impl DeadSlots {
    pub fn new() -> DeadSlots {
        DeadSlots { slots: Vec::new() }
    }

    pub fn insert(&mut self, key: i64) -> bool {
        // @begin 2d-c4
        match self.slots.binary_search_by_key(&key, |s| s.0) {
            Ok(i) => {
                let was_live = self.slots[i].1;
                self.slots[i].1 = true;
                !was_live
            }
            Err(i) => {
                self.slots.insert(i, (key, true));
                true
            }
        }
        //~ let at = self.slots.partition_point(|s| s.0 < key);
        //~ if self.slots.get(at).is_some_and(|s| s.0 == key && s.1) {
        //~     return false;
        //~ }
        //~ self.slots.insert(at, (key, true));
        //~ true
        // @end
    }

    pub fn remove(&mut self, key: i64) -> bool {
        match self.slots.binary_search_by_key(&key, |s| s.0) {
            Ok(i) if self.slots[i].1 => {
                self.slots[i].1 = false;
                true
            }
            _ => false,
        }
    }

    pub fn contains(&self, key: i64) -> bool {
        self.slots.binary_search_by_key(&key, |s| s.0).is_ok_and(|i| self.slots[i].1)
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.1).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn keys(&self) -> Vec<i64> {
        self.slots.iter().filter(|s| s.1).map(|s| s.0).collect()
    }
}

impl Default for DeadSlots {
    fn default() -> Self {
        DeadSlots::new()
    }
}
'''),
  test=("tests/stages_2d.rs", '''
use bustub::storage::index::dead_slots::DeadSlots;
use std::collections::BTreeSet;

#[test]
fn s2d_c4_a_removed_key_can_be_inserted_again_and_is_found_once() {
    let mut d = DeadSlots::new();
    assert!(d.insert(5));
    assert!(d.remove(5));
    assert!(d.insert(5));
    assert!(d.contains(5));
    assert_eq!((d.len(), d.keys()), (1, vec![5]));
}

#[test]
fn s2d_c4_reviving_repeatedly_never_duplicates() {
    let mut d = DeadSlots::new();
    for _ in 0..5 {
        assert!(d.insert(1));
        assert!(d.remove(1));
    }
    assert!(d.insert(1));
    assert_eq!((d.len(), d.keys()), (1, vec![1]));
    assert!(!d.insert(1), "already live");
}

#[test]
fn s2d_c4_other_keys_are_not_disturbed() {
    let mut d = DeadSlots::new();
    for k in [3, 1, 2] {
        d.insert(k);
    }
    d.remove(2);
    d.insert(2);
    d.remove(1);
    assert_eq!(d.keys(), vec![2, 3]);
    assert!(!d.contains(1));
}

#[test]
fn s2d_c4_removing_what_is_not_live_is_false() {
    let mut d = DeadSlots::new();
    assert!(!d.remove(9));
    d.insert(9);
    d.remove(9);
    assert!(!d.remove(9));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: against a `BTreeSet`, with `len` and `keys` agreeing.
    #[test]
    fn s2d_c4_property_a_dead_slot_list_is_a_set(ops in proptest::collection::vec((any::<bool>(), 0i64..6), 0..60)) {
        let mut d = DeadSlots::new();
        let mut m = BTreeSet::new();
        for (ins, k) in ops {
            if ins { prop_assert_eq!(d.insert(k), m.insert(k)); } else { prop_assert_eq!(d.remove(k), m.remove(&k)); }
            prop_assert_eq!(d.len(), m.len());
            prop_assert_eq!(d.keys(), m.iter().copied().collect::<Vec<_>>());
            for key in 0..6 { prop_assert_eq!(d.contains(key), m.contains(&key)); }
        }
    }
}
''')))
