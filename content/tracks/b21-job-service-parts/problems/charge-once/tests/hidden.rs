use solution::*;

#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tokio::time::{Instant, sleep, timeout};

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
pub async fn settle() {
    sleep(ms(1)).await;
}

#[allow(dead_code)]
/// A bank that counts how often it is asked.
#[derive(Clone, Default)]
pub struct Bank(pub Arc<AtomicUsize>);

#[allow(dead_code)]
impl Bank {
    pub fn calls(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// A `send` that takes `millis` and answers `answer`. The call is counted when `send` is invoked.
    pub fn send(&self, millis: u64, answer: Result<&str, &str>) -> impl FnOnce(u64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> + Send + 'static + use<> {
        let calls = self.0.clone();
        let answer = answer.map(str::to_string).map_err(str::to_string);
        move |_amount| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                sleep(ms(millis)).await;
                answer
            })
        }
    }
}

#[allow(dead_code)]
pub fn ok(receipt: &str) -> Result<String, ChargeError> {
    Ok(receipt.to_string())
}

#[allow(dead_code)]
/// `charge` on a spawned task.
pub fn start(p: &Arc<Payments>, bank: &Bank, key: &'static str, amount: u64, millis: u64, answer: Result<&'static str, &'static str>) -> tokio::task::JoinHandle<Result<String, ChargeError>> {
    let (p, send) = (p.clone(), bank.send(millis, answer));
    tokio::spawn(async move { p.charge(key, amount, send).await })
}

#[test]
fn another_amount_while_on_its_way() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let first = start(&p, &bank, "k", 500, 100, Ok("r1"));
        settle().await;
        check!("another amount", p.charge("k", 501, bank.send(1, Ok("r2"))).await, Err(ChargeError::KeyReused));
        check!("first", first.await.unwrap(), ok("r1"));
        check!("calls", bank.calls(), 1);
    });
}

#[test]
fn a_refused_key_takes_a_new_amount() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        let _ = p.charge("k", 500, bank.send(1, Err("too much"))).await;
        check!("a smaller amount", p.charge("k", 100, bank.send(1, Ok("r1"))).await, ok("r1"));
        check!("the first amount is now the wrong one", p.charge("k", 500, bank.send(1, Ok("r2"))).await, Err(ChargeError::KeyReused));
    });
}

#[test]
fn a_client_that_gives_up_frees_the_key() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        let gave_up = timeout(ms(10), p.charge("k", 5, bank.send(100, Ok("lost")))).await;
        check!("timed out", gave_up.is_err(), true);
        check!("retry", p.charge("k", 5, bank.send(1, Ok("r2"))).await, ok("r2"));
        check!("calls", bank.calls(), 2);
    });
}

#[test]
fn an_aborted_task_frees_the_key() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let first = start(&p, &bank, "k", 5, 100, Ok("lost"));
        settle().await;
        first.abort();
        let _ = first.await;
        check!("retry with another amount", p.charge("k", 6, bank.send(1, Ok("r2"))).await, ok("r2"));
    });
}

#[test]
fn a_panic_frees_the_key() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let q = p.clone();
        let in_future = tokio::spawn(async move {
            q.charge("k", 5, |_| async {
                sleep(ms(5)).await;
                if true {
                    panic!("bank client bug");
                }
                Ok(String::new())
            })
            .await
        });
        check!("the future panicked", in_future.await.is_err(), true);
        let q = p.clone();
        let in_call = tokio::spawn(async move {
            q.charge("j", 5, |amount| {
                if amount == 5 {
                    panic!("could not even start");
                }
                async { Ok(String::new()) }
            })
            .await
        });
        check!("send itself panicked", in_call.await.is_err(), true);
        check!("retry k", p.charge("k", 5, bank.send(1, Ok("r1"))).await, ok("r1"));
        check!("retry j", p.charge("j", 5, bank.send(1, Ok("r2"))).await, ok("r2"));
    });
}

#[test]
fn send_gets_the_amount() {
    run(async {
        let p = Payments::new();
        let got = p.charge("k", u64::MAX, |amount| async move { Ok(amount.to_string()) }).await;
        check!("send(u64::MAX)", got, ok("18446744073709551615"));
    });
}

