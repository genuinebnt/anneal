//! Tests for module R (optional): the Rust a storage engine needs before module 1a: bytes and positional I/O, errors as values, ownership
//! shapes, shared state, and testing against a model.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use proptest::prelude::*;

use bustub::rust_primer::bytes::{pack_u32s, unpack_u32s, Header, PageFile, PAGE};
use bustub::rust_primer::records::{encode_records, load_records, parse_records, FormatError, LoadError, Record};
use bustub::rust_primer::ring::RingBuffer;
use bustub::rust_primer::shapes::{Slab, Stack};
use bustub::rust_primer::shared::{parallel_sum, BoundedQueue, IdGenerator};

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("anneal-r-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_file(&dir);
    dir
}

/// Runs `f` on its own thread and fails the test, instead of hanging, if it does not finish: a loop that never ends, or a thread waiting for a wake-up that never came.
fn finish_within<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)));
    });
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(_) => panic!("{what}: still waiting after 10 seconds. Something waits for a thing that never comes: a loop that never ends, or a thread nobody wakes."),
    }
}

// ---- r-01: bytes and positional I/O -------------------------------------------------------------------------------------------------------

fn page_of(byte: u8) -> [u8; PAGE] {
    [byte; PAGE]
}

#[test]
fn sr_01_a_header_is_sixteen_little_endian_bytes() {
    let h = Header { magic: 0x0102_0304, version: 0x0506, flags: 0x0708, len: 0x090A_0B0C_0D0E_0F10 };
    assert_eq!(
        h.to_bytes(),
        [0x04, 0x03, 0x02, 0x01, 0x06, 0x05, 0x08, 0x07, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, 0x09],
        "magic, version, flags and len, each little-endian, one after the other"
    );
}

#[test]
fn sr_01_a_header_reads_back_and_wrong_sizes_are_refused() {
    let h = Header { magic: 0xDEAD_BEEF, version: 7, flags: 0xF00D, len: u64::MAX - 5 };
    assert_eq!(Header::from_bytes(&h.to_bytes()), Some(h), "Header::from_bytes(&h.to_bytes()) == Some(h)");
    assert_eq!(Header::from_bytes(&[0; 15]), None, "15 bytes are not a header");
    assert_eq!(Header::from_bytes(&[0; 17]), None, "17 bytes are not a header");
    assert_eq!(Header::from_bytes(&[]), None, "no bytes are not a header");
}

#[test]
fn sr_01_numbers_pack_four_bytes_each() {
    assert_eq!(pack_u32s(&[1, 256]), vec![1, 0, 0, 0, 0, 1, 0, 0], "1 and 256, little-endian");
    assert_eq!(pack_u32s(&[]), Vec::<u8>::new(), "sr 01 numbers pack four bytes each");
    assert_eq!(unpack_u32s(&[1, 0, 0, 0, 0, 1, 0, 0]), vec![1, 256], "sr 01 numbers pack four bytes each");
    assert_eq!(unpack_u32s(&[1, 0, 0, 0, 9, 9]), vec![1], "a last group of fewer than 4 bytes is ignored");
}

#[test]
fn sr_01_a_page_file_keeps_what_was_written_where_it_was_written() {
    finish_within("a page file keeps what was written where it was written", move || {
        let path = temp_path("pages");
        let file = PageFile::open(&path).unwrap();
        file.write_page(2, &page_of(0xAA)).unwrap();
        file.write_page(0, &page_of(0x11)).unwrap();
        assert_eq!(file.read_page(2).unwrap(), page_of(0xAA), "sr 01 a page file keeps what was written where it was written");
        assert_eq!(file.read_page(0).unwrap(), page_of(0x11), "sr 01 a page file keeps what was written where it was written");
        assert_eq!(file.read_page(1).unwrap(), page_of(0), "page 1 was never written: zeros (a hole in the file)");
        assert_eq!(file.page_count().unwrap(), 3, "pages 0 to 2 exist");
        let _ = std::fs::remove_file(&path);
    });
}

#[test]
fn sr_01_reading_beyond_the_end_gives_zeros_and_reopening_keeps_the_data() {
    finish_within("reading beyond the end gives zeros and reopening keeps the data", move || {
        let path = temp_path("reopen");
        {
            let file = PageFile::open(&path).unwrap();
            assert_eq!(file.page_count().unwrap(), 0, "a new file is empty");
            file.write_page(0, &page_of(7)).unwrap();
            assert_eq!(file.read_page(40).unwrap(), page_of(0), "far beyond the end of the file");
        }
        let file = PageFile::open(&path).unwrap();
        assert_eq!(file.read_page(0).unwrap(), page_of(7), "opening an existing file keeps its contents (do not truncate)");
        let _ = std::fs::remove_file(&path);
    });
}

#[test]
fn sr_01_a_partly_written_last_page_reads_with_zeros_after_its_bytes() {
    finish_within("a partly written last page reads with zeros after its bytes", move || {
        use std::io::Write;
        let path = temp_path("partial");
        std::fs::File::create(&path).unwrap().write_all(&[5u8; PAGE + 10]).unwrap();
        let file = PageFile::open(&path).unwrap();
        assert_eq!(file.page_count().unwrap(), 2, "256 + 10 bytes: one full page and a part of a second");
        let page = file.read_page(1).unwrap();
        assert_eq!(&page[..10], &[5u8; 10], "sr 01 a partly written last page reads with zeros after its bytes");
        assert!(page[10..].iter().all(|&b| b == 0), "the part after the end of the file reads as zeros");
        let _ = std::fs::remove_file(&path);
    });
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a header always reads back as written, and its bytes are what the format says.
    #[test]
    fn sr_01_property_headers_round_trip(magic in any::<u32>(), version in any::<u16>(), flags in any::<u16>(), len in any::<u64>()) {
        let h = Header { magic, version, flags, len };
        let bytes = h.to_bytes();
        prop_assert_eq!(&bytes[0..4], &magic.to_le_bytes()[..]);
        prop_assert_eq!(&bytes[8..16], &len.to_le_bytes()[..]);
        prop_assert_eq!(Header::from_bytes(&bytes), Some(h));
    }

    /// Property: numbers survive packing, and packing is four bytes per number.
    #[test]
    fn sr_01_property_numbers_round_trip(values in proptest::collection::vec(any::<u32>(), 0..64)) {
        let bytes = pack_u32s(&values);
        prop_assert_eq!(bytes.len(), values.len() * 4);
        prop_assert_eq!(unpack_u32s(&bytes), values);
    }
}

