//! RECIPE 216 (patterns): a mini re-implementation of tower's Service/Layer model with timeout, retry, limits and a circuit breaker.
//! LEVEL: advanced
//! Use in: HTTP clients/servers (axum, hyper, tonic), RPC middleware, resilience policies around any async call.   Run: cargo run --release --example tower_service_layer
#![allow(dead_code)]
//!
//! tower (https://github.com/tower-rs/tower , https://docs.rs/tower) models everything as `Service<Request>`: an async function
//! `Request -> Result<Response, Error>`. A `Layer` wraps one service in another (decorator pattern), and `ServiceBuilder` stacks
//! layers. The real trait is `fn poll_ready(&mut self, cx)` + `fn call(&mut self, req) -> Self::Future`, which lets a service
//! apply BACKPRESSURE before accepting work. Here we simplify to `async fn call(&self, req)` (shared `&self`, readiness folded into
//! `call`), so the middleware logic is easy to read; concurrency limit and rate limit therefore wait/reject inside `call`.
//! Everything is generic (static dispatch): a whole stack monomorphises into one concrete type with no boxing.
//!
//! Layer ORDER is behaviour. `ServiceBuilder::new().layer(A).layer(B).service(s)` is `A(B(s))`: A is OUTERMOST (sees the request
//! first, the response last). So `Retry` outside `Timeout` gives EACH attempt its own deadline; `Timeout` outside `Retry` puts ONE
//! deadline over all attempts including backoff sleeps. A circuit breaker outside a retry counts the final outcome, inside it counts
//! every attempt. All of these orders are tested below.
//!
//! axum: `Router` and `MethodRouter` are `tower::Service`s, and axum turns your handler functions into services for you. That is
//! why `.layer(...)` accepts middleware from `tower` or `tower-http` (CORS, tracing, compression) and why `ServiceExt::oneshot`
//! can call an app directly in tests (shown at the end, using the real `tower` crate from this cookbook's dependencies).
//! COMMON USES: client resilience (timeouts, retries with backoff, circuit breakers, rate limits) in hyper/tonic/reqwest-style stacks,
//! server middleware (auth, tracing, compression), load shedding and concurrency limiting at service boundaries.
//! Read: https://github.com/tower-rs/tower , https://docs.rs/tower , https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
use futures_util::poll;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};

// ----------------------------------------------------------------- core traits
#[derive(Debug, Clone, PartialEq)]
enum Error {
    Timeout,
    RateLimited,
    CircuitOpen,
    Failed(String), // errors from the inner service (real tower uses `Box<dyn Error + Send + Sync>`)
}

trait Service<Req: Send>: Send + Sync {
    type Response: Send;
    fn call(&self, req: Req) -> impl Future<Output = Result<Self::Response, Error>> + Send;
}
trait Layer<S> {
    type Service;
    fn layer(&self, inner: S) -> Self::Service;
}
struct Identity;
impl<S> Layer<S> for Identity {
    type Service = S;
    fn layer(&self, inner: S) -> S {
        inner
    }
}
struct Stack<Inner, Outer> {
    inner: Inner,
    outer: Outer,
}
impl<S, I: Layer<S>, O: Layer<I::Service>> Layer<S> for Stack<I, O> {
    type Service = O::Service;
    fn layer(&self, s: S) -> Self::Service {
        self.outer.layer(self.inner.layer(s))
    }
}
/// First `.layer()` call = outermost, exactly like tower's `ServiceBuilder`.
struct ServiceBuilder<L>(L);
impl ServiceBuilder<Identity> {
    fn new() -> Self {
        ServiceBuilder(Identity)
    }
}
impl<L> ServiceBuilder<L> {
    fn layer<T>(self, t: T) -> ServiceBuilder<Stack<T, L>> {
        ServiceBuilder(Stack {
            inner: t,
            outer: self.0,
        })
    }
    fn service<S>(&self, s: S) -> L::Service
    where
        L: Layer<S>,
    {
        self.0.layer(s)
    }
}

