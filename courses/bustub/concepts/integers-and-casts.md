---
title: Integers, casts and overflow
summary: Rust's integer types, the three kinds of conversion (as, From, TryFrom), and what overflow does in debug and release builds, against C's promotions and C++'s static_cast.
minutes: 8
---
Almost every bug in the first stage of this course is an integer bug: a slot number multiplied by a page size, a file length compared with a count, a hash shifted by its own width. Rust and C disagree about integers in one way that matters more than any other: **Rust never converts between integer types behind your back.**

## The types

| Rust | width | C / C++ | notes |
|---|---|---|---|
| `u8` `u16` `u32` `u64` `u128` | 8 to 128 bits, unsigned | `uint8_t` … `uint64_t` | fixed width on every platform |
| `i8` `i16` `i32` `i64` `i128` | the same, signed (two's complement) | `int8_t` … `int64_t` | |
| `usize` / `isize` | pointer width (64 on a 64-bit machine, 32 on a 32-bit one) | `size_t` / `ptrdiff_t` | what indexing and lengths use |
| `PageId(i32)` | a newtype around `i32` | `typedef int32_t page_id_t` | a distinct type: it will not add to a frame id by accident |

C's `int`, `long` and `size_t` change width between platforms (`long` is 32 bits on 64-bit Windows and 64 bits on 64-bit Linux), which is how code that works on the developer's machine breaks in production. Rust's `u64` is 64 bits everywhere; only `usize` and `isize` vary, and they vary on purpose.

## No implicit conversions

C converts silently by two rules. *Integer promotion* widens anything smaller than `int` to `int`; the *usual arithmetic conversions* then make both operands of a binary operator the same type, which turns a `-1` into 4 294 967 295 the moment it meets an unsigned value. Rust has neither: `let a: u32 = 1; let b: u64 = 2; a + b` does not compile, and neither does `usize + u64`.

```cpp
int slot = 3000000;
off_t offset = slot * 8192;          // int * int = int: 24 576 000 000 does not fit, so this is signed overflow: undefined behaviour
```

```rust
let slot: usize = 3_000_000;
let offset: u64 = slot * 8192;                   // error: expected u64, found usize
let offset: u64 = slot as u64 * 8192;            // ok: widen first, then multiply
```

## Three ways to convert

| you write | meaning | when |
|---|---|---|
| `x as T` | a **cast**: always succeeds; widening extends, narrowing **truncates** the high bits, signed/unsigned just reinterprets the bits, float to int saturates | when you know the value fits, or truncation is what you want (hashing) |
| `T::from(x)` / `x.into()` | **lossless** only: exists only if every value of the source fits (`u32` to `u64`, not the other way) | the default for widening; the compiler proves it is safe |
| `T::try_from(x)` / `x.try_into()` | **checked**: returns `Err` if the value does not fit | when the value comes from outside (a file header, a user) |

```rust
let wide: u64 = u64::from(small_u32);      // cannot fail
let n: u32 = 300_i32 as u32;               // 300
let m: u8  = 300_i32 as u8;                // 44: 300 mod 256, silently
let k = u8::try_from(300_i32);              // Err(TryFromIntError): it does not fit
let neg: u32 = -1_i32 as u32;              // 4294967295: the same bits, read as unsigned
```

The rule of thumb for this course: use `as` to **widen** (`slot as u64`), `try_from` for anything that came from disk or from a caller, and treat every narrowing `as` as a bug until you can say why truncation is intended.

## Overflow

`a + b` that does not fit is a **panic in a debug build** (`attempt to add with overflow`) and **wraps in a release build** by default (the release profile turns the check off for speed; `overflow-checks = true` turns it back on). In C, *signed* overflow is undefined behaviour (the compiler may assume it never happens and optimise accordingly) while *unsigned* overflow wraps modulo 2<sup>n</sup>. Rust gives you the intent explicitly:

| method | on overflow |
|---|---|
| `a.checked_add(b)` | `None` |
| `a.wrapping_add(b)` | wraps modulo 2<sup>n</sup>, on purpose |
| `a.saturating_add(b)` | sticks at `MAX` or `MIN` |
| `a.overflowing_add(b)` | `(wrapped, did_overflow)` |

Hashing wants `wrapping_*` (the algorithm is defined modulo 2<sup>32</sup>); a slot computation wants a type wide enough that it cannot overflow; a length read from disk wants `checked_*`.

## Shifts are a conversion problem too

`x >> n` and `x << n` panic in debug (and are masked or undefined in release or C) when `n >= x.BITS`. The shift amount's *type* is also free: `u32 >> u8` is fine. `hash >> (32 - depth)` is wrong at depth 0 for exactly this reason; see the concept on shifts in module 2b.

> [!PORT] Porting rule
> Every integer expression in C++ that mixes types is a place where Rust will ask a question. Answer it with the widest type involved, convert the *operands* (not the result), and prefer `From` to `as`. A bare `as` in ported code should always have a comment saying which of widen, truncate or reinterpret it is.

## In real code

### The API you will use

| expression | meaning | when |
|---|---|---|
| `x as u64` | cast: widens, truncates or reinterprets, never fails | widening, or truncation on purpose |
| `u64::from(x)` / `x.into()` | lossless conversion; only compiles if it cannot lose data | the default for widening |
| `u8::try_from(x)` / `x.try_into()` | `Result`: `Err` if the value does not fit | values from outside |
| `a.checked_add(b)` / `checked_mul` / `checked_sub` | `Option`: `None` on overflow | lengths and offsets from disk |
| `a.wrapping_add(b)` / `wrapping_mul` | wrap modulo 2<sup>n</sup>, on purpose | hashing |
| `a.saturating_sub(b)` | stops at 0 or `MAX` | counters that must not go below zero |
| `a.overflowing_add(b)` | `(wrapped, overflowed)` | multi-word arithmetic |
| `u32::MAX`, `i32::MIN`, `u32::BITS` | the limits | edge cases |
| `a.rem_euclid(n)`, `a.div_ceil(n)`, `a.next_power_of_two()` | non-negative remainder, rounding up | shard indexes, page counts |

```rust test
use std::convert::TryFrom;

fn slot_offset(slot: usize, page_size: usize) -> u64 {
    slot as u64 * page_size as u64                            // widen BOTH operands first, then multiply
}

#[test]
fn offsets_past_four_gigabytes() {
    assert_eq!(slot_offset(1_000_000, 8192), 8_192_000_000);   // would overflow if multiplied as 32-bit
    assert_eq!(u64::from(7u32), 7);                           // lossless: From
    assert!(u8::try_from(300i32).is_err());                   // checked: it does not fit
    assert_eq!(300i32 as u8, 44);                             // a cast truncates silently: 300 mod 256
    assert_eq!(-1i32 as u32, u32::MAX);                       // the same bits read as unsigned
}
```

```rust test
#[test]
fn overflow_on_purpose_and_by_mistake() {
    assert_eq!(250u8.checked_add(10), None);                  // for a length read from disk: refuse
    assert_eq!(250u8.wrapping_add(10), 4);                    // for a hash: wrap
    assert_eq!(250u8.saturating_add(10), 255);                // for a clamp
    assert_eq!(250u8.overflowing_add(10), (4, true));

    let free: usize = 3;
    assert_eq!(free.saturating_sub(5), 0);                    // `free - 5` would panic in debug, wrap to a huge number in release
    assert_eq!((-7i32).rem_euclid(3), 2);                     // `-7 % 3` is -1
    assert_eq!(10usize.div_ceil(4), 3);                       // pages needed for 10 entries, 4 per page
    assert_eq!(17usize.next_power_of_two(), 32);              // the doubling in the file-growth stage
}
```

### In the exercises

- **1a-01 and 1a-02:** file offsets are `u64` and page positions are `usize`; widen with `as u64` *before* multiplying by the page size or a page id past four gigabytes overflows (the large-page-id test uses 1 000 000 and a billion). The file grows when space runs out; `checked_mul` is one way to make a doubling that cannot wrap fail loudly.
- **2a-01:** `u32::from_le_bytes`, `i32` page ids that are `-1` on disk, and `const fn array_size` with `usize` arithmetic.
- **2b-01 (MurmurHash3):** every multiply and add on the state is `wrapping_mul` / `wrapping_add`, and `rotate_left` is a rotation, not a shift.

### Where it is used

- **File formats and network protocols**: every length field is validated with `try_from` / `checked_*` before it is used to index or allocate (a classic source of security bugs when skipped).
- **Hashing, checksums, PRNGs**: wrapping arithmetic is the whole algorithm.
- **Time and sizes**: `saturating_sub` for elapsed time that must not go negative; `div_ceil` for "how many blocks".