/// Property: a page file behaves like an array of pages that start as zeros.
#[test]
fn sr_01_property_a_page_file_is_an_array_of_pages() {
    use proptest::test_runner::{Config, TestRunner};
    finish_within("a page file is an array of pages", move || {
        let mut runner = TestRunner::new(Config { cases: 48, failure_persistence: None, ..Config::default() });
        let strategy = proptest::collection::vec((0u64..8, any::<u8>(), any::<bool>()), 1..40);
        let result = runner.run(&strategy, |ops| {
            let path = temp_path("prop");
            let file = PageFile::open(&path).unwrap();
            let mut model: Vec<[u8; PAGE]> = vec![[0; PAGE]; 8];
            let mut written = 0u64;
            for (index, byte, write) in ops {
                if write {
                    file.write_page(index, &page_of(byte)).unwrap();
                    model[index as usize] = page_of(byte);
                    written = written.max(index + 1);
                } else {
                    prop_assert_eq!(file.read_page(index).unwrap(), model[index as usize]);
                }
            }
            prop_assert_eq!(file.page_count().unwrap(), written);
            let _ = std::fs::remove_file(&path);
            Ok(())
        });
        if let Err(e) = result {
            panic!("{e}");
        }
    });
}

// ---- r-02: errors as values ---------------------------------------------------------------------------------------------------------------

fn rec(key: &str, value: &[u8]) -> Record {
    Record { key: key.to_string(), value: value.to_vec() }
}

#[test]
fn sr_02_records_are_a_length_header_then_the_key_then_the_value() {
    let bytes = encode_records(&[rec("ab", &[9, 8, 7])]).unwrap();
    assert_eq!(bytes, vec![2, 3, 0, b'a', b'b', 9, 8, 7], "key_len (u8), value_len (u16 little-endian), key, value");
    assert_eq!(encode_records(&[]).unwrap(), Vec::<u8>::new(), "sr 02 records are a length header then the key then the value");
}

#[test]
fn sr_02_records_read_back_in_order() {
    let records = vec![rec("a", b"1"), rec("", b""), rec("héllo", &[0; 300])];
    let bytes = encode_records(&records).unwrap();
    assert_eq!(parse_records(&bytes), Ok(records), "sr 02 records read back in order");
    assert_eq!(parse_records(&[]), Ok(vec![]), "sr 02 records read back in order");
}

#[test]
fn sr_02_a_truncated_stream_says_where_and_how_much_is_missing() {
    let mut bytes = encode_records(&[rec("a", b"12"), rec("bc", b"xyz")]).unwrap();
    let first = 3 + 1 + 2;
    bytes.pop();
    assert_eq!(
        parse_records(&bytes),
        Err(FormatError::Truncated { at: first, needed: 3 + 2 + 3, have: 3 + 2 + 3 - 1 }),
        "the second record starts at byte {first} and is one byte short"
    );
    assert_eq!(parse_records(&[1, 0]), Err(FormatError::Truncated { at: 0, needed: 3, have: 2 }), "not even a whole header");
}

#[test]
fn sr_02_a_key_that_is_not_utf8_is_an_error_not_a_panic() {
    let bytes = vec![2, 0, 0, 0xFF, 0xFE];
    assert_eq!(parse_records(&bytes), Err(FormatError::BadKey { at: 0 }), "sr 02 a key that is not utf8 is an error not a panic");
    let mut two = encode_records(&[rec("ok", b"")]).unwrap();
    let at = two.len();
    two.extend_from_slice(&[1, 0, 0, 0xC0]);
    assert_eq!(parse_records(&two), Err(FormatError::BadKey { at }), "the offset is where the bad record starts");
}

#[test]
fn sr_02_a_key_or_value_that_does_not_fit_is_refused() {
    let long_key = "k".repeat(256);
    assert_eq!(
        encode_records(&[rec(&long_key, b"")]),
        Err(FormatError::KeyTooLong { len: 256 }),
        "sr 02 a key or value that does not fit is refused"
    );
    assert!(encode_records(&[rec(&"k".repeat(255), b"")]).is_ok(), "255 bytes still fit");
    assert_eq!(
        encode_records(&[rec("k", &vec![0; 65536])]),
        Err(FormatError::ValueTooLong { len: 65536 }),
        "sr 02 a key or value that does not fit is refused"
    );
    assert!(
        encode_records(&[rec("k", &vec![0; 65535])]).is_ok(),
        "sr 02 a key or value that does not fit is refused: expected `encode_records(&[rec(\"k\", &vec![0; 65535])]).is_ok()`"
    );
}

#[test]
fn sr_02_errors_explain_themselves() {
    let e = FormatError::Truncated { at: 12, needed: 8, have: 3 };
    let text = e.to_string();
    assert!(text.contains("12") && text.contains('8') && text.contains('3'), "the message names its numbers: {text}");
    let text = FormatError::KeyTooLong { len: 300 }.to_string();
    assert!(text.contains("300"), "the message names its numbers: {text}");
}

#[test]
fn sr_02_a_load_error_keeps_its_cause_and_tells_the_two_failures_apart() {
    use std::error::Error;
    let missing = load_records(temp_path("does-not-exist"));
    match missing {
        Err(LoadError::Io(ref e)) => {
            assert_eq!(e.kind(), std::io::ErrorKind::NotFound, "sr 02 a load error keeps its cause and tells the two failures apart")
        }
        other => panic!("a missing file is an I/O error, got {other:?}"),
    }
    let err = missing.unwrap_err();
    assert!(err.source().is_some(), "`source()` is the wrapped error");
    assert!(
        !err.to_string().is_empty(),
        "sr 02 a load error keeps its cause and tells the two failures apart: expected `!err.to_string().is_empty()`"
    );

    let path = temp_path("broken-records");
    std::fs::write(&path, [5, 0, 0, b'x']).unwrap();
    match load_records(&path) {
        Err(LoadError::Format(FormatError::Truncated { at: 0, .. })) => {}
        other => panic!("a truncated file is a format error, got {other:?}"),
    }
    std::fs::write(&path, encode_records(&[rec("a", b"b")]).unwrap()).unwrap();
    assert_eq!(load_records(&path).unwrap(), vec![rec("a", b"b")], "sr 02 a load error keeps its cause and tells the two failures apart");
    let _ = std::fs::remove_file(&path);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: records encode and parse back, whatever they hold.
    #[test]
    fn sr_02_property_records_round_trip(items in proptest::collection::vec(("\\PC{0,12}", proptest::collection::vec(any::<u8>(), 0..40)), 0..12)) {
        let records: Vec<Record> = items.into_iter().map(|(key, value)| Record { key, value }).collect();
        let bytes = encode_records(&records).unwrap();
        prop_assert_eq!(parse_records(&bytes), Ok(records));
    }

    /// Property: parsing never panics, whatever the bytes are; and every strict prefix of a good stream is either complete records or an error.
    #[test]
    fn sr_02_property_parsing_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
        let _ = parse_records(&bytes);
    }

    #[test]
    fn sr_02_property_a_cut_stream_is_an_error_or_a_shorter_list(items in proptest::collection::vec(("[a-z]{0,6}", proptest::collection::vec(any::<u8>(), 0..10)), 1..8), cut in any::<prop::sample::Index>()) {
        let records: Vec<Record> = items.into_iter().map(|(key, value)| Record { key, value }).collect();
        let bytes = encode_records(&records).unwrap();
        let cut = cut.index(bytes.len());
        match parse_records(&bytes[..cut]) {
            Ok(parsed) => {
                prop_assert!(parsed.len() < records.len(), "a cut stream cannot hold every record");
                prop_assert_eq!(&parsed[..], &records[..parsed.len()]);
            }
            Err(FormatError::Truncated { at, needed, have }) => {
                prop_assert!(at <= cut && have < needed && at + have == cut, "the numbers describe what was left: at {at}, needed {needed}, have {have}, cut {cut}");
            }
            Err(other) => prop_assert!(false, "only Truncated is possible for ASCII keys, got {other:?}"),
        }
    }
}

