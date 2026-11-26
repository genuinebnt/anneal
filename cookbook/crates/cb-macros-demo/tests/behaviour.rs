#![allow(dead_code)]
use cb_macros_demo::*;
use std::cell::Cell;

#[derive(Describe)]
struct Point {
    x: i32,
    y: Vec<u8>,
}
#[derive(Describe)]
struct Wrap<T: Clone>(T)
where
    T: Send;
#[derive(Describe)]
enum Shape {
    Circle,
    Square(u8),
}

#[test]
fn describe_struct_enum_generics() {
    assert_eq!(Point::describe(), "struct Point { x: i32, y: Vec<u8> }");
    assert_eq!(Wrap::<u8>::describe(), "struct Wrap(T)");
    assert_eq!(Shape::describe(), "enum Shape { Circle, Square }");
}

#[derive(Builder, Debug, PartialEq)]
struct Server {
    host: String,
    port: u16,
    tls: Option<bool>,
}

#[derive(Builder, Debug)]
struct G<T: Clone>
where
    T: Send,
{
    v: T,
}

#[test]
fn builder_required_and_optional() {
    let s = Server::builder().host("h".into()).port(80).build().unwrap();
    assert_eq!(
        s,
        Server {
            host: "h".into(),
            port: 80,
            tls: None
        }
    );
    let s = Server::builder()
        .host("h".into())
        .port(80)
        .tls(true)
        .build()
        .unwrap();
    assert_eq!(s.tls, Some(true));
    let e = Server::builder().host("h".into()).build().unwrap_err();
    assert_eq!(e, "missing required field `port`");
    assert_eq!(G::builder().v(3u8).build().unwrap().v, 3);
}

/// Doc comments survive.
#[timed]
pub fn add(a: i32, b: i32) -> i32 {
    if a < 0 {
        return 0;
    }
    a + b
}

#[timed]
async fn aadd(a: i32) -> i32 {
    a + 1
}

#[test]
fn timed_preserves_behaviour() {
    assert_eq!(add(1, 2), 3);
    assert_eq!(add(-1, 2), 0); // early return still works
    // Poll the future by hand: no runtime dependency needed.
    let mut fut = std::pin::pin!(aadd(1));
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    assert_eq!(fut.as_mut().poll(&mut cx), std::task::Poll::Ready(2));
}
use std::future::Future;

thread_local!(static CALLS: Cell<u32> = const { Cell::new(0) });

#[memoize]
fn fib(n: u64) -> u64 {
    CALLS.with(|c| c.set(c.get() + 1));
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

#[test]
fn memoize_caches() {
    assert_eq!(fib(30), 832040);
    assert_eq!(CALLS.with(|c| c.get()), 31); // exponential without the cache
    assert_eq!(fib(30), 832040);
    assert_eq!(CALLS.with(|c| c.get()), 31);
}

#[test]
fn hashmap_macro() {
    let m = hashmap! { "a" => 1, "b" => 1 + 1, };
    assert_eq!(m["b"], 2);
    assert_eq!(m.len(), 2);
    let e: std::collections::HashMap<u8, u8> = hashmap! {};
    assert!(e.is_empty());
}

const_assert!(std::mem::size_of::<u32>() == 4, "u32 must be 4 bytes");

#[test]
fn const_assert_passes() {}
