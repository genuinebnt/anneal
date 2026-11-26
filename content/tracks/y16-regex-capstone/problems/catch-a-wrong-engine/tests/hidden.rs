use solution::*;

/// The ways the engines below are broken. `None` is a correct engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Bug {
    None,
    ShortestWins,
    FirstWayWins,
    NoMatchAtTheVeryEnd,
    CountsBytes,
    CaretHoldsAtEveryStart,
    DollarHoldsBeforeALastNewline,
    DotSkipsNewline,
    PlusAllowsNone,
    OptAllowsMany,
    NegatedSetSkipsNonAscii,
    RangeLosesItsTop,
    NestedRepeatRunsOnce,
    EmptyBranchIsLost,
    WordForgetsUnderscore,
    OnlyTwoBranches,
}

#[allow(dead_code)]
struct Broken<'a> {
    bug: Bug,
    text: &'a [char],
    /// Where the current attempt began.
    began: usize,
    memo: std::cell::RefCell<std::collections::HashMap<(usize, usize), Vec<usize>>>,
}

#[allow(dead_code)]
impl Broken<'_> {
    /// The ends of matches of `ast` from `at`, without repeats, the preferred way first.
    fn ends(&self, ast: &Ast, at: usize) -> Vec<usize> {
        let key = (ast as *const Ast as usize, at);
        if let Some(known) = self.memo.borrow().get(&key) {
            return known.clone();
        }
        let n = self.text.len();
        let here = self.text.get(at).copied();
        let one = |ok: bool| if ok { vec![at + 1] } else { vec![] };
        let stay = |ok: bool| if ok { vec![at] } else { vec![] };
        let out = match ast {
            Ast::Empty => vec![at],
            Ast::Class(class) => one(here.is_some_and(|c| self.in_class(class, c))),
            Ast::Any => one(here.is_some_and(|c| self.bug != Bug::DotSkipsNewline || c != '\n')),
            Ast::Start => stay(at == 0 || (self.bug == Bug::CaretHoldsAtEveryStart && at == self.began)),
            Ast::End => stay(at == n || (self.bug == Bug::DollarHoldsBeforeALastNewline && at + 1 == n && here == Some('\n'))),
            Ast::Cat(items) => items.iter().fold(vec![at], |from, item| {
                unique(from.iter().flat_map(|&p| self.ends(item, p)))
            }),
            Ast::Alt(branches) => {
                let used = branches.iter().filter(|b| self.bug != Bug::EmptyBranchIsLost || **b != Ast::Empty);
                let take = if self.bug == Bug::OnlyTwoBranches { 2 } else { usize::MAX };
                unique(used.take(take).flat_map(|b| self.ends(b, at)))
            }
            Ast::Star(inner) => {
                if self.bug == Bug::NestedRepeatRunsOnce && repeats(inner) {
                    unique(self.ends(inner, at).into_iter().chain([at]))
                } else {
                    self.rounds(inner, at)
                }
            }
            Ast::Plus(inner) => {
                if self.bug == Bug::PlusAllowsNone {
                    self.rounds(inner, at)
                } else {
                    let firsts = self.ends(inner, at);
                    unique(firsts.into_iter().flat_map(|p| self.rounds(inner, p)))
                }
            }
            Ast::Opt(inner) => {
                if self.bug == Bug::OptAllowsMany {
                    self.rounds(inner, at)
                } else {
                    unique(self.ends(inner, at).into_iter().chain([at]))
                }
            }
        };
        self.memo.borrow_mut().insert(key, out.clone());
        out
    }

    /// Any number of rounds, more rounds preferred.
    fn rounds(&self, inner: &Ast, at: usize) -> Vec<usize> {
        // A second key space: the odd address cannot belong to a node.
        let key = (inner as *const Ast as usize + 1, at);
        if let Some(known) = self.memo.borrow().get(&key) {
            return known.clone();
        }
        let mut out = Vec::new();
        for next in self.ends(inner, at) {
            if next > at {
                out.extend(self.rounds(inner, next));
            }
        }
        out.push(at);
        let out = unique(out.into_iter());
        self.memo.borrow_mut().insert(key, out.clone());
        out
    }

    fn in_class(&self, class: &Class, c: char) -> bool {
        if self.bug == Bug::NegatedSetSkipsNonAscii && class.negated && !c.is_ascii() {
            return false;
        }
        let word = class.ranges.contains(&('_', '_')) && class.ranges.contains(&('a', 'z')) && class.ranges.contains(&('0', '9'));
        let inside = class.ranges.iter().any(|&(lo, hi)| {
            if self.bug == Bug::WordForgetsUnderscore && word && lo == '_' {
                return false;
            }
            let top_lost = self.bug == Bug::RangeLosesItsTop && lo < hi && !word && c == hi;
            lo <= c && c <= hi && !top_lost
        });
        inside != class.negated
    }
}

