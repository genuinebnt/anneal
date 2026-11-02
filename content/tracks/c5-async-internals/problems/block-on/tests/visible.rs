use solution::*;

#[test]
fn ready_value() {
    check!(r#"an async block"#, block_on(async { 1 + 2 }), 3);
}

#[test]
fn awaits_chain() {
    check!(r#"await inside await"#, block_on(async { let a = async { 20 }.await; let b = async { 22 }.await; a + b }), 42);
}

#[test]
fn self_waking_future() {
    check!(r#"pending 5 times, waking itself each time"#, { let mut n = 5; let mut polls = 0; let out = block_on(std::future::poll_fn(|cx| { polls += 1; if n == 0 { std::task::Poll::Ready(7) } else { n -= 1; cx.waker().wake_by_ref(); std::task::Poll::Pending } })); (out, polls) }, (7, 6));
}

#[test]
fn borrows_from_the_caller() {
    check!(r#"a non-'static future"#, { let data = vec![1, 2, 3]; block_on(async { data.iter().sum::<i32>() }) }, 6);
}

#[test]
fn moves_the_output_out() {
    check!(r#"a String comes back"#, block_on(async { format!("{}-{}", "a", 1) }), "a-1".to_string());
}
