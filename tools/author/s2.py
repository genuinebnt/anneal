from author import T, write_track

P = []


def rs(s: str) -> str:
    """A Rust string literal for `s` (ASCII only)."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '".to_string()'


def receipt(item, qty, price):
    return f"{item:<10.10}{qty:>3} {price:>8.2f}"


# ---------------------------------------------------------------- use (easy)

P.append(dict(
    slug="string-vs-str", title="String vs &str", level="easy", stage="use",
    tags=["&str", "String", "deref"],
    teaches=["Take `&str`, return `String` when you build something new.", "A `&String` coerces to `&str`, so `&str` parameters accept both."],
    statement="""
        - `greet` returns `"Hello, <name>!"`.
        - `exclaim` appends `!` to a `String` in place.
        - `first_word` returns the text before the first space, borrowed from the input.
    """,
    examples=[('greet("Ada")', '"Hello, Ada!"'), ('first_word("hello world")', '"hello"')],
    starter="""
        pub fn greet(name: &str) -> String {
            todo!()
        }

        pub fn exclaim(s: &mut String) {
            todo!()
        }

        pub fn first_word(s: &str) -> &str {
            todo!()
        }
    """,
    solution="""
        pub fn greet(name: &str) -> String {
            format!("Hello, {name}!")
        }

        pub fn exclaim(s: &mut String) {
            s.push('!');
        }

        pub fn first_word(s: &str) -> &str {
            s.split(' ').next().unwrap_or(s)
        }
    """,
    visible=[
        T("greets_literal_and_owned", 'greet("Ada") and greet(&String::from("Bo"))', '(greet("Ada"), greet(&name))', '("Hello, Ada!".to_string(), "Hello, Bo!".to_string())',
          setup='let name = String::from("Bo");'),
        T("first", '"hello world"', 'first_word("hello world")', '"hello"'),
        T("exclaim_once", 'exclaim on "wow"', "s", '"wow!".to_string()', setup='let mut s = String::from("wow");\nexclaim(&mut s);'),
        T("first_whole_word", '"rust"', 'first_word("rust")', '"rust"'),
        T("greet_empty", '""', 'greet("")', '"Hello, !".to_string()'),
    ],
    hidden=[
        T("in_place", 'exclaim twice on "wow"', "s", '"wow!!".to_string()', setup='let mut s = String::from("wow");\nexclaim(&mut s);\nexclaim(&mut s);'),
        T("no_space", '"single"', 'first_word("single")', '"single"'),
        T("leading_space", '" x"', 'first_word(" x")', '""'),
        T("first_empty", '""', 'first_word("")', '""'),
        T("two_spaces", '"a  b"', 'first_word("a  b")', '"a"'),
        T("tab_is_not_a_space", '"a\\tb c"', 'first_word("a\\tb c")', '"a\\tb"'),
        T("trailing_space", '"ab "', 'first_word("ab ")', '"ab"'),
        T("unicode", '"héllo wörld" and greet("日本")', '(first_word("héllo wörld"), greet("日本"))', '("héllo", "Hello, 日本!".to_string())'),
        T("exclaim_empty", "exclaim on \"\"", "s", '"!".to_string()', setup="let mut s = String::new();\nexclaim(&mut s);"),
        T("borrows_input", "first_word points into its input", "first_word(&s).as_ptr() == s.as_ptr()", "true", setup='let s = String::from("abc def");'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2201);
            for _ in 0..300 {
                let len = rng.below(10);
                let s = rng.string(len, "ab é\\t");
                let want = match s.find(' ') {
                    Some(i) => &s[..i],
                    None => &s[..],
                };
                let mut shouted = s.clone();
                exclaim(&mut shouted);
                check!(format!("s = {s:?}"), (first_word(&s), greet(&s), shouted), (want, format!("Hello, {s}!"), format!("{s}!")));
            }
        }
        """,
    ],
    wrong=dict(
        any_whitespace="""
            pub fn greet(name: &str) -> String {
                format!("Hello, {name}!")
            }

            pub fn exclaim(s: &mut String) {
                s.push('!');
            }

            pub fn first_word(s: &str) -> &str {
                s.split_whitespace().next().unwrap_or("")
            }
        """,
        no_space_gives_empty="""
            pub fn greet(name: &str) -> String {
                format!("Hello, {name}!")
            }

            pub fn exclaim(s: &mut String) {
                s.push('!');
            }

            pub fn first_word(s: &str) -> &str {
                &s[..s.find(' ').unwrap_or(0)]
            }
        """,
    ),
    hints=[("rust", "A `&str` parameter accepts string literals and `&String` alike, thanks to deref coercion."),
           ("rust", "`first_word` can return a slice of its input; no allocation needed.")],
    notes=("`String` owns a growable buffer; `&str` borrows some UTF-8 text from anywhere. Taking `&str` is the most flexible parameter; returning `String` is right when you build new text.", "O(n)", "O(n) for greet"),
    follow_up="When would you take `impl Into<String>` instead of `&str`?",
    related=["L1"],
))

P.append(dict(
    slug="split-trim-parse", title="split, trim, parse", level="easy", stage="use",
    tags=["split", "parse", "Result"],
    teaches=["`split(',').map(str::trim)` pipelines.", "Summing an iterator of `Result`s into a `Result`."],
    statement="""
        Sum a comma-separated list of integers. Spaces around numbers don't matter and empty fields are
        skipped. Return the parse error if any field isn't an integer. The running sum never overflows an `i64`.
    """,
    examples=[('"1, 2 ,3"', "Ok(6)"), ('"1,x"', "Err(..)")],
    starter="""
        use std::num::ParseIntError;

        pub fn sum_csv(line: &str) -> Result<i64, ParseIntError> {
            todo!()
        }
    """,
    solution="""
        use std::num::ParseIntError;

        pub fn sum_csv(line: &str) -> Result<i64, ParseIntError> {
            line.split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(str::parse::<i64>)
                .sum()
        }
    """,
    visible=[
        T("spaces", '"1, 2 ,3"', 'sum_csv("1, 2 ,3")', "Ok(6)"),
        T("bad_field", '"1,x"', 'sum_csv("1,x").is_err()', "true"),
        T("single", '"42"', 'sum_csv("42")', "Ok(42)"),
        T("negatives", '"-1, 1, -5"', 'sum_csv("-1, 1, -5")', "Ok(-5)"),
        T("empty_field_skipped", '"1,,2,"', 'sum_csv("1,,2,")', "Ok(3)"),
    ],
    hidden=[
        T("empty", '""', 'sum_csv("")', "Ok(0)"),
        T("empty_fields_and_negatives", '" -4 ,, 10"', 'sum_csv(" -4 ,, 10")', "Ok(6)"),
        T("float_is_not_int", '"1.5"', 'sum_csv("1.5").is_err()', "true"),
        T("only_commas", '",,,"', 'sum_csv(",,,")', "Ok(0)"),
        T("only_spaces", '"   "', 'sum_csv("   ")', "Ok(0)"),
        T("plus_sign", '"+5,+6"', 'sum_csv("+5,+6")', "Ok(11)"),
        T("tabs_trimmed", '"\\t3\\t,4\\n"', 'sum_csv("\\t3\\t,4\\n")', "Ok(7)"),
        T("space_inside_field", '"1 2,3"', 'sum_csv("1 2,3").is_err()', "true"),
        T("beyond_i32", '"3000000000,3000000000"', 'sum_csv("3000000000,3000000000")', "Ok(6_000_000_000)"),
        T("i64_bounds", '"9223372036854775807,-9223372036854775808"', 'sum_csv("9223372036854775807,-9223372036854775808")', "Ok(-1)"),
        T("too_big_for_i64", '"9223372036854775808"', 'sum_csv("9223372036854775808").is_err()', "true"),
        T("bad_field_last", '"1,2,3,4,z"', 'sum_csv("1,2,3,4,z").is_err()', "true"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2202);
            let pieces = ["", " ", "7", " -3 ", "12", "x", "1e3", "0"];
            for _ in 0..400 {
                let n = rng.below(6);
                let mut fields = Vec::new();
                for _ in 0..n {
                    fields.push(*rng.pick(&pieces));
                }
                let line = fields.join(",");
                let mut total = 0i64;
                let mut bad = false;
                for f in &fields {
                    match f.trim() {
                        "" => {}
                        "7" => total += 7,
                        "-3" => total -= 3,
                        "12" => total += 12,
                        "0" => {}
                        _ => bad = true,
                    }
                }
                check!(format!("line = {line:?}"), sum_csv(&line).ok(), if bad { None } else { Some(total) });
            }
        }

        #[test]
        fn scale_200k_fields() {
            let line = "1, ".repeat(200_000);
            check!("line = \\"1, 1, …\\" (200000 fields)", sum_csv(&line), Ok(200_000));
        }
        """,
    ],
    wrong=dict(
        empty_fields_fail="""
            use std::num::ParseIntError;

            pub fn sum_csv(line: &str) -> Result<i64, ParseIntError> {
                line.split(',').map(str::trim).map(str::parse::<i64>).sum()
            }
        """,
        skips_bad_fields="""
            use std::num::ParseIntError;

            pub fn sum_csv(line: &str) -> Result<i64, ParseIntError> {
                Ok(line.split(',').filter_map(|f| f.trim().parse::<i64>().ok()).sum())
            }
        """,
        parses_i32="""
            use std::num::ParseIntError;

            pub fn sum_csv(line: &str) -> Result<i64, ParseIntError> {
                let mut total = 0i64;
                for f in line.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                    total += f.parse::<i32>()? as i64;
                }
                Ok(total)
            }
        """,
    ),
    hints=[("rust", "`str::trim` and `str::parse::<i64>` can be passed to `map` directly."),
           ("rust", "`Sum` is implemented for `Result<T, E>`: summing `Result`s stops at the first `Err`.")],
    notes=("The whole function is one pipeline. Summing into `Result` short-circuits on the first bad field, so no manual loop or early return is needed.", "O(n)", "O(1)"),
    follow_up="How would you report which field failed, and at what position?",
    related=["S1"],
))

P.append(dict(
    slug="fix-string-plus-string", title="Fix: + with two Strings", mode="fix", level="easy", stage="use",
    tags=["E0308", "E0369", "Add"],
    teaches=["`String + &str` consumes the left side and borrows the right.", "`&str + ...` doesn't exist; use `format!`."],
    statement="Both functions should build a new string. Neither compiles.",
    starter="""
        /// "last, first"
        pub fn full_name(first: String, last: String) -> String {
            last + ", " + first
        }

        /// "name#id"
        pub fn tag(name: &str, id: u32) -> String {
            name + "#" + id
        }
    """,
    solution="""
        /// "last, first"
        pub fn full_name(first: String, last: String) -> String {
            last + ", " + &first
        }

        /// "name#id"
        pub fn tag(name: &str, id: u32) -> String {
            format!("{name}#{id}")
        }
    """,
    visible=[
        T("name", '"Ada", "Lovelace"', 'full_name("Ada".into(), "Lovelace".into())', rs("Lovelace, Ada")),
        T("tagged", '"ferris", 7', 'tag("ferris", 7)', rs("ferris#7")),
        T("tag_two_digits", '"x", 42', 'tag("x", 42)', rs("x#42")),
        T("empty_last", '"Ada", ""', 'full_name("Ada".into(), String::new())', rs(", Ada")),
        T("last_comes_first", '"Grace", "Hopper"', 'full_name("Grace".into(), "Hopper".into())', rs("Hopper, Grace")),
    ],
    hidden=[
        T("empty_first", '"", "X"', 'full_name(String::new(), "X".into())', rs("X, ")),
        T("zero", '"a", 0', 'tag("a", 0)', rs("a#0")),
        T("max_id", '"a", u32::MAX', 'tag("a", u32::MAX)', rs("a#4294967295")),
        T("ten", '"v", 10', 'tag("v", 10)', rs("v#10")),
        T("empty_name_tag", '"", 5', 'tag("", 5)', rs("#5")),
        T("both_empty", '"", ""', "full_name(String::new(), String::new())", rs(", ")),
        T("unicode_names", '"Zoë", "Ünal"', 'full_name("Zoë".into(), "Ünal".into())', '"Ünal, Zoë".to_string()'),
        T("unicode_tag", '"日本", 3', 'tag("日本", 3)', '"日本#3".to_string()'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2203);
            for _ in 0..300 {
                let (l1, l2) = (rng.below(6), rng.below(6));
                let first = rng.string(l1, "ab é");
                let last = rng.string(l2, "xy ü");
                let id = rng.next_u64() as u32 >> rng.below(32);
                let want = (format!("{last}, {first}"), format!("{first}#{id}"));
                check!(format!("first = {first:?}, last = {last:?}, id = {id}"), (full_name(first.clone(), last.clone()), tag(&first, id)), want);
            }
        }
        """,
    ],
    wrong=dict(
        first_then_last="""
            /// "last, first"
            pub fn full_name(first: String, last: String) -> String {
                first + ", " + &last
            }

            /// "name#id"
            pub fn tag(name: &str, id: u32) -> String {
                format!("{name}#{id}")
            }
        """,
        digit_as_char="""
            /// "last, first"
            pub fn full_name(first: String, last: String) -> String {
                last + ", " + &first
            }

            /// "name#id"
            pub fn tag(name: &str, id: u32) -> String {
                name.to_string() + "#" + &((b'0' + id as u8) as char).to_string()
            }
        """,
    ),
    hints=[("rust", "`impl Add<&str> for String` is the only `+` for strings: left side owned `String`, right side `&str`."),
           ("rust", "A `&str` on the left can't grow, and a number isn't a `&str`. `format!` handles both.")],
    notes=("`+` reuses the left `String`'s buffer, which is why it takes it by value. `&first` derefs `&String` to `&str`. For anything mixed, `format!` is clearer.", "O(n)", "O(n)"),
    follow_up="Why is `a + &b` often faster than `format!(\"{a}{b}\")`?",
    rules=dict(lines=2),
))

