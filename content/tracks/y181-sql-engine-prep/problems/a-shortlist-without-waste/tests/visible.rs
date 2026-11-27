use solution::*;

/// Even entries qualify, the score is the entry's last digit, and everything is written down.
#[allow(dead_code)]
#[derive(Default)]
struct Desk {
    calls: Vec<String>,
}

impl Judge for Desk {
    fn qualifies(&mut self, entry: u32) -> Result<bool, Refusal> {
        self.calls.push(format!("qualifies {entry}"));
        Ok(entry.is_multiple_of(2))
    }
    fn score(&mut self, entry: u32) -> Result<u64, Refusal> {
        self.calls.push(format!("score {entry}"));
        Ok(u64::from(entry % 10))
    }
    fn caption(&mut self, entry: u32) -> Result<String, Refusal> {
        self.calls.push(format!("caption {entry}"));
        Ok(format!("no. {entry}"))
    }
}

#[test]
fn in_the_order_given() {
    let mut desk = Desk::default();
    let got = shortlist(&[3, 8, 5, 4, 6, 2], 2, false, &mut desk);
    check!("the shortlist", got, Ok(vec!["no. 8".to_string(), "no. 4".to_string()]));
    let want = ["qualifies 3", "qualifies 8", "caption 8", "qualifies 5", "qualifies 4", "caption 4"].map(String::from);
    check!("nothing after the second caption", desk.calls, want.to_vec());
}

#[test]
fn best_first() {
    let mut desk = Desk::default();
    let got = shortlist(&[18, 3, 12, 22], 2, true, &mut desk);
    check!("lowest scores, ties in the order given", got, Ok(vec!["no. 12".to_string(), "no. 22".to_string()]));
    let want = ["qualifies 18", "score 18", "qualifies 3", "qualifies 12", "score 12", "qualifies 22", "score 22", "caption 12", "caption 22"].map(String::from);
    check!("the calls", desk.calls, want.to_vec());
}

#[test]
fn an_empty_shortlist_asks_nothing() {
    for ranked in [false, true] {
        let mut desk = Desk::default();
        check!("n = 0", shortlist(&[2, 4], 0, ranked, &mut desk), Ok(vec![]));
        check!("no calls", desk.calls, Vec::<String>::new());
    }
}

#[test]
fn too_few_entries_qualify() {
    let mut desk = Desk::default();
    check!("one even entry", shortlist(&[1, 2, 3], 5, false, &mut desk), Ok(vec!["no. 2".to_string()]));
    check!("every entry was asked about", desk.calls.len(), 4);
}

#[test]
fn the_same_entry_twice() {
    let mut desk = Desk::default();
    check!("both are shown", shortlist(&[4, 4], 2, true, &mut desk), Ok(vec!["no. 4".to_string(); 2]));
    check!("and each is asked about", desk.calls.len(), 6);
}
