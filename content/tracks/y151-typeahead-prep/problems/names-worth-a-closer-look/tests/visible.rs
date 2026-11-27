use solution::*;

fn slips_between(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
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

fn ask(names: &[&str], typed: &str, slips: usize) -> Vec<(usize, usize)> {
    let owned: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let filed = Filed::new(&owned);
    near(typed, slips, &mut |key| filed.list(key), &mut |n| Some(slips_between(typed, names[n])).filter(|&d| d <= slips))
}

#[test]
fn closest_first_then_by_number() {
    let names = ["ibuprofen", "ibuprofin", "paracetamol", "ibuprofen"];
    check!("one slip", ask(&names, "ibuprofen", 1), vec![(0, 0), (3, 0), (1, 1)]);
    check!("no slips", ask(&names, "ibuprofin", 0), vec![(1, 0)]);
}

#[test]
fn a_swap_in_a_long_name() {
    check!("two letters swapped", ask(&["paracetamol", "ibuprofen"], "paraectamol", 1), vec![(0, 1)]);
}

#[test]
fn short_names_are_not_missed() {
    check!("ab / ba", ask(&["ab", "cd", ""], "ba", 1), vec![(0, 1)]);
    check!("one letter", ask(&["x", "y"], "z", 1), vec![(0, 1), (1, 1)]);
}

#[test]
fn nothing_close() {
    check!("far", ask(&["ibuprofen", "paracetamol"], "amoxicillin", 2), vec![]);
}

#[test]
fn case_matters() {
    check!("one replaced", ask(&["Aspirin"], "aspirin", 1), vec![(0, 1)]);
    check!("no slips", ask(&["Aspirin"], "aspirin", 0), vec![]);
}