// ------------------------------------------------------------------ clocks
trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}
#[derive(Default)]
struct ManualClock(AtomicU64); // microseconds
impl ManualClock {
    fn advance(&self, d: Duration) {
        self.0.fetch_add(d.as_micros() as u64, SeqCst);
    }
    fn set(&self, d: Duration) {
        self.0.store(d.as_micros() as u64, SeqCst);
    }
}
impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_micros(self.0.load(SeqCst))
    }
}

// ----------------------------------------------------------- mock inner service
#[derive(Clone, Debug)]
enum Step {
    Ok,
    Fail,
    SleepThenOk(Duration),
    /// Answers `Ok` only after the test calls `notify_one` on the gate: no timing involved.
    WaitThenOk(Arc<Notify>),
}
/// Behaviour scripted per call number; after the script runs out it answers `Ok`. Counts calls and peak concurrency.
#[derive(Clone)]
struct Mock {
    script: Arc<Vec<Step>>,
    calls: Arc<AtomicUsize>,
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}
impl Mock {
    fn new(script: Vec<Step>) -> Self {
        Mock {
            script: Arc::new(script),
            calls: Default::default(),
            in_flight: Default::default(),
            peak: Default::default(),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(SeqCst)
    }
}
impl Service<u32> for Mock {
    type Response = String;
    async fn call(&self, req: u32) -> Result<String, Error> {
        let n = self.calls.fetch_add(1, SeqCst);
        let now = self.in_flight.fetch_add(1, SeqCst) + 1;
        let _leave = Leave(&self.in_flight); // runs even if a timeout drops this future
        self.peak.fetch_max(now, SeqCst);
        let step = self.script.get(n).cloned().unwrap_or(Step::Ok);
        match step {
            Step::Ok => Ok(format!("ok:{req}")),
            Step::Fail => Err(Error::Failed(format!("boom #{n}"))),
            Step::SleepThenOk(d) => {
                tokio::time::sleep(d).await;
                Ok(format!("ok:{req}"))
            }
            Step::WaitThenOk(gate) => {
                gate.notified().await;
                Ok(format!("ok:{req}"))
            }
        }
    }
}
struct Leave<'a>(&'a AtomicUsize);
impl Drop for Leave<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, SeqCst);
    }
}

// ---------------------------------------------------------------- Trace layer
type Log = Arc<Mutex<Vec<String>>>;
struct TraceLayer(&'static str, Log);
struct Trace<S> {
    name: &'static str,
    log: Log,
    inner: S,
}
impl<S> Layer<S> for TraceLayer {
    type Service = Trace<S>;
    fn layer(&self, inner: S) -> Trace<S> {
        Trace {
            name: self.0,
            log: self.1.clone(),
            inner,
        }
    }
}
impl<S: Service<Req>, Req: Send> Service<Req> for Trace<S> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        self.log.lock().unwrap().push(format!("{}:in", self.name));
        let t = Instant::now();
        let r = self.inner.call(req).await;
        let verdict = if r.is_ok() { "ok" } else { "err" };
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:out:{verdict}", self.name));
        let _elapsed = t.elapsed(); // a real layer would emit a metric/log line with this
        r
    }
}

// -------------------------------------------------------------- Timeout layer
struct TimeoutLayer(Duration);
struct Timeout<S> {
    inner: S,
    dur: Duration,
}
impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;
    fn layer(&self, inner: S) -> Timeout<S> {
        Timeout { inner, dur: self.0 }
    }
}
impl<S: Service<Req>, Req: Send> Service<Req> for Timeout<S> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        // dropping the inner future on timeout CANCELS it at its next await point
        tokio::time::timeout(self.dur, self.inner.call(req))
            .await
            .map_err(|_| Error::Timeout)?
    }
}

