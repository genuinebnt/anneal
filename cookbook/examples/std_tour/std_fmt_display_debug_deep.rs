//! RECIPE 1008 (std): the format string mini-language, Display and Debug by hand, Formatter methods, fmt::Write vs io::Write.
//! LEVEL: intermediate
//! Use in: logs, CLI tables, error messages, custom ID types, redacting secrets, zero-allocation rendering.   Run: cargo run --release --example std_fmt_display_debug_deep
//! COMMON USES: aligned CLI output, fixed-width protocol fields, hex dumps, `Display` for IDs and durations,
//! `Debug` that hides passwords, writing into one reused String.
//! METHODS: (everyday first) std::format, std::println, std::write, std::writeln, std::fmt::Display, std::fmt::Debug, ToString::to_string,
//! METHODS: Formatter::debug_struct, Formatter::debug_tuple, Formatter::debug_list, Formatter::debug_set, Formatter::debug_map, DebugStruct::field,
//! METHODS: DebugStruct::finish, DebugStruct::finish_non_exhaustive, DebugList::entry, DebugList::entries, DebugList::finish_non_exhaustive,
//! METHODS: DebugMap::entry, DebugMap::key, DebugMap::value, Formatter::pad, Formatter::pad_integral, Formatter::width, Formatter::precision,
//! METHODS: Formatter::alternate, Formatter::fill, Formatter::align, Formatter::sign_plus, Formatter::sign_minus, Formatter::sign_aware_zero_pad,
//! METHODS: Formatter::write_str, Formatter::write_fmt, std::fmt::Write::write_char, std::io::Write::write_all, std::format_args,
//! METHODS: std::fmt::Arguments::as_str, std::fmt::write, std::fmt::from_fn, std::fmt::Binary, std::fmt::LowerHex, std::fmt::UpperHex,
//! METHODS: std::fmt::Octal, std::fmt::LowerExp, str::escape_debug, str::escape_default, str::escape_unicode, char::escape_default
//! DOCS: https://doc.rust-lang.org/std/fmt/index.html
//! DEFINED IN: traits, Formatter, Arguments, Debug builders, `write!`, `format_args!`: core (`core::fmt`). `format!` and ToString: alloc.
//! `println!`/`eprintln!` and `io::Write`: std.
//! NOT COVERED (and why): UpperExp (same as LowerExp with 'E'), Pointer beyond a shape check (addresses are not deterministic),
//! DebugMap::key_with/value_with, Debug*::entry_with/field_with and Formatter::sign/options (unstable in 1.98.1), `{x?}`-hex forms
//! `{:x?}` shown once.
#![allow(dead_code)]
//!
//! CHEAT SHEET: {[argument][:[[fill]align][sign]['#']['0'][width]['.' precision][type]]}
//!   left / right / centre in width 8          -> {:<8}  {:>8}  {:^8}          custom fill: {:*^8}
//!   width or precision from a variable        -> {:>w$}  {:.p$}   (positional pair: {:.*} takes precision THEN value)
//!   zero-pad numbers                          -> {:05}      (after the sign and 0x prefix; overrides fill and align)
//!   always show the sign                      -> {:+}
//!   hex / octal / binary / scientific         -> {:x} {:X} {:o} {:b} {:e}     with prefix: {:#x} {:#010b} (width includes prefix)
//!   debug / pretty debug / debug with hex     -> {:?}  {:#?}  {:x?}
//!   inline variable                           -> {name}  {name:>5}            (identifiers only: no `{a.b}`, no calls)
//!   literal braces                            -> {{ }}
//!   truncate a string to N chars              -> {:.N}
//!   make my type honour width/precision       -> f.pad(..) / f.pad_integral(..) / forward the Formatter
//!   append into an existing String            -> use std::fmt::Write; write!(s, ..)
//!   render lazily without a new type          -> std::fmt::from_fn(|f| ..)   (since 1.93)
//!
//! RULE (std::fmt docs): a formatting impl returns Err only when the Formatter's writer returned Err. `format!` and
//! `to_string()` panic if an impl reports an error of its own.
//!
//! Read: https://doc.rust-lang.org/std/fmt/index.html
//! TRY NEXT:
//!   - [S] Replace `f.pad(&s)` in `UserId` with `f.write_str(&s)` and predict which width assert fails.
//!   - [S] Print a CJK or emoji name in `table` and look at the terminal. Why is `{:<w$}` misaligned although the asserts pass?
//!   - [M] Add `{:#}` support to `ByteSize` that prints exact bytes, and test it with width and alignment.
//!   - [M] Count allocations (see std_string_building_utf8's counting allocator) for `to_string()` in Display versus writing parts directly.
//!   - [A] PROJECT: a structured log formatter: one reused String, `escape_debug` for values, secrets redacted through a
//!         wrapper type, flushed through a BufWriter (std_bufreader_bufwriter_patterns).
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::{self, Write as _};
use std::io::Write as _;

