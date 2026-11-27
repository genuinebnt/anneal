---
title: Unicode and case mapping: strings are not byte arrays
summary: The difference between bytes, characters and letters; why to_uppercase can make a string longer; ASCII-only versus Unicode-aware functions; and where locale enters.
minutes: 7
---
`upper('straße')` should be `STRASSE`. Six letters become seven, and a `char` loop that replaces each character by one other character cannot do it. Text in a database is Unicode (UTF-8 here), and three different things all get called "the length":

| what | `"École"` | `"straße"` | `"🥰"` |
|---|---|---|---|
| **bytes** (`s.len()`) | 6 (`É` is 2 bytes) | 7 | 4 |
| **characters** = Unicode scalar values (`s.chars().count()`) | 5 | 6 | 1 |
| **"letters" a reader sees** (grapheme clusters) | 5 | 6 | 1 (but `🇫🇷` is 2 characters, one symbol) |

Rust's `String` is always valid UTF-8, so slicing in the middle of a character panics instead of corrupting text, and indexing `s[3]` does not compile. That is the cost of making the C `char *` bug class impossible.

## Case mapping

Unicode defines, per character, its upper- and lower-case forms. Most are one-to-one (`é` ↔ `É`), but some are not:

- `ß` upper-cases to `SS` (one character becomes two);
- `ﬁ` (a ligature) upper-cases to `FI`;
- `İ` (dotted capital I) lower-cases to `i̇` (an `i` and a combining dot);
- the Greek final sigma `ς` is the lower form of `Σ` only at the end of a word.

`str::to_lowercase` and `str::to_uppercase` implement the default mapping, including these special cases, and return a new `String`. `to_ascii_lowercase`/`to_ascii_uppercase` change only `A-Z`/`a-z` and leave every other byte alone; they are faster and never change the length, which is why an engine uses them when it knows the data is ASCII, and why they are *wrong* for SQL's `lower()` on user data.

## Locale

Some mappings depend on language: in Turkish `I` lower-cases to `ı` (dotless), not `i`. Real databases follow the column's *collation*. Rust's standard library is locale-independent; crates such as `icu` offer locale-aware mapping.

## Case-insensitive comparison is not `lower(a) == lower(b)`

Different strings can share a lower-case form and equal strings may not (`ß` vs `SS`). Correct case-insensitive comparison uses *case folding* or a collation, not a round trip through `lower`.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::toupper(c)` on a `char`/`int` (one byte, current locale) | `c.to_uppercase()` on a `char` returns an iterator (may yield several characters); `s.to_uppercase()` on a `str` |
| `std::string` of bytes; `.size()` is bytes | `String` is UTF-8; `.len()` is bytes, `.chars().count()` characters |
| `s[i]` is a byte; slicing anywhere is allowed | `&s[a..b]` panics unless `a` and `b` are character boundaries |
| `std::transform(..., ::toupper)` | `s.to_uppercase()` |

## In real code

### Using it: bytes, characters, and case

```rust test
fn shout(s: &str) -> String {
    s.to_uppercase()
}

fn ascii_shout(s: &str) -> String {
    s.to_ascii_uppercase()
}

#[test]
fn the_three_lengths_of_a_string() {
    assert_eq!("École".len(), 6);
    assert_eq!("École".chars().count(), 5);
    assert_eq!("🥰".len(), 4);
    assert_eq!("🥰".chars().count(), 1);
    assert!("École".is_char_boundary(2) && !"École".is_char_boundary(1), "É occupies bytes 0 and 1");
}

#[test]
fn unicode_aware_case_mapping_can_change_the_length() {
    assert_eq!(shout("straße"), "STRASSE");
    assert_eq!("straße".chars().count() + 1, "STRASSE".chars().count(), "one more character (the bytes happen to stay 7)");
    assert_eq!(shout("école"), "ÉCOLE");
    assert_eq!("ÉCOLE".to_lowercase(), "école");
    assert_eq!(shout("🥰 ok"), "🥰 OK", "characters without case are untouched");
}

#[test]
fn ascii_only_mapping_leaves_other_letters_alone() {
    assert_eq!(ascii_shout("école"), "éCOLE");
    assert_eq!(ascii_shout("straße"), "STRAßE");
    assert_eq!(ascii_shout("plain"), "PLAIN");
}

#[test]
fn a_round_trip_through_lower_is_not_case_folding() {
    // two different words that share an upper-case form: lower() does not identify them
    assert_ne!("ß".to_lowercase(), "ss");
    assert_eq!("ß".to_uppercase(), "SS".to_string());
    assert_eq!("SS".to_lowercase(), "ss");
}
```

### In the exercises

- **3d-02:** `StringExpression::compute` is `to_lowercase`/`to_uppercase`; the tests include `école`, `ß` and an emoji.
- **Module 3e onwards:** the mock tables contain `🥰` and `💩` strings, so every executor that copies a string exercises UTF-8.

### Where it is used

- **PostgreSQL**: `lower()`/`upper()` follow the database's locale (`LC_CTYPE`); `citext` and `ILIKE` use them for case-insensitive search.
- **SQLite**: `lower`/`upper` change only ASCII unless built with ICU.
- **MySQL**: collations (`utf8mb4_0900_ai_ci`) define comparison, not just case mapping.
- **Rust**: `str::to_uppercase` implements the Unicode default case mapping from the standard's data tables.
