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
fn a_retry_gets_the_same_receipt() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        check!("first charge", p.charge("k1", 500, bank.send(5, Ok("r1"))).await, ok("r1"));
        check!("retry", p.charge("k1", 500, bank.send(5, Ok("r2"))).await, ok("r1"));
        check!("calls to the bank", bank.calls(), 1);
    });
}

#[test]
fn the_amount_must_match() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        let _ = p.charge("k1", 500, bank.send(5, Ok("r1"))).await;
        check!("same key, another amount", p.charge("k1", 900, bank.send(5, Ok("r2"))).await, Err(ChargeError::KeyReused));
        check!("calls to the bank", bank.calls(), 1);
    });
}

#[test]
fn a_second_attempt_while_the_first_is_on_its_way() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let first = start(&p, &bank, "k1", 500, 100, Ok("r1"));
        settle().await;
        let clock = Instant::now();
        let second = p.charge("k1", 500, bank.send(1, Ok("r2"))).await;
        check!("second attempt: (answer, time it took)", (second, clock.elapsed()), (Err(ChargeError::InProgress), ms(0)));
        check!("first attempt", first.await.unwrap(), ok("r1"));
        check!("calls to the bank", bank.calls(), 1);
    });
}

#[test]
fn a_refusal_can_be_retried() {
    run(async {
        let (p, bank) = (Payments::new(), Bank::default());
        check!("refused", p.charge("k1", 500, bank.send(1, Err("no funds"))).await, Err(ChargeError::Declined("no funds".to_string())));
        check!("retry", p.charge("k1", 500, bank.send(1, Ok("r1"))).await, ok("r1"));
        check!("calls to the bank", bank.calls(), 2);
    });
}

#[test]
fn different_keys_do_not_wait() {
    run(async {
        let (p, bank) = (Arc::new(Payments::new()), Bank::default());
        let clock = Instant::now();
        let a = start(&p, &bank, "a", 1, 100, Ok("ra"));
        let b = start(&p, &bank, "b", 1, 100, Ok("rb"));
        check!("both receipts", (a.await.unwrap(), b.await.unwrap()), (ok("ra"), ok("rb")));
        check!("two 100 ms payments took", clock.elapsed(), ms(100));
    });
}