P.append(dict(
    slug="compare-ignoring-case", title="Compare ignoring case without allocating", level="easy", stage="use",
    tags=["eq_ignore_ascii_case", "zero allocation"],
    teaches=["`eq_ignore_ascii_case` compares without building lowercase copies."],
    perf=dict(allocs=True),
    statement="""
        - `is_command`: does `input`, with surrounding whitespace ignored, name `command`, ignoring ASCII case?
        - `count_word`: how many whitespace-separated words of `text` equal `word`, ignoring ASCII case?

        Don't allocate: no `to_lowercase` or `to_uppercase`, and no `to_ascii_lowercase` either (it builds a new
        `String` too). The tests count heap allocations.
    """,
    starter="""
        pub fn is_command(input: &str, command: &str) -> bool {
            todo!()
        }

        pub fn count_word(text: &str, word: &str) -> usize {
            todo!()
        }
    """,
    solution="""
        pub fn is_command(input: &str, command: &str) -> bool {
            input.trim().eq_ignore_ascii_case(command)
        }

        pub fn count_word(text: &str, word: &str) -> usize {
            text.split_whitespace()
                .filter(|w| w.eq_ignore_ascii_case(word))
                .count()
        }
    """,
    visible=[
        T("command", '"  QUIT \\n", "quit"', 'is_command("  QUIT \\n", "quit")', "true"),
        T("count", '"The cat saw the THE", "the"', 'count_word("The cat saw the THE", "the")', "3"),
        T("different_word", '"help", "quit"', 'is_command("help", "quit")', "false"),
        T("whole_words_only", '"cats cat category", "CAT"', 'count_word("cats cat category", "CAT")', "1"),
        T("any_whitespace", '"a\\nA\\ta", "a"', 'count_word("a\\nA\\ta", "a")', "3"),
    ],
    hidden=[
        T("prefix_is_not_equal", '"quitter", "quit"', 'is_command("quitter", "quit")', "false"),
        T("none", '"", "a"', 'count_word("", "a")', "0"),
        T("empty_command", '"   ", ""', 'is_command("   ", "")', "true"),
        T("command_not_trimmed", '"QUIT", "quit "', 'is_command("QUIT", "quit ")', "false"),
        T("inner_space_kept", '"qu it", "quit"', 'is_command("qu it", "quit")', "false"),
        T("ascii_only_command", '"É", "é"', 'is_command("É", "é")', "false"),
        T("ascii_only_count", '"ÉCOLE école École", "école"', 'count_word("ÉCOLE école École", "école")', "1"),
        T("punctuation_counts", '"the, the. the", "the"', 'count_word("the, the. the", "the")', "1"),
        T("empty_word", '"a b", ""', 'count_word("a b", "")', "0"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2204);
            let words = ["ab", "AB", "aB", "ba", "a", "abc", "Ab"];
            for _ in 0..300 {
                let n = rng.below(6);
                let mut text = String::new();
                for _ in 0..n {
                    text.push_str(*rng.pick(&words));
                    text.push_str(*rng.pick(&[" ", "  ", "\\n", "\\t"]));
                }
                let word = *rng.pick(&words);
                let fold = |s: &str| -> Vec<u8> { s.bytes().map(|b| if b.is_ascii_uppercase() { b + 32 } else { b }).collect() };
                let want = text.split_whitespace().filter(|w| fold(w) == fold(word)).count();
                let padded = format!(" {word}\\t");
                check!(format!("text = {text:?}, word = {word:?}"), (count_word(&text, word), is_command(&padded, &word.to_uppercase())), (want, true));
            }
        }

        #[test]
        fn scale_200k_words() {
            let text = "Go gO go stop ".repeat(50_000);
            check!("text = \\"Go gO go stop …\\" (200000 words), word = \\"GO\\"", count_word(&text, "GO"), 150_000);
        }
        """,
        """
            #[test]
            fn no_allocations() {
                let (got, seen) = anneal_prelude::allocs(|| (is_command("  QUIT \\n", "quit"), count_word("The cat saw the THE", "the")));
                check!(r#"is_command("  QUIT \\n", "quit") and count_word("The cat saw the THE", "the"): (results, heap allocations)"#, (got, seen.count), ((true, 3), 0));
            }
        """,
    ],
    wrong=dict(
        unicode_lowercase="""
            pub fn is_command(input: &str, command: &str) -> bool {
                input.trim().to_lowercase() == command.to_lowercase()
            }

            pub fn count_word(text: &str, word: &str) -> usize {
                text.split_whitespace().filter(|w| w.to_lowercase() == word.to_lowercase()).count()
            }
        """,
        prefix_match="""
            pub fn is_command(input: &str, command: &str) -> bool {
                let input = input.trim();
                input.len() >= command.len() && input.as_bytes()[..command.len()].eq_ignore_ascii_case(command.as_bytes())
            }

            pub fn count_word(text: &str, word: &str) -> usize {
                text.split_whitespace().filter(|w| w.eq_ignore_ascii_case(word)).count()
            }
        """,
        split_on_space="""
            pub fn is_command(input: &str, command: &str) -> bool {
                input.trim().eq_ignore_ascii_case(command)
            }

            pub fn count_word(text: &str, word: &str) -> usize {
                text.split(' ').filter(|w| w.eq_ignore_ascii_case(word)).count()
            }
        """,
        ascii_lowercase_copies="""
            // Wrong: lowercases copies of both sides, which allocates on every comparison.
            pub fn is_command(input: &str, command: &str) -> bool {
                input.trim().to_ascii_lowercase() == command.to_ascii_lowercase()
            }

            pub fn count_word(text: &str, word: &str) -> usize {
                let word = word.to_ascii_lowercase();
                text.split_whitespace()
                    .filter(|w| w.to_ascii_lowercase() == word)
                    .count()
            }
        """,
    ),
    hints=[("rust", "`str::eq_ignore_ascii_case` compares byte by byte, folding ASCII letters, with no allocation.")],
    notes=("Lowercasing both sides allocates two Strings per comparison; `eq_ignore_ascii_case` allocates nothing. It only folds ASCII, which is right for commands and keywords.", "O(n)", "O(1)"),
    follow_up="What would you use for case-insensitive comparison of arbitrary Unicode text?",
    rules=dict(methods=["to_lowercase", "to_uppercase", "to_ascii_lowercase", "to_ascii_uppercase"]),
))

P.append(dict(
    slug="format-width-precision", title="format! width & precision", level="easy", stage="use",
    tags=["format!", "alignment", "{:#x}"],
    teaches=["`{:<10}` / `{:>8.2}`: alignment, width, precision.", "Precision on a string truncates it; `#` adds `0x`, and the width includes it."],
    statement="""
        - `receipt_line`: the item left-aligned in 10 columns (cut to 10 if longer), the quantity
          right-aligned in 3, a space, then the price right-aligned in 8 with two decimals.
        - `hex_id`: the id as `0x` plus 8 zero-padded lowercase hex digits.

        Use format specifiers, not manual padding.
    """,
    examples=[('receipt_line("Coffee", 2, 3.5)', repr(receipt("Coffee", 2, 3.5)).replace("'", '"')), ("hex_id(255)", '"0x000000ff"')],
    starter="""
        pub fn receipt_line(item: &str, qty: u32, price: f64) -> String {
            todo!()
        }

        pub fn hex_id(id: u32) -> String {
            todo!()
        }
    """,
    solution="""
        pub fn receipt_line(item: &str, qty: u32, price: f64) -> String {
            format!("{item:<10.10}{qty:>3} {price:>8.2}")
        }

        pub fn hex_id(id: u32) -> String {
            format!("{id:#010x}")
        }
    """,
    visible=[
        T("coffee", '"Coffee", 2, 3.5', 'receipt_line("Coffee", 2, 3.5)', rs(receipt("Coffee", 2, 3.5))),
        T("hex", "255", "hex_id(255)", rs("0x000000ff")),
        T("hex_zero", "0", "hex_id(0)", rs("0x00000000")),
        T("ten_char_item", '"Croissant!", 3, 2.5', 'receipt_line("Croissant!", 3, 2.5)', rs(receipt("Croissant!", 3, 2.5))),
        T("rounds_price", '"Tea", 1, 2.999', 'receipt_line("Tea", 1, 2.999)', rs(receipt("Tea", 1, 2.999))),
    ],
    hidden=[
        T("long_item_cut", '"Cappuccino grande", 1, 4.25', 'receipt_line("Cappuccino grande", 1, 4.25)', rs(receipt("Cappuccino grande", 1, 4.25))),
        T("big_price", '"Tea", 120, 1234.5', 'receipt_line("Tea", 120, 1234.5)', rs(receipt("Tea", 120, 1234.5))),
        T("max_id", "u32::MAX", "hex_id(u32::MAX)", rs("0xffffffff")),
        T("hex_letters", "0xDEADBEEF", "hex_id(0xDEAD_BEEF)", rs("0xdeadbeef")),
        T("hex_sixteen", "16", "hex_id(16)", rs("0x00000010")),
        T("empty_item", '"", 0, 0.0', 'receipt_line("", 0, 0.0)', rs(receipt("", 0, 0.0))),
        T("wide_qty", '"Bag", 1000, 1.0 (the width is a minimum)', 'receipt_line("Bag", 1000, 1.0)', rs(receipt("Bag", 1000, 1.0))),
        T("wide_price", '"Car", 1, 123456.789', 'receipt_line("Car", 1, 123456.789)', rs(receipt("Car", 1, 123456.789))),
        T("negative_price", '"Refund", 1, -3.5', 'receipt_line("Refund", 1, -3.5)', rs(receipt("Refund", 1, -3.5))),
        T("unicode_item", '"Crème brûlée", 2, 7.25', 'receipt_line("Crème brûlée", 2, 7.25)', rs(receipt("Crème brûlée", 2, 7.25))),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2205);
            for _ in 0..300 {
                let len = rng.below(14);
                let item = rng.string(len, "abcé ");
                let qty = rng.below(1500) as u32;
                let price = rng.int(-100_000, 10_000_000) as f64 / 100.0;
                let mut want: String = item.chars().take(10).collect();
                while want.chars().count() < 10 {
                    want.push(' ');
                }
                let q = qty.to_string();
                want.push_str(&" ".repeat(3usize.saturating_sub(q.len())));
                want.push_str(&q);
                want.push(' ');
                let p = format!("{price:.2}");
                want.push_str(&" ".repeat(8usize.saturating_sub(p.len())));
                want.push_str(&p);
                let id = rng.next_u64() as u32;
                let mut hex = String::new();
                for shift in (0..8).rev() {
                    hex.push(char::from_digit((id >> (shift * 4)) & 0xf, 16).unwrap());
                }
                check!(format!("item = {item:?}, qty = {qty}, price = {price}, id = {id}"), (receipt_line(&item, qty, price), hex_id(id)), (want, format!("0x{hex}")));
            }
        }
        """,
    ],
    wrong=dict(
        width_excludes_prefix="""
            pub fn receipt_line(item: &str, qty: u32, price: f64) -> String {
                format!("{item:<10.10}{qty:>3} {price:>8.2}")
            }

            pub fn hex_id(id: u32) -> String {
                format!("{id:#08x}")
            }
        """,
        item_not_cut="""
            pub fn receipt_line(item: &str, qty: u32, price: f64) -> String {
                format!("{item:<10}{qty:>3} {price:>8.2}")
            }

            pub fn hex_id(id: u32) -> String {
                format!("{id:#010x}")
            }
        """,
        upper_hex="""
            pub fn receipt_line(item: &str, qty: u32, price: f64) -> String {
                format!("{item:<10.10}{qty:>3} {price:>8.2}")
            }

            pub fn hex_id(id: u32) -> String {
                format!("0x{id:08X}")
            }
        """,
    ),
    hints=[("rust", "`{:<10}` pads to 10 on the right; `{:.10}` on a string keeps at most 10 characters. Combine as `{item:<10.10}`."),
           ("rust", "`{:#010x}`: `#` adds `0x`, `0` pads with zeros, and the width 10 includes the `0x`.")],
    notes=("One `format!` call does all the layout. The same specifiers work in `write!`, `println!` and `Display` impls that call `f.pad`.", "O(1)", "O(1)"),
    follow_up="How would you make the column widths configurable at runtime (`{:>width$}`)?",
    rules=dict(methods=["repeat"]),
))