const COVERED: &[&str] = &[
    "format",
    "println",
    "write",
    "writeln",
    "Display",
    "Debug",
    "to_string",
    "debug_struct",
    "debug_tuple",
    "debug_list",
    "debug_set",
    "debug_map",
    "field",
    "finish",
    "finish_non_exhaustive",
    "entry",
    "entries",
    "key",
    "value",
    "pad",
    "pad_integral",
    "width",
    "precision",
    "alternate",
    "fill",
    "align",
    "sign_plus",
    "sign_minus",
    "sign_aware_zero_pad",
    "write_str",
    "write_fmt",
    "write_char",
    "write_all",
    "format_args",
    "as_str",
    "from_fn",
    "Binary",
    "LowerHex",
    "UpperHex",
    "Octal",
    "LowerExp",
    "escape_debug",
    "escape_default",
    "escape_unicode",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

// ---- 1. Everyday core: width, alignment, precision -------------------------------------------------------------
fn section_everyday() {
    let (name, qty, price) = ("tea", 7u32, 3.14159f64);
    assert_eq!(format!("{name}={qty}"), "tea=7"); // inline captured identifiers
    assert_eq!(format!("{} {} {0}", "a", "b"), "a b a"); // next, next, explicit index
    assert_eq!(format!("{x}+{y}", x = qty, y = qty * 2), "7+14"); // named args may be any expression
    assert_eq!(format!("{{}} {qty}"), "{} 7");

    assert_eq!(format!("[{name:<6}]"), "[tea   ]");
    assert_eq!(format!("[{name:>6}]"), "[   tea]");
    assert_eq!(format!("[{name:^7}]"), "[  tea  ]");
    assert_eq!(format!("[{name:*^8}]"), "[**tea***]"); // odd padding: the extra goes to the right
    // Defaults differ: strings align left, numbers align right.
    assert_eq!(format!("[{name:5}][{qty:5}]"), "[tea  ][    7]");
    // Width is a MINIMUM: longer values are never cut. Precision on a string is the maximum.
    assert_eq!(format!("[{:2}]", "abcdef"), "[abcdef]");
    assert_eq!(format!("[{:.3}]", "abcdef"), "[abc]");
    assert_eq!(format!("[{:6.3}]", "abcdef"), "[abc   ]");
    assert_eq!(format!("{:.2}", "héllo"), "hé"); // precision counts chars, so it never splits one

    // Floats: precision = digits after the point; ties follow the exact binary value, rounded to even on true ties.
    assert_eq!(format!("{price:.2}"), "3.14");
    assert_eq!(format!("{:.0} {:.0} {:.0}", 0.5, 1.5, 2.5), "0 2 2");
    assert_eq!(format!("{:.2}", 2.675), "2.67"); // 2.675 is stored as 2.67499999...
    assert_eq!(
        format!("{} {} {}", 1.0, 0.1 + 0.2, 1e21),
        "1 0.30000000000000004 1000000000000000000000"
    );
    assert_eq!(format!("{:?} {:?}", 1.0, 1e21), "1.0 1e21"); // Debug keeps the ".0" and may use exponent form
    assert_eq!(
        format!("{:.2} {:8.3} [{:<8.1}]", f64::NAN, f64::INFINITY, -0.25),
        "NaN      inf [-0.2    ]"
    );
    println!("1. width/alignment/precision ok");
}

// ---- 2. Sign, zero padding, radix, dynamic width ---------------------------------------------------------------
fn section_flags() {
    assert_eq!(format!("{:+} {:+} {:+.1}", 5, -5, 2.0), "+5 -5 +2.0");
    assert_eq!(
        format!("{:05} {:+05} {:05}", 42, 42, -42),
        "00042 +0042 -0042"
    );
    assert_eq!(format!("[{:<05}]", 42), "[00042]"); // `0` wins over the alignment
    assert_eq!(format!("{:08.3}", -3.5), "-003.500");
    assert_eq!(
        format!("{:x} {:X} {:o} {:b}", 255, 255, 8, 5u8),
        "ff FF 10 101"
    );
    assert_eq!(
        format!("{:#x} {:#X} {:#o} {:#b}", 255, 255, 8, 5u8),
        "0xff 0xFF 0o10 0b101"
    );
    assert_eq!(format!("{:#06x} {:#010b}", 255, 5u8), "0x00ff 0b00000101"); // width counts the prefix
    assert_eq!(format!("{:02x}{:02x}", 10u8, 255u8), "0aff");
    // Negative integers in hex/binary print their two's complement bits; there is no "-0x1".
    assert_eq!(format!("{:x} {:08b}", -1i16, -2i8), "ffff 11111110");
    assert_eq!(
        format!("{:e} {:.2e} {:e}", 1234.5, 1234.5, 1200u32),
        "1.2345e3 1.23e3 1.2e3"
    );

    // Dynamic width and precision.
    let (w, p) = (8usize, 3usize);
    assert_eq!(format!("[{:>w$.p$}]", 3.14159), "[   3.142]");
    assert_eq!(format!("[{:>width$}]", 7, width = 4), "[   7]");
    assert_eq!(format!("[{:>1$}]", 7, 3), "[  7]"); // `1$`: argument 1 is the width
    assert_eq!(format!("[{:.*}]", 2, 1.23456), "[1.23]"); // .* consumes precision first, then the value
    // Pointers: only the shape is deterministic.
    assert!(format!("{:p}", &w).starts_with("0x"));
    println!("2. flags, radix, dynamic width ok");
}

// ---- 3. Debug output shapes ------------------------------------------------------------------------------------
#[derive(Debug, Default)]
struct Unit;
#[derive(Debug)]
struct Pair(u8, &'static str);
#[derive(Debug)]
struct Job {
    id: u32,
    tags: Vec<&'static str>,
}
#[derive(Debug)]
enum State {
    Idle,
    Failed { code: i32 },
}

fn section_debug_shapes() {
    assert_eq!(format!("{:?}", Unit), "Unit");
    assert_eq!(format!("{:?}", Pair(1, "a")), "Pair(1, \"a\")");
    let j = Job {
        id: 1,
        tags: vec!["x"],
    };
    assert_eq!(format!("{j:?}"), "Job { id: 1, tags: [\"x\"] }");
    assert_eq!(
        format!("{j:#?}"),
        "Job {\n    id: 1,\n    tags: [\n        \"x\",\n    ],\n}"
    );
    assert_eq!(
        format!("{:?} {:?}", State::Idle, State::Failed { code: -1 }),
        "Idle Failed { code: -1 }"
    );
    assert_eq!(
        format!("{:?} {:?} {:?}", "a\n\"b", '\'', Some((1,))),
        "\"a\\n\\\"b\" '\\'' Some((1,))"
    );
    assert_eq!(format!("{:x?}", [10u8, 255]), "[a, ff]");
    // Width is applied by the LEAF formatters, so it reaches every element.
    assert_eq!(format!("{:3?}", [1, 2]), "[  1,   2]");
    // The exact text of derived Debug is not a stable format: fine for logs, never for parsing or storage.
    println!("3. Debug shapes ok");
}

// ---- 4. Hand-written Debug with the builders -------------------------------------------------------------------
struct Secret(String);
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
struct Conn {
    user: &'static str,
    password: Secret,
    buffer: [u8; 4096],
}
impl fmt::Debug for Conn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // finish_non_exhaustive prints `..` so readers know fields were left out.
        f.debug_struct("Conn")
            .field("user", &self.user)
            .field("password", &self.password)
            .finish_non_exhaustive()
    }
}
struct Preview<'a>(&'a [u32]);
impl fmt::Debug for Preview<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        list.entries(self.0.iter().take(3));
        if self.0.len() > 3 {
            list.finish_non_exhaustive() // since 1.83 for list/set/map/tuple
        } else {
            list.finish()
        }
    }
}
struct Shapes;
impl fmt::Debug for Shapes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("T").field(&1).field(&"x").finish()?;
        f.write_str(" ")?;
        f.debug_set().entry(&1).entries([2, 3]).finish()?;
        f.write_str(" ")?;
        let mut m = f.debug_map();
        m.entry(&"a", &1);
        m.key(&"b").value(&2); // key then value must alternate, otherwise the builder panics
        m.finish()
    }
}

