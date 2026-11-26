from y17_parts.common import broken

STARTER = r'''use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

/// Answers "is this key possibly in the file?". "No" is always right; "yes" is sometimes wrong.
pub struct Filter {
    bits: Vec<u8>,
    probes: u8,
    hasher: RandomState,
}

impl Filter {
    /// A filter holding `keys`.
    pub fn new(keys: &[&[u8]]) -> Filter {
        let bytes = (keys.len() * 10).max(64).div_ceil(8);
        let mut filter = Filter {
            bits: vec![0; bytes],
            probes: 4,
            hasher: RandomState::new(),
        };
        for key in keys {
            for bit in filter.positions(key) {
                filter.bits[bit / 8] |= 1 << (bit % 8);
            }
        }
        filter
    }

    fn positions(&self, key: &[u8]) -> Vec<usize> {
        let hash = self.hasher.hash_one(key);
        let (first, step) = (hash, hash.rotate_left(31) | 1);
        let count = (self.bits.len() * 8) as u64;
        (0..self.probes as u64)
            .map(|i| (first.wrapping_add(i.wrapping_mul(step)) % count) as usize)
            .collect()
    }

    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.positions(key)
            .iter()
            .all(|&bit| self.bits[bit / 8] & (1 << (bit % 8)) != 0)
    }

    /// The bytes to put in the file.
    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![self.probes];
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
        out
    }

    /// The filter that `save` wrote, or `None` if these are not the bytes of a filter.
    pub fn load(bytes: &[u8]) -> Option<Filter> {
        let probes = bytes[0];
        let len = u32::from_le_bytes(bytes[1..5].try_into().ok()?) as usize;
        Some(Filter {
            bits: bytes[5..5 + len].to_vec(),
            probes,
            hasher: RandomState::new(),
        })
    }
}
'''

SOLUTION = r'''/// Answers "is this key possibly in the file?". "No" is always right; "yes" is sometimes wrong.
pub struct Filter {
    bits: Vec<u8>,
    probes: u8,
}

/// FNV-1a, 64 bit, then the high half folded into the low half. This function is part of the file format.
fn hash(key: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &byte in key {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (h >> 32)
}

impl Filter {
    /// A filter holding `keys`.
    pub fn new(keys: &[&[u8]]) -> Filter {
        let bytes = (keys.len() * 10).max(64).div_ceil(8);
        let mut filter = Filter {
            bits: vec![0; bytes],
            probes: 4,
        };
        for key in keys {
            for bit in filter.positions(key) {
                filter.bits[bit / 8] |= 1 << (bit % 8);
            }
        }
        filter
    }

    fn positions(&self, key: &[u8]) -> Vec<usize> {
        let hash = hash(key);
        let (first, step) = (hash, hash.rotate_left(31) | 1);
        let count = (self.bits.len() * 8) as u64;
        (0..self.probes as u64)
            .map(|i| (first.wrapping_add(i.wrapping_mul(step)) % count) as usize)
            .collect()
    }

    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.positions(key)
            .iter()
            .all(|&bit| self.bits[bit / 8] & (1 << (bit % 8)) != 0)
    }

    /// The bytes to put in the file.
    pub fn save(&self) -> Vec<u8> {
        let mut out = vec![self.probes];
        out.extend((self.bits.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bits);
        out
    }

    /// The filter that `save` wrote, or `None` if these are not the bytes of a filter.
    pub fn load(bytes: &[u8]) -> Option<Filter> {
        let (&probes, rest) = bytes.split_first()?;
        let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let bits = &rest[4..];
        if probes == 0 || len == 0 || bits.len() != len {
            return None;
        }
        Some(Filter {
            bits: bits.to_vec(),
            probes,
        })
    }
}
'''

HARNESS = r'''
#[allow(dead_code)]
fn built(keys: &[&str]) -> Filter {
    let keys: Vec<&[u8]> = keys.iter().map(|k| k.as_bytes()).collect();
    Filter::new(&keys)
}

#[allow(dead_code)]
const THREE: [u8; 13] = [4, 8, 0, 0, 0, 8, 34, 8, 6, 193, 0, 80, 16];
'''

