use anneal_prelude::Rng;
use solution::*;

fn mistake(line: &str) -> Option<(Problem, usize, usize)> {
    fields(line).err().map(|e| (e.problem, e.start, e.end))
}

fn field(value: Option<&str>, start: usize, end: usize) -> Field {
    Field { value: value.map(String::from), start, end }
}

#[test]
fn the_empty_line_and_bare_commas() {
    check!("empty", fields(""), Ok(vec![field(None, 0, 0)]));
    check!("one comma", fields(","), Ok(vec![field(None, 0, 0), field(None, 1, 1)]));
    check!("a trailing comma", fields("a,"), Ok(vec![field(Some("a"), 0, 1), field(None, 2, 2)]));
}

#[test]
fn quotes_by_themselves() {
    check!("an empty text", fields(r#""""#), Ok(vec![field(Some(""), 0, 2)]));
    check!("one quote", fields(r#""""""#), Ok(vec![field(Some("\""), 0, 4)]));
    check!("three never close", mistake(r#"""""#), Some((Problem::Unclosed, 0, 3)));
    check!("one never closes", mistake(r#"""#), Some((Problem::Unclosed, 0, 1)));
}

#[test]
fn what_may_follow_a_closing_quote() {
    check!("a space", mistake(r#""a" ,b"#), Some((Problem::AfterQuote, 3, 4)));
    check!("a wide character", mistake(r#""a"é"#), Some((Problem::AfterQuote, 3, 5)));
}

#[test]
fn a_space_before_a_quote_makes_it_plain() {
    check!("space then quote", mistake(r#"a, "b""#), Some((Problem::StrayQuote, 3, 4)));
}

#[test]
fn the_first_mistake_wins() {
    check!("stray before open", mistake(r#"a"b,"c"#), Some((Problem::StrayQuote, 1, 2)));
    check!("open takes the rest", mistake(r#""a,b"c"#), Some((Problem::AfterQuote, 5, 6)));
}

#[test]
fn random_lines_read_back() {
    let mut rng = Rng::new(18131);
    for _ in 0..1000 {
        let mut line = String::new();
        let mut want = Vec::new();
        for i in 0..rng.int(1, 5) {
            if i > 0 {
                line.push(',');
            }
            let len = rng.below(5);
            let text = rng.string(len, "ab ,\"é");
            let start = line.len();
            let must_quote = text.contains([',', '"']);
            let value = if rng.below(5) == 0 {
                None
            } else if must_quote || text.is_empty() || rng.bool() {
                line.push_str(&format!("\"{}\"", text.replace('"', "\"\"")));
                Some(text)
            } else {
                line.push_str(&text);
                Some(text)
            };
            want.push(Field { value, start, end: line.len() });
        }
        check!(format!("{line:?}"), fields(&line), Ok(want));
    }
}

#[test]
fn any_line_at_all() {
    let mut rng = Rng::new(18132);
    for _ in 0..4000 {
        let len = rng.below(10);
        let line = rng.string(len, "a,\"é ");
        let inside = match fields(&line) {
            Ok(all) => all.iter().all(|f| line.get(f.start..f.end).is_some()),
            Err(e) => line.get(e.start..e.end).is_some(),
        };
        check!(format!("{line:?}: every range is inside the line"), inside, true);
    }
}

#[test]
fn long_lines() {
    let line = format!("\"{}\"", "x\"\"".repeat(200_000));
    check!("200 000 doubled quotes", fields(&line).map(|f| f.len()), Ok(1));
    check!("200 001 missing fields", fields(&",".repeat(200_000)).map(|f| f.len()), Ok(200_001));
}
