use solution::*;
use std::collections::HashSet;

fn table(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        for j in 0..=b.len() {
            d[i][j] = if i == 0 || j == 0 {
                i + j
            } else {
                let mut best = (d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1])).min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    best = best.min(d[i - 2][j - 2] + 1);
                }
                best
            };
        }
    }
    d[a.len()][b.len()]
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn brute(names: &[Vec<char>], typed: &[char], slips: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> =
        (0..names.len()).map(|n| (n, table(typed, &names[n]))).filter(|&(_, d)| d <= slips).collect();
    out.sort_by_key(|&(n, d)| (d, n));
    out
}

/// The remote index: it files each name's number under the keys that `keys` returns, in a mixed order.
struct Filed(std::collections::HashMap<String, Vec<usize>>);

impl Filed {
    fn new(names: &[String]) -> Filed {
        let mut order: Vec<usize> = (0..names.len()).collect();
        anneal_prelude::Rng::new(names.len() as u64 + 7).shuffle(&mut order);
        let mut lists: std::collections::HashMap<String, Vec<usize>> = Default::default();
        for number in order {
            for key in keys(&names[number]) {
                lists.entry(key).or_default().push(number);
            }
        }
        for list in lists.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        Filed(lists)
    }

    fn list(&self, key: &str) -> Vec<usize> {
        self.0.get(key).cloned().unwrap_or_default()
    }
}