// ----------------------------------------------------------------- Retry layer
trait Policy: Clone + Send + Sync {
    /// `Some(backoff)` to try again after sleeping, `None` to give up. `attempt` counts failures so far (1-based).
    fn retry(&self, attempt: u32, err: &Error) -> Option<Duration>;
}
#[derive(Clone)]
struct ExpBackoff {
    max_attempts: u32, // total calls allowed, including the first
    base: Duration,
    cap: Duration,
}
impl ExpBackoff {
    fn delay(&self, attempt: u32) -> Duration {
        // `Duration * u32` panics on overflow, and `attempt - 1` would underflow for 0
        let factor = 2u32.saturating_pow(attempt.saturating_sub(1));
        self.base
            .checked_mul(factor)
            .unwrap_or(self.cap)
            .min(self.cap)
    }
}
impl Policy for ExpBackoff {
    fn retry(&self, attempt: u32, err: &Error) -> Option<Duration> {
        let retryable = matches!(err, Error::Failed(_) | Error::Timeout); // never hammer an open breaker or a rate limiter
        (retryable && attempt < self.max_attempts).then(|| self.delay(attempt))
    }
}
struct RetryLayer<P>(P);
struct Retry<S, P> {
    inner: S,
    policy: P,
}
impl<S, P: Policy> Layer<S> for RetryLayer<P> {
    type Service = Retry<S, P>;
    fn layer(&self, inner: S) -> Retry<S, P> {
        Retry {
            inner,
            policy: self.0.clone(),
        }
    }
}
impl<S: Service<Req>, P: Policy, Req: Clone + Send> Service<Req> for Retry<S, P> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        let mut failures = 0;
        loop {
            match self.inner.call(req.clone()).await {
                Err(e) => {
                    failures += 1;
                    match self.policy.retry(failures, &e) {
                        Some(d) => tokio::time::sleep(d).await,
                        None => return Err(e),
                    }
                }
                ok => return ok,
            }
        }
    }
}

// ---------------------------------------------------------- Concurrency limit
struct ConcurrencyLimitLayer(Arc<Semaphore>);
struct ConcurrencyLimit<S> {
    inner: S,
    sem: Arc<Semaphore>,
}
impl ConcurrencyLimitLayer {
    fn new(max: usize) -> Self {
        Self(Arc::new(Semaphore::new(max)))
    }
}
impl<S> Layer<S> for ConcurrencyLimitLayer {
    type Service = ConcurrencyLimit<S>;
    fn layer(&self, inner: S) -> ConcurrencyLimit<S> {
        ConcurrencyLimit {
            inner,
            sem: self.0.clone(),
        }
    }
}
impl<S: Service<Req>, Req: Send> Service<Req> for ConcurrencyLimit<S> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed"); // waits: backpressure
        self.inner.call(req).await // permit released when this future finishes OR is dropped
    }
}

// ----------------------------------------------------------------- Rate limit
/// Token bucket in integer milli-tokens (no float drift): `refill_per_sec` tokens/s up to `capacity` tokens.
struct Bucket {
    capacity: u64,
    refill_per_sec: u64,
    state: Mutex<(u64, Duration)>, // (milli-tokens, last refill time)
    clock: Arc<dyn Clock>,
}
impl Bucket {
    fn try_take(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        let now = self.clock.now();
        // a clock that went backwards counts as no time passed
        let whole_ms = u64::try_from(now.saturating_sub(s.1).as_millis()).unwrap_or(u64::MAX);
        let added = whole_ms.saturating_mul(self.refill_per_sec); // ms * tokens/s = milli-tokens
        s.0 =
            s.0.saturating_add(added)
                .min(self.capacity.saturating_mul(1000));
        // move the mark only past what was counted: with `s.1 = now`, calls under 1 ms apart never refill
        s.1 += Duration::from_millis(whole_ms);
        if s.0 >= 1000 {
            s.0 -= 1000;
            true
        } else {
            false
        }
    }
}
struct RateLimitLayer(Arc<Bucket>);
struct RateLimit<S> {
    inner: S,
    bucket: Arc<Bucket>,
}
impl RateLimitLayer {
    fn new(capacity: u64, refill_per_sec: u64, clock: Arc<dyn Clock>) -> Self {
        let state = Mutex::new((capacity.saturating_mul(1000), clock.now()));
        Self(Arc::new(Bucket {
            capacity,
            refill_per_sec,
            state,
            clock,
        }))
    }
}
impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimit<S>;
    fn layer(&self, inner: S) -> RateLimit<S> {
        RateLimit {
            inner,
            bucket: self.0.clone(),
        }
    }
}
impl<S: Service<Req>, Req: Send> Service<Req> for RateLimit<S> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        // tower's own RateLimit makes callers WAIT; shedding load with an error is the other common choice.
        if !self.bucket.try_take() {
            return Err(Error::RateLimited);
        }
        self.inner.call(req).await
    }
}