#[test]
fn a_replay_answers_while_another_key_is_slow() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let _ = p.charge("done", 1, bank.send(1, Ok("r0"))).await;
        let slow = start(&p, &bank, "slow", 1, 10_000, Ok("rs"));
        settle().await;
        let clock = Instant::now();
        check!("replay", p.charge("done", 1, bank.send(1, Ok("x"))).await, ok("r0"));
        check!("a new key", p.charge("new", 1, bank.send(1, Ok("rn"))).await, ok("rn"));
        check!("neither waited for the slow key", clock.elapsed() < ms(100), true);
        let _ = slow.await;
    });
}

#[test]
fn empty_and_long_keys_are_ordinary() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        let long = "k".repeat(10_000);
        for key in ["", " ", "K", "k", long.as_str()] {
            let _ = p.charge(key, 1, bank.send(1, Ok("r"))).await;
        }
        check!("five different keys: calls", bank.calls(), 5);
    });
}

#[test]
fn many_threads_one_send() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        let p = Arc::new(Payments::new());
        for round in 0..500 {
            let calls = Arc::new(AtomicUsize::new(0));
            let gate = Arc::new(tokio::sync::Barrier::new(4));
            let mut tasks = Vec::new();
            for _ in 0..4 {
                let (p, calls, gate) = (p.clone(), calls.clone(), gate.clone());
                tasks.push(tokio::spawn(async move {
                    gate.wait().await;
                    p.charge(&format!("key-{round}"), 1, |_| async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        Ok("r".to_string())
                    })
                    .await
                }));
            }
            let mut answers = Vec::new();
            for t in tasks {
                answers.push(t.await.unwrap());
            }
            check!(format!("round {round}: calls to the bank (answers {answers:?})"), calls.load(Ordering::SeqCst), 1);
        }
    });
}

type Handle = tokio::task::JoinHandle<Result<String, ChargeError>>;

#[test]
fn random_histories_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21305);
    for _ in 0..300 {
        // Each step starts a payment that lasts 10 ms, or lets time pass, or gives up on one.
        let steps: Vec<(usize, u64, bool, usize)> = (0..14).map(|_| (rng.below(2), rng.below(2) as u64 + 1, rng.bool(), rng.below(5))).collect();
        run(async {
            let (p, bank) = (Arc::new(Payments::new()), Bank::default());
            // key -> (amount, receipt); a missing receipt means on its way.
            let mut model: std::collections::HashMap<&str, (u64, Option<String>)> = Default::default();
            let mut flying: Vec<(&str, bool, u64, Handle)> = Vec::new();
            let mut log = Vec::new();
            let mut calls = 0;
            for (n, &(k, amount, succeeds, action)) in steps.iter().enumerate() {
                let key = ["a", "b"][k];
                if action == 0 {
                    log.push("everything on its way finishes".to_string());
                    sleep(ms(20_000)).await;
                    for (key, succeeds, _, handle) in flying.drain(..) {
                        let _ = handle.await.unwrap();
                        if succeeds {
                            model.get_mut(key).unwrap().1 = Some(format!("r-{key}"));
                        } else {
                            model.remove(key);
                        }
                    }
                } else if action == 1 && !flying.is_empty() {
                    let (key, _, _, handle) = flying.pop().unwrap();
                    log.push(format!("give up on {key}"));
                    handle.abort();
                    let _ = handle.await;
                    model.remove(key);
                } else {
                    log.push(format!("step {n}: charge {key} {amount}, bank will {}", if succeeds { "accept" } else { "refuse" }));
                    let receipt: &'static str = ["r-a", "r-b"][k];
                    let answer = if succeeds { Ok(receipt) } else { Err("no") };
                    let handle = start(&p, &bank, key, amount, 10_000, answer);
                    match model.get(key) {
                        None => {
                            model.insert(key, (amount, None));
                            calls += 1;
                            settle().await;
                            check!(format!("{log:?}: still on its way"), handle.is_finished(), false);
                            flying.push((key, succeeds, amount, handle));
                        }
                        Some((a, receipt)) => {
                            let want = if *a != amount {
                                Err(ChargeError::KeyReused)
                            } else {
                                receipt.clone().ok_or(ChargeError::InProgress)
                            };
                            settle().await;
                            check!(format!("{log:?}: answered at once"), handle.is_finished(), true);
                            check!(format!("{log:?}"), handle.await.unwrap(), want);
                        }
                    }
                }
                check!(format!("{log:?}: calls to the bank"), bank.calls(), calls);
            }
            for (_, _, _, handle) in flying {
                handle.abort();
            }
        });
    }
}
