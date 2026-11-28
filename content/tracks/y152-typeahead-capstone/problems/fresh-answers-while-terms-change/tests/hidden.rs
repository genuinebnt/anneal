use solution::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// What the tests can do from inside `judge`.
#[derive(Default)]
struct Hooks {
    calls: AtomicUsize,
    /// When set, the next call reports that it arrived and then waits to be released.
    pause: Mutex<Option<(Sender<()>, Receiver<()>)>>,
}

impl Hooks {
    fn arm(&self) -> (Receiver<()>, Sender<()>) {
        let (arrived, has_arrived) = channel();
        let (release, released) = channel();
        *self.pause.lock().unwrap() = Some((arrived, released));
        (has_arrived, release)
    }
    fn disarm(&self) {
        *self.pause.lock().unwrap() = None;
    }
}

fn service(capacity: usize) -> (Arc<Service>, Arc<Hooks>) {
    let hooks = Arc::new(Hooks::default());
    let inside = hooks.clone();
    let service = Service::new(
        capacity,
        Box::new(move |typed, term| {
            inside.calls.fetch_add(1, Ordering::SeqCst);
            let pause = inside.pause.lock().unwrap().take();
            if let Some((arrived, released)) = pause {
                let _ = arrived.send(());
                let _ = released.recv_timeout(Duration::from_secs(1));
            }
            classify(typed, term)
        }),
    );
    (Arc::new(service), hooks)
}

fn terms(a: &Answer) -> String {
    a.items.iter().map(|s| s.term.as_str()).collect::<Vec<_>>().join(", ")
}

/// Runs `job` on a thread; the channel gets a message when it ends.
fn start<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> (thread::JoinHandle<T>, Receiver<()>) {
    let (done, is_done) = channel();
    let thread = thread::spawn(move || {
        let out = job();
        let _ = done.send(());
        out
    });
    (thread, is_done)
}

#[test]
fn shared_between_threads() {
    fn needs<T: Send + Sync>() {}
    needs::<Service>();
}

#[test]
fn a_cached_answer_asks_the_judge_nothing() {
    let (s, hooks) = service(8);
    s.add(&[("javascript", 5), ("typescript", 3)], 0);
    s.suggest("script", 5, 0);
    let before = hooks.calls.load(Ordering::SeqCst);
    let again = s.suggest("SCRIPT", 5, 0);
    check!("cached and silent", (again.cached, hooks.calls.load(Ordering::SeqCst) - before), (true, 0));
}

#[test]
fn a_term_not_yet_listed_still_spoils_an_answer() {
    let (s, _) = service(8);
    s.add(&[("sprint", 50), ("blueprint", 40)], 0);
    check!("before", terms(&s.suggest("print", 2, 0)), "sprint, blueprint".to_string());
    check!("empty before", terms(&s.suggest("zeb", 2, 0)), String::new());
    s.add(&[("footprint", 100), ("zebra", 1)], 0);
    check!("a better term", terms(&s.suggest("print", 2, 0)), "footprint, sprint".to_string());
    check!("no longer empty", terms(&s.suggest("zeb", 2, 0)), "zebra".to_string());
    check!("known", s.choose("blueprint", 0), true);
    for _ in 0..70 {
        s.choose("blueprint", 0);
    }
    check!("it climbed in", terms(&s.suggest("print", 2, 0)), "blueprint, footprint".to_string());
}

#[test]
fn writes_that_match_nothing_drop_nothing() {
    let (s, _) = service(8);
    s.add(&[("javascript", 5), ("rust", 3), ("haskell", 2)], 0);
    for typed in ["script", "ru", "pyt"] {
        s.suggest(typed, 5, 0);
    }
    s.choose("haskell", 5);
    s.add(&[("erlang", 1), ("haskell", 4), ("  ", 3)], 9);
    check!("unknown", s.choose("zig", 9), false);
    check!("unknown but matching", s.choose("python", 9), false);
    s.add(&[], 9);
    let kept: Vec<bool> = ["script", "ru", "pyt"].iter().map(|t| s.suggest(t, 5, 9).cached).collect();
    check!("all kept", kept, vec![true; 3]);
}

#[test]
fn the_least_recently_used_answer_goes_first() {
    let (s, _) = service(2);
    s.add(&[("ab", 1), ("cd", 1), ("ef", 1)], 0);
    s.suggest("a", 5, 0);
    s.suggest("c", 5, 0);
    check!("a hit", s.suggest("a", 5, 0).cached, true);
    s.suggest("e", 5, 0);
    check!("a stayed", s.suggest("a", 5, 0).cached, true);
    check!("e stayed", s.suggest("e", 5, 0).cached, true);
    check!("c went", s.suggest("c", 5, 0).cached, false);
}

#[test]
fn huge_limits_and_capacities() {
    let (s, _) = service(usize::MAX);
    s.add(&[("ab", 1)], 0);
    s.suggest("a", usize::MAX, 0);
    let again = s.suggest("a", 20, 0);
    check!("same stored answer", (again.cached, terms(&again)), (true, "ab".to_string()));
    check!("limit 0", s.suggest("a", 0, 0).items, vec![]);
}

