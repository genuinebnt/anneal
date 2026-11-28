use solution::*;

#[test]
fn empty_item_is_bad() {
    check!(r#"["1", ""]"#, parse_all(&["1", ""]), Err("bad number: ".to_string()));
}

#[test]
fn i32_bounds() {
    check!(r#"["-2147483648", "2147483647"], and "2147483648""#, (parse_all(&["-2147483648", "2147483647"]), parse_all(&["2147483648"])), (Ok(vec![i32::MIN, i32::MAX]), Err("bad number: 2147483648".to_string())));
}

#[test]
fn sum_negative_bounds() {
    check!(r#"["-2147483648", "-2147483648", "5"]"#, sum_all(&["-2147483648", "-2147483648", "5"]), Ok(-4_294_967_291));
}

#[test]
fn sum_first_error() {
    check!(r#"["1", "x", "y"]"#, sum_all(&["1", "x", "y"]), Err("bad number: x".to_string()));
}

#[test]
fn every_error_all_good() {
    check!(r#"["3", "1", "3"]"#, parse_every_error(&["3", "1", "3"]), Ok(vec![3, 1, 3]));
}

#[test]
fn every_error_keeps_duplicates() {
    check!(r#"["x", "x", " 1"]"#, parse_every_error(&["x", "x", " 1"]), Err(vec!["bad number: x".to_string(), "bad number: x".to_string(), "bad number:  1".to_string()]));
}

#[test]
fn validate_all_good_checks_everything() {
    let mut calls = 0;
    let r = validate(&["a", "b", "c"], |_| {
        calls += 1;
        Ok(())
    });
    check!(r#"["a", "b", "c"]"#, (r, calls), (Ok(()), 3));
}

#[test]
fn validate_first_item_fails() {
    let mut calls = 0;
    let r = validate(&["x", "y"], |s| {
        calls += 1;
        Err(format!("{s}!"))
    });
    check!(r#"["x", "y"], check always fails"#, (r, calls), (Err("x!".to_string()), 1));
}

#[test]
fn order_kept() {
    check!(r#"["3", "-1", "+2"]"#, parse_all(&["3", "-1", "+2"]), Ok(vec![3, -1, 2]));
}

#[test]
fn unicode_item() {
    check!(r#"["7", "٣"]"#, parse_every_error(&["7", "٣"]), Err(vec!["bad number: ٣".to_string()]));
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(7107);
    for _ in 0..400 {
        let n = rng.below(6);
        let mut items: Vec<String> = Vec::new();
        for _ in 0..n {
            if rng.below(4) == 0 {
                let len = rng.below(3);
                items.push(rng.string(len, "x1-"));
            } else if rng.bool() {
                items.push(rng.int(-99, 99).to_string());
            } else {
                items.push(rng.int(i64::from(i32::MAX) - 2, i64::from(i32::MAX) + 1).to_string());
            }
        }
        let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
        let parsed: Vec<Result<i32, String>> = refs.iter().map(|s| s.parse::<i32>().map_err(|_| format!("bad number: {s}"))).collect();
        let first_err = parsed.iter().find_map(|r| r.clone().err());
        let errs: Vec<String> = parsed.iter().filter_map(|r| r.clone().err()).collect();
        let vals: Vec<i32> = parsed.iter().filter_map(|r| r.clone().ok()).collect();
        let desc = format!("items = {refs:?}");
        check!(format!("parse_all, {desc}"), parse_all(&refs), match &first_err { Some(e) => Err(e.clone()), None => Ok(vals.clone()) });
        check!(format!("sum_all, {desc}"), sum_all(&refs), match &first_err { Some(e) => Err(e.clone()), None => Ok(vals.iter().map(|&v| i64::from(v)).sum()) });
        check!(format!("parse_every_error, {desc}"), parse_every_error(&refs), if errs.is_empty() { Ok(vals.clone()) } else { Err(errs.clone()) });
        let mut calls = 0;
        let got = validate(&refs, |s| {
            calls += 1;
            s.parse::<i32>().map(|_| ()).map_err(|_| s.to_string())
        });
        let stop = parsed.iter().position(|r| r.is_err());
        check!(format!("validate, {desc}"), (got, calls), (stop.map_or(Ok(()), |i| Err(refs[i].to_string())), stop.map_or(refs.len(), |i| i + 1)));
    }
}

#[test]
fn scale_bad_item_at_the_end() {
    let mut items: Vec<String> = (0..200_000).map(|i: i32| (i - 100_000).to_string()).collect();
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let want: Vec<i32> = (0..200_000).map(|i| i - 100_000).collect();
    check!("200000 numbers from -100000 up", (parse_all(&refs), sum_all(&refs), parse_every_error(&refs)), (Ok(want.clone()), Ok(-100_000), Ok(want)));
    items.push("oops".to_string());
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    check!("the same 200000 numbers, then \"oops\"", (parse_all(&refs), parse_every_error(&refs)), (Err("bad number: oops".to_string()), Err(vec!["bad number: oops".to_string()])));
}

#[test]
fn sum_all_builds_no_vec() {
    let items: Vec<String> = (0..10_000).map(|i: i32| i.to_string()).collect();
    let refs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    let (got, used) = anneal_prelude::allocs(|| sum_all(&refs));
    check!("10000 good items: the sum", got, Ok(49_995_000));
    check!(format!("sum_all allocated {} bytes in {} allocations, at most 1024", used.bytes, used.count), used.bytes <= 1024, true);
}