P.append(dict(
    slug="join-without-join", title="Build a String in one allocation", level="easy", stage="use",
    tags=["with_capacity", "push_str"],
    teaches=["`String::with_capacity` when you know the final size.", "`push_str` into one buffer instead of repeated `+`."],
    statement="""
        Join `words` with `sep` between them, without `join` or `concat`. Compute the final length first and
        allocate once. The tests count heap allocations: a non-empty result takes exactly one.
    """,
    perf=dict(allocs=True),
    examples=[('words = ["a", "bc", "d"], sep = ", "', '"a, bc, d"')],
    starter="""
        pub fn join_words(words: &[&str], sep: &str) -> String {
            todo!()
        }
    """,
    solution="""
        pub fn join_words(words: &[&str], sep: &str) -> String {
            let len =
                words.iter().map(|w| w.len()).sum::<usize>() + sep.len() * words.len().saturating_sub(1);
            let mut out = String::with_capacity(len);
            for (i, w) in words.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                out.push_str(w);
            }
            out
        }
    """,
    visible=[
        T("three", 'words = ["a", "bc", "d"], sep = ", "', 'join_words(&["a", "bc", "d"], ", ")', rs("a, bc, d")),
        T("one", 'words = ["solo"], sep = "-"', 'join_words(&["solo"], "-")', rs("solo")),
        T("no_words", 'words = [], sep = ", "', 'join_words(&[], ", ")', "String::new()"),
        T("no_trailing_sep", 'words = ["a", "b"], sep = "+"', 'join_words(&["a", "b"], "+")', rs("a+b")),
        T("empty_words_keep_seps", 'words = ["", "", ""], sep = "-"', 'join_words(&["", "", ""], "-")', rs("--")),
        T("allocates_once", 'words = ["a", "bc", "d"], sep = ", ": heap allocations', 'anneal_prelude::allocs(|| join_words(&["a", "bc", "d"], ", ")).1.count', "1"),
    ],
    hidden=[
        T("none", "words = [], sep = \"-\"", 'join_words(&[], "-")', "String::new()"),
        T("empty_sep", 'words = ["a", "b"], sep = ""', 'join_words(&["a", "b"], "")', rs("ab")),
        T("unicode_sep", 'words = ["x", "y"], sep = " → "', 'join_words(&["x", "y"], " → ")', '"x → y".to_string()'),
        T("single_empty_word", 'words = [""], sep = "-"', 'join_words(&[""], "-")', "String::new()"),
        T("unicode_words", 'words = ["日本", "é"], sep = "/"', 'join_words(&["日本", "é"], "/")', '"日本/é".to_string()'),
        T("long_sep", 'words = ["a", "b", "c"], sep = "<->"', 'join_words(&["a", "b", "c"], "<->")', rs("a<->b<->c")),
        T("one_allocation", 'words = ["abc"; 10], sep = ", ": (length, capacity, heap allocations)', '{ let (s, seen) = anneal_prelude::allocs(|| join_words(&["abc"; 10], ", ")); (s.len(), s.capacity(), seen.count) }', "(48, 48, 1)"),
        T("exact_capacity_unicode", 'words = ["é", "日"], sep = " → ": capacity is exactly the length', '{ let s = join_words(&["é", "日"], " → "); (s.len(), s.capacity()) }', "(10, 10)"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2206);
            for _ in 0..300 {
                let n = rng.below(6);
                let mut words = Vec::new();
                for _ in 0..n {
                    let len = rng.below(4);
                    words.push(rng.string(len, "ab日"));
                }
                let len = rng.below(3);
                let sep = rng.string(len, ",→");
                let refs: Vec<&str> = words.iter().map(|w| w.as_str()).collect();
                let mut want = String::new();
                for (i, w) in words.iter().enumerate() {
                    if i > 0 {
                        want += &sep;
                    }
                    want += w;
                }
                let got = join_words(&refs, &sep);
                check!(format!("words = {words:?}, sep = {sep:?}"), (got.capacity(), got), (want.len(), want));
            }
        }

        #[test]
        fn scale_500k_words() {
            let words = vec!["ab"; 500_000];
            let s = join_words(&words, ",");
            check!("words = [\\"ab\\"; 500000], sep = \\",\\"", (s.len(), &s[..5], &s[s.len() - 5..]), (1_499_999, "ab,ab", "ab,ab"));
        }
        """,
    ],
    wrong=dict(
        grows_as_it_goes="""
            pub fn join_words(words: &[&str], sep: &str) -> String {
                let mut out = String::new();
                for (i, w) in words.iter().enumerate() {
                    if i > 0 {
                        out.push_str(sep);
                    }
                    out.push_str(w);
                }
                out
            }
        """,
        rebuilds_each_time="""
            pub fn join_words(words: &[&str], sep: &str) -> String {
                let mut out = String::new();
                for (i, w) in words.iter().enumerate() {
                    out = if i == 0 { w.to_string() } else { format!("{out}{sep}{w}") };
                }
                out.shrink_to_fit();
                out
            }
        """,
        trailing_separator="""
            pub fn join_words(words: &[&str], sep: &str) -> String {
                let len = words.iter().map(|w| w.len() + sep.len()).sum::<usize>();
                let mut out = String::with_capacity(len);
                for w in words {
                    out.push_str(w);
                    out.push_str(sep);
                }
                out
            }
        """,
    ),
    hints=[("approach", "Total length is the sum of the word lengths plus one separator between each pair."),
           ("rust", "`saturating_sub(1)` keeps the separator count at 0 for an empty slice.")],
    notes=("Lengths are in bytes, which is what `String` capacity counts, so Unicode separators need no special case. One allocation, then only copies.", "O(total length)", "O(total length)"),
    follow_up="How does `[&str]::join` compute its capacity internally?",
    rules=dict(methods=["join", "concat", "collect", "fold"]),
))

P.append(dict(
    slug="word-frequency", title="Word frequency", level="easy", stage="use",
    tags=["BufRead", "io::Result", "HashMap"], source="W22",
    teaches=["Reading lines from any `BufRead`, propagating `io::Error` with `?`.", "`trim_matches` with a closure to strip punctuation."],
    statement="""
        Count the words read from `input`. A word is a whitespace-separated piece with leading and trailing
        non-alphanumeric characters stripped, lowercased; pieces that end up empty don't count.

        Return `(word, count)` pairs sorted by count descending, then alphabetically. Invalid UTF-8 in the
        input is an error, not a panic.
    """,
    examples=[('"The cat. the DOG!\\ncat"', '[("cat", 2), ("the", 2), ("dog", 1)]')],
    starter="""
        use std::collections::HashMap;
        use std::io::{self, BufRead};

        pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
            todo!()
        }
    """,
    solution="""
        use std::collections::HashMap;
        use std::io::{self, BufRead};

        pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
            let mut counts: HashMap<String, u32> = HashMap::new();
            for line in input.lines() {
                let line = line?;
                for raw in line.split_whitespace() {
                    let word = raw.trim_matches(|c: char| !c.is_alphanumeric());
                    if !word.is_empty() {
                        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
                    }
                }
            }
            let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
            pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            Ok(pairs)
        }
    """,
    visible=[
        T("counts", '"The cat. the DOG!\\ncat"', 'word_frequencies("The cat. the DOG!\\ncat".as_bytes()).unwrap()',
          'vec![("cat".to_string(), 2), ("the".to_string(), 2), ("dog".to_string(), 1)]'),
        T("empty", '""', 'word_frequencies("".as_bytes()).unwrap().len()', "0"),
        T("leetcode_692_first", '"i love leetcode i love coding"', 'word_frequencies("i love leetcode i love coding".as_bytes()).unwrap()',
          'vec![("i".to_string(), 2), ("love".to_string(), 2), ("coding".to_string(), 1), ("leetcode".to_string(), 1)]'),
        T("leetcode_692_second", '"the day is sunny the the the sunny is is"', 'word_frequencies("the day is sunny the the the sunny is is".as_bytes()).unwrap()',
          'vec![("the".to_string(), 4), ("is".to_string(), 3), ("sunny".to_string(), 2), ("day".to_string(), 1)]'),
        T("punctuation_stripped", '"(hi) hi! HI"', 'word_frequencies("(hi) hi! HI".as_bytes()).unwrap()', 'vec![("hi".to_string(), 3)]'),
    ],
    hidden=[
        T("invalid_utf8", 'b"ok \\xff"', 'word_frequencies(&b"ok \\xff"[..]).is_err()', "true"),
        T("punctuation_only", '"-- !! ..."', 'word_frequencies("-- !! ...".as_bytes()).unwrap().len()', "0"),
        T("inner_punctuation_kept", '"don\'t Don\'t"', 'word_frequencies("don\'t Don\'t".as_bytes()).unwrap()', 'vec![("don\'t".to_string(), 2)]'),
        T("unicode_lowercase", '"École école ÉCOLE"', 'word_frequencies("École école ÉCOLE".as_bytes()).unwrap()', 'vec![("école".to_string(), 3)]'),
        T("unicode_punctuation", '"«mot» ¡hola!"', 'word_frequencies("«mot» ¡hola!".as_bytes()).unwrap()', 'vec![("hola".to_string(), 1), ("mot".to_string(), 1)]'),
        T("digits_are_words", '"42 42 x"', 'word_frequencies("42 42 x".as_bytes()).unwrap()', 'vec![("42".to_string(), 2), ("x".to_string(), 1)]'),
        T("crlf_and_blank_lines", '"a\\r\\n\\r\\nb a\\n"', 'word_frequencies("a\\r\\n\\r\\nb a\\n".as_bytes()).unwrap()', 'vec![("a".to_string(), 2), ("b".to_string(), 1)]'),
        T("hyphen_inside_kept", '"well-known -well- known"', 'word_frequencies("well-known -well- known".as_bytes()).unwrap()', 'vec![("known".to_string(), 1), ("well".to_string(), 1), ("well-known".to_string(), 1)]'),
        T("invalid_utf8_later_line", 'b"fine\\nbad \\xc3\\x28"', 'word_frequencies(&b"fine\\nbad \\xc3\\x28"[..]).is_err()', "true"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2207);
            // Each piece and the word it counts as (None: it doesn't count).
            let pieces = [("Cat", Some("cat")), ("cat", Some("cat")), ("dog", Some("dog")), ("DOG!", Some("dog")), ("(cat)", Some("cat")),
                          ("--", None), ("bird.", Some("bird")), ("Émile", Some("émile")), ("b", Some("b"))];
            for _ in 0..300 {
                let n = rng.below(12);
                let mut text = String::new();
                let mut counts = std::collections::BTreeMap::new();
                for _ in 0..n {
                    let (piece, word) = *rng.pick(&pieces);
                    text.push_str(piece);
                    text.push_str(*rng.pick(&[" ", "\\n", "  "]));
                    if let Some(w) = word {
                        *counts.entry(w.to_string()).or_insert(0u32) += 1;
                    }
                }
                let mut want: Vec<(String, u32)> = counts.into_iter().collect();
                want.sort_by(|a, b| b.1.cmp(&a.1));
                check!(format!("input = {text:?}"), word_frequencies(text.as_bytes()).unwrap(), want);
            }
        }

        #[test]
        fn scale_50k_distinct() {
            let mut text = String::new();
            for _ in 0..4 {
                for i in 0..50_000 {
                    text.push_str(&format!("w{i:05} "));
                }
                text.push('\\n');
            }
            let out = word_frequencies(text.as_bytes()).unwrap();
            check!("w00000 … w49999, each 4 times (200000 words)", (out.len(), out[0].clone(), out[49_999].clone()), (50_000, ("w00000".to_string(), 4), ("w49999".to_string(), 4)));
        }
        """,
        """
            #[test]
            fn tab_separates_words() {
                check!(r#""a\\tb a""#, word_frequencies("a\\tb a".as_bytes()).unwrap(), vec![("a".to_string(), 2), ("b".to_string(), 1)]);
            }
        """,
    ],
    wrong=dict(
        no_tiebreak="""
            use std::collections::HashMap;
            use std::io::{self, BufRead};

            pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
                let mut counts: HashMap<String, u32> = HashMap::new();
                for line in input.lines() {
                    let line = line?;
                    for raw in line.split_whitespace() {
                        let word = raw.trim_matches(|c: char| !c.is_alphanumeric());
                        if !word.is_empty() {
                            *counts.entry(word.to_lowercase()).or_insert(0) += 1;
                        }
                    }
                }
                let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(&a.1));
                Ok(pairs)
            }
        """,
        ascii_punctuation_only="""
            use std::collections::HashMap;
            use std::io::{self, BufRead};

            pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
                let mut counts: HashMap<String, u32> = HashMap::new();
                for line in input.lines() {
                    let line = line?;
                    for raw in line.split_whitespace() {
                        let word = raw.trim_matches(|c: char| c.is_ascii_punctuation());
                        if !word.is_empty() {
                            *counts.entry(word.to_lowercase()).or_insert(0) += 1;
                        }
                    }
                }
                let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                Ok(pairs)
            }
        """,
        linear_search="""
            use std::io::{self, BufRead};

            pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
                let mut pairs: Vec<(String, u32)> = Vec::new();
                for line in input.lines() {
                    let line = line?;
                    for raw in line.split_whitespace() {
                        let word = raw.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                        if word.is_empty() {
                            continue;
                        }
                        match pairs.iter().position(|(w, _)| *w == word) {
                            Some(i) => pairs[i].1 += 1,
                            None => pairs.push((word, 1)),
                        }
                    }
                }
                pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                Ok(pairs)
            }
        """,
        splits_on_space_only="""
            use std::collections::HashMap;
            use std::io::{self, BufRead};

            pub fn word_frequencies<R: BufRead>(input: R) -> io::Result<Vec<(String, u32)>> {
                let mut counts: HashMap<String, u32> = HashMap::new();
                for line in input.lines() {
                    let line = line?;
                    // Wrong: splits on ' ' only, so a tab inside a line joins two words.
                    for raw in line.split(' ') {
                        let word = raw.trim_matches(|c: char| !c.is_alphanumeric());
                        if !word.is_empty() {
                            *counts.entry(word.to_lowercase()).or_insert(0) += 1;
                        }
                    }
                }
                let mut pairs: Vec<(String, u32)> = counts.into_iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                Ok(pairs)
            }
        """,
    ),
    hints=[("rust", "`BufRead::lines()` yields `io::Result<String>`; `let line = line?;` propagates a read or UTF-8 error."),
           ("rust", "`raw.trim_matches(|c: char| !c.is_alphanumeric())` strips punctuation from both ends only."),
           ("rust", "Sort with `b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))` for count-descending, then alphabetical.")],
    notes=("Taking `R: BufRead` means the same code reads a file, stdin or a byte slice in tests. Memory grows with the number of distinct words, not the input size.", "O(n + k log k)", "O(k)"),
    follow_up="How would you find the top 10 words without sorting all k of them?",
    related=["S4", "S1"],
))

# ---------------------------------------------------------------- understand (medium)