fn section_debug_builders() {
    let c = Conn {
        user: "ann",
        password: Secret("hunter2".into()),
        buffer: [0; 4096],
    };
    let d = format!("{c:?}");
    assert_eq!(
        d,
        "Conn { user: \"ann\", password: Secret(<redacted>), .. }"
    );
    assert!(!d.contains("hunter2") && !format!("{c:#?}").contains("hunter2"));
    assert_eq!(format!("{:?}", Preview(&[1, 2])), "[1, 2]");
    assert_eq!(format!("{:?}", Preview(&[1, 2, 3, 4, 5])), "[1, 2, 3, ..]");
    assert_eq!(
        format!("{Shapes:?}"),
        "T(1, \"x\") {1, 2, 3} {\"a\": 1, \"b\": 2}"
    );
    // The std collections already use these builders.
    assert_eq!(format!("{:?}", BTreeSet::from([2, 1])), "{1, 2}");
    assert_eq!(format!("{:?}", BTreeMap::from([(1, 'a')])), "{1: 'a'}");
    println!("4. Debug builders ok");
}

// ---- 5. Hand-written Display that honours the caller's flags ---------------------------------------------------
struct UserId(u32);
impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // pad applies width, fill, alignment and precision (as truncation) to a finished string.
        f.pad(&format!("user-{}", self.0))
    }
}
/// Ignores every flag: the common bug.
struct Naive(u32);
impl fmt::Display for Naive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "user-{}", self.0) // write! starts a NEW format spec: the caller's width is lost
    }
}
/// A number-like type: pad_integral handles sign, `#` prefix and zero padding like the built-in integers.
struct Offset(i64);
impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad_integral(self.0 >= 0, "off:", &self.0.unsigned_abs().to_string()) // prefix only printed with `#`
    }
}
/// Forwarding keeps all flags with no allocation.
struct Celsius(f64);
impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)?;
        f.write_str("C") // note: the width applies to the number, not to number + unit
    }
}
struct ByteSize(u64);
impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let places = f.precision().unwrap_or(1);
        let (mut v, mut unit) = (self.0 as f64, 0);
        while v >= 1024.0 && unit < 4 {
            v /= 1024.0;
            unit += 1;
        }
        let name = ["B", "KiB", "MiB", "GiB", "TiB"][unit];
        let places = if unit == 0 { 0 } else { places };
        let body = format!("{v:.places$} {name}");
        match f.width() {
            // Not f.pad: pad would treat the precision as "truncate the string".
            Some(w) => write!(f, "{body:>w$}"),
            None => f.write_str(&body),
        }
    }
}
/// Reports the flags it received.
struct Probe;
impl fmt::Display for Probe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fill = f.fill();
        let align = match f.align() {
            Some(fmt::Alignment::Left) => "L",
            Some(fmt::Alignment::Right) => "R",
            Some(fmt::Alignment::Center) => "C",
            None => "-",
        };
        let (w, p, alt, plus) = (f.width(), f.precision(), f.alternate(), f.sign_plus());
        let (minus, zero) = (f.sign_minus(), f.sign_aware_zero_pad());
        f.write_fmt(format_args!(
            "{fill}|{align}|{w:?}|{p:?}|{alt}|{plus}|{minus}|{zero}"
        ))
    }
}