// ------------------------------------------------------------ Circuit breaker
#[derive(Debug, Clone, Copy, PartialEq)]
enum Breaker {
    Closed { failures: u32 },
    Open { until: Duration },
    HalfOpen, // exactly one probe call is in flight
}
/// The state, plus a number that goes up whenever the breaker changes phase. A call remembers the number it
/// started under, so a slow answer from an earlier phase cannot change the current one.
struct Core {
    state: Breaker,
    phase: u64,
}
impl Core {
    fn enter(&mut self, state: Breaker) {
        self.state = state;
        self.phase += 1;
    }
}
#[derive(Clone)]
struct Shared {
    threshold: u32,
    cooldown: Duration,
    clock: Arc<dyn Clock>,
    core: Arc<Mutex<Core>>,
}
impl Shared {
    /// `Err` while open; otherwise the phase this call belongs to, and whether it is the probe.
    fn admit(&self) -> Result<(u64, bool), Error> {
        let mut core = self.core.lock().unwrap();
        match core.state {
            Breaker::Open { until } if self.clock.now() >= until => {
                core.enter(Breaker::HalfOpen); // this call is the probe
                Ok((core.phase, true))
            }
            Breaker::Open { .. } | Breaker::HalfOpen => Err(Error::CircuitOpen),
            Breaker::Closed { .. } => Ok((core.phase, false)),
        }
    }
    fn record(&self, phase: u64, ok: bool) {
        let mut core = self.core.lock().unwrap();
        if core.phase != phase {
            return; // a late answer from before the breaker last changed
        }
        let open = Breaker::Open {
            until: self.clock.now().saturating_add(self.cooldown),
        };
        match (ok, core.state) {
            (true, Breaker::Closed { .. }) => core.state = Breaker::Closed { failures: 0 },
            (true, Breaker::HalfOpen) => core.enter(Breaker::Closed { failures: 0 }),
            (false, Breaker::HalfOpen) => core.enter(open),
            (false, Breaker::Closed { failures }) if failures + 1 >= self.threshold => {
                core.enter(open)
            }
            (false, Breaker::Closed { failures }) => {
                core.state = Breaker::Closed {
                    failures: failures + 1,
                }
            }
            (_, Breaker::Open { .. }) => {}
        }
    }
}
/// If the probe's future is dropped before it answers, count that as a failed probe. Without this the
/// breaker would stay `HalfOpen`, and refuse every later call.
struct ProbeGuard<'a> {
    shared: &'a Shared,
    phase: u64,
    armed: bool,
}
impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.shared.record(self.phase, false);
        }
    }
}
struct CircuitBreakerLayer(Shared);
struct CircuitBreaker<S> {
    inner: S,
    shared: Shared,
}
impl CircuitBreakerLayer {
    fn new(threshold: u32, cooldown: Duration, clock: Arc<dyn Clock>) -> Self {
        let core = Core {
            state: Breaker::Closed { failures: 0 },
            phase: 0,
        };
        Self(Shared {
            threshold,
            cooldown,
            clock,
            core: Arc::new(Mutex::new(core)),
        })
    }
    /// A handle that can still be read after the layer has built its service.
    fn watch(&self) -> impl Fn() -> Breaker + use<> {
        let core = self.0.core.clone();
        move || core.lock().unwrap().state
    }
}
impl<S> Layer<S> for CircuitBreakerLayer {
    type Service = CircuitBreaker<S>;
    fn layer(&self, inner: S) -> CircuitBreaker<S> {
        CircuitBreaker {
            inner,
            shared: self.0.clone(),
        }
    }
}
impl<S: Service<Req>, Req: Send> Service<Req> for CircuitBreaker<S> {
    type Response = S::Response;
    async fn call(&self, req: Req) -> Result<S::Response, Error> {
        // decide under the lock inside `admit`: a std Mutex is never held across an await
        let (phase, probe) = self.shared.admit()?;
        let mut guard = ProbeGuard {
            shared: &self.shared,
            phase,
            armed: probe,
        };
        let r = self.inner.call(req).await;
        guard.armed = false;
        self.shared.record(phase, r.is_ok());
        r
    }
}

