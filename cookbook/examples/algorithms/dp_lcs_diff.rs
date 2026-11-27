//! RECIPE 74 (dp): what changed between two versions of a text: a longest-common-subsequence table, and Myers' shortest edit script.
//! LEVEL: intermediate
//! Use in: showing the changes between two versions of a file, a config or any ordered list.   Run: cargo run --release --example dp_lcs_diff
//! COMMON USES: code review views, version control, config change reports, test failure output.
//!
//! A diff keeps some items, deletes some from the old version and inserts some from the new one. The fewer deletes and
//! inserts, the longer the kept part, which is a longest common subsequence (LCS) of the two versions.
//! `table_diff` fills an (n+1) by (m+1) table: simple, but memory grows with n times m.
//! `myers_diff` tries 0 changes, then 1, then 2, and so on. It takes a budget and gives up when the budget is passed.
//! This is the basic form of Myers' algorithm. It saves one row per round, so memory grows with the square of the number
//! of changes. The paper also describes a linear-space refinement, which is not written here.
//! NOT COVERED: hunks with context lines, patch files, moved blocks, changes inside a line.
//! Read: http://www.xmailserver.org/diff2.pdf

/// One step of an edit script. Reading the steps in order turns the old lines into the new ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit<'a> {
    Keep(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

/// Lines with their endings left on, so a missing final newline or a changed `\r\n` is a change.
fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn table_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Edit<'a>> {
    let (n, m) = (old.len(), new.len());
    // table[i][j] = length of the LCS of old[..i] and new[..j]
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            table[i][j] = if old[i - 1] == new[j - 1] {
                table[i - 1][j - 1] + 1
            } else {
                table[i - 1][j].max(table[i][j - 1])
            };
        }
    }
    // Walk back from the far corner. The steps come out last first.
    let (mut i, mut j) = (n, m);
    let mut edits = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            edits.push(Edit::Keep(old[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            // preferred on a tie: after reversing, a replaced line reads delete, then insert
            edits.push(Edit::Insert(new[j - 1]));
            j -= 1;
        } else {
            edits.push(Edit::Delete(old[i - 1]));
            i -= 1;
        }
    }
    edits.reverse();
    edits
}

/// Does round `d` reach diagonal `k` by an insert (from `k + 1`) rather than by a delete (from `k - 1`)?
/// `row[k + d]` is the furthest `x` known on diagonal `k`, for `k` in `-d..=d`.
fn came_by_insert(row: &[usize], d: isize, k: isize) -> bool {
    k == -d || (k != d && row[(k - 1 + d) as usize] < row[(k + 1 + d) as usize])
}

/// A shortest edit script with at most `max_edits` deletes and inserts, or `None` if it needs more.
fn myers_diff<'a>(old: &[&'a str], new: &[&'a str], max_edits: usize) -> Option<Vec<Edit<'a>>> {
    let (n, m) = (old.len(), new.len());
    let max = max_edits.min(n + m) as isize;
    // x counts old lines used, y counts new lines used, and diagonal k is x - y.
    // furthest[k + offset] is the largest x reached on diagonal k.
    let offset = max + 1;
    let mut furthest = vec![0usize; 2 * max as usize + 3];
    // history[d] is the part of `furthest` that round d reads: diagonals -d..=d
    let mut history: Vec<Vec<usize>> = Vec::new();

    for d in 0..=max {
        history.push(furthest[(offset - d) as usize..=(offset + d) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let at = (k + offset) as usize;
            let mut x = if d == 0 {
                0
            } else if came_by_insert(&history[d as usize], d, k) {
                furthest[at + 1]
            } else {
                furthest[at - 1] + 1
            };
            let mut y = (x as isize - k) as usize;
            // follow equal lines for free
            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
            }
            furthest[at] = x;
            if x >= n && y >= m {
                return Some(walk_back(old, new, &history));
            }
        }
    }
    None
}

/// Rebuilds the path from the end, one round at a time.
fn walk_back<'a>(old: &[&'a str], new: &[&'a str], history: &[Vec<usize>]) -> Vec<Edit<'a>> {
    let (mut x, mut y) = (old.len(), new.len());
    let mut edits = Vec::new();
    for d in (1..history.len() as isize).rev() {
        let row = &history[d as usize];
        let k = x as isize - y as isize;
        let insert = came_by_insert(row, d, k);
        let from_k = if insert { k + 1 } else { k - 1 };
        let from_x = row[(from_k + d) as usize];
        let from_y = (from_x as isize - from_k) as usize;
        // where the free run of this round began: just after its one edit
        let (start_x, start_y) = if insert {
            (from_x, from_y + 1)
        } else {
            (from_x + 1, from_y)
        };
        while x > start_x && y > start_y {
            x -= 1;
            y -= 1;
            edits.push(Edit::Keep(old[x]));
        }
        edits.push(if insert {
            Edit::Insert(new[from_y])
        } else {
            Edit::Delete(old[from_x])
        });
        (x, y) = (from_x, from_y);
    }
    // round 0 is only the equal lines at the start
    while x > 0 && y > 0 {
        x -= 1;
        y -= 1;
        edits.push(Edit::Keep(old[x]));
    }
    edits.reverse();
    edits
}

/// Rebuilds both versions from a script.
fn replay<'a>(edits: &[Edit<'a>]) -> (Vec<&'a str>, Vec<&'a str>) {
    let (mut old, mut new) = (Vec::new(), Vec::new());
    for edit in edits {
        match *edit {
            Edit::Keep(line) => {
                old.push(line);
                new.push(line);
            }
            Edit::Delete(line) => old.push(line),
            Edit::Insert(line) => new.push(line),
        }
    }
    (old, new)
}