VISIBLE = HARNESS + r'''
#[test]
fn works_before_saving() {
    let f = built(&["alice", "bob", "carol"]);
    check!("alice, bob, carol", (f.may_contain(b"alice"), f.may_contain(b"bob"), f.may_contain(b"carol")), (true, true, true));
}

#[test]
fn still_works_after_save_and_load() {
    let keys: Vec<String> = (0..50).map(|i| format!("user-{i}")).collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let loaded = Filter::load(&built(&refs).save()).expect("its own bytes");
    let lost: Vec<&String> = keys.iter().filter(|k| !loaded.may_contain(k.as_bytes())).collect();
    check!("keys the loaded filter denies", lost, Vec::<&String>::new());
}

#[test]
fn the_exact_bytes_in_the_file() {
    check!("alice, bob, carol", built(&["alice", "bob", "carol"]).save(), THREE.to_vec());
}

#[test]
fn bytes_written_by_another_run_are_understood() {
    let f = Filter::load(&THREE).expect("a stored filter");
    check!("the three keys", (f.may_contain(b"alice"), f.may_contain(b"bob"), f.may_contain(b"carol")), (true, true, true));
}

#[test]
fn an_empty_input_is_not_a_filter() {
    check!("load of no bytes", Filter::load(&[]).is_none(), true);
}
'''

HIDDEN = HARNESS + r'''
#[test]
fn no_keys() {
    let f = built(&[]);
    check!("the bytes of an empty filter", f.save(), vec![4, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    check!("it contains nothing", f.may_contain(b"anything"), false);
}

#[test]
fn one_key() {
    check!("apple", built(&["apple"]).save(), vec![4, 8, 0, 0, 0, 2, 0, 16, 8, 0, 64, 0, 0]);
}

#[test]
fn ten_keys_need_thirteen_bytes() {
    let keys: Vec<String> = (0..10).map(|i| format!("key-{i}")).collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    check!("key-0 to key-9", built(&refs).save(), vec![4, 13, 0, 0, 0, 136, 36, 8, 3, 16, 0, 67, 42, 240, 21, 136, 230, 32]);
}

#[test]
fn the_number_of_probes_comes_from_the_file() {
    // one probe, and every bit set but those a four-probe reading would want
    let mut bytes = vec![1u8, 8, 0, 0, 0];
    bytes.extend([0xff; 8]);
    check!("all bits set", Filter::load(&bytes).map(|f| (f.may_contain(b"q"), f.save() == bytes)), Some((true, true)));
    // the first probe of "apple" alone
    let h = {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for &b in b"apple" {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^ (h >> 32)
    };
    let mut one = vec![1u8, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    one[5 + (h % 64) as usize / 8] |= 1 << ((h % 64) % 8);
    check!("only the first bit of apple, one probe", Filter::load(&one).map(|f| f.may_contain(b"apple")), Some(true));
    one[0] = 4;
    check!("the same bits read with four probes", Filter::load(&one).map(|f| f.may_contain(b"apple")), Some(false));
}

#[test]
fn every_cut_is_rejected() {
    for keep in 0..THREE.len() {
        check!(format!("first {keep} bytes"), Filter::load(&THREE[..keep]).is_none(), true);
    }
}

#[test]
fn other_bytes_that_are_not_filters() {
    let mut extra = THREE.to_vec();
    extra.push(0);
    check!("one byte too many", Filter::load(&extra).is_none(), true);
    let mut zero = THREE;
    zero[0] = 0;
    check!("no probes", Filter::load(&zero).is_none(), true);
    check!("no bits", Filter::load(&[4, 0, 0, 0, 0]).is_none(), true);
    check!("a huge length", Filter::load(&[4, 255, 255, 255, 255, 1]).is_none(), true);
}

#[test]
fn random_filters_never_deny_their_keys() {
    let mut rng = anneal_prelude::Rng::new(170601);
    for case in 0..300 {
        let n = rng.below(60);
        let mut keys = Vec::new();
        for _ in 0..n {
            let len = rng.below(12);
            let key: Vec<u8> = rng.vec(len, 0, 255);
            keys.push(key);
        }
        let refs: Vec<&[u8]> = keys.iter().map(Vec::as_slice).collect();
        let bytes = Filter::new(&refs).save();
        check!(format!("case {case}: size for {n} keys"), bytes.len(), 5 + (n * 10).max(64).div_ceil(8));
        let loaded = Filter::load(&bytes).expect("its own bytes");
        check!(format!("case {case}: {keys:?}"), keys.iter().all(|k| loaded.may_contain(k)), true);
        check!(format!("case {case}: saved again"), loaded.save(), bytes);
    }
}

#[test]
fn most_absent_keys_are_refused() {
    let keys: Vec<String> = (0..1000).map(|i| format!("in-{i}")).collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let f = Filter::load(&built(&refs).save()).expect("its own bytes");
    let wrong = (0..2000).filter(|i| f.may_contain(format!("out-{i}").as_bytes())).count();
    check!(format!("{wrong} of 2000 absent keys let through, at most 100"), wrong <= 100, true);
}
'''