// ---- r-03: ownership shapes ---------------------------------------------------------------------------------------------------------------

#[test]
fn sr_03_a_stack_pops_in_reverse_order() {
    let mut s = Stack::new();
    assert!(
        s.is_empty() && s.pop().is_none() && s.peek().is_none(),
        "sr 03 a stack pops in reverse order: expected `s.is_empty() && s.pop().is_none() && s.peek().is_none()`"
    );
    for i in 1..=3 {
        s.push(i);
    }
    assert_eq!(s.len(), 3, "sr 03 a stack pops in reverse order");
    assert_eq!(s.peek(), Some(&3), "sr 03 a stack pops in reverse order");
    assert_eq!(s.pop(), Some(3), "sr 03 a stack pops in reverse order");
    assert_eq!(s.pop(), Some(2), "sr 03 a stack pops in reverse order");
    assert_eq!(s.pop(), Some(1), "sr 03 a stack pops in reverse order");
    assert_eq!(s.pop(), None, "sr 03 a stack pops in reverse order");
    assert_eq!(s.len(), 0, "sr 03 a stack pops in reverse order");
}

#[test]
fn sr_03_peek_mut_changes_the_top_in_place() {
    let mut s = Stack::new();
    s.push(String::from("a"));
    s.push(String::from("b"));
    s.peek_mut().unwrap().push('!');
    assert_eq!(s.to_vec(), vec!["b!".to_string(), "a".to_string()], "sr 03 peek mut changes the top in place");
}

#[test]
fn sr_03_reverse_turns_the_stack_upside_down() {
    let mut s = Stack::new();
    for i in 0..5 {
        s.push(i);
    }
    s.reverse();
    assert_eq!(s.to_vec(), vec![0, 1, 2, 3, 4], "sr 03 reverse turns the stack upside down");
    assert_eq!(s.len(), 5, "sr 03 reverse turns the stack upside down");
    let mut empty: Stack<i32> = Stack::new();
    empty.reverse();
    assert!(empty.is_empty(), "sr 03 reverse turns the stack upside down: expected `empty.is_empty()`");
}

#[test]
fn sr_03_values_are_moved_not_cloned_and_dropped_exactly_once() {
    use std::rc::Rc;
    let token = Rc::new(());
    {
        let mut s = Stack::new();
        for _ in 0..10 {
            s.push(Rc::clone(&token));
        }
        assert_eq!(Rc::strong_count(&token), 11, "pushing moves the Rc in: one count each, no extra clones");
        s.reverse();
        assert_eq!(Rc::strong_count(&token), 11, "reverse moves nodes, it does not clone values");
        drop(s.pop());
        assert_eq!(Rc::strong_count(&token), 10, "a popped value is dropped when the caller drops it");
    }
    assert_eq!(Rc::strong_count(&token), 1, "dropping the stack drops every value left in it");
}

#[test]
fn sr_03_a_very_long_stack_can_be_dropped() {
    // A stack overflow aborts the whole process, so the real work runs in a child process that this test starts again.
    if std::env::var_os("ANNEAL_R_DEEP_DROP").is_some() {
        let mut s = Stack::new();
        for i in 0..200_000u32 {
            s.push(i);
        }
        drop(s);
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["sr_03_a_very_long_stack_can_be_dropped", "--exact", "--test-threads=1"])
        .env("ANNEAL_R_DEEP_DROP", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "dropping a stack of 200 000 nodes crashed the program (a recursive drop overflows the call stack): drop the nodes in a loop"
    );
}

#[test]
fn sr_03_a_slab_hands_out_handles_and_gives_values_back() {
    let mut slab = Slab::new();
    let a = slab.insert("a");
    let b = slab.insert("b");
    assert_eq!((slab.get(a), slab.get(b)), (Some(&"a"), Some(&"b")), "sr 03 a slab hands out handles and gives values back");
    assert_eq!(slab.len(), 2, "sr 03 a slab hands out handles and gives values back");
    *slab.get_mut(a).unwrap() = "A";
    assert_eq!(slab.get(a), Some(&"A"), "sr 03 a slab hands out handles and gives values back");
    assert_eq!(slab.remove(a), Some("A"), "sr 03 a slab hands out handles and gives values back");
    assert_eq!(slab.get(a), None, "a removed value is gone");
    assert_eq!(slab.remove(a), None, "removing twice finds nothing");
    assert_eq!(slab.len(), 1, "sr 03 a slab hands out handles and gives values back");
}