// ------------------------------------------------------------------- the tests
fn backoff(max: u32) -> ExpBackoff {
    ExpBackoff {
        max_attempts: max,
        base: Duration::from_millis(2),
        cap: Duration::from_millis(20),
    }
}
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn test_retry() {
    let mock = Mock::new(vec![Step::Fail, Step::Fail, Step::Ok]);
    let svc = ServiceBuilder::new()
        .layer(RetryLayer(backoff(5)))
        .service(mock.clone());
    assert_eq!(svc.call(7).await, Ok("ok:7".into()));
    assert_eq!(mock.calls(), 3, "two failures then success");

    let always = Mock::new(vec![Step::Fail; 10]);
    let svc = ServiceBuilder::new()
        .layer(RetryLayer(backoff(4)))
        .service(always.clone());
    assert!(matches!(svc.call(1).await, Err(Error::Failed(_))));
    assert_eq!(always.calls(), 4, "max_attempts bounds total calls");
    // backoff schedule is a pure function, so it is asserted without sleeping
    let b = ExpBackoff {
        max_attempts: 9,
        base: ms(10),
        cap: ms(100),
    };
    let d: Vec<_> = (1..=6).map(|a| b.delay(a).as_millis()).collect();
    assert_eq!(d, [10, 20, 40, 80, 100, 100]);
    // edges: attempt 0, and a base so large that doubling it overflows a `Duration`
    assert_eq!(b.delay(0), ms(10));
    let huge = ExpBackoff {
        max_attempts: 9,
        base: Duration::from_secs(u64::MAX / 2),
        cap: ms(100),
    };
    assert_eq!(huge.delay(3), ms(100));
}
async fn test_timeout() {
    let slow = Mock::new(vec![Step::SleepThenOk(ms(300))]);
    let svc = ServiceBuilder::new()
        .layer(TimeoutLayer(ms(20)))
        .service(slow.clone());
    assert_eq!(svc.call(1).await, Err(Error::Timeout));
    assert_eq!(
        slow.in_flight.load(SeqCst),
        0,
        "the cancelled call has left"
    );
    assert_eq!(svc.call(2).await, Ok("ok:2".into()), "second call is fast");
}
async fn test_order() {
    // Retry OUTSIDE timeout: every attempt gets its own 20ms deadline, so a slow first attempt is retried and succeeds.
    let m = Mock::new(vec![Step::SleepThenOk(ms(300)), Step::Ok]);
    let svc = ServiceBuilder::new()
        .layer(RetryLayer(backoff(3)))
        .layer(TimeoutLayer(ms(20)))
        .service(m.clone());
    assert_eq!(svc.call(1).await, Ok("ok:1".into()));
    assert_eq!(m.calls(), 2);

    // always-slow service: per-attempt timeout => exactly max_attempts calls
    let m = Mock::new(vec![Step::SleepThenOk(ms(300)); 5]);
    let svc = ServiceBuilder::new()
        .layer(RetryLayer(backoff(3)))
        .layer(TimeoutLayer(ms(10)))
        .service(m.clone());
    assert_eq!(svc.call(1).await, Err(Error::Timeout));
    assert_eq!(m.calls(), 3);

    // Timeout OUTSIDE retry: one 60ms budget for ALL attempts and backoffs; the retry loop is cancelled mid-flight.
    let m = Mock::new(vec![Step::Fail; 100]);
    let policy = ExpBackoff {
        max_attempts: 100,
        base: ms(20),
        cap: ms(20),
    };
    let svc = ServiceBuilder::new()
        .layer(TimeoutLayer(ms(60)))
        .layer(RetryLayer(policy))
        .service(m.clone());
    assert_eq!(svc.call(1).await, Err(Error::Timeout));
    assert!(
        m.calls() < 100 && m.calls() >= 1,
        "budget cut the retries short: {}",
        m.calls()
    );

    // Layer order is also visible in a trace: first layer added is outermost.
    let log: Log = Default::default();
    let svc = ServiceBuilder::new()
        .layer(TraceLayer("outer", log.clone()))
        .layer(TraceLayer("inner", log.clone()))
        .service(Mock::new(vec![]));
    svc.call(1).await.unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        ["outer:in", "inner:in", "inner:out:ok", "outer:out:ok"]
    );
}
async fn test_concurrency() {
    // limited to 3: ten tasks, never more than 3 inside the mock at once
    let mock = Mock::new(vec![Step::SleepThenOk(ms(30)); 10]);
    let svc = Arc::new(
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(3))
            .service(mock.clone()),
    );
    let tasks: Vec<_> = (0..10u32)
        .map(|i| {
            let svc = svc.clone();
            tokio::spawn(async move { svc.call(i).await })
        })
        .collect();
    for t in tasks {
        assert!(t.await.unwrap().is_ok());
    }
    assert_eq!(mock.peak.load(SeqCst), 3, "cap respected");
    assert_eq!(mock.calls(), 10, "all work still completed");
    // unlimited baseline for contrast
    let mock = Mock::new(vec![Step::SleepThenOk(ms(30)); 10]);
    let svc = Arc::new(ServiceBuilder::new().service(mock.clone()));
    let tasks: Vec<_> = (0..10u32)
        .map(|i| {
            let svc = svc.clone();
            tokio::spawn(async move { svc.call(i).await })
        })
        .collect();
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    assert!(
        mock.peak.load(SeqCst) > 3,
        "without the layer concurrency is {}",
        mock.peak.load(SeqCst)
    );
}
async fn test_rate_limit() {
    let clock = Arc::new(ManualClock::default());
    let mock = Mock::new(vec![]);
    let svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(3, 2, clock.clone()))
        .service(mock.clone());
    let mut verdicts = vec![];
    for i in 0..5 {
        verdicts.push(svc.call(i).await.is_ok());
    }
    assert_eq!(
        verdicts,
        [true, true, true, false, false],
        "burst of 3 then shed"
    );
    assert_eq!(
        mock.calls(),
        3,
        "rejected calls never reach the inner service"
    );
    clock.advance(ms(1000)); // 2 tokens/s * 1s = 2 tokens
    assert!(svc.call(10).await.is_ok() && svc.call(11).await.is_ok());
    assert_eq!(svc.call(12).await, Err(Error::RateLimited));
    clock.advance(Duration::from_secs(60)); // refill is capped at capacity
    let ok = (0..10).map(|i| svc.call(i)).collect::<Vec<_>>();
    let mut n = 0;
    for f in ok {
        n += f.await.is_ok() as u32;
    }
    assert_eq!(n, 3);

    // calls half a millisecond apart still earn tokens: 1 second at 2/s gives 2 (after the burst of 1)
    let clock = Arc::new(ManualClock::default());
    let svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(1, 2, clock.clone()))
        .service(Mock::new(vec![]));
    let mut admitted = 0;
    for i in 0..=2000 {
        admitted += svc.call(i).await.is_ok() as u32;
        clock.advance(Duration::from_micros(500));
    }
    assert_eq!(admitted, 3);
    // a clock stepped backwards must not panic, and earns nothing
    clock.set(ms(0));
    assert_eq!(svc.call(0).await, Err(Error::RateLimited));

    // an enormous rate: the multiplication saturates instead of wrapping round to nothing
    let clock = Arc::new(ManualClock::default());
    let svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(1, 1 << 63, clock.clone()))
        .service(Mock::new(vec![]));
    assert!(svc.call(1).await.is_ok());
    clock.advance(ms(2));
    assert!(svc.call(2).await.is_ok());
}
async fn test_breaker() {
    let clock = Arc::new(ManualClock::default());
    let breaker = CircuitBreakerLayer::new(3, ms(1000), clock.clone());
    // 3 failures, then the service would recover (4th scripted call is Ok)
    let mock = Mock::new(vec![Step::Fail, Step::Fail, Step::Fail, Step::Ok]);
    let state = breaker.watch();
    let svc = ServiceBuilder::new().layer(breaker).service(mock.clone());
    for _ in 0..3 {
        assert!(matches!(svc.call(1).await, Err(Error::Failed(_))));
    }
    assert_eq!(
        state(),
        Breaker::Open { until: ms(1000) },
        "opened after exactly 3 failures"
    );
    assert_eq!(svc.call(1).await, Err(Error::CircuitOpen));
    assert_eq!(
        mock.calls(),
        3,
        "open breaker fails fast without calling the dependency"
    );
    clock.advance(ms(999));
    assert_eq!(
        svc.call(1).await,
        Err(Error::CircuitOpen),
        "still cooling down"
    );
    clock.advance(ms(1));
    assert_eq!(
        svc.call(1).await,
        Ok("ok:1".into()),
        "half-open probe succeeds"
    );
    assert_eq!(state(), Breaker::Closed { failures: 0 });
    assert_eq!(mock.calls(), 4);

    // a failing probe re-opens the breaker for another cooldown
    let breaker = CircuitBreakerLayer::new(1, ms(500), clock.clone());
    let mock = Mock::new(vec![Step::Fail, Step::Fail]);
    let state = breaker.watch();
    let svc = ServiceBuilder::new().layer(breaker).service(mock.clone());
    assert!(svc.call(1).await.is_err());
    clock.advance(ms(500));
    assert!(
        matches!(svc.call(1).await, Err(Error::Failed(_))),
        "probe reached the dependency and failed"
    );
    assert!(matches!(state(), Breaker::Open { .. }));
    assert_eq!(svc.call(1).await, Err(Error::CircuitOpen));
    assert_eq!(mock.calls(), 2);

    // a slow call from before the breaker opened succeeds while the probe is still running:
    // only the probe may close the breaker
    let (old, probe) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let breaker = CircuitBreakerLayer::new(1, ms(500), clock.clone());
    let state = breaker.watch();
    let steps = vec![
        Step::WaitThenOk(old.clone()),
        Step::Fail,
        Step::WaitThenOk(probe.clone()),
    ];
    let svc = ServiceBuilder::new()
        .layer(breaker)
        .service(Mock::new(steps));
    let mut slow = std::pin::pin!(svc.call(1));
    assert!(poll!(&mut slow).is_pending()); // admitted while closed
    assert!(svc.call(2).await.is_err()); // opens the breaker
    clock.advance(ms(500));
    let mut probing = std::pin::pin!(svc.call(3));
    assert!(poll!(&mut probing).is_pending());
    old.notify_one();
    assert!(slow.await.is_ok());
    assert_eq!(
        state(),
        Breaker::HalfOpen,
        "a stale success changes nothing"
    );
    probe.notify_one();
    assert!(probing.await.is_ok());
    assert_eq!(state(), Breaker::Closed { failures: 0 });

    // the probe's caller gives up: the breaker must not stay half-open for ever
    let never = Arc::new(Notify::new());
    let breaker = CircuitBreakerLayer::new(1, ms(500), clock.clone());
    let state = breaker.watch();
    let mock = Mock::new(vec![Step::Fail, Step::WaitThenOk(never)]);
    let svc = ServiceBuilder::new().layer(breaker).service(mock);
    assert!(svc.call(1).await.is_err());
    clock.advance(ms(500));
    let gave_up = tokio::time::timeout(ms(10), svc.call(2)).await;
    assert!(gave_up.is_err());
    assert!(matches!(state(), Breaker::Open { .. }), "{:?}", state());
    clock.advance(ms(500));
    assert_eq!(
        svc.call(3).await,
        Ok("ok:3".into()),
        "a new probe gets through"
    );
}
async fn test_breaker_and_retry_order() {
    let clock = Arc::new(ManualClock::default());
    // retry OUTSIDE the breaker: every attempt is counted, and the open breaker ends the retrying
    let breaker = CircuitBreakerLayer::new(3, ms(1000), clock.clone());
    let mock = Mock::new(vec![Step::Fail; 10]);
    let svc = ServiceBuilder::new()
        .layer(RetryLayer(backoff(5)))
        .layer(breaker)
        .service(mock.clone());
    assert_eq!(svc.call(1).await, Err(Error::CircuitOpen));
    assert_eq!(mock.calls(), 3);

    // breaker OUTSIDE the retry: five attempts are one failure to the breaker
    let breaker = CircuitBreakerLayer::new(3, ms(1000), clock);
    let state = breaker.watch();
    let mock = Mock::new(vec![Step::Fail; 10]);
    let svc = ServiceBuilder::new()
        .layer(breaker)
        .layer(RetryLayer(backoff(5)))
        .service(mock.clone());
    assert!(matches!(svc.call(1).await, Err(Error::Failed(_))));
    assert_eq!(
        (mock.calls(), state()),
        (5, Breaker::Closed { failures: 1 })
    );
}
async fn test_full_stack() {
    // A realistic client stack: trace -> retry -> timeout per attempt -> rate limit, over a flaky dependency.
    let clock = Arc::new(ManualClock::default());
    let log: Log = Default::default();
    let mock = Mock::new(vec![Step::Fail, Step::SleepThenOk(ms(200)), Step::Ok]);
    let svc = ServiceBuilder::new()
        .layer(TraceLayer("client", log.clone()))
        .layer(RetryLayer(backoff(5)))
        .layer(TimeoutLayer(ms(30)))
        .layer(RateLimitLayer::new(100, 100, clock))
        .service(mock.clone());
    assert_eq!(svc.call(42).await, Ok("ok:42".into()));
    assert_eq!(mock.calls(), 3, "fail, timeout, success");
    assert_eq!(
        *log.lock().unwrap(),
        ["client:in", "client:out:ok"],
        "the trace sees ONE logical call"
    );
}

/// An axum `Router` IS a tower service; the real `tower` crate drives it without a socket.
async fn test_axum_is_a_service() {
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;
    let app = Router::new().route("/hi", get(|| async { "hello" }));
    let resp = app
        .oneshot(Request::get("/hi").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let all = async {
            test_retry().await;
            println!("retry:       counts and backoff schedule ok");
            test_timeout().await;
            println!("timeout:     slow call cancelled, fast call ok");
            test_order().await;
            println!("order:       retry-outside-timeout vs timeout-outside-retry ok");
            test_concurrency().await;
            println!("concurrency: peak in-flight == 3 under limit 3");
            test_rate_limit().await;
            println!("rate limit:  burst 3, refill 2/s, capped bucket");
            test_breaker().await;
            println!("breaker:     opens after 3, half-open probe recovers / re-opens");
            test_breaker_and_retry_order().await;
            println!("order:       retry-outside-breaker vs breaker-outside-retry ok");
            test_full_stack().await;
            println!("stack:       trace -> retry -> timeout -> rate limit composed");
            test_axum_is_a_service().await;
            println!("axum:        Router::oneshot via tower::ServiceExt");
        };
        tokio::time::timeout(Duration::from_secs(15), all)
            .await
            .expect("example timed out");
    });
}