fn section_display() {
    assert_eq!(format!("[{:>10}]", UserId(7)), "[    user-7]");
    assert_eq!(format!("[{:-^10}]", UserId(7)), "[--user-7--]");
    assert_eq!(format!("[{:.4}]", UserId(7)), "[user]");
    assert_eq!(format!("[{:>10}]", Naive(7)), "[user-7]"); // padding silently ignored
    assert_eq!(UserId(7).to_string(), "user-7"); // Display provides to_string for free
    assert_eq!(
        format!("{}|{:+}|{}", Offset(-5), Offset(5), Offset(5)),
        "-5|+5|5"
    );
    assert_eq!(
        format!("{:#}|[{:6}]", Offset(5), Offset(5)),
        "off:5|[     5]"
    );
    assert_eq!(format!("{:#09}", Offset(-5)), "-off:0005"); // sign, then prefix, then zeros up to width 9
    assert_eq!(format!("{:+.1}C", 3.0), "+3.0C");
    assert_eq!(format!("{:+.1}", Celsius(3.0)), "+3.0C");
    assert_eq!(format!("{}", ByteSize(512)), "512 B");
    assert_eq!(format!("{:.2}", ByteSize(1536)), "1.50 KiB");
    assert_eq!(format!("[{:10}]", ByteSize(3 << 20)), "[   3.0 MiB]");
    assert_eq!(format!("{Probe}"), " |-|None|None|false|false|false|false");
    assert_eq!(
        format!("{Probe:*^+#09.2}"),
        "*|C|Some(9)|Some(2)|true|true|false|true"
    );
    assert_eq!(
        format!("{Probe:-<}"),
        "-|L|None|None|false|false|false|false"
    );

    // PITFALL: returning Err on your own makes format!/to_string panic.
    struct Liar;
    impl fmt::Display for Liar {
        fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    assert!(panics(|| {
        let _ = Liar.to_string();
    }));
    let mut s = String::new();
    assert!(write!(s, "{Liar}").is_err()); // write! hands the error back instead
    println!("5. custom Display ok");
}

// ---- 6. Radix traits for a custom type -------------------------------------------------------------------------
struct Flags(u8);
macro_rules! forward_radix {
    ($($tr:ident),*) => {$(
        impl fmt::$tr for Flags {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::$tr::fmt(&self.0, f) // forwards width, `#`, `0` ...
            }
        }
    )*};
}
forward_radix!(Binary, LowerHex, UpperHex, Octal, LowerExp);

