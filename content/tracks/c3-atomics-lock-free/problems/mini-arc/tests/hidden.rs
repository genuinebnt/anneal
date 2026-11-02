use solution::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Probe(Arc<AtomicUsize>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn value_dropped_once_when_last_handle_goes() {
    let n = Arc::new(AtomicUsize::new(0));
    let a = MiniArc::new(Probe(n.clone()));
    let b = a.clone();
    let c = b.clone();
    drop(a);
    drop(c);
    check!("two of three handles dropped", n.load(Ordering::SeqCst), 0);
    drop(b);
    check!("last handle dropped", n.load(Ordering::SeqCst), 1);
}

#[test]
fn try_unwrap_moves_value_without_dropping_it() {
    let n = Arc::new(AtomicUsize::new(0));
    let a = MiniArc::new(Probe(n.clone()));
    let p = MiniArc::try_unwrap(a).ok().unwrap();
    check!("after try_unwrap, value alive", n.load(Ordering::SeqCst), 0);
    drop(p);
    check!("after dropping the moved value", n.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_try_unwrap_keeps_everything_alive() {
    let n = Arc::new(AtomicUsize::new(0));
    let a = MiniArc::new(Probe(n.clone()));
    let b = a.clone();
    let back = MiniArc::try_unwrap(a).err().unwrap();
    check!("shared try_unwrap fails: count, drops", (MiniArc::strong_count(&back), n.load(Ordering::SeqCst)), (2, 0));
    drop(back);
    drop(b);
    check!("both handles dropped", n.load(Ordering::SeqCst), 1);
}

#[test]
fn sixteen_threads_clone_and_drop() {
    let n = Arc::new(AtomicUsize::new(0));
    let a = MiniArc::new(Probe(n.clone()));
    let hs: Vec<_> = (0..16)
        .map(|_| {
            let a = a.clone();
            std::thread::spawn(move || {
                for _ in 0..5_000 {
                    let b = a.clone();
                    let c = b.clone();
                    drop(b);
                    drop(c);
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    check!("after the threads finish: count, drops", (MiniArc::strong_count(&a), n.load(Ordering::SeqCst)), (1, 0));
    drop(a);
    check!("after the last drop", n.load(Ordering::SeqCst), 1);
}

#[test]
fn last_dropper_frees_exactly_once_under_a_race() {
    for _ in 0..300 {
        let n = Arc::new(AtomicUsize::new(0));
        let a = MiniArc::new(Probe(n.clone()));
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let a = a.clone();
                std::thread::spawn(move || drop(a))
            })
            .collect();
        drop(a);
        for h in hs {
            h.join().unwrap();
        }
        check!("five handles dropped on five threads", n.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn data_written_before_clone_is_visible() {
    check!(r#"a thread reads a Vec built before the spawn"#, { let a = MiniArc::new((0..1000u32).collect::<Vec<_>>()); let b = a.clone(); std::thread::spawn(move || b.iter().sum::<u32>()).join().unwrap() }, 499_500);
}

#[test]
fn make_mut_unique_does_not_copy() {
    check!(r#"unique owner keeps its allocation"#, { let mut a = MiniArc::new(vec![1]); let before = &*a as *const Vec<i32>; MiniArc::make_mut(&mut a).push(2); let same = before == &*a as *const Vec<i32>; (same, (*a).clone()) }, (true, vec![1, 2]));
}

#[test]
fn make_mut_shared_copies() {
    check!(r#"shared owner detaches; the other copy is unchanged"#, { let mut a = MiniArc::new(vec![1]); let b = a.clone(); MiniArc::make_mut(&mut a).push(2); (MiniArc::ptr_eq(&a, &b), (*a).clone(), (*b).clone(), MiniArc::strong_count(&b)) }, (false, vec![1, 2], vec![1], 1));
}

#[test]
fn zero_sized_values() {
    check!(r#"MiniArc<()> counts correctly"#, { let a = MiniArc::new(()); let b = a.clone(); let n = MiniArc::strong_count(&a); drop(b); (n, MiniArc::strong_count(&a)) }, (2, 1));
}

#[test]
fn send_and_sync_for_sendable_data() {
    check!(r#"compile-time bounds"#, { fn s<T: Send + Sync>() {} s::<MiniArc<String>>(); s::<MiniArc<Vec<u8>>>(); true }, true);
}

#[test]
fn random_clone_drop_matches_a_model() {
    let mut rng = anneal_prelude::Rng::new(4202);
    for _ in 0..200 {
        let first = MiniArc::new(0u8);
        let mut handles = vec![first];
        for _ in 0..rng.below(40) {
            if handles.len() == 1 || rng.below(3) != 0 {
                let i = rng.below(handles.len());
                let c = handles[i].clone();
                handles.push(c);
            } else {
                let i = rng.below(handles.len());
                handles.swap_remove(i);
            }
            check!(format!("{} live handles", handles.len()), MiniArc::strong_count(&handles[0]), handles.len());
        }
    }
}