#[test]
fn sr_03_a_handle_to_a_removed_value_never_finds_the_value_that_reuses_its_slot() {
    let mut slab = Slab::new();
    let old = slab.insert(1);
    slab.remove(old);
    let new = slab.insert(2);
    assert_ne!(old, new, "same slot, other generation: the handles differ");
    assert_eq!(slab.get(old), None, "the old handle is dead even though its slot is in use again");
    assert_eq!(slab.get(new), Some(&2), "sr 03 a handle to a removed value never finds the value that reuses its slot");
    assert_eq!(slab.remove(old), None, "sr 03 a handle to a removed value never finds the value that reuses its slot");
    assert_eq!(slab.len(), 1, "sr 03 a handle to a removed value never finds the value that reuses its slot");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a stack is a vector that grows and shrinks at one end.
    #[test]
    fn sr_03_property_a_stack_is_a_vec(ops in proptest::collection::vec(prop_oneof![any::<i32>().prop_map(Some), Just(None)], 0..80)) {
        let mut stack = Stack::new();
        let mut model: Vec<i32> = Vec::new();
        for op in ops {
            match op {
                Some(v) => { stack.push(v); model.push(v); }
                None => prop_assert_eq!(stack.pop(), model.pop()),
            }
            prop_assert_eq!(stack.len(), model.len());
            prop_assert_eq!(stack.peek(), model.last());
        }
        let mut top_down = model.clone();
        top_down.reverse();
        prop_assert_eq!(stack.to_vec(), top_down);
        stack.reverse();
        prop_assert_eq!(stack.to_vec(), model);
    }

    /// Property: a slab is a map from handles to values; handles of removed values stay dead.
    #[test]
    fn sr_03_property_a_slab_is_a_map(ops in proptest::collection::vec((0u8..3, any::<u16>(), any::<prop::sample::Index>()), 0..120)) {
        let mut slab = Slab::new();
        let mut live: Vec<(bustub::rust_primer::shapes::Handle, u16)> = Vec::new();
        let mut dead = Vec::new();
        for (op, value, pick) in ops {
            match op {
                0 => live.push((slab.insert(value), value)),
                1 if !live.is_empty() => {
                    let (h, v) = live.swap_remove(pick.index(live.len()));
                    prop_assert_eq!(slab.remove(h), Some(v));
                    dead.push(h);
                }
                _ if !live.is_empty() => {
                    let (h, v) = live[pick.index(live.len())];
                    prop_assert_eq!(slab.get(h), Some(&v));
                }
                _ => {}
            }
            prop_assert_eq!(slab.len(), live.len());
            for h in &dead {
                prop_assert_eq!(slab.get(*h), None, "a dead handle stays dead");
            }
        }
    }
}

// ---- r-04: shared state -------------------------------------------------------------------------------------------------------------------

#[test]
fn sr_04_a_queue_gives_values_back_in_order() {
    let q = BoundedQueue::new(4);
    assert!(
        q.is_empty() && q.try_pop().is_none(),
        "sr 04 a queue gives values back in order: expected `q.is_empty() && q.try_pop().is_none()`"
    );
    for i in 0..3 {
        q.push(i).unwrap();
    }
    assert_eq!(q.len(), 3, "sr 04 a queue gives values back in order");
    assert_eq!((q.pop(), q.try_pop(), q.pop()), (Some(0), Some(1), Some(2)), "sr 04 a queue gives values back in order");
    assert!(q.try_pop().is_none(), "sr 04 a queue gives values back in order: expected `q.try_pop().is_none()`");
}

#[test]
fn sr_04_a_closed_queue_refuses_pushes_and_drains_what_is_left() {
    let q = BoundedQueue::new(4);
    q.push(1).unwrap();
    q.push(2).unwrap();
    q.close();
    assert_eq!(q.push(3), Err(3), "push after close gives the value back");
    assert_eq!(q.pop(), Some(1), "sr 04 a closed queue refuses pushes and drains what is left");
    assert_eq!(q.pop(), Some(2), "sr 04 a closed queue refuses pushes and drains what is left");
    assert_eq!(q.pop(), None, "closed and empty: None, not waiting forever");
}

#[test]
fn sr_04_pop_waits_for_a_push_and_close_wakes_a_waiting_pop() {
    finish_within("pop waits for a push and close wakes a waiting pop", move || {
        let q = Arc::new(BoundedQueue::new(2));
        let waiting = {
            let q = Arc::clone(&q);
            thread::spawn(move || (q.pop(), q.pop()))
        };
        thread::sleep(Duration::from_millis(60));
        q.push(41).unwrap();
        thread::sleep(Duration::from_millis(60));
        q.close();
        assert_eq!(waiting.join().unwrap(), (Some(41), None), "the first pop waited for the push; the second was woken by close");
    });
}

#[test]
fn sr_04_push_waits_while_the_queue_is_full() {
    finish_within("push waits while the queue is full", move || {
        let q = Arc::new(BoundedQueue::new(1));
        q.push(1).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let pusher = {
            let (q, done) = (Arc::clone(&q), Arc::clone(&done));
            thread::spawn(move || {
                q.push(2).unwrap();
                done.store(true, Ordering::SeqCst);
            })
        };
        thread::sleep(Duration::from_millis(80));
        assert!(!done.load(Ordering::SeqCst), "the queue holds 1 value of 1: the second push must wait");
        assert_eq!(q.pop(), Some(1), "sr 04 push waits while the queue is full");
        pusher.join().unwrap();
        assert_eq!(q.pop(), Some(2), "sr 04 push waits while the queue is full");
    });
}

#[test]
fn sr_04_close_wakes_a_waiting_push_with_its_value() {
    finish_within("close wakes a waiting push with its value", move || {
        let q = Arc::new(BoundedQueue::new(1));
        q.push(1).unwrap();
        let pusher = {
            let q = Arc::clone(&q);
            thread::spawn(move || q.push(2))
        };
        thread::sleep(Duration::from_millis(60));
        q.close();
        assert_eq!(pusher.join().unwrap(), Err(2), "sr 04 close wakes a waiting push with its value");
    });
}

#[test]
fn sr_04_ids_are_handed_out_once_each() {
    let ids = Arc::new(IdGenerator::new());
    assert_eq!((ids.next(), ids.next(), ids.next()), (0, 1, 2), "sr 04 ids are handed out once each");
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let ids = Arc::clone(&ids);
            thread::spawn(move || (0..2000).map(|_| ids.next()).collect::<Vec<_>>())
        })
        .collect();
    let mut all: Vec<u64> = workers.into_iter().flat_map(|w| w.join().unwrap()).collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 8000, "four threads took 2000 numbers each and no number was handed out twice");
    assert_eq!(ids.next(), 8003, "sr 04 ids are handed out once each");
}