fn section_radix_traits() {
    let fl = Flags(0b1010);
    assert_eq!(
        format!("{fl:b} {fl:#010b} {fl:x} {fl:#04X} {fl:o} {fl:e}"),
        "1010 0b00001010 a 0x0A 12 1e1"
    );
    println!("6. radix traits ok");
}

// ---- 7. Where the bytes go: fmt::Write, io::Write, format_args!, from_fn ---------------------------------------
struct Item {
    name: &'static str,
    qty: u32,
}
/// Generic over fmt::Write: works for String, a Formatter, or your own sink. Errors propagate with `?`.
fn table(out: &mut impl fmt::Write, items: &[Item]) -> fmt::Result {
    // chars().count() approximates display width. It is wrong for wide (CJK, emoji) and combining characters.
    let w = items
        .iter()
        .map(|i| i.name.chars().count())
        .max()
        .unwrap_or(0)
        .max(4);
    writeln!(out, "{:<w$} {:>5}", "name", "qty")?;
    for it in items {
        writeln!(out, "{:<w$} {:>5}", it.name, it.qty)?;
    }
    Ok(())
}
/// A sink with a hard size limit: a place where fmt errors are real.
struct Bounded {
    buf: String,
    max: usize,
}
impl fmt::Write for Bounded {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.buf.len() + s.len() > self.max {
            return Err(fmt::Error);
        }
        self.buf.push_str(s);
        Ok(())
    }
}

