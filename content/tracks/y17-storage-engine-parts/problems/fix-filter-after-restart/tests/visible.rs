use solution::*;

#[allow(dead_code)]
fn built(keys: &[&str]) -> Filter {
    let keys: Vec<&[u8]> = keys.iter().map(|k| k.as_bytes()).collect();
    Filter::new(&keys)
}

#[allow(dead_code)]
const THREE: [u8; 13] = [4, 8, 0, 0, 0, 8, 34, 8, 6, 193, 0, 80, 16];

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
