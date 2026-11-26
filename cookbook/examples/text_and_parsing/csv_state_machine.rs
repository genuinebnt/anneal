//! RECIPE 93 (parsing): a CSV parser as an explicit character state machine, plus a round-trip property test.
//! LEVEL: intermediate
//! Use in: any format where context changes the meaning of a character (quotes, escapes, comments, heredocs).   Run: cargo run --release --example csv_state_machine
//! COMMON USES: CSV/TSV ingestion, any format where quoting/escaping changes the meaning of characters.
#![allow(dead_code)]
//!
//! When "what does this character mean?" depends on what came before, model the parser as states + transitions (state_machine, generic_state_machine).
//! Here: inside/outside quotes changes what a comma, a newline, and a quote mean. One pass, O(n), no backtracking, and every
//! (state, char) pair is handled in ONE exhaustive `match`, so no case is forgotten. The streaming-friendly structure also lets
//! you feed chunks of input. Production-grade: https://github.com/BurntSushi/rust-csv (read how it uses a DFA for speed).
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    FieldStart,    // at the beginning of a field
    Unquoted,      // inside a plain field
    Quoted,        // inside "..."
    QuoteInQuoted, // saw a `"` while quoted: either an escaped quote ("") or the closing quote
}

#[derive(Debug, PartialEq)]
enum CsvError {
    UnterminatedQuote,
    QuoteInUnquotedField { line: usize },
    GarbageAfterQuote { line: usize },
}

fn parse(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let (mut rows, mut row, mut field) = (vec![], vec![], String::new());
    let mut state = State::FieldStart;
    let mut line = 1;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            continue; // normalise CRLF: the \n will end the record
        }
        state = match (state, c) {
            (State::FieldStart, '"') => State::Quoted,
            (State::FieldStart | State::Unquoted, ',') => {
                row.push(std::mem::take(&mut field));
                State::FieldStart
            }
            (State::FieldStart | State::Unquoted, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                line += 1;
                State::FieldStart
            }
            (State::Unquoted, '"') => return Err(CsvError::QuoteInUnquotedField { line }), // a quote may only START a field
            (State::FieldStart | State::Unquoted, c) => {
                field.push(c);
                State::Unquoted
            }
            (State::Quoted, '"') => State::QuoteInQuoted,
            (State::Quoted, c) => {
                if c == '\n' {
                    line += 1;
                }
                field.push(c); // commas and newlines are DATA inside quotes
                State::Quoted
            }
            (State::QuoteInQuoted, '"') => {
                field.push('"'); // "" is an escaped quote
                State::Quoted
            }
            (State::QuoteInQuoted, ',') => {
                row.push(std::mem::take(&mut field));
                State::FieldStart
            }
            (State::QuoteInQuoted, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                line += 1;
                State::FieldStart
            }
            (State::QuoteInQuoted, _) => return Err(CsvError::GarbageAfterQuote { line }),
        };
    }
    match state {
        State::Quoted => return Err(CsvError::UnterminatedQuote),
        State::FieldStart if row.is_empty() && field.is_empty() => {} // input ended right after a newline (or was empty)
        _ => {
            row.push(field);
            rows.push(row);
        }
    }
    Ok(rows)
}

fn write_field(f: &str, out: &mut String) {
    if f.contains([',', '"', '\n', '\r']) || f.starts_with(' ') {
        out.push('"');
        out.push_str(&f.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(f);
    }
}
fn write(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for r in rows {
        for (i, f) in r.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            write_field(f, &mut out);
        }
        out.push('\n');
    }
    out
}

fn main() {
    let cases = [
        ("a,b,c\n1,2,3\n", 2),
        ("name,quote\nana,\"say \"\"hi\"\", ok\"\n", 2),
        ("multi,\"line\nfield\"\nx,y", 2),
        ("a,,c\n", 1),
        ("crlf,row\r\nsecond,row\r\n", 2),
        ("", 0),
    ];
    for (input, want_rows) in cases {
        let rows = parse(input).unwrap();
        println!("{input:?} -> {rows:?}");
        assert_eq!(rows.len(), want_rows);
    }
    for bad in ["a,\"unterminated", "ab\"c,d", "\"x\"y,z"] {
        println!("{bad:?} -> {:?}", parse(bad));
    }

    // PROPERTY: write then parse gives back the original, for nasty random fields
    let alphabet: Vec<char> = ['a', 'b', ' ', ',', '"', '\n', 'é'].to_vec();
    let mut x = 0xDEAD_BEEF_CAFE_F00Du64;
    let mut rnd = |n: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % n as u64) as usize
    };
    for _ in 0..500 {
        let rows: Vec<Vec<String>> = (0..1 + rnd(4))
            .map(|_| {
                (0..1 + rnd(4))
                    .map(|_| {
                        (0..rnd(6))
                            .map(|_| alphabet[rnd(alphabet.len())])
                            .collect::<String>()
                    })
                    .collect()
            })
            .collect();
        // an empty single field row is written as an empty line, which is ambiguous in CSV: skip that known edge case
        if rows.iter().any(|r| r.len() == 1 && r[0].is_empty()) {
            continue;
        }
        let text = write(&rows);
        assert_eq!(
            parse(&text).as_ref(),
            Ok(&rows),
            "round trip failed for {rows:?} via {text:?}"
        );
    }
    println!("500 random round trips OK");
}
