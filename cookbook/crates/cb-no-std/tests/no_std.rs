use cb_no_std::{CapacityError, arena::Bump, fmtbuf::FmtBuf, parse::*, ring::Ring, stack::Stack};
use core::fmt::Write;
use std::{cell::Cell, rc::Rc};

#[test]
fn stack_basic_and_overflow() {
    let mut s = Stack::<u8, 2>::new();
    assert_eq!(s.push(1), Ok(()));
    assert_eq!(s.push(2), Ok(()));
    assert_eq!(s.push(3), Err(3));
    assert_eq!(s.as_slice(), [1, 2]);
    assert_eq!(s.to_vec(), vec![1, 2]);
    assert_eq!(s.pop(), Some(2));
    assert_eq!(s.capacity(), 2);
    assert_eq!(CapacityError.to_string(), "capacity exceeded");
}

struct D(Rc<Cell<u32>>);
impl Drop for D {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn containers_drop_exactly_once() {
    let n = Rc::new(Cell::new(0));
    {
        let mut s = Stack::<D, 4>::new();
        let mut r = Ring::<D, 4>::new();
        for _ in 0..3 {
            s.push(D(n.clone())).ok().unwrap();
            r.push_back(D(n.clone())).ok().unwrap();
        }
        drop(s.pop()); // one drop now
        assert_eq!(n.get(), 1);
    }
    assert_eq!(n.get(), 6);
}

#[test]
fn ring_wraps_and_overwrites() {
    let mut r = Ring::<u32, 3>::new();
    for i in 1..=3 {
        r.push_back(i).unwrap();
    }
    assert!(r.is_full());
    assert_eq!(r.push_back(4), Err(4));
    assert_eq!(r.pop_front(), Some(1));
    r.push_back(4).unwrap(); // wraps physically
    assert_eq!(r.iter().copied().collect::<Vec<_>>(), [2, 3, 4]);
    assert_eq!(r.push_overwrite(5), Some(2));
    assert_eq!(r.iter().copied().collect::<Vec<_>>(), [3, 4, 5]);
    assert_eq!(r.get(3), None);
}

#[test]
fn write_macro_into_stack_buffer() {
    let mut b = FmtBuf::<16>::new();
    write!(b, "x={} y={:03}", 5, 7).unwrap();
    assert_eq!(b.as_str(), "x=5 y=007");
    assert!(write!(b, "{}", "this is way too long").is_err());
    assert_eq!(b.as_str(), "x=5 y=007"); // failed chunk not partially written
    b.clear();
    assert_eq!(b.as_str(), "");
}

#[test]
fn bump_alignment_and_exhaustion() {
    let mut a = Bump::<32>::new();
    {
        let x = a.alloc(1u8).unwrap();
        let y = a.alloc(0x1122_3344u32).unwrap();
        let z = a.alloc(7u64).unwrap();
        assert_eq!((y as *mut u32 as usize) % 4, 0);
        assert_eq!((z as *mut u64 as usize) % 8, 0);
        *x += 1;
        assert_eq!((*x, *y, *z), (2, 0x1122_3344, 7));
        assert!(a.alloc([0u8; 32]).is_none());
    }
    assert!(a.used() <= 32);
    a.reset();
    assert_eq!(a.used(), 0);
    assert!(a.alloc([0u8; 32]).is_some()); // align 1: whole buffer is usable again
}

#[test]
fn parsing() {
    assert_eq!(parse_u64(b"12345"), Ok(12345));
    assert_eq!(parse_u64(b""), Err(ParseError::Empty));
    assert_eq!(parse_u64(b"1x"), Err(ParseError::InvalidDigit));
    assert_eq!(
        parse_u64(b"18446744073709551616"),
        Err(ParseError::Overflow)
    );
    assert_eq!(parse_i64(b"-9223372036854775808"), Ok(i64::MIN));
    assert_eq!(parse_i64(b"9223372036854775808"), Err(ParseError::Overflow));
    assert_eq!(parse_i64(b"+42"), Ok(42));
    assert_eq!(parse_i64(b"-"), Err(ParseError::Empty));
}

/// Prove the library really is no_std by building it for a bare-metal target.
/// Skipped (with a message) when the target isn't installed.
#[test]
fn builds_for_bare_metal_if_target_installed() {
    use std::process::Command;
    let target = "thumbv7em-none-eabihf";
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(target))
        .unwrap_or(false);
    if !installed {
        eprintln!("skipping: {target} not installed");
        return;
    }
    let out = Command::new(env!("CARGO"))
        .args([
            "check",
            "-q",
            "--release",
            "--no-default-features",
            "--target",
            target,
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ])
        .env(
            "CARGO_TARGET_DIR",
            std::env::temp_dir().join("cb_no_std_target"),
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