#[allow(dead_code)]
fn unique(list: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut out = Vec::new();
    for x in list {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

#[allow(dead_code)]
fn repeats(ast: &Ast) -> bool {
    match ast {
        Ast::Star(_) | Ast::Plus(_) => true,
        Ast::Cat(v) | Ast::Alt(v) => v.iter().any(repeats),
        Ast::Opt(a) => repeats(a),
        _ => false,
    }
}

/// What the engine with this bug answers.
#[allow(dead_code)]
fn engine(bug: Bug, pattern: &str, text: &str) -> Option<(usize, usize)> {
    let tree = parse(pattern).expect("a valid pattern");
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let starts = if bug == Bug::NoMatchAtTheVeryEnd { n } else { n + 1 };
    let (start, end) = (0..starts).find_map(|start| {
        let search = Broken { bug, text: &chars, began: start, memo: Default::default() };
        let ends = search.ends(&tree, start);
        let end = match bug {
            Bug::ShortestWins => ends.iter().copied().min(),
            Bug::FirstWayWins => ends.first().copied(),
            _ => ends.iter().copied().max(),
        };
        Some((start, end?))
    })?;
    if bug == Bug::CountsBytes {
        let bytes = |k: usize| chars[..k].iter().map(|c| c.len_utf8()).sum();
        return Some((bytes(start), bytes(end)));
    }
    Some((start, end))
}

#[derive(Default)]
#[allow(dead_code)]
struct Log {
    calls: usize,
    patterns: std::collections::HashSet<String>,
    broken_rules: Vec<String>,
}

/// Runs the learner's search against one engine and checks the rules it must keep.
#[allow(dead_code)]
fn hunt(bug: Bug) -> (Option<(String, String)>, Log) {
    let log = std::cell::RefCell::new(Log::default());
    let candidate = |pattern: &str, text: &str| {
        let mut log = log.borrow_mut();
        log.calls += 1;
        let (p, t) = (pattern.chars().count(), text.chars().count());
        if parse(pattern).is_err() || p > MAX_PATTERN || t > MAX_TEXT || log.calls > MAX_CALLS {
            if log.broken_rules.len() < 3 {
                let call = log.calls;
                log.broken_rules.push(format!("call {call}: pattern {pattern:?}, text {text:?}"));
            }
            return None;
        }
        log.patterns.insert(pattern.to_string());
        drop(log);
        engine(bug, pattern, text)
    };
    let found = find_difference(&candidate);
    (found, log.into_inner())
}

/// Panics unless the search finds a real disagreement for this bug.
#[allow(dead_code)]
fn must_catch(bug: Bug) {
    let (found, log) = hunt(bug);
    check!(format!("{bug:?}: calls that broke a rule"), log.broken_rules, Vec::<String>::new());
    let Some((pattern, text)) = found else {
        check!(format!("{bug:?} was not caught in {} calls", log.calls), false, true);
        return;
    };
    check!(format!("{bug:?}: {pattern:?} must be a valid pattern"), parse(&pattern).is_ok(), true);
    let right = engine(Bug::None, &pattern, &text);
    check!(
        format!("{bug:?}: on {pattern:?} and {text:?} the broken engine must differ from the right answer {right:?}"),
        engine(bug, &pattern, &text) != right,
        true
    );
}

#[test]
fn no_match_at_the_very_end() {
    must_catch(Bug::NoMatchAtTheVeryEnd);
}

#[test]
fn caret_holds_wherever_an_attempt_starts() {
    must_catch(Bug::CaretHoldsAtEveryStart);
}

#[test]
fn dollar_holds_before_a_final_newline() {
    must_catch(Bug::DollarHoldsBeforeALastNewline);
}

#[test]
fn dot_skips_newlines() {
    must_catch(Bug::DotSkipsNewline);
}

#[test]
fn question_mark_allows_many() {
    must_catch(Bug::OptAllowsMany);
}

#[test]
fn negated_sets_skip_non_ascii() {
    must_catch(Bug::NegatedSetSkipsNonAscii);
}

#[test]
fn ranges_lose_their_last_character() {
    must_catch(Bug::RangeLosesItsTop);
}

#[test]
fn a_repeat_around_a_repeat_runs_once() {
    must_catch(Bug::NestedRepeatRunsOnce);
}

#[test]
fn empty_branches_are_dropped() {
    must_catch(Bug::EmptyBranchIsLost);
}

#[test]
fn word_shorthand_forgets_the_underscore() {
    must_catch(Bug::WordForgetsUnderscore);
}

#[test]
fn only_two_branches_are_tried() {
    must_catch(Bug::OnlyTwoBranches);
}

#[test]
fn the_search_is_repeatable() {
    for bug in [Bug::None, Bug::ShortestWins, Bug::NestedRepeatRunsOnce] {
        let (first, a) = hunt(bug);
        let (second, b) = hunt(bug);
        check!(format!("{bug:?}: same result and the same number of calls"), (first, a.calls), (second, b.calls));
    }
}