#[test]
fn sr_04_producers_and_consumers_lose_nothing() {
    finish_within("producers and consumers lose nothing", move || {
        let q = Arc::new(BoundedQueue::new(3));
        let producers: Vec<_> = (0..3u64)
            .map(|p| {
                let q = Arc::clone(&q);
                thread::spawn(move || {
                    for i in 0..500u64 {
                        q.push(p * 1000 + i).unwrap();
                    }
                })
            })
            .collect();
        let consumers: Vec<_> = (0..2)
            .map(|_| {
                let q = Arc::clone(&q);
                thread::spawn(move || {
                    let mut got = Vec::new();
                    while let Some(v) = q.pop() {
                        got.push(v);
                    }
                    got
                })
            })
            .collect();
        for p in producers {
            p.join().unwrap();
        }
        q.close();
        let mut got: Vec<u64> = consumers.into_iter().flat_map(|c| c.join().unwrap()).collect();
        got.sort_unstable();
        let mut want: Vec<u64> = (0..3u64).flat_map(|p| (0..500).map(move |i| p * 1000 + i)).collect();
        want.sort_unstable();
        assert_eq!(got, want, "every value pushed was popped exactly once");
    });
}

#[test]
fn sr_04_a_parallel_sum_is_the_sum() {
    let values: Vec<u64> = (1..=1000).collect();
    for threads in [0, 1, 3, 7, 1000, 5000] {
        assert_eq!(parallel_sum(&values, threads), 500_500, "{threads} threads");
    }
    assert_eq!(parallel_sum(&[], 4), 0, "sr 04 a parallel sum is the sum");
    assert_eq!(parallel_sum(&[u64::MAX, 2], 2), 1, "the sum wraps");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: a parallel sum equals the sequential one, whatever the split.
    #[test]
    fn sr_04_property_parallel_sum_equals_sequential(values in proptest::collection::vec(any::<u64>(), 0..300), threads in 0usize..12) {
        let want = values.iter().fold(0u64, |a, &b| a.wrapping_add(b));
        prop_assert_eq!(parallel_sum(&values, threads), want);
    }
}

// ---- r-05: testing against a model --------------------------------------------------------------------------------------------------------

#[test]
fn sr_05_a_ring_buffer_is_first_in_first_out() {
    let mut r = RingBuffer::new(3);
    assert!(
        r.is_empty() && !r.is_full() && r.capacity() == 3,
        "sr 05 a ring buffer is first in first out: expected `r.is_empty() && !r.is_full() && r.capacity() == 3`"
    );
    assert_eq!((r.pop(), r.front(), r.back()), (None, None, None), "sr 05 a ring buffer is first in first out");
    r.push('a').unwrap();
    r.push('b').unwrap();
    r.push('c').unwrap();
    assert!(r.is_full(), "sr 05 a ring buffer is first in first out: expected `r.is_full()`");
    assert_eq!((r.front(), r.back()), (Some(&'a'), Some(&'c')), "sr 05 a ring buffer is first in first out");
    assert_eq!((r.pop(), r.pop(), r.pop(), r.pop()), (Some('a'), Some('b'), Some('c'), None), "sr 05 a ring buffer is first in first out");
}

#[test]
fn sr_05_a_full_buffer_gives_the_value_back() {
    let mut r = RingBuffer::new(2);
    r.push(String::from("x")).unwrap();
    r.push(String::from("y")).unwrap();
    assert_eq!(r.push(String::from("z")), Err(String::from("z")), "refused, and the caller gets the value back");
    assert_eq!(r.len(), 2, "sr 05 a full buffer gives the value back");
}

#[test]
fn sr_05_the_buffer_wraps_around_the_end_of_its_array() {
    let mut r = RingBuffer::new(3);
    for round in 0..10 {
        r.push(round * 2).unwrap();
        r.push(round * 2 + 1).unwrap();
        assert_eq!(r.pop(), Some(round * 2), "sr 05 the buffer wraps around the end of its array");
        assert_eq!(r.pop(), Some(round * 2 + 1), "sr 05 the buffer wraps around the end of its array");
    }
    r.push(100).unwrap();
    r.push(101).unwrap();
    r.push(102).unwrap();
    assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![100, 101, 102], "after many wraps the order is still the order of the pushes");
    assert_eq!(r.back(), Some(&102), "sr 05 the buffer wraps around the end of its array");
}

#[test]
fn sr_05_push_overwriting_drops_the_oldest() {
    let mut r = RingBuffer::new(2);
    assert_eq!(r.push_overwriting(1), None, "sr 05 push overwriting drops the oldest");
    assert_eq!(r.push_overwriting(2), None, "sr 05 push overwriting drops the oldest");
    assert_eq!(r.push_overwriting(3), Some(1), "the oldest value was dropped to make room, and is returned");
    assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![2, 3], "sr 05 push overwriting drops the oldest");
    let mut zero = RingBuffer::new(0);
    assert_eq!(zero.push(5), Err(5), "sr 05 push overwriting drops the oldest");
    assert_eq!(zero.push_overwriting(6), Some(6), "a buffer of capacity 0 cannot hold anything");
    assert!(
        zero.is_empty() && zero.pop().is_none(),
        "sr 05 push overwriting drops the oldest: expected `zero.is_empty() && zero.pop().is_none()`"
    );
}

#[test]
fn sr_05_clear_drops_the_values_at_once_and_the_buffer_is_reusable() {
    use std::rc::Rc;
    let token = Rc::new(());
    let mut r = RingBuffer::new(4);
    for _ in 0..3 {
        r.push(Rc::clone(&token)).unwrap();
    }
    r.pop();
    r.clear();
    assert_eq!(Rc::strong_count(&token), 1, "clear drops every value now, not later");
    assert!(r.is_empty(), "sr 05 clear drops the values at once and the buffer is reusable: expected `r.is_empty()`");
    r.push(Rc::clone(&token)).unwrap();
    assert_eq!(r.len(), 1, "sr 05 clear drops the values at once and the buffer is reusable");
}

