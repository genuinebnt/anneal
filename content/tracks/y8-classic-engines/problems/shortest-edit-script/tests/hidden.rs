use solution::*;

#[test]
fn a_moved_line_costs_two_edits() {
    check!(r#"a b c -> c a b"#, { let ops = diff(&["a", "b", "c"], &["c", "a", "b"]); ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count() }, 2);
}

#[test]
fn completely_different() {
    check!(r#"no common lines"#, diff(&["a", "b"], &["c", "d"]).len(), 4);
}

#[test]
fn duplicate_lines() {
    check!(r#"repeated lines are matched in order"#, { let ops = diff(&["x", "x", "y"], &["x", "y", "x"]); ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count() }, 2);
}

#[test]
fn trailing_deletes_and_inserts() {
    check!(r#"tail handling"#, (diff(&["a", "b", "c"], &["a"]), diff(&["a"], &["a", "b", "c"])), (vec![Op::Keep(0, 0), Op::Delete(1), Op::Delete(2)], vec![Op::Keep(0, 0), Op::Insert(1), Op::Insert(2)]));
}

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        fn apply(a: &[&str], b: &[&str], ops: &[Op]) -> Option<Vec<String>> {
            // replays the script: must consume a and b exactly, in order
            let (mut i, mut j) = (0, 0);
            let mut out = Vec::new();
            for op in ops {
                match *op {
                    Op::Keep(x, y) => {
                        if x != i || y != j || i >= a.len() || j >= b.len() || a[i] != b[j] {
                            return None;
                        }
                        out.push(a[i].to_string());
                        i += 1;
                        j += 1;
                    }
                    Op::Delete(x) => {
                        if x != i || i >= a.len() {
                            return None;
                        }
                        i += 1;
                    }
                    Op::Insert(y) => {
                        if y != j || j >= b.len() {
                            return None;
                        }
                        out.push(b[j].to_string());
                        j += 1;
                    }
                }
            }
            if i == a.len() && j == b.len() { Some(out) } else { None }
        }

        fn lcs_len(a: &[&str], b: &[&str]) -> usize {
            let mut prev = vec![0usize; b.len() + 1];
            for x in a {
                let mut cur = vec![0usize; b.len() + 1];
                for (j, y) in b.iter().enumerate() {
                    cur[j + 1] = if x == y { prev[j] + 1 } else { cur[j].max(prev[j + 1]) };
                }
                prev = cur;
            }
            prev[b.len()]
        }

        fn edits(ops: &[Op]) -> usize {
            ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count()
        }

        fn check_diff(a: &[&str], b: &[&str]) {
            let ops = diff(a, b);
            let replay = apply(a, b, &ops);
            assert!(replay.is_some(), "the script does not turn a into b: a={a:?} b={b:?} ops={ops:?}");
            assert_eq!(replay.unwrap(), b.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "replay differs from b");
            assert_eq!(edits(&ops), a.len() + b.len() - 2 * lcs_len(a, b), "not a shortest script for a={a:?} b={b:?}: {ops:?}");
        }

        #[test]
        fn greedy_is_not_enough() {
            // deleting on every mismatch costs 4 here; the shortest script costs 2
            check_diff(&["a", "b", "c"], &["c", "a", "b"]);
            check_diff(&["a", "b", "c", "d"], &["b", "c", "d", "a"]);
            check_diff(&["x", "a", "b", "c"], &["a", "b", "c", "y"]);
        }

        #[test]
        fn scripts_replay_and_are_shortest_on_random_inputs() {
            let mut rng = anneal_prelude::Rng::new(77);
            let words = ["a", "b", "c", "d", "e"];
            for _ in 0..500 {
                let la = rng.below(12);
                let lb = rng.below(12);
                let a: Vec<&str> = (0..la).map(|_| words[rng.below(5)]).collect();
                let b: Vec<&str> = (0..lb).map(|_| words[rng.below(5)]).collect();
                check_diff(&a, &b);
            }
        }

        #[test]
        fn small_alphabet_stress() {
            let mut rng = anneal_prelude::Rng::new(5);
            for _ in 0..300 {
                let la = rng.below(25);
                let lb = rng.below(25);
                let a: Vec<&str> = (0..la).map(|_| if rng.bool() { "x" } else { "y" }).collect();
                let b: Vec<&str> = (0..lb).map(|_| if rng.bool() { "x" } else { "y" }).collect();
                check_diff(&a, &b);
            }
        }

        #[test]
        fn big_files_with_a_few_edits() {
            let r = within(10_000, || {
                let lines: Vec<String> = (0..2500).map(|i| format!("line {i}")).collect();
                let a: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let mut b = a.clone();
                b.remove(1200);
                b.insert(10, "added near the top");
                b[2000] = "changed";
                let ops = diff(&a, &b);
                (apply(&a, &b, &ops).is_some(), edits(&ops))
            });
            check!("replays to b, with four edits: one insert, one delete, one replacement (2)", r, Some((true, 4)));
        }

        #[test]
        fn ties_prefer_delete_first() {
            let ops = diff(&["a", "b"], &["c", "d"]);
            check!("the whole replaced block deletes before it inserts", ops, vec![Op::Delete(0), Op::Delete(1), Op::Insert(0), Op::Insert(1)]);
            let ops = diff(&["a", "x", "b"], &["a", "y", "b"]);
            check!("a replaced middle line", ops, vec![Op::Keep(0, 0), Op::Delete(1), Op::Insert(1), Op::Keep(2, 2)]);
        }