fn section_sinks() {
    let items = [
        Item {
            name: "tea",
            qty: 7,
        },
        Item {
            name: "crème",
            qty: 120,
        },
    ];
    let mut s = String::new();
    table(&mut s, &items).unwrap();
    assert_eq!(s, "name    qty\ntea       7\ncrème   120\n");
    assert!(s.lines().all(|l| l.chars().count() == 11));

    let mut b = Bounded {
        buf: String::new(),
        max: 14,
    };
    assert!(table(&mut b, &items).is_err());
    assert!(b.buf.len() <= 14);
    b.buf.clear();
    b.write_char('x').unwrap(); // provided method on top of write_str
    // fmt::write is the function behind write!.
    fmt::write(&mut b, format_args!("{}", 12)).unwrap();
    assert_eq!(b.buf, "x12");

    // io::Write writes BYTES and fails with io::Error. A Vec<u8> stands in for a socket or stdout lock.
    let mut bytes: Vec<u8> = Vec::new();
    writeln!(bytes, "id={}", 5).unwrap();
    bytes.write_all(b"end").unwrap();
    assert_eq!(bytes, b"id=5\nend");
    // println! locks stdout on every call and panics if writing fails; for bulk output lock once and handle errors.
    let out = std::io::stdout();
    let mut h = out.lock();
    writeln!(h, "   (written through a locked stdout)").unwrap();

    // format_args! builds Arguments without allocating; hand it to any writer.
    fn log_line(sink: &mut String, args: fmt::Arguments<'_>) {
        match args.as_str() {
            Some(lit) => sink.push_str(lit), // literal with no arguments: copy directly (since 1.52)
            None => sink.write_fmt(args).expect("writing to a String"),
        }
    }
    let mut log = String::new();
    log_line(&mut log, format_args!("start;"));
    log_line(&mut log, format_args!("n={}", items.len()));
    assert_eq!(log, "start;n=2");
    // from_fn (since 1.93): a Display value from a closure, e.g. a lazy "join with commas" with no temporary String.
    let list = [1, 2, 3];
    let joined = fmt::from_fn(|f| {
        for (i, x) in list.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{x}")?;
        }
        Ok(())
    });
    assert_eq!(format!("ids: {joined}"), "ids: 1, 2, 3");
    println!("7. sinks ok");
}

// ---- 8. Escaping -----------------------------------------------------------------------------------------------
fn section_escape() {
    let raw = "a\tb\n\"c\" é\u{7}";
    // escape_debug: like {:?} without the surrounding quotes; printable non-ASCII stays.
    assert_eq!(raw.escape_debug().to_string(), "a\\tb\\n\\\"c\\\" é\\u{7}");
    // escape_default: ASCII-only output; every non-ASCII char becomes \u{..}.
    assert_eq!(
        raw.escape_default().to_string(),
        "a\\tb\\n\\\"c\\\" \\u{e9}\\u{7}"
    );
    assert_eq!("é".escape_unicode().to_string(), "\\u{e9}");
    assert_eq!('\n'.escape_default().collect::<String>(), "\\n");
    // None of these are HTML, SQL, shell or JSON escaping.
    println!("8. escaping ok");
}

// ---- 9. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Append with write!, not push_str(&format!(..)) (clippy::format_push_string).
    let mut s = String::with_capacity(32);
    for i in 0..3 {
        write!(s, "{i};").expect("writing to a String cannot fail");
    }
    assert_eq!(s, "0;1;2;");
    // 2. Inline the arguments (clippy::uninlined_format_args).
    // 3. format!("{x}") where x is already a &str: use x.to_string() (clippy::useless_format).
    // 4. Do not call to_string() inside format arguments (clippy::to_string_in_format_args).
    // 5. Implement Display, never ToString (clippy::to_string_trait_impl).
    // 6. Redact secrets in the TYPE's Debug (section 4), so every derived Debug above it is safe automatically.
    // 7. Log untrusted input with {:?} so newlines cannot forge log lines.
    let evil = "bob\nERROR forged";
    assert!(!format!("user={evil:?}").contains('\n') && format!("user={evil}").contains('\n'));
    // 8. In Display impls write directly into f; building a String first allocates (use f.pad only when you need padding).
    // 9. Never recurse: `write!(f, "{self}")` inside Display for Self overflows the stack (clippy::recursive_format_impl).
    // 10. Library code writing to a generic sink returns fmt::Result; do not unwrap it.
    // 11. Golden tests should pin your own Display, not derived Debug of std or third-party types.
    // 12. print!/println! go to stdout, eprintln! to stderr: keep results and diagnostics apart so pipes work.
    println!("9. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_flags();
    section_debug_shapes();
    section_debug_builders();
    section_display();
    section_radix_traits();
    section_sinks();
    section_escape();
    section_production_tips();
    println!("covered {} fmt names; all asserts passed", COVERED.len());
}