/// One operation on a ring buffer.
#[derive(Clone, Debug)]
enum Op {
    Push(u8),
    PushOverwriting(u8),
    Pop,
    Clear,
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(
        prop_oneof![
            4 => any::<u8>().prop_map(Op::Push),
            2 => any::<u8>().prop_map(Op::PushOverwriting),
            4 => Just(Op::Pop),
            1 => Just(Op::Clear),
        ],
        0..150,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: whatever the capacity and the operations, the ring buffer answers like a `VecDeque` with a limit.
    #[test]
    fn sr_05_property_a_ring_buffer_is_a_limited_deque(capacity in 0usize..9, ops in ops()) {
        let mut ring = RingBuffer::new(capacity);
        let mut model: VecDeque<u8> = VecDeque::new();
        for op in ops {
            match op {
                Op::Push(v) => {
                    let want = if model.len() < capacity { model.push_back(v); Ok(()) } else { Err(v) };
                    prop_assert_eq!(ring.push(v), want);
                }
                Op::PushOverwriting(v) => {
                    let want = if capacity == 0 { Some(v) } else {
                        let dropped = if model.len() == capacity { model.pop_front() } else { None };
                        model.push_back(v);
                        dropped
                    };
                    prop_assert_eq!(ring.push_overwriting(v), want);
                }
                Op::Pop => prop_assert_eq!(ring.pop(), model.pop_front()),
                Op::Clear => { ring.clear(); model.clear(); }
            }
            prop_assert_eq!(ring.len(), model.len());
            prop_assert_eq!(ring.is_full(), model.len() == capacity);
            prop_assert_eq!(ring.front(), model.front());
            prop_assert_eq!(ring.back(), model.back());
            prop_assert_eq!(ring.iter().copied().collect::<Vec<_>>(), model.iter().copied().collect::<Vec<_>>());
        }
    }
}

// ---- r-c1: challenge, a varint codec ------------------------------------------------------------------------------------------------------

use bustub::rust_primer::rle::{rle_decode, rle_encode, RleError};
use bustub::rust_primer::varint::{decode_varint, encode_varint, VarintError};

fn varint(v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_varint(v, &mut out);
    out
}

#[test]
fn sr_c1_small_numbers_take_one_byte_and_bigger_ones_follow_the_format() {
    assert_eq!(varint(0), vec![0], "sr c1 small numbers take one byte and bigger ones follow the format");
    assert_eq!(varint(1), vec![1], "sr c1 small numbers take one byte and bigger ones follow the format");
    assert_eq!(varint(127), vec![0x7f], "sr c1 small numbers take one byte and bigger ones follow the format");
    assert_eq!(varint(128), vec![0x80, 0x01], "128 needs a second byte: the low 7 bits first with the high bit set");
    assert_eq!(varint(300), vec![0xac, 0x02], "sr c1 small numbers take one byte and bigger ones follow the format");
    assert_eq!(varint(16_384), vec![0x80, 0x80, 0x01], "sr c1 small numbers take one byte and bigger ones follow the format");
    assert_eq!(varint(u64::MAX).len(), 10, "sr c1 small numbers take one byte and bigger ones follow the format");
    let mut out = vec![9, 9];
    encode_varint(5, &mut out);
    assert_eq!(out, vec![9, 9, 5], "encoding appends; it does not clear");
}

#[test]
fn sr_c1_decoding_reports_the_number_and_how_many_bytes_it_took() {
    assert_eq!(decode_varint(&[0x7f]), Ok((127, 1)), "sr c1 decoding reports the number and how many bytes it took");
    assert_eq!(decode_varint(&[0xac, 0x02]), Ok((300, 2)), "sr c1 decoding reports the number and how many bytes it took");
    assert_eq!(decode_varint(&[0xac, 0x02, 0xff, 0xff]), Ok((300, 2)), "bytes after the number are not looked at");
    assert_eq!(decode_varint(&[1, 2, 3]), Ok((1, 1)), "sr c1 decoding reports the number and how many bytes it took");
    assert_eq!(decode_varint(&varint(u64::MAX)), Ok((u64::MAX, 10)), "sr c1 decoding reports the number and how many bytes it took");
}

#[test]
fn sr_c1_bad_bytes_are_errors_not_panics() {
    assert_eq!(decode_varint(&[]), Err(VarintError::Truncated), "sr c1 bad bytes are errors not panics");
    assert_eq!(decode_varint(&[0x80]), Err(VarintError::Truncated), "a continuation bit with nothing after it");
    assert_eq!(decode_varint(&[0x80, 0x00]), Err(VarintError::NonMinimal), "0 written with two bytes");
    assert_eq!(decode_varint(&[0xac, 0x82, 0x00]), Err(VarintError::NonMinimal), "sr c1 bad bytes are errors not panics");
    let too_big = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02];
    assert_eq!(decode_varint(&too_big), Err(VarintError::Overflow), "needs 65 bits");
    assert_eq!(decode_varint(&[0x80; 12]), Err(VarintError::Overflow), "more than ten bytes of continuation");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: every number round-trips, takes exactly as many bytes as its bits need, and nothing but its own bytes is consumed.
    #[test]
    fn sr_c1_property_numbers_round_trip_in_the_fewest_bytes(v in prop_oneof![any::<u64>(), 0u64..300, any::<u32>().prop_map(u64::from)], tail in proptest::collection::vec(any::<u8>(), 0..4)) {
        let mut bytes = varint(v);
        let bits = 64 - v.leading_zeros().min(63) as usize;
        prop_assert_eq!(bytes.len(), bits.div_ceil(7).max(1), "the fewest bytes for {} bits", bits);
        let n = bytes.len();
        bytes.extend_from_slice(&tail);
        prop_assert_eq!(decode_varint(&bytes), Ok((v, n)));
    }

    /// Property: a stream of varints decodes back in order, and cutting it inside a number is a Truncated error for that number.
    #[test]
    fn sr_c1_property_a_stream_decodes_number_by_number(values in proptest::collection::vec(any::<u64>(), 1..12), cut in any::<prop::sample::Index>()) {
        let stream: Vec<u8> = values.iter().flat_map(|&v| varint(v)).collect();
        let mut at = 0;
        let mut starts = Vec::new();
        for &v in &values {
            let (got, n) = decode_varint(&stream[at..]).unwrap();
            prop_assert_eq!(got, v);
            starts.push(at);
            at += n;
        }
        prop_assert_eq!(at, stream.len());
        let cut = cut.index(stream.len());
        if let Some(&s) = starts.iter().rev().find(|&&s| s <= cut) {
            let piece = &stream[s..cut];
            match decode_varint(piece) {
                Ok((_, n)) => prop_assert!(n <= piece.len()),
                Err(e) => prop_assert_eq!(e, VarintError::Truncated, "a prefix of a valid number can only be cut short"),
            }
        }
    }

    /// Property: whatever the bytes are, decoding never panics, and an Ok answer re-encodes to exactly the bytes it consumed.
    #[test]
    fn sr_c1_property_any_bytes_decode_or_fail_cleanly(bytes in proptest::collection::vec(any::<u8>(), 0..14)) {
        if let Ok((v, n)) = decode_varint(&bytes) {
            prop_assert_eq!(varint(v), bytes[..n].to_vec(), "decode then encode gives back the bytes read");
        }
    }
}

// ---- r-c2: challenge, a run-length codec with a bug ---------------------------------------------------------------------------------------