fn changes(edits: &[Edit]) -> usize {
    edits.iter().filter(|e| !matches!(e, Edit::Keep(_))).count()
}

fn print_diff(edits: &[Edit]) {
    for edit in edits {
        let (mark, line) = match *edit {
            Edit::Keep(line) => (' ', line),
            Edit::Delete(line) => ('-', line),
            Edit::Insert(line) => ('+', line),
        };
        println!("{mark} {}", line.trim_end_matches(['\r', '\n']));
        if !line.ends_with('\n') {
            println!("\\ No newline at end of file");
        }
    }
}

/// An oracle that shares no idea with the code under test: try every subsequence of `a`. Only for short inputs.
fn lcs_by_trying_all(a: &[&str], b: &[&str]) -> usize {
    assert!(a.len() <= 16);
    let mut best = 0;
    for mask in 0u32..1 << a.len() {
        let mut rest = b.iter();
        let picked = (0..a.len()).filter(|i| mask >> i & 1 == 1);
        if picked.clone().all(|i| rest.any(|line| *line == a[i])) {
            best = best.max(picked.count());
        }
    }
    best
}

/// Checks both diffs against `fewest`, the smallest possible number of changes.
fn check_pair(old: &[&str], new: &[&str], fewest: usize) {
    let by_table = table_diff(old, new);
    let by_myers = myers_diff(old, new, usize::MAX).expect("no budget");
    for edits in [&by_table, &by_myers] {
        assert_eq!(
            replay(edits),
            (old.to_vec(), new.to_vec()),
            "{old:?} -> {new:?}: {edits:?}"
        );
        assert_eq!(changes(edits), fewest, "{old:?} -> {new:?}: {edits:?}");
    }
    // the budget is exact: enough at `fewest`, not enough one below
    assert!(myers_diff(old, new, fewest).is_some());
    if fewest > 0 {
        assert!(
            myers_diff(old, new, fewest - 1).is_none(),
            "{old:?} -> {new:?}"
        );
    }
}

/// Every list of at most `max_len` lines drawn from `lines`.
fn all_lists<'a>(lines: &[&'a str], max_len: usize) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&str>> = vec![Vec::new()];
    let mut from = 0;
    for _ in 0..max_len {
        let to = out.len();
        for i in from..to {
            for &line in lines {
                let mut longer = out[i].clone();
                longer.push(line);
                out.push(longer);
            }
        }
        from = to;
    }
    out
}

/// A small seeded random generator (xorshift), so every run tests the same inputs.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn main() {
    let old_text = "name = web\nport = 80\nworkers = 4\nlog = info\n";
    let new_text = "name = web\nport = 8080\nworkers = 4\ntimeout = 30\nlog = info\n";
    let (old, new) = (split_lines(old_text), split_lines(new_text));
    let edits = myers_diff(&old, &new, 100).expect("within the budget");
    print_diff(&edits);
    use Edit::{Delete, Insert, Keep};
    let expected = [
        Keep("name = web\n"),
        Delete("port = 80\n"),
        Insert("port = 8080\n"),
        Keep("workers = 4\n"),
        Insert("timeout = 30\n"),
        Keep("log = info\n"),
    ];
    assert_eq!(edits, expected);
    assert_eq!(table_diff(&old, &new), expected);

    // a lost final newline and a changed line ending are changes
    for (before, after) in [("a\nb\n", "a\nb"), ("a\nb\n", "a\r\nb\r\n")] {
        let (before, after) = (split_lines(before), split_lines(after));
        assert!(changes(&table_diff(&before, &after)) > 0);
        assert!(changes(&myers_diff(&before, &after, 10).unwrap()) > 0);
    }

    // every pair of lists of up to 5 lines from an alphabet of 3, against trying every subsequence
    let lists = all_lists(&["a\n", "b\n", "c\n"], 5);
    for old in &lists {
        for new in &lists {
            check_pair(
                old,
                new,
                old.len() + new.len() - 2 * lcs_by_trying_all(old, new),
            );
        }
    }
    println!("checked {} small pairs", lists.len() * lists.len());

    // longer random pairs: the two methods are independent, so each checks the other
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let words = ["a\n", "b\n", "c\n", "d\n", "é\n", "\n", "a"];
    for _ in 0..3_000 {
        let kinds = 1 + rng.below(words.len());
        let old: Vec<&str> = (0..rng.below(60))
            .map(|_| words[rng.below(kinds)])
            .collect();
        let mut new = old.clone();
        for _ in 0..rng.below(30) {
            let at = rng.below(new.len() + 1);
            if at < new.len() && rng.below(2) == 0 {
                new.remove(at);
            } else {
                new.insert(at, words[rng.below(words.len())]);
            }
        }
        let by_table = table_diff(&old, &new);
        check_pair(&old, &new, changes(&by_table));
    }

    // large inputs: nearly equal files are cheap, and very different ones are refused by the budget
    let numbers: Vec<String> = (0..200_000).map(|i| format!("line {i}\n")).collect();
    let big: Vec<&str> = numbers.iter().map(String::as_str).collect();
    let mut changed = big.clone();
    changed.remove(150_000);
    changed.insert(10, "new line\n");
    let edits = myers_diff(&big, &changed, 10).expect("two changes");
    assert_eq!(changes(&edits), 2);
    assert_eq!(replay(&edits), (big.clone(), changed));
    let other: Vec<&str> = std::iter::repeat_n("x\n", 200_000).collect();
    assert!(myers_diff(&big, &other, 1_000).is_none());
    println!("all checks passed");
}