PROBLEM = dict(
    slug="fix-filter-after-restart", title="Fix: a saved filter that forgets its keys", level="medium", stage="sorted-data",
    tags=["Bloom filter", "stable hashing", "file formats", "decoding", "fix"], mode="fix",
    statement=r'''
Each data file stores a small filter, so that a lookup can skip files that surely lack the key. All tests passed in the process that wrote the files. After a restart, the service says that thousands of stored keys do not exist.

Fix `Filter`. The saved bytes are read by **other processes, later, built by other compiler versions**. So the format is fixed completely.

**Saved bytes:** one byte with the number of probes, a `u32` little-endian with the number of bit bytes, then the bit bytes. Bit `p` is bit `p % 8` (from the lowest) of byte `p / 8`.

**The hash** of a key is 64-bit FNV-1a (start `0xcbf29ce484222325`, and for each byte: XOR it in, then multiply by `0x100000001b3`, wrapping). Then XOR the result with itself shifted right by 32.

**Positions:** with `h` the hash, `step = h.rotate_left(31) | 1`, and `m` the number of bits, probe `i` is `(h + i * step) % m`, with wrapping add and multiply.

**`new(keys)`** uses 4 probes and `max(10 * keys.len(), 64)` bits, rounded up to whole bytes.

**`load(bytes)`** returns `None` unless the bytes are exactly one saved filter with at least one probe and at least one bit byte. It must never panic. A loaded filter uses the probe count **from the bytes**.

You may not use `RandomState` or `DefaultHasher`.
''',
    starter=STARTER, solution=SOLUTION, visible=[VISIBLE], hidden=[HIDDEN], use="use solution::*;",
    rules=dict(types=["RandomState", "DefaultHasher"]),
    hints=[
        ("approach", "Ask what the loaded filter knows that the writing filter knew. One thing the bits depend on is not in the file."),
        ("rust", "`split_first` and `slice.get(..4)` return `None` on short input. The documentation of `DefaultHasher` says what it promises across releases."),
        ("edge", "Check the whole length: too long is as wrong as too short. A length field of four billion must not be used to slice before it is checked."),
    ],
    notes=("The bits depend on the hash function, so the hash is part of the file format. A randomly seeded hasher gives every instance different positions, and a loaded filter then denies keys it holds. Fixing the function in the specification makes the bytes meaningful anywhere; checking the lengths makes damaged bytes harmless.", "O(probes + key)", "O(bits)"),
    follow_up="1) Rust's `DefaultHasher::new()` gives the same results within one build. Why is it still wrong for a file? 2) What would you add to the format so that you can change the hash later without misreading old files? 3) A flipped bit can turn a present key into a refused one. Where should that be caught?",
    teaches=["A hash whose output is stored is part of the file format: fix it exactly, and test its bytes.", "`HashMap`'s hasher is seeded at random on purpose, so it must never reach a file.", "A round-trip test in one process cannot see a format that depends on that process."],
    wrong={
        "fnv_without_the_fold": broken(SOLUTION, ("    h ^ (h >> 32)\n}", "    h\n}")),
        "step_may_be_even": broken(SOLUTION, ("hash.rotate_left(31) | 1", "hash.rotate_left(31)")),
        "load_assumes_four_probes": broken(SOLUTION, ("            bits: bits.to_vec(),\n            probes,", "            bits: bits.to_vec(),\n            probes: 4,")),
        "extra_bytes_accepted": broken(SOLUTION, ("let bits = &rest[4..];", "let bits = rest.get(4..4 + len)?;")),
        "still_panics_on_a_short_body": broken(SOLUTION, ("let bits = &rest[4..];", "let bits = &rest[4..4 + len];")),
        "fnv1_instead_of_fnv1a": broken(SOLUTION, ("h = (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);", "h = h.wrapping_mul(0x0000_0100_0000_01b3) ^ u64::from(byte);")),
    },
)