P.append(dict(
    slug="bytes-chars-char-indices", title="Bytes vs chars vs char_indices", level="medium", stage="understand",
    tags=["UTF-8", "chars", "char_indices"],
    teaches=["`len()` counts bytes; `chars().count()` counts Unicode scalar values.", "`char_indices` gives byte offsets you can slice at."],
    statement="""
        - `sizes` returns `(bytes, chars)` for `s`.
        - `positions` returns the byte offset of every occurrence of `target`.
        - `nth_char` returns the `n`th character, if there is one.
    """,
    examples=[('sizes("naïve")', "(6, 5)"), ("positions(\"añoño\", 'ñ')", "[1, 4]")],
    starter="""
        pub fn sizes(s: &str) -> (usize, usize) {
            todo!()
        }

        pub fn positions(s: &str, target: char) -> Vec<usize> {
            todo!()
        }

        pub fn nth_char(s: &str, n: usize) -> Option<char> {
            todo!()
        }
    """,
    solution="""
        pub fn sizes(s: &str) -> (usize, usize) {
            (s.len(), s.chars().count())
        }

        pub fn positions(s: &str, target: char) -> Vec<usize> {
            s.char_indices().filter(|&(_, c)| c == target).map(|(i, _)| i).collect()
        }

        pub fn nth_char(s: &str, n: usize) -> Option<char> {
            s.chars().nth(n)
        }
    """,
    visible=[
        T("naive", '"naïve"', 'sizes("naïve")', "(6, 5)"),
        T("byte_offsets", "\"añoño\", 'ñ'", "positions(\"añoño\", 'ñ')", "vec![1, 4]"),
        T("ascii_sizes", '"abc"', 'sizes("abc")', "(3, 3)"),
        T("nth_is_by_char", '"héllo", 2', 'nth_char("héllo", 2)', "Some('l')"),
        T("no_occurrence", "\"abc\", 'z'", "positions(\"abc\", 'z')", "Vec::<usize>::new()"),
    ],
    hidden=[
        T("emoji", '"🦀!"', 'sizes("🦀!")', "(5, 2)"),
        T("nth", '"héllo", 1 and 9', '(nth_char("héllo", 1), nth_char("héllo", 9))', "(Some('é'), None)"),
        T("offsets_slice_cleanly", "every offset from positions(\"x→y→z\", '→') is a char boundary", "ok", "true",
          setup="let s = \"x→y→z\";\nlet ok = positions(s, '→').iter().all(|&i| s.is_char_boundary(i) && s[i..].starts_with('→'));"),
        T("empty_everything", '""', "(sizes(\"\"), positions(\"\", 'a'), nth_char(\"\", 0))", "((0, 0), vec![], None)"),
        T("emoji_target", "\"🦀a🦀\", '🦀'", "positions(\"🦀a🦀\", '🦀')", "vec![0, 5]"),
        T("ascii_after_unicode", "\"éaéa\", 'a'", "positions(\"éaéa\", 'a')", "vec![2, 5]"),
        T("adjacent", "\"ññ\", 'ñ'", "positions(\"ññ\", 'ñ')", "vec![0, 2]"),
        T("nth_last_and_past", '"héllo", 4 and 5', '(nth_char("héllo", 4), nth_char("héllo", 5))', "(Some('o'), None)"),
        T("combining_mark", '"e\\u{301}" (e + combining acute)', 'sizes("e\\u{301}")', "(3, 2)"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2208);
            for _ in 0..300 {
                let len = rng.below(10);
                let s = rng.string(len, "aé🦀");
                let target = *rng.pick(&['a', 'é', '🦀']);
                let chars = s.bytes().filter(|b| b & 0xC0 != 0x80).count();
                let want_pos: Vec<usize> = (0..s.len()).filter(|&i| s.is_char_boundary(i) && s[i..].starts_with(target)).collect();
                let n = rng.below(12);
                let want_nth = s.chars().collect::<Vec<_>>().get(n).copied();
                check!(format!("s = {s:?}, target = {target:?}, n = {n}"), (sizes(&s), positions(&s, target), nth_char(&s, n)), ((s.len(), chars), want_pos, want_nth));
            }
        }

        #[test]
        fn scale_200k_chars() {
            let s = "aé".repeat(100_000);
            let p = positions(&s, 'é');
            check!("s = \\"aéaé…\\" (200000 chars), target = 'é'", (p.len(), p[0], p[99_999], sizes(&s)), (100_000, 1, 299_998, (300_000, 200_000)));
        }
        """,
    ],
    wrong=dict(
        char_index_positions="""
            pub fn sizes(s: &str) -> (usize, usize) {
                (s.len(), s.chars().count())
            }

            pub fn positions(s: &str, target: char) -> Vec<usize> {
                s.chars().enumerate().filter(|&(_, c)| c == target).map(|(i, _)| i).collect()
            }

            pub fn nth_char(s: &str, n: usize) -> Option<char> {
                s.chars().nth(n)
            }
        """,
        byte_nth="""
            pub fn sizes(s: &str) -> (usize, usize) {
                (s.len(), s.chars().count())
            }

            pub fn positions(s: &str, target: char) -> Vec<usize> {
                s.char_indices().filter(|&(_, c)| c == target).map(|(i, _)| i).collect()
            }

            pub fn nth_char(s: &str, n: usize) -> Option<char> {
                s.as_bytes().get(n).map(|&b| b as char)
            }
        """,
        offsets_by_recounting="""
            pub fn sizes(s: &str) -> (usize, usize) {
                (s.len(), s.chars().count())
            }

            pub fn positions(s: &str, target: char) -> Vec<usize> {
                let mut out = Vec::new();
                for (k, c) in s.chars().enumerate() {
                    if c == target {
                        out.push(s.chars().take(k).map(char::len_utf8).sum());
                    }
                }
                out
            }

            pub fn nth_char(s: &str, n: usize) -> Option<char> {
                s.chars().nth(n)
            }
        """,
    ),
    hints=[("rust", "`str::len` is bytes. A character outside ASCII takes 2–4 bytes in UTF-8."),
           ("rust", "`char_indices()` yields `(byte_offset, char)`; offsets are always valid slice points.")],
    notes=("`chars().nth(n)` is O(n): UTF-8 has no random access by character. That's why `str` can't be indexed by position.", "O(n)", "O(k)"),
    follow_up="What does a user think of as one character that is several `char`s (grapheme clusters)?",
))

P.append(dict(
    slug="fix-slicing-mid-utf8", title="Fix: slicing mid-UTF-8 panics", mode="fix", level="medium", stage="understand",
    tags=["panic", "char boundaries"],
    teaches=["`&s[..n]` takes bytes and panics if `n` splits a character.", "Finding a byte offset with `char_indices().nth(n)`."],
    statement="`prefix` should return the first `n` characters of `s`. It panics on accented text. Fix it.",
    starter="""
        /// The first `n` characters of `s` (all of `s` if it's shorter).
        pub fn prefix(s: &str, n: usize) -> &str {
            if s.len() <= n {
                s
            } else {
                &s[..n]
            }
        }
    """,
    solution="""
        /// The first `n` characters of `s` (all of `s` if it's shorter).
        pub fn prefix(s: &str, n: usize) -> &str {
            match s.char_indices().nth(n) {
                Some((i, _)) => &s[..i],
                None => s,
            }
        }
    """,
    visible=[
        T("ascii", '"hello", 3', 'prefix("hello", 3)', '"hel"'),
        T("accented", '"héllo", 2', 'prefix("héllo", 2)', '"hé"'),
        T("shorter", '"hi", 5', 'prefix("hi", 5)', '"hi"'),
        T("exact_length", '"abc", 3', 'prefix("abc", 3)', '"abc"'),
        T("empty", '"", 2', 'prefix("", 2)', '""'),
    ],
    hidden=[
        T("emoji", '"🦀🦀🦀", 2', 'prefix("🦀🦀🦀", 2)', '"🦀🦀"'),
        T("zero", '"abc", 0', 'prefix("abc", 0)', '""'),
        T("shorter_in_chars_than_bytes", '"日本", 3', 'prefix("日本", 3)', '"日本"'),
        T("zero_unicode", '"é", 0', 'prefix("é", 0)', '""'),
        T("cjk", '"日本語", 2', 'prefix("日本語", 2)', '"日本"'),
        T("exact_char_count", '"日本", 2', 'prefix("日本", 2)', '"日本"'),
        T("mixed", '"a🦀b", 2', 'prefix("a🦀b", 2)', '"a🦀"'),
        T("combining_mark_is_a_char", '"e\\u{301}x", 1', 'prefix("e\\u{301}x", 1)', '"e"'),
        T("huge_n", '"abc", usize::MAX', 'prefix("abc", usize::MAX)', '"abc"'),
        T("borrows_input", "prefix points into its input", "prefix(&s, 2).as_ptr() == s.as_ptr()", "true", setup='let s = String::from("héllo");'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2209);
            for _ in 0..300 {
                let len = rng.below(10);
                let s = rng.string(len, "aé日🦀");
                let n = rng.below(12);
                let want: String = s.chars().take(n).collect();
                check!(format!("s = {s:?}, n = {n}"), prefix(&s, n), want.as_str());
            }
        }

        #[test]
        fn scale_200k_chars() {
            let s = "é".repeat(200_000);
            check!("s = 200000 × 'é', n = 150000", prefix(&s, 150_000).len(), 300_000);
        }
        """,
    ],
    wrong=dict(
        byte_budget="""
            /// The first `n` characters of `s` (all of `s` if it's shorter).
            pub fn prefix(s: &str, n: usize) -> &str {
                if s.len() <= n {
                    s
                } else {
                    let mut end = n;
                    while !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    &s[..end]
                }
            }
        """,
        one_char_too_many="""
            /// The first `n` characters of `s` (all of `s` if it's shorter).
            pub fn prefix(s: &str, n: usize) -> &str {
                match s.char_indices().nth(n) {
                    Some((i, c)) => &s[..i + c.len_utf8()],
                    None => s,
                }
            }
        """,
    ),
    hints=[("rust", "`s.len()` and `&s[..n]` both count bytes, not characters."),
           ("rust", "The byte offset where character `n` starts is `s.char_indices().nth(n)`; if there isn't one, the whole string fits.")],
    notes=("Slicing is by byte offset and panics off a character boundary. `char_indices` only yields boundaries, so slicing at one is always safe.", "O(n)", "O(1)"),
    follow_up="When would you use `s.get(..n)` instead, and what does it return?",
))

P.append(dict(
    slug="fix-len-counts-bytes", title="Fix: len() counts bytes", mode="fix", level="medium", stage="understand",
    tags=["UTF-8", "chars().count()"],
    teaches=["Width calculations need a character count, not `len()`."],
    statement="`center` pads text to a width in characters. It gets accented text wrong. Fix it.",
    starter="""
        /// Centers `s` in a field `width` characters wide, padding with `fill`.
        /// Odd padding puts the extra character on the right.
        pub fn center(s: &str, width: usize, fill: char) -> String {
            let len = s.len();
            if len >= width {
                return s.to_string();
            }
            let left = (width - len) / 2;
            let right = width - len - left;
            let mut out = String::new();
            out.extend(std::iter::repeat(fill).take(left));
            out.push_str(s);
            out.extend(std::iter::repeat(fill).take(right));
            out
        }
    """,
    solution="""
        /// Centers `s` in a field `width` characters wide, padding with `fill`.
        /// Odd padding puts the extra character on the right.
        pub fn center(s: &str, width: usize, fill: char) -> String {
            let len = s.chars().count();
            if len >= width {
                return s.to_string();
            }
            let left = (width - len) / 2;
            let right = width - len - left;
            let mut out = String::new();
            out.extend(std::iter::repeat(fill).take(left));
            out.push_str(s);
            out.extend(std::iter::repeat(fill).take(right));
            out
        }
    """,
    visible=[
        T("ascii", "\"ab\", 6, '*'", "center(\"ab\", 6, '*')", rs("**ab**")),
        T("accented", "\"né\", 4, '.'", "center(\"né\", 4, '.')", '".né.".to_string()'),
        T("even_padding", "\"abcd\", 8, '-'", "center(\"abcd\", 8, '-')", rs("--abcd--")),
        T("no_padding_needed", "\"abc\", 3, '*'", "center(\"abc\", 3, '*')", rs("abc")),
        T("odd_extra_right", "\"a\", 4, '*'", "center(\"a\", 4, '*')", rs("*a**")),
    ],
    hidden=[
        T("odd", "\"abc\", 6, '-'", "center(\"abc\", 6, '-')", rs("-abc--")),
        T("too_wide", "\"日本語\", 3, ' '", "center(\"日本語\", 3, ' ')", '"日本語".to_string()'),
        T("multibyte_fill", "\"x\", 3, '·'", "center(\"x\", 3, '·')", '"·x·".to_string()'),
        T("empty", "\"\", 3, 'x'", "center(\"\", 3, 'x')", rs("xxx")),
        T("width_zero", "\"ab\", 0, '*'", "center(\"ab\", 0, '*')", rs("ab")),
        T("emoji", "\"🦀\", 3, '.'", "center(\"🦀\", 3, '.')", '".🦀.".to_string()'),
        T("cjk", "\"日本\", 6, ' '", "center(\"日本\", 6, ' ')", '"  日本  ".to_string()'),
        T("fits_in_chars_not_bytes", "\"éé\", 3, '.'", "center(\"éé\", 3, '.')", '"éé.".to_string()'),
        T("inner_spaces", "\"a b\", 5, '*'", "center(\"a b\", 5, '*')", rs("*a b*")),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2210);
            for _ in 0..300 {
                let len = rng.below(6);
                let s = rng.string(len, "a é日");
                let width = rng.below(10);
                let fill = *rng.pick(&['*', '·']);
                let n = s.chars().count();
                let mut want = String::new();
                if n >= width {
                    want.push_str(&s);
                } else {
                    let left = (width - n) / 2;
                    for _ in 0..left {
                        want.push(fill);
                    }
                    want.push_str(&s);
                    for _ in 0..width - n - left {
                        want.push(fill);
                    }
                }
                check!(format!("s = {s:?}, width = {width}, fill = {fill:?}"), center(&s, width, fill), want);
            }
        }
        """,
    ],
    wrong=dict(
        ascii_only_count="""
            /// Centers `s` in a field `width` characters wide, padding with `fill`.
            /// Odd padding puts the extra character on the right.
            pub fn center(s: &str, width: usize, fill: char) -> String {
                let len = s.bytes().filter(u8::is_ascii).count();
                if len >= width {
                    return s.to_string();
                }
                let left = (width - len) / 2;
                let right = width - len - left;
                let mut out = String::new();
                out.extend(std::iter::repeat(fill).take(left));
                out.push_str(s);
                out.extend(std::iter::repeat(fill).take(right));
                out
            }
        """,
        letters_only="""
            /// Centers `s` in a field `width` characters wide, padding with `fill`.
            /// Odd padding puts the extra character on the right.
            pub fn center(s: &str, width: usize, fill: char) -> String {
                let len = s.chars().filter(|c| c.is_alphanumeric()).count();
                if len >= width {
                    return s.to_string();
                }
                let left = (width - len) / 2;
                let right = width - len - left;
                let mut out = String::new();
                out.extend(std::iter::repeat(fill).take(left));
                out.push_str(s);
                out.extend(std::iter::repeat(fill).take(right));
                out
            }
        """,
    ),
    hints=[("rust", "`\"né\".len()` is 3: `é` takes two bytes. The padding math wants 2.")],
    notes=("Counting chars fixes accented and CJK text. It still isn't display width: CJK characters usually take two terminal columns, and combining marks take none.", "O(n)", "O(n)"),
    follow_up="How would you center text by terminal display width?",
    rules=dict(lines=1),
))

P.append(dict(
    slug="reverse-each-word", title="Reverse each word, Unicode-safe", level="medium", stage="understand",
    tags=["chars().rev()", "collect::<String>"],
    teaches=["Reversing `chars()`, not bytes.", "Collecting chars straight into a `String`."],
    statement="Reverse the characters of each whitespace-separated word, keeping word order. Separate words with single spaces.",
    examples=[('"héllo wörld"', '"olléh dlröw"')],
    starter="""
        pub fn reverse_each_word(s: &str) -> String {
            todo!()
        }
    """,
    solution="""
        pub fn reverse_each_word(s: &str) -> String {
            s.split_whitespace()
                .map(|w| w.chars().rev().collect::<String>())
                .collect::<Vec<_>>()
                .join(" ")
        }
    """,
    visible=[
        T("accents", '"héllo wörld"', 'reverse_each_word("héllo wörld")', '"olléh dlröw".to_string()'),
        T("ascii", '"ab cd"', 'reverse_each_word("ab cd")', rs("ba dc")),
        T("leetcode_557_first", '"Let\'s take LeetCode contest"', 'reverse_each_word("Let\'s take LeetCode contest")', rs("s'teL ekat edoCteeL tsetnoc")),
        T("leetcode_557_second", '"Mr Ding"', 'reverse_each_word("Mr Ding")', rs("rM gniD")),
        T("one_word", '"hello"', 'reverse_each_word("hello")', rs("olleh")),
    ],
    hidden=[
        T("emoji", '"🦀x"', 'reverse_each_word("🦀x")', '"x🦀".to_string()'),
        T("extra_spaces", '"  a   bc "', 'reverse_each_word("  a   bc ")', rs("a cb")),
        T("empty", '""', 'reverse_each_word("")', "String::new()"),
        T("only_spaces", '"   "', 'reverse_each_word("   ")', "String::new()"),
        T("tabs_and_newlines", '"ab\\tcd\\nef"', 'reverse_each_word("ab\\tcd\\nef")', rs("ba dc fe")),
        T("palindrome", '"abba x"', 'reverse_each_word("abba x")', rs("abba x")),
        T("cjk", '"日本語 テスト"', 'reverse_each_word("日本語 テスト")', '"語本日 トステ".to_string()'),
        T("single_char", '"a"', 'reverse_each_word("a")', rs("a")),
        T("punctuation_moves", '"a1! b2?"', 'reverse_each_word("a1! b2?")', rs("!1a ?2b")),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2211);
            for _ in 0..300 {
                let len = rng.below(14);
                let s = rng.string(len, "abé🦀  \\t");
                let mut words: Vec<String> = Vec::new();
                let mut cur: Vec<char> = Vec::new();
                for c in s.chars().chain(std::iter::once(' ')) {
                    if c.is_whitespace() {
                        if !cur.is_empty() {
                            cur.reverse();
                            words.push(cur.iter().collect());
                            cur.clear();
                        }
                    } else {
                        cur.push(c);
                    }
                }
                check!(format!("s = {s:?}"), reverse_each_word(&s), words.join(" "));
            }
        }

        #[test]
        fn scale_200k_words() {
            let s = "abc ".repeat(200_000);
            let out = reverse_each_word(&s);
            check!("s = \\"abc abc …\\" (200000 words)", (out.len(), &out[..7]), (799_999, "cba cba"));
        }
        """,
    ],
    wrong=dict(
        reverses_bytes="""
            pub fn reverse_each_word(s: &str) -> String {
                s.split_whitespace()
                    .map(|w| String::from_utf8_lossy(&w.bytes().rev().collect::<Vec<u8>>()).into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        """,
        reverses_word_order_too="""
            pub fn reverse_each_word(s: &str) -> String {
                s.split_whitespace()
                    .rev()
                    .map(|w| w.chars().rev().collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        """,
        keeps_spacing="""
            pub fn reverse_each_word(s: &str) -> String {
                s.split(' ')
                    .map(|w| w.chars().rev().collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        """,
    ),
    hints=[("rust", "Reversing bytes would split multi-byte characters into invalid UTF-8. `chars().rev()` reverses whole characters."),
           ("rust", "`collect::<String>()` builds a String from an iterator of `char`.")],
    notes=("`chars().rev()` works because `Chars` is a `DoubleEndedIterator`: UTF-8 can be decoded backwards. Combining characters (e + ◌́) would still come out in the wrong order.", "O(n)", "O(n)"),
    follow_up="Which crate would you reach for to reverse by grapheme cluster?",
))