#[test]
fn sr_c2_runs_become_count_and_byte_pairs() {
    assert_eq!(rle_encode(b""), Vec::<u8>::new(), "sr c2 runs become count and byte pairs");
    assert_eq!(rle_encode(b"aaab"), vec![3, b'a', 1, b'b'], "sr c2 runs become count and byte pairs");
    assert_eq!(rle_encode(b"abc"), vec![1, b'a', 1, b'b', 1, b'c'], "sr c2 runs become count and byte pairs");
    assert_eq!(rle_encode(&[7; 255]), vec![255, 7], "255 is the longest run one pair can hold");
}

#[test]
fn sr_c2_a_run_longer_than_a_pair_can_hold_is_split() {
    assert_eq!(rle_encode(&[7; 256]), vec![255, 7, 1, 7], "sr c2 a run longer than a pair can hold is split");
    assert_eq!(rle_encode(&[7; 600]), vec![255, 7, 255, 7, 90, 7], "sr c2 a run longer than a pair can hold is split");
    assert_eq!(rle_decode(&rle_encode(&[1; 1000])).unwrap(), vec![1; 1000], "a long run of equal bytes comes back whole");
}

#[test]
fn sr_c2_decoding_rejects_what_is_not_pairs() {
    assert_eq!(rle_decode(&[3]), Err(RleError::Truncated), "sr c2 decoding rejects what is not pairs");
    assert_eq!(rle_decode(&[2, b'a', 1]), Err(RleError::Truncated), "sr c2 decoding rejects what is not pairs");
    assert_eq!(rle_decode(&[0, b'a']), Err(RleError::ZeroRun), "sr c2 decoding rejects what is not pairs");
    assert_eq!(rle_decode(&[]), Ok(vec![]), "sr c2 decoding rejects what is not pairs");
    assert_eq!(rle_decode(&[2, b'a', 1, b'b']), Ok(b"aab".to_vec()), "sr c2 decoding rejects what is not pairs");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

    /// Property: whatever the data, decoding the encoding gives the data back.
    #[test]
    fn sr_c2_property_data_round_trips(data in prop_oneof![
        proptest::collection::vec(any::<u8>(), 0..200),
        proptest::collection::vec((0u8..3, 1usize..700), 0..6).prop_map(|runs| runs.into_iter().flat_map(|(b, n)| std::iter::repeat_n(b, n)).collect::<Vec<u8>>()),
    ]) {
        prop_assert_eq!(rle_decode(&rle_encode(&data)), Ok(data));
    }

    /// Property: the encoding is never more than twice the size of the data, and every count is between 1 and 255 with no two equal neighbours merged wrongly.
    #[test]
    fn sr_c2_property_the_encoding_is_compact_and_well_formed(data in proptest::collection::vec(0u8..3, 0..2000)) {
        let enc = rle_encode(&data);
        prop_assert!(enc.len() <= data.len() * 2);
        prop_assert!(enc.chunks(2).all(|p| p.len() == 2 && p[0] >= 1));
        let total: usize = enc.chunks(2).map(|p| p[0] as usize).sum();
        prop_assert_eq!(total, data.len(), "the counts add up to the length of the data");
    }
}

// @@ challenge r-c3 begin
mod ch_r_c3 {
    use proptest::prelude::*;

    use bustub::rust_primer::bits::BitSet;
    use std::collections::BTreeSet;

    #[test]
    fn sr_c3_bits_across_word_boundaries() {
        let mut b = BitSet::new(70);
        for i in [0, 63, 64, 69] {
            assert!(b.set(i), "bit {i} was clear");
        }
        assert_eq!(b.count(), 4, "this check");
        assert_eq!(b.iter().collect::<Vec<_>>(), vec![0, 63, 64, 69], "this check");
        assert!(b.test(63) && b.test(64) && !b.test(65), "this check: expected `b.test(63) && b.test(64) && !b.test(65)`");
    }

    #[test]
    fn sr_c3_set_and_clear_report_whether_they_changed_anything() {
        let mut b = BitSet::new(10);
        assert!(b.set(3), "this check: expected `b.set(3)`");
        assert!(!b.set(3), "already set");
        assert!(b.clear(3), "this check: expected `b.clear(3)`");
        assert!(!b.clear(3), "already clear");
        assert_eq!(b.count(), 0, "this check");
    }

    #[test]
    fn sr_c3_indexes_past_the_capacity_change_nothing() {
        let mut b = BitSet::new(70);
        assert!(!b.set(70), "this check: expected `!b.set(70)`");
        assert!(!b.set(127), "inside the last word but past the capacity");
        assert!(!b.set(10_000), "this check: expected `!b.set(10_000)`");
        assert!(!b.test(70) && !b.test(10_000), "this check: expected `!b.test(70) && !b.test(10_000)`");
        assert!(!b.clear(70), "this check: expected `!b.clear(70)`");
        assert_eq!(b.count(), 0, "this check");
        assert_eq!(b.capacity(), 70, "this check");
    }

    #[test]
    fn sr_c3_first_clear_on_empty_partial_and_full_sets() {
        let mut b = BitSet::new(130);
        assert_eq!(b.first_clear(), Some(0), "this check");
        for i in 0..65 {
            b.set(i);
        }
        assert_eq!(b.first_clear(), Some(65), "this check");
        for i in 0..130 {
            b.set(i);
        }
        assert_eq!(b.first_clear(), None, "a full set has no clear bit, and the unused bits of the last word do not count");
        b.clear(100);
        assert_eq!(b.first_clear(), Some(100), "this check");
        assert_eq!(BitSet::new(0).first_clear(), None, "this check");
    }

    #[test]
    fn sr_c3_union_sets_what_the_other_has() {
        let (mut a, mut b) = (BitSet::new(100), BitSet::new(200));
        a.set(1);
        b.set(1);
        b.set(99);
        b.set(150);
        a.union_with(&b);
        assert_eq!(a.iter().collect::<Vec<_>>(), vec![1, 99], "bit 150 is past a's capacity and is ignored");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a bit set behaves like a `BTreeSet` of indexes below the capacity.
        #[test]
        fn sr_c3_property_a_bit_set_is_a_set(capacity in 0usize..200, ops in proptest::collection::vec((0u8..3, 0usize..220), 0..100)) {
            let mut b = BitSet::new(capacity);
            let mut m = BTreeSet::new();
            for (op, i) in ops {
                let inside = i < capacity;
                match op {
                    0 => prop_assert_eq!(b.set(i), inside && m.insert(i)),
                    1 => prop_assert_eq!(b.clear(i), inside && m.remove(&i)),
                    _ => prop_assert_eq!(b.test(i), m.contains(&i)),
                }
                prop_assert_eq!(b.count(), m.len());
            }
            prop_assert_eq!(b.iter().collect::<Vec<_>>(), m.iter().copied().collect::<Vec<_>>());
            prop_assert_eq!(b.first_clear(), (0..capacity).find(|i| !m.contains(i)));
        }
    }
}
// @@ challenge r-c3 end