/// A search is stopped inside the judge while a write that changes its answer arrives.
#[test]
fn a_write_during_a_search_leaves_nothing_old_behind() {
    let (s, hooks) = service(8);
    s.add(&[("javascript", 5)], 0);
    let (arrived, release) = hooks.arm();
    let reader = {
        let s = s.clone();
        start(move || s.suggest("script", 5, 0))
    };
    arrived.recv_timeout(Duration::from_secs(10)).expect("the search never asked the judge");
    let writer = {
        let s = s.clone();
        start(move || s.add(&[("typescript", 9)], 0))
    };
    // either the write finishes while the search waits, or it waits for the search: both are fine
    let _ = writer.1.recv_timeout(Duration::from_millis(300));
    let _ = release.send(());
    reader.0.join().unwrap();
    writer.0.join().unwrap();
    check!("after the write", terms(&s.suggest("script", 5, 0)), "typescript, javascript".to_string());
}

/// A write is stopped inside the judge (if it asks while it cleans up) while a search arrives.
#[test]
fn a_search_during_a_write_leaves_nothing_old_behind() {
    let (s, hooks) = service(8);
    s.add(&[("javascript", 5), ("fortran", 1)], 0);
    s.suggest("rtra", 5, 0);
    let (arrived, release) = hooks.arm();
    let writer = {
        let s = s.clone();
        start(move || s.add(&[("typescript", 9)], 0))
    };
    let paused = loop {
        if arrived.recv_timeout(Duration::from_millis(5)).is_ok() {
            break true;
        }
        if writer.1.try_recv().is_ok() {
            break false;
        }
    };
    if paused {
        let reader = {
            let s = s.clone();
            start(move || s.suggest("script", 5, 0))
        };
        let _ = reader.1.recv_timeout(Duration::from_millis(300));
        let _ = release.send(());
        reader.0.join().unwrap();
    }
    hooks.disarm();
    writer.0.join().unwrap();
    check!("after the write", terms(&s.suggest("script", 5, 0)), "typescript, javascript".to_string());
}

fn word(rng: &mut anneal_prelude::Rng, max: usize) -> String {
    let n = rng.below(max + 1);
    (0..n).map(|_| *rng.pick(&['a', 'b', 'c', 'A'])).collect()
}

fn same(a: &[Suggestion], b: &[Suggestion]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| x.term == y.term && x.kind == y.kind && x.dist == y.dist && (x.score - y.score).abs() <= y.score * 2e-5)
}

/// The same writes go to a service with a cache and to one without.
#[test]
fn agrees_with_a_service_that_never_caches() {
    let mut rng = anneal_prelude::Rng::new(15204);
    for round in 0..120 {
        let (cached, _) = service(1000);
        let (fresh, _) = service(0);
        // stored answers that nothing has touched since: (typed key, limit)
        let mut must: BTreeMap<(String, usize), ()> = BTreeMap::new();
        let mut now = 0;
        for step in 0..60 {
            now += rng.below(900) as u64;
            if rng.below(4) == 0 {
                let named: Vec<String> = if rng.bool() {
                    let text = word(&mut rng, 7);
                    let a = cached.choose(&text, now);
                    check!("choose", a, fresh.choose(&text, now));
                    if a { vec![text.to_lowercase()] } else { vec![] }
                } else {
                    let n = rng.below(3);
                    let batch: Vec<(String, u32)> = (0..n).map(|_| (word(&mut rng, 7), rng.below(5) as u32)).collect();
                    let pairs: Vec<(&str, u32)> = batch.iter().map(|(t, c)| (t.as_str(), *c)).collect();
                    check!("add", cached.add(&pairs, now), fresh.add(&pairs, now));
                    batch.iter().map(|(t, _)| t.to_lowercase()).filter(|k| !k.is_empty()).collect()
                };
                must.retain(|(typed, _), _| !named.iter().any(|term| classify(typed, term).is_some()));
                check!("len", cached.len(), fresh.len());
            } else {
                let typed = word(&mut rng, 5);
                let limit = 1 + rng.below(4);
                let got = cached.suggest(&typed, limit, now);
                let want = fresh.suggest(&typed, limit, now);
                let label = format!("round {round} step {step}: suggest({typed:?}, {limit}) gave {:?}, fresh gives {:?}", got.items, want.items);
                check!(label.clone(), same(&got.items, &want.items), true);
                check!("a service without a cache says cached", want.cached, false);
                let key = (typed.to_lowercase(), limit);
                if must.contains_key(&key) {
                    check!(format!("{label}: should have been cached"), got.cached, true);
                }
                must.insert(key, ());
            }
        }
    }
}