P.append(dict(
    slug="cow-str", title="Cow<str>: allocate only when needed", level="medium", stage="understand",
    tags=["Cow", "zero-copy"],
    teaches=["`Cow::Borrowed` when the input is already right, `Cow::Owned` when it had to change.", "Finding the first byte that needs work before allocating."],
    statement="""
        Escape `&`, `<`, `>`, `"` and `'` as `&amp;`, `&lt;`, `&gt;`, `&quot;` and `&#39;`. When the input
        contains none of them, return it borrowed, without allocating.
    """,
    examples=[('"a<b"', '"a&lt;b" (Owned)'), ('"plain"', '"plain" (Borrowed)')],
    starter="""
        use std::borrow::Cow;

        pub fn escape_html(s: &str) -> Cow<'_, str> {
            todo!()
        }
    """,
    solution="""
        use std::borrow::Cow;

        pub fn escape_html(s: &str) -> Cow<'_, str> {
            let special = |c: char| matches!(c, '&' | '<' | '>' | '"' | '\\'');
            let Some(first) = s.find(special) else {
                return Cow::Borrowed(s);
            };
            let mut out = String::with_capacity(s.len() + 8);
            out.push_str(&s[..first]);
            for c in s[first..].chars() {
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '"' => out.push_str("&quot;"),
                    '\\'' => out.push_str("&#39;"),
                    _ => out.push(c),
                }
            }
            Cow::Owned(out)
        }
    """,
    visible=[
        T("escapes", '"a<b"', 'escape_html("a<b").into_owned()', rs("a&lt;b")),
        T("borrows", '"plain"', 'matches!(escape_html("plain"), std::borrow::Cow::Borrowed("plain"))', "true"),
        T("ampersand", '"a&b"', 'escape_html("a&b").into_owned()', rs("a&amp;b")),
        T("already_escaped", '"&lt;"', 'escape_html("&lt;").into_owned()', rs("&amp;lt;")),
        T("quotes_and_gt", '"\\"x\\" > y"', 'escape_html("\\"x\\" > y").into_owned()', rs("&quot;x&quot; &gt; y")),
    ],
    hidden=[
        T("all_five", "\"&<>\\\"'\"", "escape_html(\"&<>\\\"'\").into_owned()", rs("&amp;&lt;&gt;&quot;&#39;")),
        T("owned_when_changed", '"x&y"', 'matches!(escape_html("x&y"), std::borrow::Cow::Owned(_))', "true"),
        T("unicode", '"é<é"', 'escape_html("é<é").into_owned()', '"é&lt;é".to_string()'),
        T("empty", '""', 'matches!(escape_html(""), std::borrow::Cow::Borrowed(""))', "true"),
        T("special_at_end", '"abc>"', 'escape_html("abc>").into_owned()', rs("abc&gt;")),
        T("special_at_start", '"<abc"', 'escape_html("<abc").into_owned()', rs("&lt;abc")),
        T("consecutive", '"<<>>"', 'escape_html("<<>>").into_owned()', rs("&lt;&lt;&gt;&gt;")),
        T("single_quote", "\"'\"", "escape_html(\"'\").into_owned()", rs("&#39;")),
        T("unicode_borrowed", '"日本語 ✓"', 'matches!(escape_html("日本語 ✓"), std::borrow::Cow::Borrowed(_))', "true"),
        T("borrowed_same_pointer", '"no specials here"', "escape_html(&s).as_ptr() == s.as_ptr()", "true", setup='let s = String::from("no specials here");'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2212);
            for _ in 0..300 {
                let len = rng.below(10);
                let s = rng.string(len, "a é&<>\\"'");
                let mut want = String::new();
                for c in s.chars() {
                    match c {
                        '&' => want += "&amp;",
                        '<' => want += "&lt;",
                        '>' => want += "&gt;",
                        '"' => want += "&quot;",
                        '\\'' => want += "&#39;",
                        _ => want.push(c),
                    }
                }
                let out = escape_html(&s);
                let borrowed = matches!(out, std::borrow::Cow::Borrowed(_));
                check!(format!("s = {s:?}"), (out.into_owned(), borrowed), (want.clone(), want == s));
            }
        }

        #[test]
        fn scale_200k() {
            let s = "a<b&".repeat(50_000);
            let out = escape_html(&s);
            check!("s = \\"a<b&…\\" (200000 chars)", (out.len(), &out[..12]), (550_000, "a&lt;b&amp;a"));
        }
        """,
    ],
    wrong=dict(
        amp_replaced_last="""
            use std::borrow::Cow;

            pub fn escape_html(s: &str) -> Cow<'_, str> {
                if !s.contains(['&', '<', '>', '"', '\\'']) {
                    return Cow::Borrowed(s);
                }
                Cow::Owned(s.replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\\'', "&#39;").replace('&', "&amp;"))
            }
        """,
        always_owned="""
            use std::borrow::Cow;

            pub fn escape_html(s: &str) -> Cow<'_, str> {
                Cow::Owned(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\\'', "&#39;"))
            }
        """,
        forgets_single_quote="""
            use std::borrow::Cow;

            pub fn escape_html(s: &str) -> Cow<'_, str> {
                if !s.contains(['&', '<', '>', '"']) {
                    return Cow::Borrowed(s);
                }
                Cow::Owned(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;"))
            }
        """,
    ),
    hints=[("approach", "Most inputs need no escaping. Scan for the first special character before allocating anything."),
           ("rust", "`let Some(i) = s.find(pred) else { return Cow::Borrowed(s) };` handles the common case in one line."),
           ("rust", "Copy `&s[..first]` in one go, then handle characters from `first` on.")],
    notes=("The common case costs one scan and no allocation. Callers can treat both variants as `&str` via `Deref`, or call `into_owned` when they need a `String`.", "O(n)", "O(n) only when escaping"),
    follow_up="Where does std use `Cow<str>` (hint: `String::from_utf8_lossy`)?",
    related=["S1"],
))

P.append(dict(
    slug="display-vs-debug", title="Display vs Debug", level="medium", stage="understand",
    tags=["Display", "f.pad", "Formatter"],
    teaches=["`Display` is for users, `Debug` for programmers.", "`f.pad` makes your `Display` respect width and alignment."],
    statement="""
        Implement `Display` for `Money` as `<amount> <currency>` with two decimals, and a leading `-` for
        negative amounts. It must respect width and alignment, so `format!("{:>12}", m)` right-aligns it.
        `Debug` stays derived.
    """,
    examples=[("Money { cents: 1234, currency: \"USD\" }", '"12.34 USD"'), ("cents: -5", '"-0.05 USD"')],
    starter="""
        use std::fmt;

        #[derive(Debug)]
        pub struct Money {
            pub cents: i64,
            pub currency: &'static str,
        }

        impl fmt::Display for Money {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                todo!()
            }
        }
    """,
    solution="""
        use std::fmt;

        #[derive(Debug)]
        pub struct Money {
            pub cents: i64,
            pub currency: &'static str,
        }

        impl fmt::Display for Money {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let sign = if self.cents < 0 { "-" } else { "" };
                let abs = self.cents.unsigned_abs();
                f.pad(&format!("{sign}{}.{:02} {}", abs / 100, abs % 100, self.currency))
            }
        }
    """,
    visible=[
        T("display", "12.34 USD", 'format!("{}", Money { cents: 1234, currency: "USD" })', rs("12.34 USD")),
        T("negative_cents", "-5 cents", 'format!("{}", Money { cents: -5, currency: "EUR" })', rs("-0.05 EUR")),
        T("whole_amount", "500 cents", 'format!("{}", Money { cents: 500, currency: "USD" })', rs("5.00 USD")),
        T("zero", "0 cents", 'format!("{}", Money { cents: 0, currency: "X" })', rs("0.00 X")),
        T("right_aligned", "{:>10} then |", 'format!("{:>10}|", Money { cents: 99, currency: "EUR" })', rs("  0.99 EUR|")),
    ],
    hidden=[
        T("width", "{:>12} then |", 'format!("{:>12}|", Money { cents: 1234, currency: "USD" })', rs("   12.34 USD|")),
        T("left", "{:<10} then |", 'format!("{:<10}|", Money { cents: 7, currency: "GBP" })', rs("0.07 GBP  |")),
        T("debug_unchanged", "{:?}", 'format!("{:?}", Money { cents: 1, currency: "USD" })', r'"Money { cents: 1, currency: \"USD\" }".to_string()'),
        T("min_value", "i64::MIN cents", 'format!("{}", Money { cents: i64::MIN, currency: "X" })', rs("-92233720368547758.08 X")),
        T("max_value", "i64::MAX cents", 'format!("{}", Money { cents: i64::MAX, currency: "X" })', rs("92233720368547758.07 X")),
        T("centered", "{:^12} then |", 'format!("{:^12}|", Money { cents: 1234, currency: "USD" })', rs(" 12.34 USD  |")),
        T("fill_char", "{:*<12}", 'format!("{:*<12}", Money { cents: 7, currency: "GBP" })', rs("0.07 GBP****")),
        T("width_too_small", "{:>3}", 'format!("{:>3}", Money { cents: 1234, currency: "USD" })', rs("12.34 USD")),
        T("negative_whole", "-100 cents", 'format!("{}", Money { cents: -100, currency: "USD" })', rs("-1.00 USD")),
        T("negative_padded", "{:>11} with -12345 cents", 'format!("{:>11}", Money { cents: -12345, currency: "USD" })', rs("-123.45 USD")),
        T("negative_width", "{:>12} with -5 cents", 'format!("{:>12}", Money { cents: -5, currency: "EUR" })', rs("   -0.05 EUR")),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2213);
            for _ in 0..300 {
                let cents = rng.int(-1_000_000_000_000, 1_000_000_000_000);
                let width = rng.below(24);
                let abs = cents.unsigned_abs();
                let mut plain = String::new();
                if cents < 0 {
                    plain.push('-');
                }
                plain += &(abs / 100).to_string();
                plain.push('.');
                if abs % 100 < 10 {
                    plain.push('0');
                }
                plain += &(abs % 100).to_string();
                plain += " JPY";
                let mut padded = " ".repeat(width.saturating_sub(plain.len()));
                padded += &plain;
                let m = Money { cents, currency: "JPY" };
                check!(format!("cents = {cents}, {{:>{width}}}"), (format!("{m}"), format!("{m:>width$}")), (plain, padded));
            }
        }
        """,
    ],
    wrong=dict(
        write_ignores_width="""
            use std::fmt;

            #[derive(Debug)]
            pub struct Money {
                pub cents: i64,
                pub currency: &'static str,
            }

            impl fmt::Display for Money {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    let sign = if self.cents < 0 { "-" } else { "" };
                    let abs = self.cents.unsigned_abs();
                    write!(f, "{sign}{}.{:02} {}", abs / 100, abs % 100, self.currency)
                }
            }
        """,
        sign_lost="""
            use std::fmt;

            #[derive(Debug)]
            pub struct Money {
                pub cents: i64,
                pub currency: &'static str,
            }

            impl fmt::Display for Money {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.pad(&format!("{}.{:02} {}", self.cents / 100, (self.cents % 100).abs(), self.currency))
                }
            }
        """,
        abs_overflows="""
            use std::fmt;

            #[derive(Debug)]
            pub struct Money {
                pub cents: i64,
                pub currency: &'static str,
            }

            impl fmt::Display for Money {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    let sign = if self.cents < 0 { "-" } else { "" };
                    let abs = self.cents.abs();
                    f.pad(&format!("{sign}{}.{:02} {}", abs / 100, abs % 100, self.currency))
                }
            }
        """,
    ),
    hints=[("rust", "`write!(f, ...)` ignores the caller's width. `f.pad(s)` applies width, fill and alignment to `s`."),
           ("edge case", "`-5 / 100` is `0`, so the sign must be handled separately. `unsigned_abs` also survives `i64::MIN`.")],
    notes=("Formatting the whole amount first, then padding once, keeps alignment correct. `unsigned_abs` avoids the overflow `abs()` hits on `i64::MIN`.", "O(1)", "O(1)"),
    follow_up="How would you honour `{:.1}` precision for Money without allocating?",
    related=["L4"],
))

