use solution::*;

#[allow(dead_code)]
fn built(keys: &[&str]) -> Filter {
    let keys: Vec<&[u8]> = keys.iter().map(|k| k.as_bytes()).collect();
    Filter::new(&keys)
}

#[allow(dead_code)]
const THREE: [u8; 13] = [4, 8, 0, 0, 0, 8, 34, 8, 6, 193, 0, 80, 16];

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