#[test]
fn many_threads_end_in_the_right_state() {
    let (s, _) = service(16);
    let (fresh, _) = service(0);
    let words: Vec<String> = (0..40).map(|i| format!("{}script{}", ["java", "type", "coffee", "post"][i % 4], i / 4)).collect();
    let mut workers = Vec::new();
    for w in 0..3 {
        let (s, words) = (s.clone(), words.clone());
        workers.push(thread::spawn(move || {
            for i in 0..120 {
                if i % 3 == 0 {
                    s.add(&[(words[(i / 3 + w * 13) % 40].as_str(), 1)], 0);
                } else {
                    let a = s.suggest(["script", "java", "scirpt", "type"][i % 4], 20, 0);
                    assert!(a.items.len() <= 20);
                }
            }
        }));
    }
    for w in workers {
        w.join().unwrap();
    }
    for w in 0..3 {
        for i in (0..120).step_by(3) {
            fresh.add(&[(words[(i / 3 + w * 13) % 40].as_str(), 1)], 0);
        }
    }
    for typed in ["script", "java", "scirpt", "type"] {
        check!(typed.to_string(), same(&s.suggest(typed, 20, 0).items, &fresh.suggest(typed, 20, 0).items), true);
    }
}

/// Real threads and no waiting on purpose. One thread keeps adding a term that beats all before it. The others keep
/// asking, and an answer must never be older than the last write that had finished before the question began.
#[test]
fn no_old_answer_once_a_write_has_finished() {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    for round in 0..10 {
        let (s, _) = service(4);
        // the newest write that `add` has returned from
        let done = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        s.add(&[("zq0", 1)], 0);
        let askers: Vec<thread::JoinHandle<Option<String>>> = (0..3)
            .map(|_| {
                let (s, done, stop) = (s.clone(), done.clone(), stop.clone());
                thread::spawn(move || {
                    let mut asked = 0;
                    while !stop.load(Ordering::SeqCst) || asked < 2_000 {
                        asked += 1;
                        let before = done.load(Ordering::SeqCst);
                        let answer = s.suggest("zq", 1, 0);
                        let top: u64 = answer.items.first().and_then(|t| t.term[2..].parse().ok()).unwrap_or(0);
                        if top < before {
                            return Some(format!("round {round}: the best was zq{top} (cached: {}) after zq{before} had been added", answer.cached));
                        }
                    }
                    None
                })
            })
            .collect();
        for n in 1..=1_500u64 {
            // each new term is chosen more often than any before it
            s.add(&[(format!("zq{n}").as_str(), n as u32 + 1)], 0);
            done.store(n, Ordering::SeqCst);
        }
        stop.store(true, Ordering::SeqCst);
        let wrong: Vec<String> = askers.into_iter().filter_map(|a| a.join().unwrap()).collect();
        check!("old answers served", wrong.first().cloned(), None);
    }
}

/// The hook stops a search after its last call of the judge, before its answer is stored, and a write lands there.
#[test]
fn a_write_between_the_search_and_the_store() {
    use std::sync::atomic::AtomicBool;
    for round in 0..3 {
        // judge calls made on this thread, where the searches run; the write's own calls run elsewhere
        let here = thread::current().id();
        let mine = Arc::new(AtomicUsize::new(0));
        let counter = mine.clone();
        let s = Arc::new(Service::new(
            8,
            Box::new(move |typed, term| {
                if thread::current().id() == here {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
                classify(typed, term)
            }),
        ));
        s.add(&[("javascript", 5)], 0);
        // the judge calls seen each time the hook ran, and the write it started
        let seen = Arc::new(Mutex::new(Vec::new()));
        let armed = Arc::new(AtomicBool::new(true));
        let writer: Arc<Mutex<Option<thread::JoinHandle<Added>>>> = Arc::new(Mutex::new(None));
        {
            let (s2, mine, seen, armed, writer) = (s.clone(), mine.clone(), seen.clone(), armed.clone(), writer.clone());
            s.on_searched(move || {
                seen.lock().unwrap().push(mine.load(Ordering::SeqCst));
                if armed.swap(false, Ordering::SeqCst) {
                    let s3 = s2.clone();
                    let (thread, done) = start(move || s3.add(&[("typescript", 9)], 0));
                    // the write either finishes now or waits for this search: both are fine
                    let _ = done.recv_timeout(Duration::from_secs(1));
                    *writer.lock().unwrap() = Some(thread);
                }
            });
        }
        let first = s.suggest("script", 5, 0);
        let after = mine.load(Ordering::SeqCst);
        check!(format!("round {round}: the hook ran once, after the last call of the judge"), seen.lock().unwrap().clone(), vec![after]);
        check!(format!("round {round}: the search asked the judge"), (after > 0, first.cached), (true, false));
        writer.lock().unwrap().take().expect("the hook started no write").join().unwrap();
        let again = s.suggest("script", 5, 0);
        check!(format!("round {round}: after the write"), terms(&again), "typescript, javascript".to_string());
        let count = seen.lock().unwrap().len();
        let third = s.suggest("script", 5, 0);
        check!(format!("round {round}: a cached answer does not call the hook"), (third.cached, seen.lock().unwrap().len()), (true, count));
    }
}