fn filed(words: &[&str]) -> Filed {
    Filed::new(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
}

#[derive(Default, Debug)]
struct Counts {
    lists: usize,
    numbers: usize,
    compares: usize,
}

/// Runs one question and returns the answer and what it cost. A number compared twice fails the test.
fn run(index: &Filed, names: &[Vec<char>], typed: &str, slips: usize, limit: &Counts) -> (Vec<(usize, usize)>, Counts) {
    let t = chars(typed);
    let mut seen = HashSet::new();
    let (mut lists, mut numbers) = (0, 0);
    let answer = near(
        typed,
        slips,
        &mut |key| {
            lists += 1;
            assert!(lists <= limit.lists, "more than {} calls of list, which is over the budget", limit.lists);
            let got = index.list(key);
            numbers += got.len();
            assert!(numbers <= limit.numbers, "more than {} numbers fetched, which is over the budget", limit.numbers);
            got
        },
        &mut |n| {
            assert!(seen.insert(n), "compare({n}) was called twice in one question");
            assert!(seen.len() <= limit.compares, "more than {} calls of compare, which is over the budget", limit.compares);
            let d = table(&t, &names[n]);
            (d <= slips).then_some(d)
        },
    );
    let compares = seen.len();
    (answer, Counts { lists, numbers, compares })
}

/// For small catalogues, where the counts are not graded.
const FREE: Counts = Counts { lists: 100_000, numbers: 1_000_000, compares: 100_000 };

fn slip(rng: &mut anneal_prelude::Rng, word: &mut Vec<char>, letters: &[char]) {
    match rng.below(4) {
        0 => {
            let at = rng.below(word.len() + 1);
            word.insert(at, *rng.pick(letters));
        }
        1 if !word.is_empty() => {
            let at = rng.below(word.len());
            word.remove(at);
        }
        2 if !word.is_empty() => {
            let at = rng.below(word.len());
            word[at] = *rng.pick(letters);
        }
        _ if word.len() > 1 => {
            let at = rng.below(word.len() - 1);
            word.swap(at, at + 1);
        }
        _ => {}
    }
}

#[test]
fn an_empty_catalogue_and_an_empty_text() {
    check!("no names", run(&filed(&[]), &[], "abc", 2, &FREE).0, vec![]);
    let names = vec![chars(""), chars("ab"), chars("abc")];
    let list = filed(&["", "ab", "abc"]);
    check!("empty text, 2 slips", run(&list, &names, "", 2, &FREE).0, vec![(0, 0), (1, 2)]);
}

#[test]
fn a_huge_number_of_slips_returns_everything() {
    let words = ["alpha", "z", "a very long name that shares nothing"];
    let names: Vec<Vec<char>> = words.iter().map(|w| chars(w)).collect();
    let list = filed(&words);
    for slips in [usize::MAX, usize::MAX / 2] {
        check!(format!("slips = {slips}"), run(&list, &names, "qq", slips, &FREE).0, brute(&names, &chars("qq"), slips));
    }
}

#[test]
fn repeated_letters() {
    let words = ["aaaaaaaaaaaaaa", "aaaaaaaaaaaaa", "aaaaaabaaaaaaa", "abababababab", "bababababab", "ababababbaab"];
    let names: Vec<Vec<char>> = words.iter().map(|w| chars(w)).collect();
    let list = filed(&words);
    for typed in words {
        for slips in 0..4 {
            check!(format!("{typed} within {slips}"), run(&list, &names, typed, slips, &FREE).0, brute(&names, &chars(typed), slips));
        }
    }
}

#[test]
fn other_alphabets_and_two_swaps() {
    let words = ["東京タワー前駅", "東京タワ前ー駅", "ÉCOLE maternelle", "école maternelle"];
    let names: Vec<Vec<char>> = words.iter().map(|w| chars(w)).collect();
    let list = filed(&words);
    for typed in ["京東タワー駅前", "école maternelle", "東京タワー前駅"] {
        for slips in 0..3 {
            check!(format!("{typed} within {slips}"), run(&list, &names, typed, slips, &FREE).0, brute(&names, &chars(typed), slips));
        }
    }
}

#[test]
fn nothing_close_is_missed_in_small_random_lists() {
    let mut rng = anneal_prelude::Rng::new(15104);
    let letters = ['a', 'b', 'c', 'é'];
    for _ in 0..600 {
        let count = rng.below(25);
        let names: Vec<Vec<char>> = (0..count)
            .map(|_| {
                let n = rng.below(14);
                (0..n).map(|_| *rng.pick(&letters)).collect()
            })
            .collect();
        let strings: Vec<String> = names.iter().map(|w| w.iter().collect()).collect();
        let list = Filed::new(&strings);
        let mut typed: Vec<char> = if count > 0 && rng.bool() { names[rng.below(count)].clone() } else { chars("abcabcabc") };
        for _ in 0..rng.below(4) {
            slip(&mut rng, &mut typed, &letters);
        }
        let typed: String = typed.iter().collect();
        let slips = rng.below(4);
        check!(format!("{strings:?} typed {typed:?} within {slips}"), run(&list, &names, &typed, slips, &FREE).0, brute(&names, &chars(&typed), slips));
    }
}

#[test]
fn a_large_catalogue_within_the_budget() {
    let mut rng = anneal_prelude::Rng::new(15105);
    let letters: Vec<char> = ('a'..='z').collect();
    let names: Vec<Vec<char>> = (0..4_000)
        .map(|_| {
            let n = 12 + rng.below(5);
            (0..n).map(|_| *rng.pick(&letters)).collect()
        })
        .collect();
    let list = Filed::new(&names.iter().map(|w| w.iter().collect()).collect::<Vec<String>>());
    for round in 0..24 {
        let slips = round % 3;
        let mut typed: Vec<char> = if round % 8 == 7 {
            (0..14).map(|_| *rng.pick(&letters)).collect()
        } else {
            names[rng.below(names.len())].clone()
        };
        for _ in 0..slips {
            slip(&mut rng, &mut typed, &letters);
        }
        while typed.len() < 12 {
            typed.push('q');
        }
        let typed: String = typed.iter().collect();
        // the expected answer, comparing only the names that could be close
        let want = brute_by_letters(&names, &chars(&typed), slips);
        let limit = Counts { lists: 200, numbers: 1500, compares: want.len() + 500 };
        let (answer, _) = run(&list, &names, &typed, slips, &limit);
        check!(format!("{typed} within {slips}"), answer, want);
    }
}

/// Compares only the names whose length allows it.
fn brute_by_letters(names: &[Vec<char>], typed: &[char], slips: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (n, name) in names.iter().enumerate() {
        if name.len().abs_diff(typed.len()) > slips {
            continue;
        }
        // a quick necessary condition in the test itself: letter counts differ by at most 2 per slip
        let mut count = [0i32; 128];
        for &c in name {
            count[c as usize] += 1;
        }
        for &c in typed {
            count[c as usize] -= 1;
        }
        if count.iter().map(|c| c.unsigned_abs() as usize).sum::<usize>() > 2 * slips {
            continue;
        }
        let d = table(typed, name);
        if d <= slips {
            out.push((n, d));
        }
    }
    out.sort_by_key(|&(n, d)| (d, n));
    out
}

#[test]
fn lengths_are_counted_in_characters() {
    let words = ["éèêë", "éè", "日本", "日本語テキスト"];
    let names: Vec<Vec<char>> = words.iter().map(|w| chars(w)).collect();
    let list = filed(&words);
    for typed in ["ée", "日", "éèêëx", "日本語テキス"] {
        for slips in 0..3 {
            check!(format!("{typed} within {slips}"), run(&list, &names, typed, slips, &FREE).0, brute(&names, &chars(typed), slips));
        }
    }
}

#[test]
fn equal_names_are_each_reported() {
    let names: Vec<Vec<char>> = (0..30).map(|_| chars("loratadine")).collect();
    let list = filed(&["loratadine"; 30]);
    check!("all thirty", run(&list, &names, "loratadin", 1, &FREE).0, brute(&names, &chars("loratadin"), 1));
}