// @@ challenge r-c4 begin
mod ch_r_c4 {
    use proptest::prelude::*;

    use bustub::rust_primer::framing::{FrameDecoder, FrameError};

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut v = (payload.len() as u16).to_be_bytes().to_vec();
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn sr_c4_whole_frames_come_out_in_order() {
        let mut d = FrameDecoder::new(100);
        let mut bytes = frame(b"abc");
        bytes.extend(frame(b""));
        bytes.extend(frame(b"z"));
        assert_eq!(d.push(&bytes).unwrap(), vec![b"abc".to_vec(), b"".to_vec(), b"z".to_vec()], "this check");
        assert_eq!(d.buffered(), 0, "this check");
    }

    #[test]
    fn sr_c4_a_frame_split_across_pushes_waits_for_the_rest() {
        let mut d = FrameDecoder::new(100);
        assert_eq!(d.push(&[0, 3, b'a']).unwrap(), Vec::<Vec<u8>>::new(), "this check");
        assert_eq!(d.buffered(), 3, "this check");
        assert_eq!(d.push(&[b'b', b'c', 0, 1, b'z']).unwrap(), vec![b"abc".to_vec(), b"z".to_vec()], "this check");
    }

    #[test]
    fn sr_c4_a_header_cut_in_the_middle_is_not_lost() {
        let mut d = FrameDecoder::new(100);
        assert!(d.push(&[0]).unwrap().is_empty(), "this check: expected `d.push(&[0]).unwrap().is_empty()`");
        assert_eq!(d.buffered(), 1, "this check");
        assert_eq!(d.push(&[2, 9, 8]).unwrap(), vec![vec![9, 8]], "this check");
    }

    #[test]
    fn sr_c4_pushing_nothing_changes_nothing() {
        let mut d = FrameDecoder::new(100);
        d.push(&[0, 5, 1]).unwrap();
        assert!(d.push(&[]).unwrap().is_empty(), "this check: expected `d.push(&[]).unwrap().is_empty()`");
        assert_eq!(d.buffered(), 3, "this check");
    }

    #[test]
    fn sr_c4_a_frame_that_is_too_long_is_an_error_and_the_decoder_recovers() {
        let mut d = FrameDecoder::new(10);
        assert_eq!(d.push(&[0, 11, 1, 2]), Err(FrameError::TooLong { len: 11 }), "this check");
        assert_eq!(d.buffered(), 0, "the buffered bytes are forgotten");
        assert_eq!(d.push(&frame(b"ok")).unwrap(), vec![b"ok".to_vec()], "this check");
        assert!(d.push(&frame(&[7; 10])).is_ok(), "exactly the maximum is fine");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: however the stream is cut into pieces, the same frames come out, and what is left is less than one whole frame.
        #[test]
        fn sr_c4_property_chunking_does_not_change_the_frames(frames in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..20), 0..8), cuts in proptest::collection::vec(1usize..12, 1..30), tail in 0usize..3) {
            let mut stream: Vec<u8> = frames.iter().flat_map(|f| frame(f)).collect();
            let whole = stream.len();
            stream.extend(std::iter::repeat_n(0u8, tail).take(tail.min(1))); // an unfinished header at the end
            let mut d = FrameDecoder::new(64);
            let mut got = Vec::new();
            let mut at = 0;
            let mut i = 0;
            while at < stream.len() {
                let n = cuts[i % cuts.len()].min(stream.len() - at);
                got.extend(d.push(&stream[at..at + n]).unwrap());
                at += n;
                i += 1;
            }
            prop_assert_eq!(got, frames);
            prop_assert_eq!(d.buffered(), stream.len() - whole);
        }
    }
}
// @@ challenge r-c4 end

// @@ challenge r-c5 begin
mod ch_r_c5 {
    use proptest::prelude::*;

    use bustub::rust_primer::offsets::{page_offset, pages_for};

    #[test]
    fn sr_c5_ordinary_values() {
        assert_eq!(page_offset(3, 4096), Some(12288), "this check");
        assert_eq!(page_offset(0, 4096), Some(0), "this check");
        assert_eq!(pages_for(0, 4096), Some(0), "this check");
        assert_eq!(pages_for(1, 4096), Some(1), "this check");
        assert_eq!(pages_for(4096, 4096), Some(1), "this check");
        assert_eq!(pages_for(4097, 4096), Some(2), "this check");
    }

    #[test]
    fn sr_c5_a_page_size_of_zero_has_no_answer() {
        assert_eq!(page_offset(5, 0), None, "this check");
        assert_eq!(pages_for(5, 0), None, "this check");
    }

    #[test]
    fn sr_c5_an_offset_that_does_not_fit_is_none_not_a_wrapped_number() {
        assert_eq!(page_offset(u64::MAX, 2), None, "this check");
        assert_eq!(page_offset(1 << 63, 2), None, "this check");
        assert_eq!(page_offset((1 << 63) - 1, 2), Some(u64::MAX - 1), "this check");
    }

    #[test]
    fn sr_c5_the_page_count_of_a_huge_file_is_right() {
        assert_eq!(pages_for(u64::MAX, 4096), Some(4_503_599_627_370_496), "2^64 / 4096 = 2^52, rounded up from just below");
        assert_eq!(pages_for(u64::MAX - 3, 4), Some((1 << 62) - 1), "2^64 - 4 is exactly 2^62 - 1 pages of 4");
        assert_eq!(pages_for(u64::MAX, 1), Some(u64::MAX), "this check");
        assert_eq!(pages_for(u64::MAX, u64::MAX), Some(1), "this check");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: against 128-bit arithmetic, for ordinary and for huge values alike.
        #[test]
        fn sr_c5_property_the_arithmetic_matches_128_bit(a in prop_oneof![any::<u64>(), 0u64..5000, (u64::MAX - 5000)..=u64::MAX], s in prop_oneof![1u64..10, any::<u64>(), Just(4096u64)]) {
            let wide = a as u128 * s as u128;
            prop_assert_eq!(page_offset(a, s), if wide <= u64::MAX as u128 { Some(wide as u64) } else { None });
            let pages = (a as u128).div_ceil(s as u128);
            prop_assert_eq!(pages_for(a, s), Some(pages as u64));
        }
    }
}
// @@ challenge r-c5 end