P.append(dict(
    slug="fromstr-setting", title="FromStr for a key = value line", level="medium", stage="understand",
    tags=["FromStr", "split_once", "error enums"],
    teaches=["`impl FromStr` so callers can write `line.parse::<Setting>()`.", "An error enum that says what went wrong."],
    statement="""
        Implement `FromStr` for `Setting`, parsing lines like `retries = 3`. Whitespace around the key and
        value is ignored. Errors: no `=` → `MissingEquals`, empty key → `EmptyKey`, value not an `i64` →
        `BadValue(value)` with the trimmed value text.
    """,
    starter="""
        use std::str::FromStr;

        #[derive(Debug, PartialEq)]
        pub struct Setting {
            pub key: String,
            pub value: i64,
        }

        #[derive(Debug, PartialEq)]
        pub enum SettingError {
            MissingEquals,
            EmptyKey,
            BadValue(String),
        }

        impl FromStr for Setting {
            type Err = SettingError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                todo!()
            }
        }
    """,
    solution="""
        use std::str::FromStr;

        #[derive(Debug, PartialEq)]
        pub struct Setting {
            pub key: String,
            pub value: i64,
        }

        #[derive(Debug, PartialEq)]
        pub enum SettingError {
            MissingEquals,
            EmptyKey,
            BadValue(String),
        }

        impl FromStr for Setting {
            type Err = SettingError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let (key, value) = s.split_once('=').ok_or(SettingError::MissingEquals)?;
                let key = key.trim();
                if key.is_empty() {
                    return Err(SettingError::EmptyKey);
                }
                let value = value.trim();
                let value = value.parse().map_err(|_| SettingError::BadValue(value.to_string()))?;
                Ok(Setting { key: key.to_string(), value })
            }
        }
    """,
    visible=[
        T("parses", '"retries = 3"', '"retries = 3".parse::<Setting>()', 'Ok(Setting { key: "retries".to_string(), value: 3 })'),
        T("no_equals", '"retries"', '"retries".parse::<Setting>()', "Err(SettingError::MissingEquals)"),
        T("spaces_trimmed", '"  a   =   1  "', '"  a   =   1  ".parse::<Setting>()', 'Ok(Setting { key: "a".to_string(), value: 1 })'),
        T("bad_value_text", '"x = ten"', '"x = ten".parse::<Setting>()', 'Err(SettingError::BadValue("ten".to_string()))'),
        T("empty_key_visible", '"= 3"', '"= 3".parse::<Setting>()', "Err(SettingError::EmptyKey)"),
    ],
    hidden=[
        T("empty_key", '" = 3"', '" = 3".parse::<Setting>()', "Err(SettingError::EmptyKey)"),
        T("bad_value", '"a = x "', '"a = x ".parse::<Setting>()', 'Err(SettingError::BadValue("x".to_string()))'),
        T("second_equals_is_value", '"a=b=c"', '"a=b=c".parse::<Setting>()', 'Err(SettingError::BadValue("b=c".to_string()))'),
        T("negative", '"offset=-7"', '"offset=-7".parse::<Setting>()', 'Ok(Setting { key: "offset".to_string(), value: -7 })'),
        T("empty_line", '""', '"".parse::<Setting>()', "Err(SettingError::MissingEquals)"),
        T("just_equals", '"="', '"=".parse::<Setting>()', "Err(SettingError::EmptyKey)"),
        T("missing_value", '"a ="', '"a =".parse::<Setting>()', 'Err(SettingError::BadValue(String::new()))'),
        T("key_checked_first", '" = x"', '" = x".parse::<Setting>()', "Err(SettingError::EmptyKey)"),
        T("key_with_space", '"max retries = 2"', '"max retries = 2".parse::<Setting>()', 'Ok(Setting { key: "max retries".to_string(), value: 2 })'),
        T("plus_sign", '"a=+5"', '"a=+5".parse::<Setting>()', 'Ok(Setting { key: "a".to_string(), value: 5 })'),
        T("i64_min", '"a=-9223372036854775808"', '"a=-9223372036854775808".parse::<Setting>()', 'Ok(Setting { key: "a".to_string(), value: i64::MIN })'),
        T("too_big", '"a = 9223372036854775808"', '"a = 9223372036854775808".parse::<Setting>()', 'Err(SettingError::BadValue("9223372036854775808".to_string()))'),
        T("unicode_key_and_tabs", '"délai\\t=\\t5"', '"délai\\t=\\t5".parse::<Setting>()', 'Ok(Setting { key: "délai".to_string(), value: 5 })'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2214);
            let keys = ["", "k", " k ", "a b", "é"];
            let values = ["1", " -2 ", "x", "", "3=4", "+0"];
            for _ in 0..300 {
                let key = *rng.pick(&keys);
                let value = *rng.pick(&values);
                let line = if rng.below(5) == 0 { format!("{key}{value}") } else { format!("{key}={value}") };
                let want = match line.find('=') {
                    None => Err(SettingError::MissingEquals),
                    Some(i) => {
                        let (k, v) = (line[..i].trim(), line[i + 1..].trim());
                        if k.is_empty() {
                            Err(SettingError::EmptyKey)
                        } else {
                            match v.parse::<i64>() {
                                Ok(value) => Ok(Setting { key: k.to_string(), value }),
                                Err(_) => Err(SettingError::BadValue(v.to_string())),
                            }
                        }
                    }
                };
                check!(format!("line = {line:?}"), line.parse::<Setting>(), want);
            }
        }
        """,
    ],
    wrong=dict(
        splits_at_last_equals="""
            use std::str::FromStr;

            #[derive(Debug, PartialEq)]
            pub struct Setting {
                pub key: String,
                pub value: i64,
            }

            #[derive(Debug, PartialEq)]
            pub enum SettingError {
                MissingEquals,
                EmptyKey,
                BadValue(String),
            }

            impl FromStr for Setting {
                type Err = SettingError;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let (key, value) = s.rsplit_once('=').ok_or(SettingError::MissingEquals)?;
                    let key = key.trim();
                    if key.is_empty() {
                        return Err(SettingError::EmptyKey);
                    }
                    let value = value.trim();
                    let value = value.parse().map_err(|_| SettingError::BadValue(value.to_string()))?;
                    Ok(Setting { key: key.to_string(), value })
                }
            }
        """,
        value_checked_first="""
            use std::str::FromStr;

            #[derive(Debug, PartialEq)]
            pub struct Setting {
                pub key: String,
                pub value: i64,
            }

            #[derive(Debug, PartialEq)]
            pub enum SettingError {
                MissingEquals,
                EmptyKey,
                BadValue(String),
            }

            impl FromStr for Setting {
                type Err = SettingError;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let (key, value) = s.split_once('=').ok_or(SettingError::MissingEquals)?;
                    let value = value.trim();
                    let value = value.parse().map_err(|_| SettingError::BadValue(value.to_string()))?;
                    let key = key.trim();
                    if key.is_empty() {
                        return Err(SettingError::EmptyKey);
                    }
                    Ok(Setting { key: key.to_string(), value })
                }
            }
        """,
        untrimmed_error_text="""
            use std::str::FromStr;

            #[derive(Debug, PartialEq)]
            pub struct Setting {
                pub key: String,
                pub value: i64,
            }

            #[derive(Debug, PartialEq)]
            pub enum SettingError {
                MissingEquals,
                EmptyKey,
                BadValue(String),
            }

            impl FromStr for Setting {
                type Err = SettingError;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let (key, raw) = s.split_once('=').ok_or(SettingError::MissingEquals)?;
                    let key = key.trim();
                    if key.is_empty() {
                        return Err(SettingError::EmptyKey);
                    }
                    let value = raw.trim().parse().map_err(|_| SettingError::BadValue(raw.to_string()))?;
                    Ok(Setting { key: key.to_string(), value })
                }
            }
        """,
    ),
    hints=[("rust", "`s.split_once('=')` returns `Option<(&str, &str)>`; `.ok_or(SettingError::MissingEquals)?` turns `None` into the error."),
           ("rust", "`map_err` replaces the `ParseIntError` with your own variant, keeping the text that failed.")],
    notes=("`FromStr` plugs into `str::parse`, so callers get `\"...\".parse::<Setting>()` for free. Each check exits early with a specific error.", "O(n)", "O(n)"),
    follow_up="How would you add the line number to the errors when parsing a whole file?",
    related=["S1", "L4"],
))

P.append(dict(
    slug="unicode-safe-truncate", title="Unicode-safe truncate", level="medium", stage="understand",
    tags=["is_char_boundary", "byte budgets"],
    teaches=["Cutting to a byte budget at a character boundary.", "`let ... else` for an early return."],
    statement="""
        Fit `s` into `max_bytes` bytes. If it already fits, return it unchanged. Otherwise cut it at a
        character boundary and append `…` (3 bytes), so the whole result is at most `max_bytes` bytes. If
        there's no room even for `…`, return an empty string.
    """,
    examples=[('"hello world", 8', '"hello…"'), ('"héllo", 5', '"h…"')],
    starter="""
        pub fn truncate(s: &str, max_bytes: usize) -> String {
            todo!()
        }
    """,
    solution="""
        pub fn truncate(s: &str, max_bytes: usize) -> String {
            if s.len() <= max_bytes {
                return s.to_string();
            }
            let Some(mut end) = max_bytes.checked_sub('…'.len_utf8()) else {
                return String::new();
            };
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}…", &s[..end])
        }
    """,
    visible=[
        T("ascii", '"hello world", 8', 'truncate("hello world", 8)', '"hello…".to_string()'),
        T("fits", '"short", 10', 'truncate("short", 10)', rs("short")),
        T("only_room_for_ellipsis", '"abcd", 3', 'truncate("abcd", 3)', '"…".to_string()'),
        T("cut_before_accent", '"café au lait", 7', 'truncate("café au lait", 7)', '"caf…".to_string()'),
        T("empty_input", '"", 0', 'truncate("", 0)', "String::new()"),
    ],
    hidden=[
        T("mid_char", '"héllo", 5', 'truncate("héllo", 5)', '"h…".to_string()'),
        T("cjk", '"日本語テキスト", 10', 'truncate("日本語テキスト", 10)', '"日本…".to_string()'),
        T("no_room", '"abc", 2', 'truncate("abc", 2)', "String::new()"),
        T("exact_fit", '"abcd", 4', 'truncate("abcd", 4)', rs("abcd")),
        T("emoji", '"🦀🦀🦀", 7', 'truncate("🦀🦀🦀", 7)', '"🦀…".to_string()'),
        T("emoji_back_to_zero", '"🦀🦀", 6', 'truncate("🦀🦀", 6)', '"…".to_string()'),
        T("zero_budget", '"a", 0', 'truncate("a", 0)', "String::new()"),
        T("one_over", '"abcdef", 5', 'truncate("abcdef", 5)', '"ab…".to_string()'),
        T("unicode_fits", '"é", 2', 'truncate("é", 2)', '"é".to_string()'),
        T("ellipsis_input", '"……", 5', 'truncate("……", 5)', '"…".to_string()'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2215);
            for _ in 0..400 {
                let len = rng.below(8);
                let s = rng.string(len, "aé日🦀");
                let max = rng.below(16);
                let want = if s.len() <= max {
                    s.clone()
                } else if max < 3 {
                    String::new()
                } else {
                    let mut kept = String::new();
                    for c in s.chars() {
                        if kept.len() + c.len_utf8() > max - 3 {
                            break;
                        }
                        kept.push(c);
                    }
                    kept + "…"
                };
                check!(format!("s = {s:?}, max_bytes = {max}"), truncate(&s, max), want);
            }
        }
        """,
    ],
    wrong=dict(
        ellipsis_one_byte="""
            pub fn truncate(s: &str, max_bytes: usize) -> String {
                if s.len() <= max_bytes {
                    return s.to_string();
                }
                let Some(mut end) = max_bytes.checked_sub(1) else {
                    return String::new();
                };
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}…", &s[..end])
            }
        """,
        counts_chars="""
            pub fn truncate(s: &str, max_bytes: usize) -> String {
                if s.chars().count() <= max_bytes {
                    return s.to_string();
                }
                if max_bytes == 0 {
                    return String::new();
                }
                let kept: String = s.chars().take(max_bytes - 1).collect();
                format!("{kept}…")
            }
        """,
        rounds_up_to_boundary="""
            pub fn truncate(s: &str, max_bytes: usize) -> String {
                if s.len() <= max_bytes {
                    return s.to_string();
                }
                let Some(mut end) = max_bytes.checked_sub('…'.len_utf8()) else {
                    return String::new();
                };
                while !s.is_char_boundary(end) {
                    end += 1;
                }
                format!("{}…", &s[..end])
            }
        """,
    ),
    hints=[("approach", "Reserve 3 bytes for `…`, then walk the cut point back until it lands on a character boundary."),
           ("rust", "`s.is_char_boundary(i)` is true at 0, at `len()`, and between characters.")],
    notes=("At most three steps back, because a UTF-8 character is at most four bytes. `checked_sub` covers budgets smaller than the ellipsis.", "O(n)", "O(n)"),
    follow_up="Databases often limit by bytes and UIs by characters. How would you truncate to N characters instead?",
))

# ---------------------------------------------------------------- build (hard)

