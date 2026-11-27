use anneal_prelude::Rng;
use solution::*;
use std::collections::HashMap;

/// A service whose answer to each question is fixed in advance. It writes down every call.
#[derive(Default)]
struct Script {
    yes: HashMap<u32, bool>,
    scores: HashMap<u32, u64>,
    refuses: Option<(char, u32)>,
    calls: Vec<(char, u32)>,
}

impl Script {
    fn call(&mut self, what: char, entry: u32) -> Result<(), Refusal> {
        assert!(!self.calls.iter().rev().take(1).any(|&(w, e)| Some((w, e)) == self.refuses), "called again after a refusal");
        self.calls.push((what, entry));
        if self.refuses == Some((what, entry)) {
            return Err(Refusal(format!("{what} {entry}")));
        }
        Ok(())
    }
}

impl Judge for Script {
    fn qualifies(&mut self, entry: u32) -> Result<bool, Refusal> {
        self.call('q', entry)?;
        Ok(*self.yes.get(&entry).unwrap_or(&true))
    }
    fn score(&mut self, entry: u32) -> Result<u64, Refusal> {
        self.call('s', entry)?;
        Ok(*self.scores.get(&entry).unwrap_or(&0))
    }
    fn caption(&mut self, entry: u32) -> Result<String, Refusal> {
        self.call('c', entry)?;
        Ok(entry.to_string())
    }
}

#[test]
fn a_refusal_that_is_needed_comes_back() {
    for what in ['q', 'c'] {
        let mut s = Script { refuses: Some((what, 2)), ..Script::default() };
        check!(format!("refused {what} 2"), shortlist(&[1, 2, 3], 3, false, &mut s), Err(Refusal(format!("{what} 2"))));
        check!("no call after it", s.calls.last().copied(), Some((what, 2)));
    }
    for what in ['q', 's', 'c'] {
        let mut s = Script { refuses: Some((what, 2)), ..Script::default() };
        check!(format!("ranked, refused {what} 2"), shortlist(&[1, 2, 3], 3, true, &mut s), Err(Refusal(format!("{what} 2"))));
        check!("no call after it", s.calls.last().copied(), Some((what, 2)));
    }
}

#[test]
fn a_refusal_that_is_not_needed_never_happens() {
    let mut s = Script { refuses: Some(('q', 3)), ..Script::default() };
    check!("two are enough", shortlist(&[1, 2, 3], 2, false, &mut s), Ok(vec!["1".to_string(), "2".to_string()]));
    let mut s = Script { refuses: Some(('c', 1)), scores: HashMap::from([(1, 9), (2, 1)]), ..Script::default() };
    check!("1 is not on the ranked list", shortlist(&[1, 2], 1, true, &mut s), Ok(vec!["2".to_string()]));
    // ranked: every entry is needed, even one that cannot win
    let mut s = Script { refuses: Some(('s', 3)), ..Script::default() };
    check!("the last one could have been the best", shortlist(&[1, 2, 3], 1, true, &mut s), Err(Refusal("s 3".into())));
}

#[test]
fn equal_scores_keep_their_order_in_a_long_list() {
    let entries: Vec<u32> = (0..300).rev().collect();
    let mut s = Script { scores: entries.iter().map(|&e| (e, u64::from(e % 3))).collect(), ..Script::default() };
    let want: Vec<String> = entries.iter().filter(|&&e| e % 3 == 0).take(50).map(|e| e.to_string()).collect();
    check!("score 0 first, in the order given", shortlist(&entries, 50, true, &mut s), Ok(want));
}

#[test]
fn fewer_than_asked_for() {
    for ranked in [false, true] {
        let mut s = Script { yes: HashMap::from([(1, false)]), ..Script::default() };
        check!("n is huge", shortlist(&[1, 2, 1], usize::MAX, ranked, &mut s), Ok(vec!["2".to_string()]));
        check!("nothing there", shortlist(&[], 5, ranked, &mut Script::default()), Ok(vec![]));
    }
}

type Expected = (Vec<(char, u32)>, Result<Vec<String>, Refusal>);

/// What the rules say, written the long way round: the calls allowed, in order, cut at the refusal.
fn expected(entries: &[u32], n: usize, ranked: bool, s: &Script) -> Expected {
    let mut plan = Vec::new();
    let mut shown = Vec::new();
    let yes = |e: &u32| *s.yes.get(e).unwrap_or(&true);
    if n > 0 && ranked {
        for e in entries {
            plan.push(('q', *e));
            if yes(e) {
                plan.push(('s', *e));
            }
        }
        let mut order: Vec<(u64, usize)> = entries.iter().enumerate().filter(|(_, e)| yes(e)).map(|(i, e)| (*s.scores.get(e).unwrap_or(&0), i)).collect();
        order.sort_unstable();
        for (_, i) in order.into_iter().take(n) {
            plan.push(('c', entries[i]));
            shown.push(entries[i].to_string());
        }
    } else if n > 0 {
        for e in entries {
            if shown.len() < n {
                plan.push(('q', *e));
                if yes(e) {
                    plan.push(('c', *e));
                    shown.push(e.to_string());
                }
            }
        }
    }
    match plan.iter().position(|&call| Some(call) == s.refuses) {
        Some(at) => {
            let (what, e) = plan[at];
            plan.truncate(at + 1);
            (plan, Err(Refusal(format!("{what} {e}"))))
        }
        None => (plan, Ok(shown)),
    }
}

#[test]
fn random_contests() {
    let mut rng = Rng::new(18121);
    for _ in 0..1500 {
        let len = rng.below(9);
        let entries: Vec<u32> = (0..len).map(|_| rng.below(6) as u32).collect();
        let n = rng.below(5);
        let ranked = rng.bool();
        let mut s = Script::default();
        for e in 0..6 {
            s.yes.insert(e, rng.below(3) != 0);
            s.scores.insert(e, rng.below(3) as u64);
        }
        if rng.bool() {
            s.refuses = Some((*rng.pick(&['q', 's', 'c']), rng.below(6) as u32));
        }
        let (calls, result) = expected(&entries, n, ranked, &s);
        let got = shortlist(&entries, n, ranked, &mut s);
        let label = format!("{entries:?}, n = {n}, ranked = {ranked}, refuses {:?}", s.refuses);
        check!(format!("{label}: result"), got, result);
        check!(format!("{label}: calls"), s.calls, calls);
    }
}

#[test]
fn a_large_contest() {
    let entries: Vec<u32> = (0..200_000).collect();
    let mut s = Script::default();
    check!("one caption needs two calls", shortlist(&entries, 1, false, &mut s).map(|v| v.len()), Ok(1));
    check!("calls", s.calls.len(), 2);
}

#[test]
fn without_a_ranking_there_are_no_scores() {
    let mut s = Script { refuses: Some(('s', 1)), ..Script::default() };
    check!("`score` is never called", shortlist(&[1, 2], 9, false, &mut s), Ok(vec!["1".to_string(), "2".to_string()]));
}

#[test]
fn an_entry_that_does_not_qualify_is_left_alone() {
    let mut s = Script { yes: HashMap::from([(1, false), (2, false)]), ..Script::default() };
    check!("nobody qualifies", shortlist(&[1, 2], 1, true, &mut s), Ok(vec![]));
    check!("no score and no caption", s.calls, vec![('q', 1), ('q', 2)]);
}
