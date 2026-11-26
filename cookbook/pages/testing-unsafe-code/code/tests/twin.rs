//! The `unsafe` shelf against the `unsafe`-free one: the same calls must give the same answers.

use shelf::safe::SafeShelf;
use shelf::Shelf;

#[test]
fn unsafe_and_safe_shelves_agree() {
    let mut fast = Shelf::<String, 8>::new();
    let mut safe = SafeShelf::<String, 8>::new();
    for round in 0..3 {
        for i in 0..11 {
            let word = format!("r{round}-{i}");
            assert_eq!(fast.push(word.clone()), safe.push(word.clone()).map_err(|_| word), "push {round}/{i}");
            assert_eq!(fast.len(), safe.len());
            for j in 0..10 {
                assert_eq!(fast.get(j), safe.get(j), "get({j}) after {round}/{i}");
            }
        }
        fast.clear();
        safe.clear();
        assert!(fast.is_empty() && safe.is_empty());
    }
}