P.append(dict(
    slug="parse-url-borrowed", title="Parse a URL into borrowed parts", level="hard", stage="build",
    tags=["split_once", "lifetimes", "zero-copy"],
    teaches=["A struct of `&'a str` fields borrowing one input.", "`?` on `Option` for each required piece."],
    statement="""
        Parse `scheme://host[:port][/path][?query]` into borrowed parts.

        - `scheme` and `host` must be non-empty.
        - `port`, if present, must be a valid `u16`.
        - `path` defaults to `"/"`.
        - `query` is split on `&` into `(key, value)` pairs; empty pieces are skipped, and a piece with no `=`
          has value `""`.

        Return `None` for anything malformed.
    """,
    examples=[('"https://example.com:8080/a/b?x=1&y=2"', 'scheme "https", host "example.com", port 8080, path "/a/b", query [("x","1"), ("y","2")]')],
    starter="""
        #[derive(Debug, PartialEq)]
        pub struct Url<'a> {
            pub scheme: &'a str,
            pub host: &'a str,
            pub port: Option<u16>,
            pub path: &'a str,
            pub query: Vec<(&'a str, &'a str)>,
        }

        pub fn parse_url(s: &str) -> Option<Url<'_>> {
            todo!()
        }
    """,
    solution="""
        #[derive(Debug, PartialEq)]
        pub struct Url<'a> {
            pub scheme: &'a str,
            pub host: &'a str,
            pub port: Option<u16>,
            pub path: &'a str,
            pub query: Vec<(&'a str, &'a str)>,
        }

        pub fn parse_url(s: &str) -> Option<Url<'_>> {
            let (scheme, rest) = s.split_once("://")?;
            let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
            let (authority, path) = match rest.find('/') {
                Some(i) => rest.split_at(i),
                None => (rest, "/"),
            };
            let (host, port) = match authority.split_once(':') {
                Some((h, p)) => (h, Some(p.parse().ok()?)),
                None => (authority, None),
            };
            if scheme.is_empty() || host.is_empty() {
                return None;
            }
            let query = query
                .split('&')
                .filter(|kv| !kv.is_empty())
                .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
                .collect();
            Some(Url {
                scheme,
                host,
                port,
                path,
                query,
            })
        }
    """,
    visible=[
        T("full", '"https://example.com:8080/a/b?x=1&y=2"', 'parse_url("https://example.com:8080/a/b?x=1&y=2")',
          'Some(Url { scheme: "https", host: "example.com", port: Some(8080), path: "/a/b", query: vec![("x", "1"), ("y", "2")] })'),
        T("minimal", '"http://h"', 'parse_url("http://h")', 'Some(Url { scheme: "http", host: "h", port: None, path: "/", query: vec![] })'),
        T("port_no_path", '"http://h:80"', 'parse_url("http://h:80")', 'Some(Url { scheme: "http", host: "h", port: Some(80), path: "/", query: vec![] })'),
        T("path_no_port", '"https://ex.org/docs"', 'parse_url("https://ex.org/docs")', 'Some(Url { scheme: "https", host: "ex.org", port: None, path: "/docs", query: vec![] })'),
        T("bad_port", '"http://h:abc"', 'parse_url("http://h:abc")', "None"),
    ],
    hidden=[
        T("empty_host", '"ftp://:21/"', 'parse_url("ftp://:21/")', "None"),
        T("port_too_big", '"http://h:99999/"', 'parse_url("http://h:99999/")', "None"),
        T("no_scheme", '"example.com/x"', 'parse_url("example.com/x")', "None"),
        T("flag_query", '"http://h?flag&&a="', 'parse_url("http://h?flag&&a=").map(|u| (u.path, u.query))', 'Some(("/", vec![("flag", ""), ("a", "")]))'),
        T("borrows_input", "host points into the input", "same", "true",
          setup='let s = String::from("http://host/p");\nlet u = parse_url(&s).unwrap();\nlet same = std::ptr::eq(u.host.as_ptr(), s[7..].as_ptr());'),
        T("empty_scheme", '"://h"', 'parse_url("://h")', "None"),
        T("empty_port", '"http://h:/"', 'parse_url("http://h:/")', "None"),
        T("port_bounds", '"http://h:0", "http://h:65535", "http://h:65536"', '(parse_url("http://h:0").map(|u| u.port), parse_url("http://h:65535").map(|u| u.port), parse_url("http://h:65536"))', "(Some(Some(0)), Some(Some(65535)), None)"),
        T("query_without_path", '"http://h?a=1"', 'parse_url("http://h?a=1").map(|u| (u.path, u.query))', 'Some(("/", vec![("a", "1")]))'),
        T("value_with_equals", '"http://h/?a=b=c"', 'parse_url("http://h/?a=b=c").map(|u| u.query)', 'Some(vec![("a", "b=c")])'),
        T("empty_query", '"http://h/p?"', 'parse_url("http://h/p?").map(|u| (u.path, u.query))', 'Some(("/p", vec![]))'),
        T("question_mark_in_query", '"http://h?a=?&b"', 'parse_url("http://h?a=?&b").map(|u| u.query)', 'Some(vec![("a", "?"), ("b", "")])'),
        T("trailing_slash_path", '"http://h/a/"', 'parse_url("http://h/a/").map(|u| u.path)', 'Some("/a/")'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2216);
            let schemes = ["http", "ftp", "a"];
            let hosts = ["h", "ex.org", "1.2.3.4"];
            let paths = ["", "/", "/a", "/a/b.c"];
            let keys = ["k", "x"];
            let values = ["", "1", "v=w"];
            for _ in 0..300 {
                let (scheme, host, path) = (*rng.pick(&schemes), *rng.pick(&hosts), *rng.pick(&paths));
                let port = if rng.bool() { Some(rng.below(65_536) as u16) } else { None };
                let mut s = format!("{scheme}://{host}");
                if let Some(p) = port {
                    s += &format!(":{p}");
                }
                s += path;
                let mut query = Vec::new();
                if rng.bool() {
                    let mut parts = Vec::new();
                    for _ in 0..rng.below(4) {
                        let (k, v) = (*rng.pick(&keys), *rng.pick(&values));
                        query.push((k, v));
                        parts.push(if v.is_empty() && rng.bool() { k.to_string() } else { format!("{k}={v}") });
                    }
                    s += "?";
                    s += &parts.join("&");
                }
                let want = Url { scheme, host, port, path: if path.is_empty() { "/" } else { path }, query };
                check!(format!("s = {s:?}"), parse_url(&s), Some(want));
            }
        }
        """,
        """
            #[test]
            fn slash_inside_query() {
                check!(r#""http://h?next=/a/b""#, parse_url("http://h?next=/a/b").map(|u| (u.host, u.path, u.query)), Some(("h", "/", vec![("next", "/a/b")])));
            }
        """,
    ],
    wrong=dict(
        path_without_slash="""
            #[derive(Debug, PartialEq)]
            pub struct Url<'a> {
                pub scheme: &'a str,
                pub host: &'a str,
                pub port: Option<u16>,
                pub path: &'a str,
                pub query: Vec<(&'a str, &'a str)>,
            }

            pub fn parse_url(s: &str) -> Option<Url<'_>> {
                let (scheme, rest) = s.split_once("://")?;
                let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
                let (authority, path) = rest.split_once('/').unwrap_or((rest, "/"));
                let (host, port) = match authority.split_once(':') {
                    Some((h, p)) => (h, Some(p.parse().ok()?)),
                    None => (authority, None),
                };
                if scheme.is_empty() || host.is_empty() {
                    return None;
                }
                let query = query
                    .split('&')
                    .filter(|kv| !kv.is_empty())
                    .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
                    .collect();
                Some(Url { scheme, host, port, path, query })
            }
        """,
        port_wraps="""
            #[derive(Debug, PartialEq)]
            pub struct Url<'a> {
                pub scheme: &'a str,
                pub host: &'a str,
                pub port: Option<u16>,
                pub path: &'a str,
                pub query: Vec<(&'a str, &'a str)>,
            }

            pub fn parse_url(s: &str) -> Option<Url<'_>> {
                let (scheme, rest) = s.split_once("://")?;
                let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
                let (authority, path) = match rest.find('/') {
                    Some(i) => rest.split_at(i),
                    None => (rest, "/"),
                };
                let (host, port) = match authority.split_once(':') {
                    Some((h, p)) => (h, Some(p.parse::<u32>().ok()? as u16)),
                    None => (authority, None),
                };
                if scheme.is_empty() || host.is_empty() {
                    return None;
                }
                let query = query
                    .split('&')
                    .filter(|kv| !kv.is_empty())
                    .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
                    .collect();
                Some(Url { scheme, host, port, path, query })
            }
        """,
        keeps_empty_query_pieces="""
            #[derive(Debug, PartialEq)]
            pub struct Url<'a> {
                pub scheme: &'a str,
                pub host: &'a str,
                pub port: Option<u16>,
                pub path: &'a str,
                pub query: Vec<(&'a str, &'a str)>,
            }

            pub fn parse_url(s: &str) -> Option<Url<'_>> {
                let (scheme, rest) = s.split_once("://")?;
                let (rest, query) = match rest.split_once('?') {
                    Some((r, q)) => (r, Some(q)),
                    None => (rest, None),
                };
                let (authority, path) = match rest.find('/') {
                    Some(i) => rest.split_at(i),
                    None => (rest, "/"),
                };
                let (host, port) = match authority.split_once(':') {
                    Some((h, p)) => (h, Some(p.parse().ok()?)),
                    None => (authority, None),
                };
                if scheme.is_empty() || host.is_empty() {
                    return None;
                }
                let query = match query {
                    Some(q) => q.split('&').map(|kv| kv.split_once('=').unwrap_or((kv, ""))).collect(),
                    None => Vec::new(),
                };
                Some(Url { scheme, host, port, path, query })
            }
        """,
        slash_in_query_starts_path="""
            #[derive(Debug, PartialEq)]
            pub struct Url<'a> {
                pub scheme: &'a str,
                pub host: &'a str,
                pub port: Option<u16>,
                pub path: &'a str,
                pub query: Vec<(&'a str, &'a str)>,
            }

            pub fn parse_url(s: &str) -> Option<Url<'_>> {
                let (scheme, rest) = s.split_once("://")?;
                // Wrong: cuts the authority at the first '/', even when that '/' is inside the query.
                let (authority, tail) = match rest.find('/') {
                    Some(i) => rest.split_at(i),
                    None => (rest, ""),
                };
                let (authority, early_query) = authority.split_once('?').unwrap_or((authority, ""));
                let (path, late_query) = tail.split_once('?').unwrap_or((tail, ""));
                let query = if early_query.is_empty() { late_query } else { early_query };
                let path = if path.is_empty() { "/" } else { path };
                let (host, port) = match authority.split_once(':') {
                    Some((h, p)) => (h, Some(p.parse().ok()?)),
                    None => (authority, None),
                };
                if scheme.is_empty() || host.is_empty() {
                    return None;
                }
                let query = query
                    .split('&')
                    .filter(|kv| !kv.is_empty())
                    .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
                    .collect();
                Some(Url { scheme, host, port, path, query })
            }
        """,
    ),
    hints=[("rust", "Peel pieces off with `split_once`: `\"://\"`, then `'?'`, then the first `/`, then `':'`."),
           ("rust", "`p.parse().ok()?` inside a match arm returns `None` from the whole function on a bad port."),
           ("rust", "`split_at(i)` keeps the `/` in the path, which is what you want.")],
    notes=("Every field is a slice of the input, so parsing allocates only the query Vec. The `'_` in `Url<'_>` ties the result to `s`.", "O(n)", "O(q)"),
    follow_up="Real URLs percent-encode. Where would decoding force a `Cow<'a, str>`?",
    related=["L3"],
))

P.append(dict(
    slug="zero-copy-split-iterator", title="Zero-copy split iterator", level="hard", stage="build",
    tags=["Iterator", "DoubleEndedIterator", "lifetimes"],
    teaches=["`type Item = &'a str`: items borrow the text, not the iterator.", "Two ends sharing one remaining slice."],
    statement="""
        Implement `split_on(s, delim)` with exactly the behaviour of `s.split(delim)`, from both ends: it
        yields `&str` pieces of `s`, including empty ones. Don't call any of `str`'s split methods.
    """,
    examples=[('split_on("a,b,,c", \',\')', '["a", "b", "", "c"]'), ('split_on("", \',\')', '[""]')],
    starter="""
        pub struct SplitOn<'a> {
            /// What hasn't been yielded yet; `None` once both ends have met.
            rest: Option<&'a str>,
            delim: char,
        }

        pub fn split_on(s: &str, delim: char) -> SplitOn<'_> {
            SplitOn { rest: Some(s), delim }
        }

        impl<'a> Iterator for SplitOn<'a> {
            type Item = &'a str;

            fn next(&mut self) -> Option<&'a str> {
                todo!()
            }
        }

        impl DoubleEndedIterator for SplitOn<'_> {
            fn next_back(&mut self) -> Option<Self::Item> {
                todo!()
            }
        }
    """,
    solution="""
        pub struct SplitOn<'a> {
            /// What hasn't been yielded yet; `None` once both ends have met.
            rest: Option<&'a str>,
            delim: char,
        }

        pub fn split_on(s: &str, delim: char) -> SplitOn<'_> {
            SplitOn { rest: Some(s), delim }
        }

        impl<'a> Iterator for SplitOn<'a> {
            type Item = &'a str;

            fn next(&mut self) -> Option<&'a str> {
                let rest = self.rest?;
                match rest.find(self.delim) {
                    Some(i) => {
                        self.rest = Some(&rest[i + self.delim.len_utf8()..]);
                        Some(&rest[..i])
                    }
                    None => {
                        self.rest = None;
                        Some(rest)
                    }
                }
            }
        }

        impl DoubleEndedIterator for SplitOn<'_> {
            fn next_back(&mut self) -> Option<Self::Item> {
                let rest = self.rest?;
                match rest.rfind(self.delim) {
                    Some(i) => {
                        self.rest = Some(&rest[..i]);
                        Some(&rest[i + self.delim.len_utf8()..])
                    }
                    None => {
                        self.rest = None;
                        Some(rest)
                    }
                }
            }
        }
    """,
    visible=[
        T("pieces", "\"a,b,,c\", ','", "split_on(\"a,b,,c\", ',').collect::<Vec<_>>()", 'vec!["a", "b", "", "c"]'),
        T("empty_input", "\"\", ','", "split_on(\"\", ',').collect::<Vec<_>>()", 'vec![""]'),
        T("no_delimiter", "\"abc\", ','", "split_on(\"abc\", ',').collect::<Vec<_>>()", 'vec!["abc"]'),
        T("only_delimiter", "\",\", ','", "split_on(\",\", ',').collect::<Vec<_>>()", 'vec!["", ""]'),
        T("from_the_back", "\"a,b\", ',' reversed", "split_on(\"a,b\", ',').rev().collect::<Vec<_>>()", 'vec!["b", "a"]'),
    ],
    hidden=[
        T("trailing", "\"a,\", ','", "split_on(\"a,\", ',').collect::<Vec<_>>()", 'vec!["a", ""]'),
        T("reversed", "\"a,b,c\", ',' reversed", "split_on(\"a,b,c\", ',').rev().collect::<Vec<_>>()", 'vec!["c", "b", "a"]'),
        T("both_ends", "\"1-2-3-4\": next, next_back, next, next_back, next", "(it.next(), it.next_back(), it.next(), it.next_back(), it.next())",
          '(Some("1"), Some("4"), Some("2"), Some("3"), None)', setup="let mut it = split_on(\"1-2-3-4\", '-');"),
        T("unicode_delim", "\"x→y→\", '→'", "split_on(\"x→y→\", '→').collect::<Vec<_>>()", 'vec!["x", "y", ""]'),
        T("items_outlive_iterator", "keep the first piece after dropping the iterator", "first", 'Some("ab")',
          setup="let text = String::from(\"ab cd\");\nlet first;\n{\n    let mut it = split_on(&text, ' ');\n    first = it.next();\n}"),
        T("matches_std", "same as str::split on a mixed string", "ours == theirs", "true",
          setup="let s = \";;a;bc;;d;\";\nlet ours: Vec<&str> = split_on(s, ';').collect();\nlet theirs: Vec<&str> = s.split(';').collect();"),
        T("leading", "\",a\", ','", "split_on(\",a\", ',').collect::<Vec<_>>()", 'vec!["", "a"]'),
        T("two_delimiters", "\",,\", ','", "split_on(\",,\", ',').collect::<Vec<_>>()", 'vec!["", "", ""]'),
        T("ends_meet_on_empty", "\",\": next, next_back, next", "(it.next(), it.next_back(), it.next())", '(Some(""), Some(""), None)', setup="let mut it = split_on(\",\", ',');"),
        T("stays_done", "\"a\": next, next, next_back", "(it.next(), it.next(), it.next_back())", '(Some("a"), None, None)', setup="let mut it = split_on(\"a\", ',');"),
        T("unicode_delim_at_ends", "\"→a→\", '→'", "split_on(\"→a→\", '→').rev().collect::<Vec<_>>()", 'vec!["", "a", ""]'),
        T("unicode_pieces", "\"é,日本,🦀\", ','", "split_on(\"é,日本,🦀\", ',').collect::<Vec<_>>()", 'vec!["é", "日本", "🦀"]'),
        """
        #[test]
        fn random_vs_std() {
            let mut rng = anneal_prelude::Rng::new(2217);
            for _ in 0..400 {
                let len = rng.below(10);
                let s = rng.string(len, "a,é→");
                let delim = *rng.pick(&[',', '→']);
                let mut ours = split_on(&s, delim);
                let mut theirs = s.split(delim);
                let mut steps = Vec::new();
                for _ in 0..8 {
                    let back = rng.bool();
                    steps.push(if back { "next_back" } else { "next" });
                    let (a, b) = if back { (ours.next_back(), theirs.next_back()) } else { (ours.next(), theirs.next()) };
                    check!(format!("s = {s:?}, delim = {delim:?}, calls = {steps:?}"), a, b);
                }
            }
        }

        #[test]
        fn scale_200k_pieces() {
            let s = "ab,".repeat(200_000);
            let pieces: Vec<&str> = split_on(&s, ',').collect();
            check!("s = \\"ab,ab,…\\" (200001 pieces)", (pieces.len(), pieces[199_999], pieces[200_000]), (200_001, "ab", ""));
        }
        """,
    ],
    wrong=dict(
        steps_one_byte="""
            pub struct SplitOn<'a> {
                /// What hasn't been yielded yet; `None` once both ends have met.
                rest: Option<&'a str>,
                delim: char,
            }

            pub fn split_on(s: &str, delim: char) -> SplitOn<'_> {
                SplitOn { rest: Some(s), delim }
            }

            impl<'a> Iterator for SplitOn<'a> {
                type Item = &'a str;

                fn next(&mut self) -> Option<&'a str> {
                    let rest = self.rest?;
                    match rest.find(self.delim) {
                        Some(i) => {
                            self.rest = Some(&rest[i + 1..]);
                            Some(&rest[..i])
                        }
                        None => {
                            self.rest = None;
                            Some(rest)
                        }
                    }
                }
            }

            impl DoubleEndedIterator for SplitOn<'_> {
                fn next_back(&mut self) -> Option<Self::Item> {
                    let rest = self.rest?;
                    match rest.rfind(self.delim) {
                        Some(i) => {
                            self.rest = Some(&rest[..i]);
                            Some(&rest[i + 1..])
                        }
                        None => {
                            self.rest = None;
                            Some(rest)
                        }
                    }
                }
            }
        """,
        empty_means_done="""
            pub struct SplitOn<'a> {
                /// What hasn't been yielded yet; `None` once both ends have met.
                rest: Option<&'a str>,
                delim: char,
            }

            pub fn split_on(s: &str, delim: char) -> SplitOn<'_> {
                SplitOn { rest: Some(s), delim }
            }

            impl<'a> Iterator for SplitOn<'a> {
                type Item = &'a str;

                fn next(&mut self) -> Option<&'a str> {
                    let rest = self.rest.filter(|r| !r.is_empty())?;
                    match rest.find(self.delim) {
                        Some(i) => {
                            self.rest = Some(&rest[i + self.delim.len_utf8()..]);
                            Some(&rest[..i])
                        }
                        None => {
                            self.rest = None;
                            Some(rest)
                        }
                    }
                }
            }

            impl DoubleEndedIterator for SplitOn<'_> {
                fn next_back(&mut self) -> Option<Self::Item> {
                    let rest = self.rest.filter(|r| !r.is_empty())?;
                    match rest.rfind(self.delim) {
                        Some(i) => {
                            self.rest = Some(&rest[..i]);
                            Some(&rest[i + self.delim.len_utf8()..])
                        }
                        None => {
                            self.rest = None;
                            Some(rest)
                        }
                    }
                }
            }
        """,
    ),
    hints=[("rust", "`next` returns `Option<&'a str>`, not a borrow of `self`: slice `rest` (a `&'a str` you copied out), never `self`."),
           ("approach", "`find` for the front, `rfind` for the back. When no delimiter is left, yield all of `rest` and set it to `None`."),
           ("edge case", "Step past the delimiter by `delim.len_utf8()`, not 1.")],
    notes=("Copying `self.rest` out (it's a `&'a str`, which is `Copy`) is what lets the items outlive the `&mut self` borrow. Using `Option` for 'finished' distinguishes an empty last piece from no piece.", "O(n) total", "O(1)"),
    follow_up="Generalise the delimiter to any `Pattern`-like trait: what would its methods be?",
    rules=dict(methods=["split", "rsplit", "splitn", "rsplitn", "split_terminator", "rsplit_terminator", "split_once", "rsplit_once", "split_inclusive", "split_whitespace", "split_ascii_whitespace", "lines"]),
    related=["L3", "L5"],
))

P.append(dict(
    slug="template-formatter", title="A tiny {name} template formatter", level="hard", stage="build",
    tags=["char_indices", "Peekable", "error positions"],
    teaches=["A hand-written scanner over `char_indices().peekable()`.", "`next_if` to consume `{{` and `}}` escapes."],
    statement="""
        Render `template`, replacing `{name}` with `vars[name]`. `{{` and `}}` are a literal `{` and `}`.

        Errors carry the byte offset where they start:

        - `Unknown(name)` for a name not in `vars`.
        - `Unclosed(pos)` for a `{` with no closing `}`.
        - `StrayBrace(pos)` for a lone `}`.
    """,
    examples=[('"Hi {name}!", {name: "Ada"}', 'Ok("Hi Ada!")'), ('"{{x}}"', 'Ok("{x}")')],
    starter="""
        use std::collections::HashMap;

        #[derive(Debug, PartialEq)]
        pub enum TemplateError {
            Unknown(String),
            Unclosed(usize),
            StrayBrace(usize),
        }

        pub fn render(template: &str, vars: &HashMap<&str, &str>) -> Result<String, TemplateError> {
            todo!()
        }
    """,
    solution="""
        use std::collections::HashMap;

        #[derive(Debug, PartialEq)]
        pub enum TemplateError {
            Unknown(String),
            Unclosed(usize),
            StrayBrace(usize),
        }

        pub fn render(template: &str, vars: &HashMap<&str, &str>) -> Result<String, TemplateError> {
            let mut out = String::with_capacity(template.len());
            let mut chars = template.char_indices().peekable();
            while let Some((i, c)) = chars.next() {
                match c {
                    '{' => {
                        if chars.next_if(|&(_, n)| n == '{').is_some() {
                            out.push('{');
                            continue;
                        }
                        let close = i + template[i..].find('}').ok_or(TemplateError::Unclosed(i))?;
                        let name = &template[i + 1..close];
                        let value = vars.get(name).ok_or_else(|| TemplateError::Unknown(name.to_string()))?;
                        out.push_str(value);
                        while chars.next_if(|&(j, _)| j <= close).is_some() {}
                    }
                    '}' => {
                        if chars.next_if(|&(_, n)| n == '}').is_none() {
                            return Err(TemplateError::StrayBrace(i));
                        }
                        out.push('}');
                    }
                    _ => out.push(c),
                }
            }
            Ok(out)
        }
    """,
    visible=[
        T("fills", '"Hi {name}, welcome to {lang}!"', 'render("Hi {name}, welcome to {lang}!", &vars)', 'Ok("Hi Ada, welcome to Rust!".to_string())',
          setup='let vars = std::collections::HashMap::from([("name", "Ada"), ("lang", "Rust")]);'),
        T("escapes", '"{{literal}}"', 'render("{{literal}}", &vars)', 'Ok("{literal}".to_string())',
          setup="let vars = std::collections::HashMap::new();"),
        T("no_placeholders", '"plain text"', 'render("plain text", &vars)', 'Ok("plain text".to_string())', setup="let vars = std::collections::HashMap::new();"),
        T("closing_escape", '"}} and {{"', 'render("}} and {{", &vars)', 'Ok("} and {".to_string())', setup="let vars = std::collections::HashMap::new();"),
        T("same_name_twice", '"{a}-{a}", a = "1"', 'render("{a}-{a}", &vars)', 'Ok("1-1".to_string())', setup='let vars = std::collections::HashMap::from([("a", "1")]);'),
    ],
    hidden=[
        T("unknown", '"{missing}"', 'render("{missing}", &vars)', 'Err(TemplateError::Unknown("missing".to_string()))', setup='let vars = std::collections::HashMap::from([("name", "Ada")]);'),
        T("unclosed", '"oops {name"', 'render("oops {name", &vars)', "Err(TemplateError::Unclosed(5))", setup='let vars = std::collections::HashMap::from([("name", "Ada")]);'),
        T("stray", '"a } b"', 'render("a } b", &vars)', "Err(TemplateError::StrayBrace(2))", setup="let vars = std::collections::HashMap::new();"),
        T("unicode_offsets", '"é}"', 'render("é}", &vars)', "Err(TemplateError::StrayBrace(2))", setup="let vars = std::collections::HashMap::new();"),
        T("adjacent", '"{a}{b}{{"', 'render("{a}{b}{{", &vars)', 'Ok("12{".to_string())', setup='let vars = std::collections::HashMap::from([("a", "1"), ("b", "2")]);'),
        T("empty_name", '"{}"', 'render("{}", &vars)', 'Err(TemplateError::Unknown(String::new()))', setup="let vars = std::collections::HashMap::new();"),
        T("value_not_rerendered", '"{a}", a = "{b}"', 'render("{a}", &vars)', 'Ok("{b}".to_string())', setup='let vars = std::collections::HashMap::from([("a", "{b}"), ("b", "no")]);'),
        T("escaped_name", '"{{name}}"', 'render("{{name}}", &vars)', 'Ok("{name}".to_string())', setup='let vars = std::collections::HashMap::from([("name", "Ada")]);'),
        T("unclosed_after_unicode", '"é{x"', 'render("é{x", &vars)', "Err(TemplateError::Unclosed(2))", setup="let vars = std::collections::HashMap::new();"),
        T("stray_at_end", '"abc}"', 'render("abc}", &vars)', "Err(TemplateError::StrayBrace(3))", setup="let vars = std::collections::HashMap::new();"),
        T("three_closing", '"}}}"', 'render("}}}", &vars)', "Err(TemplateError::StrayBrace(2))", setup="let vars = std::collections::HashMap::new();"),
        T("braces_around_placeholder", '"{{{a}}}", a = "1"', 'render("{{{a}}}", &vars)', 'Ok("{1}".to_string())', setup='let vars = std::collections::HashMap::from([("a", "1")]);'),
        T("empty_template", '""', 'render("", &vars)', "Ok(String::new())", setup="let vars = std::collections::HashMap::new();"),
        T("spaces_in_name", '"{ a }"', 'render("{ a }", &vars)', 'Err(TemplateError::Unknown(" a ".to_string()))', setup='let vars = std::collections::HashMap::from([("a", "1")]);'),
        T("unicode_name_and_value", '"{名前}さん", 名前 = "太郎"', 'render("{名前}さん", &vars)', 'Ok("太郎さん".to_string())', setup='let vars = std::collections::HashMap::from([("名前", "太郎")]);'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2218);
            let vars = std::collections::HashMap::from([("a", "1"), ("bb", "22")]);
            let pieces = [("{a}", "1"), ("{bb}", "22"), ("{{", "{"), ("}}", "}"), ("x", "x"), ("é", "é")];
            for _ in 0..400 {
                let mut template = String::new();
                let mut want = String::new();
                for _ in 0..rng.below(8) {
                    let (t, w) = *rng.pick(&pieces);
                    template += t;
                    want += w;
                }
                let want = if rng.below(4) == 0 {
                    let at = template.len();
                    template.push('}');
                    Err(TemplateError::StrayBrace(at))
                } else {
                    Ok(want)
                };
                check!(format!("template = {template:?}, vars = {{a: 1, bb: 22}}"), render(&template, &vars), want);
            }
        }

        #[test]
        fn scale_200k_placeholders() {
            let vars = std::collections::HashMap::from([("a", "x")]);
            let template = "{a}".repeat(200_000);
            check!("template = \\"{a}{a}…\\" (200000 placeholders)", render(&template, &vars).map(|s| s.len()), Ok(200_000));
        }
        """,
    ],
    wrong=dict(
        lone_close_is_literal="""
            use std::collections::HashMap;

            #[derive(Debug, PartialEq)]
            pub enum TemplateError {
                Unknown(String),
                Unclosed(usize),
                StrayBrace(usize),
            }

            pub fn render(template: &str, vars: &HashMap<&str, &str>) -> Result<String, TemplateError> {
                let mut out = String::with_capacity(template.len());
                let mut chars = template.char_indices().peekable();
                while let Some((i, c)) = chars.next() {
                    match c {
                        '{' => {
                            if chars.next_if(|&(_, n)| n == '{').is_some() {
                                out.push('{');
                                continue;
                            }
                            let close = i + template[i..].find('}').ok_or(TemplateError::Unclosed(i))?;
                            let name = &template[i + 1..close];
                            let value = vars.get(name).ok_or_else(|| TemplateError::Unknown(name.to_string()))?;
                            out.push_str(value);
                            while chars.next_if(|&(j, _)| j <= close).is_some() {}
                        }
                        '}' => {
                            chars.next_if(|&(_, n)| n == '}');
                            out.push('}');
                        }
                        _ => out.push(c),
                    }
                }
                Ok(out)
            }
        """,
        char_positions="""
            use std::collections::HashMap;

            #[derive(Debug, PartialEq)]
            pub enum TemplateError {
                Unknown(String),
                Unclosed(usize),
                StrayBrace(usize),
            }

            pub fn render(template: &str, vars: &HashMap<&str, &str>) -> Result<String, TemplateError> {
                let chars: Vec<char> = template.chars().collect();
                let mut out = String::with_capacity(template.len());
                let mut i = 0;
                while i < chars.len() {
                    match chars[i] {
                        '{' if chars.get(i + 1) == Some(&'{') => {
                            out.push('{');
                            i += 2;
                        }
                        '{' => {
                            let close = (i..chars.len()).find(|&j| chars[j] == '}').ok_or(TemplateError::Unclosed(i))?;
                            let name: String = chars[i + 1..close].iter().collect();
                            let value = vars.get(name.as_str()).ok_or(TemplateError::Unknown(name.clone()))?;
                            out.push_str(value);
                            i = close + 1;
                        }
                        '}' if chars.get(i + 1) == Some(&'}') => {
                            out.push('}');
                            i += 2;
                        }
                        '}' => return Err(TemplateError::StrayBrace(i)),
                        c => {
                            out.push(c);
                            i += 1;
                        }
                    }
                }
                Ok(out)
            }
        """,
    ),
    hints=[("rust", "`char_indices().peekable()` gives byte offsets for errors, and `next_if(|&(_, c)| c == '{')` consumes an escape only if it's there."),
           ("approach", "At a `{`, find the matching `}` with `template[i..].find('}')`, look the name up, then skip the iterator past the `}`."),
           ("edge case", "Report byte offsets, not character counts: `\"é}\"` has its `}` at offset 2.")],
    notes=("A small scanner with one character of lookahead handles escapes, placeholders and errors in a single pass. `HashMap<&str, &str>::get` takes a `&str` directly.", "O(n)", "O(n)"),
    follow_up="Add `{name:>8}` style alignment. Where would you parse the spec, and how would you apply it?",
    related=["S1", "S4"],
))

# The validator only accepts rules on fix problems; write problems state their limits in prose.
for p in P:
    if p.get("mode", "write") == "write":
        p.pop("rules", None)

STAGES = [
    ("use", "Use", "easy"),
    ("understand", "Understand", "medium"),
    ("build", "Build", "hard"),
]

if __name__ == "__main__":
    n = write_track("s2-strings-text", "S2", "Strings & text", "S", "core", 2,
                    "Text is UTF-8 bytes: borrow `&str`, own `String`, and never index by position.",
                    STAGES, P)
    print("S2", n)
