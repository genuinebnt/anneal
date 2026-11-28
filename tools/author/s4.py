from author import T, write_track

HM = "std::collections::HashMap"
P = []

P.append(dict(
    slug="most-common-word", title="Most common word", level="easy", stage="use-it", tags=["HashMap", "HashSet"],
    teaches=["Count into a `HashMap`, filter with a `HashSet`.", "`max_by` with a tie-breaker."],
    statement="""
        Return the most frequent word in `paragraph` that isn't in `banned`. Words are runs of ASCII
        letters, compared case-insensitively and returned in lowercase. `banned` holds lowercase words.
        Ties go to the alphabetically first word; return `""` if there are no words.
    """,
    examples=[("\"Bob hit a ball, the hit BALL flew far after it was hit.\", banned = [\"hit\"]", "\"ball\"")],
    starter="""
        pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
            todo!()
        }
    """,
    solution="""
        use std::collections::{HashMap, HashSet};

        pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
            let banned: HashSet<&str> = banned.iter().copied().collect();
            let mut counts: HashMap<String, usize> = HashMap::new();
            for w in paragraph
                .split(|c: char| !c.is_ascii_alphabetic())
                .filter(|w| !w.is_empty())
            {
                let w = w.to_ascii_lowercase();
                if !banned.contains(w.as_str()) {
                    *counts.entry(w).or_insert(0) += 1;
                }
            }
            counts
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                .map(|(w, _)| w)
                .unwrap_or_default()
        }
    """,
    visible=[
        T("ball", "\"Bob hit a ball, the hit BALL flew far after it was hit.\", banned = [\"hit\"]", 'most_common_word("Bob hit a ball, the hit BALL flew far after it was hit.", &["hit"])', '"ball"'),
        T("single", "\"a.\", banned = []", 'most_common_word("a.", &[])', '"a"'),
        T("tie_alphabetical", "\"b a b a\", banned = []", 'most_common_word("b a b a", &[])', '"a"'),
        T("all_banned", "\"x y\", banned = [\"x\", \"y\"]", 'most_common_word("x y", &["x", "y"])', '""'),
        T("case_insensitive", "\"Hello hello HELLO world\", banned = []", 'most_common_word("Hello hello HELLO world", &[])', '"hello"'),
    ],
    hidden=[
        T("empty", "\"\", banned = []", 'most_common_word("", &[])', '""'),
        T("punctuation_only", "\"!!! ,, .\", banned = []", 'most_common_word("!!! ,, .", &[])', '""'),
        T("digits_split_words", "\"abc1abc2def\", banned = []", 'most_common_word("abc1abc2def", &[])', '"abc"'),
        T("apostrophe_splits", "\"don't don't stop\", banned = []", 'most_common_word("don\'t don\'t stop", &[])', '"don"'),
        T("banned_matches_any_case", "\"HIT hit ball\", banned = [\"hit\"]", 'most_common_word("HIT hit ball", &["hit"])', '"ball"'),
        T("non_ascii_splits", "\"naïve naïve x\", banned = []", 'most_common_word("naïve naïve x", &[])', '"na"'),
        T("commas_without_spaces", "\"a, a, a, a, b,b,b,c, c\", banned = [\"a\"]", 'most_common_word("a, a, a, a, b,b,b,c, c", &["a"])', '"b"'),
        T("banned_absent", "\"x x y\", banned = [\"zzz\"]", 'most_common_word("x x y", &["zzz"])', '"x"'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4001);
            let bans = ["", "a", "b"];
            for _ in 0..300 {
                let len = rng.below(14);
                let text = rng.string(len, "abAB .,1");
                let ban = *rng.pick(&bans);
                let banned: Vec<&str> = if ban.is_empty() { vec![] } else { vec![ban] };
                let words: Vec<String> = text.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()).map(|w| w.to_ascii_lowercase()).filter(|w| !banned.contains(&w.as_str())).collect();
                let mut want = String::new();
                let mut best = 0;
                for w in &words {
                    let c = words.iter().filter(|x| *x == w).count();
                    if c > best || (c == best && *w < want) {
                        best = c;
                        want = w.clone();
                    }
                }
                check!(format!("paragraph = {text:?}, banned = {banned:?}"), most_common_word(&text, &banned), want);
            }
        }

        #[test]
        fn scale_200k_words() {
            fn word(mut i: usize) -> String {
                let mut w = String::new();
                for _ in 0..4 {
                    w.push(char::from(b'a' + (i % 26) as u8));
                    i /= 26;
                }
                w
            }
            let mut text = String::new();
            for i in 0..200_000 {
                text.push_str(&word(i % 100_000));
                text.push(' ');
            }
            text.push_str(&word(77_777));
            check!("200000 words, 100000 distinct, one of them once more", most_common_word(&text, &[]), word(77_777));
        }
        """,
    ],
    hints=[("rust", "`str::split` with a closure splits on every non-letter; skip the empty pieces."),
           ("edge case", "Without a tie-breaker, iteration order over a HashMap decides the answer.")],
    notes=("`HashMap` iteration order is unspecified, so an explicit tie-break is what makes the answer deterministic.", "O(n)", "O(n)"),
    follow_up="How would you return the top three words efficiently?",
    related=["D1"],
    wrong=dict(
        linear_search="""
            pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
                let mut counts: Vec<(String, usize)> = Vec::new();
                for w in paragraph.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()) {
                    let w = w.to_ascii_lowercase();
                    if banned.contains(&w.as_str()) {
                        continue;
                    }
                    match counts.iter_mut().find(|(k, _)| *k == w) {
                        Some((_, c)) => *c += 1,
                        None => counts.push((w, 1)),
                    }
                }
                counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(w, _)| w).unwrap_or_default()
            }
        """,
        tie_alphabetically_last="""
            use std::collections::{HashMap, HashSet};

            pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
                let banned: HashSet<&str> = banned.iter().copied().collect();
                let mut counts: HashMap<String, usize> = HashMap::new();
                for w in paragraph.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()) {
                    let w = w.to_ascii_lowercase();
                    if !banned.contains(w.as_str()) {
                        *counts.entry(w).or_insert(0) += 1;
                    }
                }
                counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0))).map(|(w, _)| w).unwrap_or_default()
            }
        """,
        splits_on_spaces="""
            use std::collections::{HashMap, HashSet};

            pub fn most_common_word(paragraph: &str, banned: &[&str]) -> String {
                let banned: HashSet<&str> = banned.iter().copied().collect();
                let mut counts: HashMap<String, usize> = HashMap::new();
                for w in paragraph.split_whitespace() {
                    let w = w.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
                    if !w.is_empty() && !banned.contains(w.as_str()) {
                        *counts.entry(w).or_insert(0) += 1;
                    }
                }
                counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(w, _)| w).unwrap_or_default()
            }
        """,
    ),
))

P.append(dict(
    slug="group-by-length", title="Group borrowed words by length", level="easy", stage="use-it", tags=["entry API", "lifetimes"],
    teaches=["`entry(k).or_default().push(..)` for grouping.", "Keeping borrowed `&'a str` in the output instead of allocating Strings."],
    statement="Group `words` by length (`str::len`, in bytes), keeping each group in input order. Don't copy the strings.",
    starter="""
        use std::collections::HashMap;

        pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
            todo!()
        }
    """,
    solution="""
        use std::collections::HashMap;

        pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
            let mut groups: HashMap<usize, Vec<&'a str>> = HashMap::new();
            for &w in words {
                groups.entry(w.len()).or_default().push(w);
            }
            groups
        }
    """,
    visible=[
        T("mixed", "[\"hi\", \"yes\", \"no\", \"ok\"]", 'group_by_len(&["hi", "yes", "no", "ok"])', f'{HM}::from([(2, vec!["hi", "no", "ok"]), (3, vec!["yes"])])'),
        T("empty", "[]", "group_by_len(&[])", f"{HM}::new()"),
        T("empty_string", "[\"\", \"a\", \"\"]", 'group_by_len(&["", "a", ""])', f'{HM}::from([(0, vec!["", ""]), (1, vec!["a"])])'),
        T("input_order_kept", "[\"bb\", \"aa\", \"cc\"]", 'group_by_len(&["bb", "aa", "cc"])', f'{HM}::from([(2, vec!["bb", "aa", "cc"])])'),
        T("single", "[\"a\"]", 'group_by_len(&["a"])', f'{HM}::from([(1, vec!["a"])])'),
    ],
    hidden=[
        T("length_in_bytes", "[\"é\", \"ab\"]", 'group_by_len(&["é", "ab"])', f'{HM}::from([(2, vec!["é", "ab"])])'),
        T("duplicates", "[\"a\", \"a\"]", 'group_by_len(&["a", "a"])', f'{HM}::from([(1, vec!["a", "a"])])'),
        T("borrows_the_words", "one word, compared by address", '{ let s = String::from("xyz"); let g = group_by_len(&[s.as_str()]); std::ptr::eq(g[&3][0], s.as_str()) }', "true"),
        T("long_word", "a 1000-byte word", '{ let w = "x".repeat(1000); group_by_len(&[w.as_str()]).keys().copied().collect::<Vec<_>>() }', "vec![1000]"),
        T("many", "10000 words of lengths 0..10", '{ let ws: Vec<String> = (0..10_000).map(|i| "a".repeat(i % 10)).collect(); let refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect(); let g = group_by_len(&refs); (g.len(), g[&0].len(), g[&9].len()) }', "(10, 1000, 1000)"),
        T("three_groups", "[\"a\", \"bbb\", \"cc\", \"d\"]", 'group_by_len(&["a", "bbb", "cc", "d"])', f'{HM}::from([(1, vec!["a", "d"]), (2, vec!["cc"]), (3, vec!["bbb"])])'),
        T("spaces_count", "[\" \", \"  \"]", 'group_by_len(&[" ", "  "])', f'{HM}::from([(1, vec![" "]), (2, vec!["  "])])'),
        T("scale_many_distinct_lengths", "200000 slices of one string, 50000 distinct lengths",
          "(g.len(), g[&0].len(), g[&49_999].len(), std::ptr::eq(g[&49_999][3], &big[..49_999]))", "(50_000, 4, 4, true)",
          setup="let big = \"a\".repeat(50_000);\nlet refs: Vec<&str> = (0..200_000).map(|i| &big[..i % 50_000]).collect();\nlet g = group_by_len(&refs);"),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4002);
            for _ in 0..300 {
                let n = rng.below(10);
                let words: Vec<String> = (0..n).map(|_| { let l = rng.below(4); rng.string(l, "aé") }).collect();
                let refs: Vec<&str> = words.iter().map(|w| w.as_str()).collect();
                let mut want: std::collections::HashMap<usize, Vec<&str>> = std::collections::HashMap::new();
                for &w in &refs {
                    let group = want.entry(w.len()).or_default();
                    group.push(w);
                }
                check!(format!("words = {refs:?}"), group_by_len(&refs), want);
            }
        }
        """,
    ],
    hints=[("rust", "`entry(len).or_default()` gives you the group's `Vec`, creating it if needed.")],
    notes=("The output borrows from the caller's strings, so the signature ties it to `'a`, the words' lifetime, not the slice's.", "O(n)", "O(n)"),
    follow_up="Why is the lifetime on `&'a str` and not on the outer slice?",
    related=["L3"],
    wrong=dict(
        sorts_each_group="""
            use std::collections::HashMap;

            pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
                let mut groups: HashMap<usize, Vec<&'a str>> = HashMap::new();
                for &w in words {
                    groups.entry(w.len()).or_default().push(w);
                }
                for g in groups.values_mut() {
                    g.sort();
                }
                groups
            }
        """,
        skips_empty_words="""
            use std::collections::HashMap;

            pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
                let mut groups: HashMap<usize, Vec<&'a str>> = HashMap::new();
                for &w in words.iter().filter(|w| !w.is_empty()) {
                    groups.entry(w.len()).or_default().push(w);
                }
                groups
            }
        """,
        counts_chars="""
            use std::collections::HashMap;

            pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
                let mut groups: HashMap<usize, Vec<&'a str>> = HashMap::new();
                for &w in words {
                    groups.entry(w.chars().count()).or_default().push(w);
                }
                groups
            }
        """,
        filters_once_per_length="""
            use std::collections::{HashMap, HashSet};

            pub fn group_by_len<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
                let lengths: HashSet<usize> = words.iter().map(|w| w.len()).collect();
                lengths
                    .into_iter()
                    .map(|len| (len, words.iter().copied().filter(|w| w.len() == len).collect()))
                    .collect()
            }
        """,
    ),
))

P.append(dict(
    slug="compare-tags", title="Set operations on tags", level="easy", stage="use-it", tags=["HashSet", "intersection"],
    teaches=["`intersection` and `difference` on `HashSet`.", "Sorting results that come out of a hash set."],
    statement="Given two tag lists, return (tags in both, tags only in `a`, tags only in `b`), each sorted and without duplicates.",
    starter="""
        pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
            todo!()
        }
    """,
    solution="""
        use std::collections::HashSet;

        pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
            let a: HashSet<&str> = a.iter().copied().collect();
            let b: HashSet<&str> = b.iter().copied().collect();
            let sorted = |it: &mut dyn Iterator<Item = &&str>| {
                let mut v: Vec<String> = it.map(|s| s.to_string()).collect();
                v.sort();
                v
            };
            (sorted(&mut a.intersection(&b)), sorted(&mut a.difference(&b)), sorted(&mut b.difference(&a)))
        }
    """,
    visible=[
        T("overlap", "a = [rust, go, sql], b = [sql, rust, c]", 'compare_tags(&["rust", "go", "sql"], &["sql", "rust", "c"])', '(vec!["rust".to_string(), "sql".to_string()], vec!["go".to_string()], vec!["c".to_string()])'),
        T("disjoint", "a = [a], b = [b]", 'compare_tags(&["a"], &["b"])', '(vec![], vec!["a".to_string()], vec!["b".to_string()])'),
        T("duplicates", "a = [x, x], b = [x]", 'compare_tags(&["x", "x"], &["x"])', '(vec!["x".to_string()], vec![], vec![])'),
        T("both_empty", "a = [], b = []", "compare_tags(&[], &[])", "(Vec::<String>::new(), Vec::<String>::new(), Vec::<String>::new())"),
        T("sorted_output", "a = [c, b, a], b = []", 'compare_tags(&["c", "b", "a"], &[])', '(vec![], vec!["a".to_string(), "b".to_string(), "c".to_string()], vec![])'),
    ],
    hidden=[
        T("b_only_duplicates", "a = [], b = [y, y]", 'compare_tags(&[], &["y", "y"])', '(vec![], vec![], vec!["y".to_string()])'),
        T("identical", "a = [a, b], b = [b, a]", 'compare_tags(&["a", "b"], &["b", "a"])', '(vec!["a".to_string(), "b".to_string()], vec![], vec![])'),
        T("case_sensitive", "a = [Rust], b = [rust]", 'compare_tags(&["Rust"], &["rust"])', '(vec![], vec!["Rust".to_string()], vec!["rust".to_string()])'),
        T("unicode", "a = [é, e], b = [e]", 'compare_tags(&["é", "e"], &["e"])', '(vec!["e".to_string()], vec!["é".to_string()], vec![])'),
        T("byte_order", "a = [b, B, a], b = []", 'compare_tags(&["b", "B", "a"], &[])', '(vec![], vec!["B".to_string(), "a".to_string(), "b".to_string()], vec![])'),
        T("empty_tag", "a = [\"\"], b = [\"\"]", 'compare_tags(&[""], &[""])', '(vec![String::new()], vec![], vec![])'),
        T("one_side_empty", "a = [x], b = []", 'compare_tags(&["x"], &[])', '(vec![], vec!["x".to_string()], vec![])'),
        T("sorted_intersection", "a = [z, m, a], b = [a, z, m]", 'compare_tags(&["z", "m", "a"], &["a", "z", "m"]).0', 'vec!["a".to_string(), "m".to_string(), "z".to_string()]'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4003);
            let pool = ["a", "b", "c", "d", "e", "B"];
            for _ in 0..300 {
                let (na, nb) = (rng.below(6), rng.below(6));
                let a: Vec<&str> = (0..na).map(|_| *rng.pick(&pool)).collect();
                let b: Vec<&str> = (0..nb).map(|_| *rng.pick(&pool)).collect();
                let sa: std::collections::BTreeSet<String> = a.iter().map(|s| s.to_string()).collect();
                let sb: std::collections::BTreeSet<String> = b.iter().map(|s| s.to_string()).collect();
                let want = (
                    sa.intersection(&sb).cloned().collect::<Vec<_>>(),
                    sa.difference(&sb).cloned().collect::<Vec<_>>(),
                    sb.difference(&sa).cloned().collect::<Vec<_>>(),
                );
                check!(format!("a = {a:?}, b = {b:?}"), compare_tags(&a, &b), want);
            }
        }

        #[test]
        fn scale_100k_each() {
            let a_tags: Vec<String> = (0..100_000).map(|i| format!("t{i}")).collect();
            let b_tags: Vec<String> = (50_000..150_000).map(|i| format!("t{i}")).collect();
            let a: Vec<&str> = a_tags.iter().map(|s| s.as_str()).collect();
            let b: Vec<&str> = b_tags.iter().map(|s| s.as_str()).collect();
            let (both, only_a, only_b) = compare_tags(&a, &b);
            check!("a = t0..t99999, b = t50000..t149999", (both.len(), only_a.len(), only_b.len(), only_a[0].clone()), (50_000, 50_000, 50_000, "t0".to_string()));
        }
        """,
    ],
    hints=[("rust", "Collect each side into a `HashSet<&str>`, then use `intersection` and `difference`.")],
    notes=("The set operations return lazy iterators of references; sorting afterwards is what makes the output stable.", "O(n + m + k log k)", "O(n + m)"),
    follow_up="When would you use `BTreeSet` instead and skip the sort?",
    related=["D1"],
    wrong=dict(
        slice_contains="""
            pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
                let pick = |from: &[&str], other: &[&str], want: bool| {
                    let mut v: Vec<String> = from.iter().filter(|t| other.contains(t) == want).map(|t| t.to_string()).collect();
                    v.sort();
                    v.dedup();
                    v
                };
                (pick(a, b, true), pick(a, b, false), pick(b, a, false))
            }
        """,
        keeps_duplicates="""
            use std::collections::HashSet;

            pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
                let sa: HashSet<&str> = a.iter().copied().collect();
                let sb: HashSet<&str> = b.iter().copied().collect();
                let pick = |from: &[&str], other: &HashSet<&str>, want: bool| {
                    let mut v: Vec<String> = from.iter().filter(|t| other.contains(*t) == want).map(|t| t.to_string()).collect();
                    v.sort();
                    v
                };
                (pick(a, &sb, true), pick(a, &sb, false), pick(b, &sa, false))
            }
        """,
        unsorted="""
            use std::collections::HashSet;

            pub fn compare_tags(a: &[&str], b: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
                let a: HashSet<&str> = a.iter().copied().collect();
                let b: HashSet<&str> = b.iter().copied().collect();
                (
                    a.intersection(&b).map(|s| s.to_string()).collect(),
                    a.difference(&b).map(|s| s.to_string()).collect(),
                    b.difference(&a).map(|s| s.to_string()).collect(),
                )
            }
        """,
    ),
))

INDEX_WRONG = """
    use std::collections::BTreeMap;

    pub fn index(words: &[&str]) -> BTreeMap<char, Vec<String>> {
        let mut idx: BTreeMap<char, Vec<String>> = BTreeMap::new();
        for w in words {
            FIRST {
                idx.entry(first.to_ascii_lowercase()).or_default().push(PUSH);
            }
        }
        for list in idx.values_mut() {
            SORT
        }
        idx
    }
"""

P.append(dict(
    slug="index-by-first-letter", title="Index words by first letter", level="easy", stage="use-it", tags=["BTreeMap", "entry API"],
    teaches=["`BTreeMap` gives sorted keys for free.", "`chars().next()` for the first character."],
    statement="Build an index from each word's first character, ASCII-lowercased, to the words starting with it, each list sorted. Skip empty strings.",
    starter="""
        use std::collections::BTreeMap;

        pub fn index(words: &[&str]) -> BTreeMap<char, Vec<String>> {
            todo!()
        }
    """,
    solution="""
        use std::collections::BTreeMap;

        pub fn index(words: &[&str]) -> BTreeMap<char, Vec<String>> {
            let mut idx: BTreeMap<char, Vec<String>> = BTreeMap::new();
            for w in words {
                if let Some(first) = w.chars().next() {
                    idx.entry(first.to_ascii_lowercase()).or_default().push(w.to_string());
                }
            }
            for list in idx.values_mut() {
                list.sort();
            }
            idx
        }
    """,
    visible=[
        T("fruit", "[\"banana\", \"Apple\", \"avocado\"]", 'index(&["banana", "Apple", "avocado"]).into_iter().collect::<Vec<_>>()', "vec![('a', vec![\"Apple\".to_string(), \"avocado\".to_string()]), ('b', vec![\"banana\".to_string()])]"),
        T("empty_string", "[\"\"]", 'index(&[""]).len()', "0"),
        T("keys_sorted", "[\"z\", \"m\", \"a\"]", 'index(&["z", "m", "a"]).keys().copied().collect::<Vec<_>>()', "vec!['a', 'm', 'z']"),
        T("case_merged_words_kept", "[\"Bob\", \"ann\", \"Al\"]", 'index(&["Bob", "ann", "Al"]).into_iter().collect::<Vec<_>>()', "vec![('a', vec![\"Al\".to_string(), \"ann\".to_string()]), ('b', vec![\"Bob\".to_string()])]"),
        T("lists_sorted", "[\"b2\", \"b10\", \"b1\"]", 'index(&["b2", "b10", "b1"])[&\'b\'].clone()', 'vec!["b1".to_string(), "b10".to_string(), "b2".to_string()]'),
    ],
    hidden=[
        T("empty", "[]", "index(&[]).len()", "0"),
        T("duplicates", "[\"a\", \"a\"]", 'index(&["a", "a"])[&\'a\'].clone()', 'vec!["a".to_string(), "a".to_string()]'),
        T("keeps_original_case", "[\"Zed\"]", 'index(&["Zed"]).into_iter().collect::<Vec<_>>()', "vec![('z', vec![\"Zed\".to_string()])]"),
        T("non_letters", "[\"1abc\", \"_x\"]", 'index(&["1abc", "_x"]).keys().copied().collect::<Vec<_>>()', "vec!['1', '_']"),
        T("non_ascii_not_folded", "[\"école\", \"Émile\"]", 'index(&["école", "Émile"]).keys().copied().collect::<Vec<_>>()', "vec!['É', 'é']"),
        T("mixed_empties", "[\"\", \"a\", \"\"]", 'index(&["", "a", ""]).len()', "1"),
        T("upper_sorts_first", "[\"apple\", \"Apple\"]", 'index(&["apple", "Apple"])[&\'a\'].clone()', 'vec!["Apple".to_string(), "apple".to_string()]'),
        T("many", "10000 words over 26 letters", '{ let ws: Vec<String> = (0..10_000u32).map(|i| format!("{}{}", char::from(b\'a\' + (i % 26) as u8), i)).collect(); let refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect(); let idx = index(&refs); (idx.len(), idx[&\'a\'].len(), idx[&\'z\'][0].clone()) }', '(26, 385, "z1013".to_string())'),
        T("scale_one_letter_reverse_order", "400000 words starting with 'a', in descending order",
          "(idx.len(), idx[&'a'].len(), idx[&'a'][0].clone(), idx[&'a'][399_999].clone())", '(1, 400_000, "a000000".to_string(), "a399999".to_string())',
          setup="let ws: Vec<String> = (0..400_000).rev().map(|i| format!(\"a{i:06}\")).collect();\nlet refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect();\nlet idx = index(&refs);"),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4004);
            for _ in 0..300 {
                let n = rng.below(8);
                let words: Vec<String> = (0..n).map(|_| { let l = rng.below(3); rng.string(l, "abAB") }).collect();
                let refs: Vec<&str> = words.iter().map(|w| w.as_str()).collect();
                let mut want: std::collections::BTreeMap<char, Vec<String>> = std::collections::BTreeMap::new();
                for w in &words {
                    if let Some(c) = w.chars().next() {
                        want.entry(c.to_ascii_lowercase()).or_default().push(w.clone());
                    }
                }
                want.values_mut().for_each(|l| l.sort());
                check!(format!("words = {refs:?}"), index(&refs), want);
            }
        }
        """,
    ],
    hints=[("rust", "`BTreeMap` iterates in key order, so the index comes out alphabetised.")],
    notes=("Sorting each list once at the end is cheaper than inserting in order.", "O(n log n)", "O(n)"),
    follow_up="What would you change to index by the first Unicode grapheme instead of the first char?",
    related=["S2"],
    wrong=dict(
        lists_not_sorted=INDEX_WRONG.replace("FIRST", "if let Some(first) = w.chars().next()").replace("PUSH", "w.to_string()").replace("SORT", ""),
        lowercases_the_words=INDEX_WRONG.replace("FIRST", "if let Some(first) = w.chars().next()").replace("PUSH", "w.to_lowercase()").replace("SORT", "list.sort();"),
        indexes_empty_words=INDEX_WRONG.replace("FIRST", "if let Some(first) = Some(w.chars().next().unwrap_or(' '))").replace("PUSH", "w.to_string()").replace("SORT", "list.sort();"),
        inserts_in_sorted_place="""
            use std::collections::BTreeMap;

            pub fn index(words: &[&str]) -> BTreeMap<char, Vec<String>> {
                let mut idx: BTreeMap<char, Vec<String>> = BTreeMap::new();
                for w in words {
                    if let Some(first) = w.chars().next() {
                        let list = idx.entry(first.to_ascii_lowercase()).or_default();
                        let at = list.binary_search(&w.to_string()).unwrap_or_else(|at| at);
                        list.insert(at, w.to_string());
                    }
                }
                idx
            }
        """,
    ),
))

P.append(dict(
    slug="btreemap-ranges", title="BTreeMap range queries", level="medium", stage="understand-it", tags=["BTreeMap::range"],
    teaches=["`range(a..=b)` walks only the keys in the range, in order.", "`range` panics if start > end, so check first."],
    statement="`events` maps a timestamp to a name. Return the names with timestamps in `from..=to`, in time order. If `from > to`, return nothing.",
    starter="""
        use std::collections::BTreeMap;

        pub fn between(events: &BTreeMap<u32, String>, from: u32, to: u32) -> Vec<&str> {
            todo!()
        }
    """,
    solution="""
        use std::collections::BTreeMap;

        pub fn between(events: &BTreeMap<u32, String>, from: u32, to: u32) -> Vec<&str> {
            if from > to {
                return Vec::new();
            }
            events.range(from..=to).map(|(_, name)| name.as_str()).collect()
        }
    """,
    visible=[
        T("middle", "events at 1, 5, 9; from 2 to 9", "between(&m, 2, 9)", 'vec!["b", "c"]', setup='let m = std::collections::BTreeMap::from([(1, "a".to_string()), (5, "b".to_string()), (9, "c".to_string())]);'),
        T("none", "events at 1; from 2 to 3", "between(&m, 2, 3)", "Vec::<&str>::new()", setup='let m = std::collections::BTreeMap::from([(1, "a".to_string())]);'),
        T("reversed_bounds", "from 9 to 1", "between(&m, 9, 1)", "Vec::<&str>::new()", setup='let m = std::collections::BTreeMap::from([(5, "b".to_string())]);'),
        T("exact_bounds", "events at 1 and 9; from 1 to 9", "between(&m, 1, 9)", 'vec!["a", "c"]', setup='let m = std::collections::BTreeMap::from([(1, "a".to_string()), (9, "c".to_string())]);'),
        T("single_point", "events at 4, 5, 6; from 5 to 5", "between(&m, 5, 5)", 'vec!["b"]', setup='let m = std::collections::BTreeMap::from([(4, "a".to_string()), (5, "b".to_string()), (6, "c".to_string())]);'),
    ],
    hidden=[
        T("empty_map", "no events; from 0 to 10", "between(&m, 0, 10)", "Vec::<&str>::new()", setup="let m: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();"),
        T("everything", "events at 0, 7, u32::MAX; from 0 to u32::MAX", "between(&m, 0, u32::MAX)", 'vec!["a", "b", "c"]', setup='let m = std::collections::BTreeMap::from([(u32::MAX, "c".to_string()), (0, "a".to_string()), (7, "b".to_string())]);'),
        T("max_point", "event at u32::MAX; from u32::MAX to u32::MAX", "between(&m, u32::MAX, u32::MAX)", 'vec!["z"]', setup='let m = std::collections::BTreeMap::from([(u32::MAX, "z".to_string())]);'),
        T("zero_point", "event at 0; from 0 to 0", "between(&m, 0, 0)", 'vec!["z"]', setup='let m = std::collections::BTreeMap::from([(0, "z".to_string())]);'),
        T("after_everything", "events at 1, 2; from 3 to 9", "between(&m, 3, 9)", "Vec::<&str>::new()", setup='let m = std::collections::BTreeMap::from([(1, "a".to_string()), (2, "b".to_string())]);'),
        T("before_everything", "events at 5, 6; from 0 to 4", "between(&m, 0, 4)", "Vec::<&str>::new()", setup='let m = std::collections::BTreeMap::from([(5, "a".to_string()), (6, "b".to_string())]);'),
        T("unicode_names", "events at 1 → \"é\", 2 → \"日本\"", "between(&m, 1, 2)", 'vec!["é", "日本"]', setup='let m = std::collections::BTreeMap::from([(1, "é".to_string()), (2, "日本".to_string())]);'),
        T("borrows_the_map", "the result points into the map", "std::ptr::eq(between(&m, 1, 1)[0], m[&1].as_str())", "true", setup='let m = std::collections::BTreeMap::from([(1, "a".to_string())]);'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4005);
            for _ in 0..300 {
                let n = rng.below(8);
                let m: std::collections::BTreeMap<u32, String> = (0..n).map(|i| (rng.below(20) as u32, format!("e{i}"))).collect();
                let (from, to) = (rng.below(22) as u32, rng.below(22) as u32);
                let want: Vec<&str> = m.iter().filter(|(k, _)| from <= **k && **k <= to).map(|(_, v)| v.as_str()).collect();
                check!(format!("events = {m:?}, from {from} to {to}"), between(&m, from, to), want);
            }
        }

        #[test]
        fn scale_100k_queries() {
            let m: std::collections::BTreeMap<u32, String> = (0..200_000u32).map(|i| (i * 2, i.to_string())).collect();
            let mut total = 0;
            for q in 0..100_000u32 {
                total += between(&m, q * 4, q * 4 + 3).len();
            }
            check!("200000 events at even times; 100000 queries of width 4", total, 200_000);
        }
        """,
    ],
    hints=[("rust", "`BTreeMap::range` takes any range expression and yields entries in key order.")],
    notes=("A range query costs O(log n + k): find the start, then walk k entries. `range` panics on inverted bounds, hence the check.", "O(log n + k)", "O(k)"),
    follow_up="How would you find the latest event at or before a timestamp?",
    related=["D4"],
    wrong=dict(
        scan_every_event="""
            use std::collections::BTreeMap;

            pub fn between(events: &BTreeMap<u32, String>, from: u32, to: u32) -> Vec<&str> {
                events.iter().filter(|(t, _)| from <= **t && **t <= to).map(|(_, name)| name.as_str()).collect()
            }
        """,
        exclusive_end="""
            use std::collections::BTreeMap;

            pub fn between(events: &BTreeMap<u32, String>, from: u32, to: u32) -> Vec<&str> {
                if from > to {
                    return Vec::new();
                }
                events.range(from..to).map(|(_, name)| name.as_str()).collect()
            }
        """,
        no_bounds_check="""
            use std::collections::BTreeMap;

            pub fn between(events: &BTreeMap<u32, String>, from: u32, to: u32) -> Vec<&str> {
                events.range(from..=to).map(|(_, name)| name.as_str()).collect()
            }
        """,
    ),
))

HASH_WRONG = """
    use std::collections::HashSet;
    use std::hash::{Hash, Hasher};

    /// A username that compares case-insensitively.
    #[derive(DERIVES)]
    pub struct Username(pub String);

    impl PartialEq for Username {
        fn eq(&self, other: &Self) -> bool {
            EQ
        }
    }

    impl Eq for Username {}

    impl Hash for Username {
        fn hash<H: Hasher>(&self, state: &mut H) {
            HASH
        }
    }

    /// How many different usernames there are, ignoring case.
    pub fn distinct(names: &[&str]) -> usize {
        DISTINCT
    }
"""

P.append(dict(
    slug="fix-hash-eq-consistency", title="Fix: Hash that disagrees with Eq", mode="fix", level="medium", stage="understand-it", tags=["Hash", "Eq", "HashSet"],
    teaches=["If `a == b`, then `hash(a) == hash(b)` must hold.", "Deriving `Hash` next to a hand-written `PartialEq` breaks that."],
    statement="""
        `Username` compares case-insensitively, but a `HashSet<Username>` still counts `"Alice"` and
        `"alice"` as different. Fix `Username` so `distinct` is right. Keep the case-insensitive comparison.
    """,
    starter="""
        use std::collections::HashSet;
        use std::hash::{Hash, Hasher};

        /// A username that compares case-insensitively.
        #[derive(Debug, Clone, Hash)]
        pub struct Username(pub String);

        impl PartialEq for Username {
            fn eq(&self, other: &Self) -> bool {
                self.0.eq_ignore_ascii_case(&other.0)
            }
        }

        impl Eq for Username {}

        /// How many different usernames there are, ignoring case.
        pub fn distinct(names: &[&str]) -> usize {
            names.iter().map(|n| Username(n.to_string())).collect::<HashSet<_>>().len()
        }
    """,
    solution="""
        use std::collections::HashSet;
        use std::hash::{Hash, Hasher};

        /// A username that compares case-insensitively.
        #[derive(Debug, Clone)]
        pub struct Username(pub String);

        impl PartialEq for Username {
            fn eq(&self, other: &Self) -> bool {
                self.0.eq_ignore_ascii_case(&other.0)
            }
        }

        impl Eq for Username {}

        /// Hashes exactly what `eq` compares: the lowercase bytes.
        impl Hash for Username {
            fn hash<H: Hasher>(&self, state: &mut H) {
                for b in self.0.bytes() {
                    state.write_u8(b.to_ascii_lowercase());
                }
                state.write_u8(0xff);
            }
        }

        /// How many different usernames there are, ignoring case.
        pub fn distinct(names: &[&str]) -> usize {
            names.iter().map(|n| Username(n.to_string())).collect::<HashSet<_>>().len()
        }
    """,
    visible=[
        T("case_folds", "[\"Alice\", \"alice\", \"Bob\"]", 'distinct(&["Alice", "alice", "Bob"])', "2"),
        T("already_distinct", "[\"a\", \"b\"]", 'distinct(&["a", "b"])', "2"),
        T("many_cases", "[\"ADMIN\", \"admin\", \"Admin\", \"aDmIn\"]", 'distinct(&["ADMIN", "admin", "Admin", "aDmIn"])', "1"),
        T("empty", "[]", "distinct(&[])", "0"),
        T("prefix_is_different", "[\"ab\", \"A\"]", 'distinct(&["ab", "A"])', "2"),
    ],
    hidden=[
        T("single", "[\"x\"]", 'distinct(&["x"])', "1"),
        T("non_ascii_is_case_sensitive", "[\"É\", \"é\"]", 'distinct(&["É", "é"])', "2"),
        T("digits", "[\"User1\", \"USER1\", \"user2\"]", 'distinct(&["User1", "USER1", "user2"])', "2"),
        T("empty_names", "[\"\", \"\"]", 'distinct(&["", ""])', "1"),
        T("four_spellings", "[\"ab\", \"AB\", \"aB\", \"Ab\"]", 'distinct(&["ab", "AB", "aB", "Ab"])', "1"),
        T("set_lookup", "insert \"alice\", look up \"ALICE\"", '{ let set: std::collections::HashSet<Username> = [Username("alice".to_string())].into_iter().collect(); set.contains(&Username("ALICE".to_string())) }', "true"),
        T("equal_names_hash_equal", "hash \"Bob\" and \"bOB\"", '{ use std::hash::{BuildHasher, RandomState}; let s = RandomState::new(); s.hash_one(Username("Bob".to_string())) == s.hash_one(Username("bOB".to_string())) }', "true"),
        T("many", "1000 names in three spellings each", '{ let ws: Vec<String> = (0..1000).flat_map(|i| [format!("user{i}"), format!("USER{i}"), format!("User{i}")]).collect(); let refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect(); distinct(&refs) }', "1000"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4006);
            for _ in 0..300 {
                let n = rng.below(8);
                let names: Vec<String> = (0..n).map(|_| { let l = rng.below(3); rng.string(l, "aAbBé") }).collect();
                let refs: Vec<&str> = names.iter().map(|w| w.as_str()).collect();
                let want = names.iter().map(|w| w.to_ascii_lowercase()).collect::<std::collections::HashSet<_>>().len();
                check!(format!("names = {refs:?}"), distinct(&refs), want);
            }
        }

        #[test]
        fn scale_200k_same_length_names() {
            let names: Vec<String> = (0..200_000).map(|i| format!("user{i:06}")).collect();
            let refs: Vec<&str> = names.iter().map(|w| w.as_str()).collect();
            check!("200000 distinct names, all 10 bytes long", distinct(&refs), 200_000);
        }
        """,
    ],
    hints=[("approach", "Two usernames that are equal must hash the same. Does the derived `Hash` know about case?"),
           ("rust", "Implement `Hash` yourself and feed the hasher the same thing `eq` compares.")],
    notes=("The derived `Hash` hashes the original bytes, so equal values land in different buckets and never get compared. clippy flags this pattern (`derived_hash_with_manual_eq`) as a correctness error. The trailing `0xff` mirrors how `str` hashes, so adjacent fields can't run together.", "O(n · len)", "O(n)"),
    follow_up="What goes wrong in a BTreeMap if `Ord` disagrees with `Eq`?",
    related=["S8"],
    wrong=dict(
        hashes_the_length=HASH_WRONG.replace("DERIVES", "Debug, Clone").replace("EQ", "self.0.eq_ignore_ascii_case(&other.0)").replace("HASH", "self.0.len().hash(state);").replace("DISTINCT", "names.iter().map(|n| Username(n.to_string())).collect::<HashSet<_>>().len()"),
        case_sensitive_again=HASH_WRONG.replace("DERIVES", "Debug, Clone").replace("EQ", "self.0 == other.0").replace("HASH", "self.0.hash(state);").replace("DISTINCT", "names.iter().map(|n| Username(n.to_string())).collect::<HashSet<_>>().len()"),
        unicode_lowercase_in_distinct=HASH_WRONG.replace("DERIVES", "Debug, Clone").replace("EQ", "self.0.eq_ignore_ascii_case(&other.0)").replace("HASH", "self.0.to_ascii_lowercase().hash(state);").replace("DISTINCT", "names.iter().map(|n| n.to_lowercase()).collect::<HashSet<_>>().len()"),
    ),
))

LOOKUP_WRONG = """
    use std::collections::HashMap;

    pub struct Registry {
        ids: HashMap<String, u32>,
    }

    impl Registry {
        pub fn new(names: &[&str]) -> Self {
            let ids = names.iter().enumerate().map(|(i, n)| (String::from(*n), i as u32)).collect();
            Registry { ids }
        }

        /// The id for `name`. Called on every request, so it must not allocate.
        pub fn id(&self, name: &str) -> Option<u32> {
            BODY
        }
    }
"""

P.append(dict(
    slug="fix-borrowed-lookup", title="Fix: allocating on every lookup", mode="fix", level="medium", stage="understand-it", tags=["Borrow<str>", "HashMap::get"],
    teaches=["`HashMap<String, V>::get` accepts `&str` because `String: Borrow<str>`.", "Building a `String` just to look one up is an allocation per call."],
    statement="`Registry::id` runs on every request and allocates a `String` each time. Make it look the name up without allocating.",
    starter="""
        use std::collections::HashMap;

        pub struct Registry {
            ids: HashMap<String, u32>,
        }

        impl Registry {
            pub fn new(names: &[&str]) -> Self {
                let ids = names.iter().enumerate().map(|(i, n)| (String::from(*n), i as u32)).collect();
                Registry { ids }
            }

            /// The id for `name`. Called on every request, so it must not allocate.
            pub fn id(&self, name: &str) -> Option<u32> {
                self.ids.get(&name.to_string()).copied()
            }
        }
    """,
    solution="""
        use std::collections::HashMap;

        pub struct Registry {
            ids: HashMap<String, u32>,
        }

        impl Registry {
            pub fn new(names: &[&str]) -> Self {
                let ids = names.iter().enumerate().map(|(i, n)| (String::from(*n), i as u32)).collect();
                Registry { ids }
            }

            /// The id for `name`. Called on every request, so it must not allocate.
            pub fn id(&self, name: &str) -> Option<u32> {
                self.ids.get(name).copied()
            }
        }
    """,
    rules=dict(methods=["to_string", "to_owned", "into", "clone"], lines=1),
    visible=[
        T("found", "names = [\"a\", \"b\"], id(\"b\")", 'Registry::new(&["a", "b"]).id("b")', "Some(1)"),
        T("missing", "names = [\"a\"], id(\"z\")", 'Registry::new(&["a"]).id("z")', "None"),
        T("first", "names = [\"x\", \"y\"], id(\"x\")", 'Registry::new(&["x", "y"]).id("x")', "Some(0)"),
        T("case_sensitive", "names = [\"A\"], id(\"a\")", 'Registry::new(&["A"]).id("a")', "None"),
        T("empty_registry", "names = [], id(\"a\")", 'Registry::new(&[]).id("a")', "None"),
    ],
    hidden=[
        T("empty_name", "names = [\"\"], id(\"\")", 'Registry::new(&[""]).id("")', "Some(0)"),
        T("later_duplicate_wins", "names = [\"a\", \"a\"], id(\"a\")", 'Registry::new(&["a", "a"]).id("a")', "Some(1)"),
        T("unicode", "names = [\"x\", \"日本\"], id(\"日本\")", 'Registry::new(&["x", "日本"]).id("日本")', "Some(1)"),
        T("prefix_is_not_a_match", "names = [\"abc\"], id(\"ab\")", 'Registry::new(&["abc"]).id("ab")', "None"),
        T("trailing_space", "names = [\"a\"], id(\"a \")", 'Registry::new(&["a"]).id("a ")', "None"),
        T("many", "10000 names, id(\"n9999\")", '{ let ws: Vec<String> = (0..10_000).map(|i| format!("n{i}")).collect(); let refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect(); Registry::new(&refs).id("n9999") }', "Some(9999)"),
        """
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::Cell;

        // Counts allocations made on the current thread, so the test below can check `id` makes none.
        struct CountingAlloc;

        thread_local! {
            static ALLOCS: Cell<usize> = const { Cell::new(0) };
        }

        unsafe impl GlobalAlloc for CountingAlloc {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
                unsafe { System.alloc(layout) }
            }

            unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
                unsafe { System.dealloc(ptr, layout) }
            }
        }

        #[global_allocator]
        static GLOBAL: CountingAlloc = CountingAlloc;

        #[test]
        fn lookups_do_not_allocate() {
            let r = Registry::new(&["alpha", "beta"]);
            let before = ALLOCS.with(|n| n.get());
            let mut answered = 0;
            for _ in 0..1000 {
                if r.id("beta") == Some(1) {
                    answered += 1;
                }
                if r.id("gamma").is_none() {
                    answered += 1;
                }
            }
            let allocations = ALLOCS.with(|n| n.get()) - before;
            check!("1000 × id(\\"beta\\") and id(\\"gamma\\"); count allocations", (answered, allocations), (2000, 0));
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4007);
            for _ in 0..300 {
                let n = rng.below(6);
                let names: Vec<String> = (0..n).map(|_| { let l = rng.below(3); rng.string(l, "ab") }).collect();
                let refs: Vec<&str> = names.iter().map(|w| w.as_str()).collect();
                let r = Registry::new(&refs);
                let l = rng.below(3);
                let q = rng.string(l, "ab");
                let want = refs.iter().rposition(|&w| w == q).map(|i| i as u32);
                check!(format!("names = {refs:?}, id({q:?})"), r.id(&q), want);
            }
        }

        #[test]
        fn scale_200k_lookups() {
            let ws: Vec<String> = (0..200_000).map(|i| format!("n{i}")).collect();
            let refs: Vec<&str> = ws.iter().map(|w| w.as_str()).collect();
            let r = Registry::new(&refs);
            let total: u64 = refs.iter().rev().map(|&w| u64::from(r.id(w).unwrap_or(0))).sum();
            check!("200000 names, look each one up", total, 19_999_900_000);
        }
        """,
    ],
    hints=[("rust", "Look at the signature of `HashMap::get`: `fn get<Q>(&self, k: &Q) where K: Borrow<Q>`.")],
    notes=("`String: Borrow<str>` and the two hash identically, so the map can be queried with a `&str` directly. That's what the `Borrow` bound on `get` is for.", "O(len)", "O(1)"),
    follow_up="Why does `Borrow` require `Hash`, `Eq` and `Ord` to agree between the owned and borrowed forms?",
    related=["S8", "S2"],
    wrong=dict(
        linear_scan=LOOKUP_WRONG.replace("BODY", "self.ids.iter().find(|(k, _)| k.as_str() == name).map(|(_, &v)| v)"),
        string_from=LOOKUP_WRONG.replace("BODY", "self.ids.get(&String::from(name)).copied()"),
    ),
))

P.append(dict(
    slug="deterministic-report", title="A deterministic report from a HashMap", level="medium", stage="understand-it", tags=["HashMap order", "sort_by"],
    teaches=["`HashMap` iteration order is unspecified and changes between runs.", "Collect, then sort with an explicit key."],
    statement="Format each entry as `\"name: score\"`, highest score first, ties by name ascending.",
    starter="""
        use std::collections::HashMap;

        pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
            todo!()
        }
    """,
    solution="""
        use std::collections::HashMap;

        pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
            let mut rows: Vec<(&String, &u32)> = scores.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            rows.into_iter().map(|(name, score)| format!("{name}: {score}")).collect()
        }
    """,
    visible=[
        T("ordered", "{ann: 5, bob: 9, cy: 5}", 'report(&std::collections::HashMap::from([("ann".to_string(), 5), ("bob".to_string(), 9), ("cy".to_string(), 5)]))', 'vec!["bob: 9", "ann: 5", "cy: 5"]'),
        T("empty", "{}", "report(&std::collections::HashMap::new())", "Vec::<String>::new()"),
        T("many_ties", "{d: 1, c: 1, b: 1, a: 1}", 'report(&std::collections::HashMap::from([("d".to_string(), 1), ("c".to_string(), 1), ("b".to_string(), 1), ("a".to_string(), 1)]))', 'vec!["a: 1", "b: 1", "c: 1", "d: 1"]'),
        T("single", "{x: 0}", 'report(&std::collections::HashMap::from([("x".to_string(), 0)]))', 'vec!["x: 0"]'),
        T("highest_first", "{a: 1, b: 2}", 'report(&std::collections::HashMap::from([("a".to_string(), 1), ("b".to_string(), 2)]))', 'vec!["b: 2", "a: 1"]'),
    ],
    hidden=[
        T("u32_max", "{a: u32::MAX, b: 0}", 'report(&std::collections::HashMap::from([("b".to_string(), 0), ("a".to_string(), u32::MAX)]))', 'vec!["a: 4294967295", "b: 0"]'),
        T("names_compare_bytewise", "{B: 1, a: 1}", 'report(&std::collections::HashMap::from([("a".to_string(), 1), ("B".to_string(), 1)]))', 'vec!["B: 1", "a: 1"]'),
        T("unicode_names", "{é: 1, e: 1}", 'report(&std::collections::HashMap::from([("é".to_string(), 1), ("e".to_string(), 1)]))', 'vec!["e: 1", "é: 1"]'),
        T("empty_name", "{\"\": 3}", 'report(&std::collections::HashMap::from([(String::new(), 3)]))', 'vec![": 3"]'),
        T("digits_are_text", "{a10: 1, a9: 1}", 'report(&std::collections::HashMap::from([("a9".to_string(), 1), ("a10".to_string(), 1)]))', 'vec!["a10: 1", "a9: 1"]'),
        T("score_beats_name", "{a: 1, z: 2}", 'report(&std::collections::HashMap::from([("a".to_string(), 1), ("z".to_string(), 2)]))', 'vec!["z: 2", "a: 1"]'),
        T("many", "1000 players, scores i % 10", '{ let m: std::collections::HashMap<String, u32> = (0..1000).map(|i| (format!("p{i:03}"), i % 10)).collect(); let r = report(&m); (r.len(), r[0].clone(), r[999].clone()) }', '(1000, "p009: 9".to_string(), "p990: 0".to_string())'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4008);
            for _ in 0..300 {
                let n = rng.below(8);
                let m: std::collections::HashMap<String, u32> = (0..n).map(|_| { let l = 1 + rng.below(2); (rng.string(l, "abB"), rng.below(4) as u32) }).collect();
                let mut rows: Vec<(String, u32)> = m.iter().map(|(k, &v)| (k.clone(), v)).collect();
                rows.sort_by_key(|(k, v)| (std::cmp::Reverse(*v), k.clone()));
                let want: Vec<String> = rows.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                let mut sorted: Vec<_> = m.iter().collect();
                sorted.sort();
                check!(format!("scores = {sorted:?}"), report(&m), want);
            }
        }

        #[test]
        fn scale_200k() {
            let m: std::collections::HashMap<String, u32> = (0..200_000u32).map(|i| (format!("p{i:06}"), i % 1000)).collect();
            let r = report(&m);
            check!("200000 players, scores i % 1000", (r.len(), r[0].clone(), r[199_999].clone()), (200_000, "p000999: 999".to_string(), "p199000: 0".to_string()));
        }
        """,
    ],
    hints=[("rust", "Collect `(&String, &u32)` pairs into a Vec and sort it; no need to clone the names first.")],
    notes=("Sorting references avoids cloning keys until the final `format!`.", "O(n log n)", "O(n)"),
    follow_up="Why does std's HashMap randomise its hash seed?",
    related=["S3"],
    wrong=dict(
        ties_descending="""
            use std::collections::HashMap;

            pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
                let mut rows: Vec<(&String, &u32)> = scores.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| b.0.cmp(a.0)));
                rows.into_iter().map(|(name, score)| format!("{name}: {score}")).collect()
            }
        """,
        lowest_first="""
            use std::collections::HashMap;

            pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
                let mut rows: Vec<(&String, &u32)> = scores.iter().collect();
                rows.sort_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)));
                rows.into_iter().map(|(name, score)| format!("{name}: {score}")).collect()
            }
        """,
        pick_the_best_each_time="""
            use std::collections::HashMap;

            pub fn report(scores: &HashMap<String, u32>) -> Vec<String> {
                let mut left: Vec<(&String, &u32)> = scores.iter().collect();
                let mut out = Vec::new();
                while !left.is_empty() {
                    let mut best = 0;
                    for i in 1..left.len() {
                        let (a, b) = (left[i], left[best]);
                        if a.1 > b.1 || (a.1 == b.1 && a.0 < b.0) {
                            best = i;
                        }
                    }
                    let (name, score) = left.remove(best);
                    out.push(format!("{name}: {score}"));
                }
                out
            }
        """,
    ),
))

NEAREST_WRONG = """
    use std::collections::BTreeSet;

    pub fn nearest(prices: &BTreeSet<u32>, x: u32) -> (Option<u32>, Option<u32>) {
        let floor = FLOOR;
        let ceiling = CEIL;
        (floor, ceiling)
    }
"""

P.append(dict(
    slug="floor-and-ceiling", title="Floor and ceiling with range", level="medium", stage="understand-it", tags=["BTreeSet", "range", "next_back"],
    teaches=["`range(..=x).next_back()` is the floor; `range(x..).next()` is the ceiling."],
    statement="Return the largest price ≤ `x` and the smallest price ≥ `x`, each `None` if there isn't one.",
    starter="""
        use std::collections::BTreeSet;

        pub fn nearest(prices: &BTreeSet<u32>, x: u32) -> (Option<u32>, Option<u32>) {
            todo!()
        }
    """,
    solution="""
        use std::collections::BTreeSet;

        pub fn nearest(prices: &BTreeSet<u32>, x: u32) -> (Option<u32>, Option<u32>) {
            let floor = prices.range(..=x).next_back().copied();
            let ceiling = prices.range(x..).next().copied();
            (floor, ceiling)
        }
    """,
    visible=[
        T("between", "prices = {10, 20, 30}, x = 25", "nearest(&std::collections::BTreeSet::from([10, 20, 30]), 25)", "(Some(20), Some(30))"),
        T("exact", "prices = {10, 20}, x = 20", "nearest(&std::collections::BTreeSet::from([10, 20]), 20)", "(Some(20), Some(20))"),
        T("below_all", "prices = {10}, x = 3", "nearest(&std::collections::BTreeSet::from([10]), 3)", "(None, Some(10))"),
        T("above_all", "prices = {10, 20}, x = 25", "nearest(&std::collections::BTreeSet::from([10, 20]), 25)", "(Some(20), None)"),
        T("empty", "prices = {}, x = 3", "nearest(&std::collections::BTreeSet::new(), 3)", "(None, None)"),
    ],
    hidden=[
        T("zero", "prices = {0}, x = 0", "nearest(&std::collections::BTreeSet::from([0]), 0)", "(Some(0), Some(0))"),
        T("max_in_set", "prices = {5, u32::MAX}, x = u32::MAX", "nearest(&std::collections::BTreeSet::from([5, u32::MAX]), u32::MAX)", "(Some(u32::MAX), Some(u32::MAX))"),
        T("max_not_in_set", "prices = {5}, x = u32::MAX", "nearest(&std::collections::BTreeSet::from([5]), u32::MAX)", "(Some(5), None)"),
        T("zero_not_in_set", "prices = {5}, x = 0", "nearest(&std::collections::BTreeSet::from([5]), 0)", "(None, Some(5))"),
        T("single_equal", "prices = {7}, x = 7", "nearest(&std::collections::BTreeSet::from([7]), 7)", "(Some(7), Some(7))"),
        T("adjacent", "prices = {1, 2}, x = 1", "nearest(&std::collections::BTreeSet::from([1, 2]), 1)", "(Some(1), Some(1))"),
        T("just_above_one", "prices = {10, 20, 30}, x = 11", "nearest(&std::collections::BTreeSet::from([10, 20, 30]), 11)", "(Some(10), Some(20))"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4009);
            for _ in 0..300 {
                let n = rng.below(8);
                let prices: std::collections::BTreeSet<u32> = (0..n).map(|_| rng.below(20) as u32).collect();
                let x = rng.below(21) as u32;
                let want = (prices.iter().copied().filter(|&p| p <= x).max(), prices.iter().copied().filter(|&p| p >= x).min());
                check!(format!("prices = {prices:?}, x = {x}"), nearest(&prices, x), want);
            }
        }

        #[test]
        fn scale_100k_queries() {
            let prices: std::collections::BTreeSet<u32> = (0..200_000u32).map(|i| i * 2).collect();
            let mut total = 0u64;
            for q in 0..100_000u32 {
                let (f, c) = nearest(&prices, q * 2 + 1);
                total += u64::from(f.unwrap_or(0)) + u64::from(c.unwrap_or(0));
            }
            check!("200000 even prices; 100000 odd queries", total, 20_000_000_000);
        }
        """,
    ],
    hints=[("rust", "A range iterator is double-ended: `next_back` gives the last element of `..=x`.")],
    notes=("Both queries are O(log n) and neither scans the set.", "O(log n)", "O(1)"),
    follow_up="How would you find the k nearest prices to x?",
    related=["D4", "D14"],
    wrong=dict(
        scan_the_set=NEAREST_WRONG.replace("FLOOR", "prices.iter().copied().filter(|&p| p <= x).max()").replace("CEIL", "prices.iter().copied().filter(|&p| p >= x).min()"),
        strict_bounds=NEAREST_WRONG.replace("FLOOR", "prices.range(..x).next_back().copied()").replace("CEIL", "prices.range(x + 1..).next().copied()"),
        swapped=NEAREST_WRONG.replace("FLOOR", "prices.range(x..).next().copied()").replace("CEIL", "prices.range(..=x).next_back().copied()"),
    ),
))

P.append(dict(
    slug="open-addressing-map", title="A hash map with open addressing", level="hard", stage="build-it", tags=["linear probing", "tombstones"],
    teaches=["Linear probing, and why removal needs tombstones.", "Resizing before the table gets too full."],
    statement="""
        Build `OpenMap<V>`, a hash map from `u64` keys to `V`, on one `Vec<Slot<V>>` with linear probing.
        Implement `new`, `insert` (returning the old value if the key existed), `get`, `remove` and `len`.
        Removing must not break lookups of keys further along a probe chain.

        Count tombstones as used. After every call, at least a quarter of the slots must be `Empty`, or a
        lookup of a missing key could walk the whole table. Double when the table gets fuller than that, and
        drop the tombstones whenever you rebuild. Under insert-then-remove churn, rebuilding at the same size
        is enough: the slot count must stay at most `16 * max(largest len so far, 1)`.

        `capacity()` returns the number of slots and `empty_slots()` the number of `Empty` ones. The tests
        use them to check both rules.
    """,
    starter="""
        enum Slot<V> {
            Empty,
            Deleted,
            Full(u64, V),
        }

        pub struct OpenMap<V> {
            slots: Vec<Slot<V>>,
            len: usize,
        }

        impl<V> OpenMap<V> {
            pub fn new() -> Self {
                todo!()
            }

            pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
                todo!()
            }

            pub fn get(&self, key: u64) -> Option<&V> {
                todo!()
            }

            pub fn remove(&mut self, key: u64) -> Option<V> {
                todo!()
            }

            pub fn len(&self) -> usize {
                todo!()
            }

            /// The number of slots.
            pub fn capacity(&self) -> usize {
                todo!()
            }

            /// The number of `Empty` slots (neither full nor a tombstone).
            pub fn empty_slots(&self) -> usize {
                todo!()
            }
        }
    """,
    solution="""
        enum Slot<V> {
            Empty,
            Deleted,
            Full(u64, V),
        }

        pub struct OpenMap<V> {
            slots: Vec<Slot<V>>,
            len: usize,
            /// Full plus Deleted slots: what probe chains have to walk past.
            used: usize,
        }

        impl<V> OpenMap<V> {
            pub fn new() -> Self {
                Self::with_slots(8)
            }

            fn with_slots(n: usize) -> Self {
                OpenMap { slots: (0..n).map(|_| Slot::Empty).collect(), len: 0, used: 0 }
            }

            /// Fibonacci hashing: multiply, then take the high bits.
            fn home(&self, key: u64) -> usize {
                let bits = self.slots.len().trailing_zeros();
                (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - bits)) as usize
            }

            fn find(&self, key: u64) -> Option<usize> {
                let mask = self.slots.len() - 1;
                let mut i = self.home(key);
                for _ in 0..self.slots.len() {
                    match &self.slots[i] {
                        Slot::Empty => return None,
                        Slot::Full(k, _) if *k == key => return Some(i),
                        _ => i = (i + 1) & mask,
                    }
                }
                None
            }

            pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
                if let Some(i) = self.find(key) {
                    if let Slot::Full(_, v) = &mut self.slots[i] {
                        return Some(std::mem::replace(v, value));
                    }
                }
                if (self.used + 1) * 4 > self.slots.len() * 3 {
                    self.grow();
                }
                let mask = self.slots.len() - 1;
                let mut i = self.home(key);
                // The key isn't present, so the first Empty or Deleted slot is where it goes.
                while let Slot::Full(..) = self.slots[i] {
                    i = (i + 1) & mask;
                }
                if let Slot::Empty = self.slots[i] {
                    self.used += 1;
                }
                self.slots[i] = Slot::Full(key, value);
                self.len += 1;
                None
            }

            pub fn get(&self, key: u64) -> Option<&V> {
                match &self.slots[self.find(key)?] {
                    Slot::Full(_, v) => Some(v),
                    _ => None,
                }
            }

            pub fn remove(&mut self, key: u64) -> Option<V> {
                let i = self.find(key)?;
                // A tombstone, not Empty: later keys in this probe chain must stay reachable.
                match std::mem::replace(&mut self.slots[i], Slot::Deleted) {
                    Slot::Full(_, v) => {
                        self.len -= 1;
                        Some(v)
                    }
                    _ => None,
                }
            }

            pub fn len(&self) -> usize {
                self.len
            }

            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn capacity(&self) -> usize {
                self.slots.len()
            }

            pub fn empty_slots(&self) -> usize {
                self.slots.len() - self.used
            }

            /// Rebuilds the table without tombstones. It doubles only when live keys fill more than half
            /// the slots; otherwise the slots are mostly tombstones, and a rebuild at the same size is
            /// enough. Without that, insert-then-remove churn would double the table forever.
            fn grow(&mut self) {
                let n = self.slots.len();
                let new_n = if (self.len + 1) * 2 > n { n * 2 } else { n };
                let old = std::mem::replace(self, Self::with_slots(new_n));
                for slot in old.slots {
                    if let Slot::Full(k, v) = slot {
                        self.insert(k, v);
                    }
                }
            }
        }

        impl<V> Default for OpenMap<V> {
            fn default() -> Self {
                Self::new()
            }
        }
    """,
    visible=[
        T("insert_get", "insert 1 → \"a\", 2 → \"b\"", '{ let mut m = OpenMap::new(); m.insert(1, "a"); m.insert(2, "b"); (m.get(1).copied(), m.get(3).copied(), m.len()) }', '(Some("a"), None, 2)'),
        T("replace", "insert 7 → 1 then 7 → 2", "{ let mut m = OpenMap::new(); let first = m.insert(7, 1); let second = m.insert(7, 2); (first, second, m.get(7).copied(), m.len()) }", "(None, Some(1), Some(2), 1)"),
        T("remove", "insert 5, remove 5", "{ let mut m = OpenMap::new(); m.insert(5, 'x'); (m.remove(5), m.get(5).copied(), m.remove(5), m.len()) }", "(Some('x'), None, None, 0)"),
        T("remove_missing", "remove 9 from an empty map", "{ let mut m: OpenMap<u8> = OpenMap::new(); (m.remove(9), m.len(), m.get(9).copied()) }", "(None, 0, None)"),
        T("reinsert_after_remove", "insert 3; remove 3; insert 3 again", '{ let mut m = OpenMap::new(); m.insert(3, "a"); m.remove(3); (m.insert(3, "b"), m.get(3).copied(), m.len()) }', '(None, Some("b"), 1)'),
        """
        #[test]
        fn a_quarter_of_the_slots_stay_empty() {
            let mut m = OpenMap::new();
            let mut always = true;
            for k in 0..3u64 {
                m.insert(k, k);
            }
            for k in 3..200u64 {
                m.insert(k, k);
                always &= m.empty_slots() * 4 >= m.capacity();
                m.remove(k);
                always &= m.empty_slots() * 4 >= m.capacity();
            }
            check!("keep 3 keys, insert and remove 197 more: empty_slots() * 4 >= capacity() after every call", (always, m.len()), (true, 3));
        }
        """,
    ],
    hidden=[
        """
        #[test]
        fn ten_thousand_keys_survive_resizes() {
            let mut m = OpenMap::new();
            for k in 0..10_000u64 {
                m.insert(k * 7919, k);
            }
            let all_found = (0..10_000u64).all(|k| m.get(k * 7919) == Some(&k));
            check!("insert 10000 keys", (m.len(), all_found), (10_000, true));
        }

        #[test]
        fn remove_keeps_probe_chains_intact() {
            let mut m = OpenMap::new();
            for k in 0..100u64 {
                m.insert(k, k);
            }
            for k in (0..100u64).step_by(2) {
                m.remove(k);
            }
            let odd_found = (1..100u64).step_by(2).all(|k| m.get(k) == Some(&k));
            let even_gone = (0..100u64).step_by(2).all(|k| m.get(k).is_none());
            check!("insert 0..100, remove the even keys", (m.len(), odd_found, even_gone), (50, true, true));
        }

        #[test]
        fn churn_reuses_tombstones() {
            let mut m = OpenMap::new();
            for round in 0..1000u64 {
                m.insert(round, round);
                m.remove(round);
            }
            m.insert(42, 1);
            check!("1000 insert/remove rounds", (m.len(), m.get(42).copied()), (1, Some(1)));
        }

        #[test]
        fn extreme_keys() {
            let mut m = OpenMap::new();
            m.insert(0, "zero");
            m.insert(u64::MAX, "max");
            check!("insert 0 and u64::MAX", (m.get(0).copied(), m.get(u64::MAX).copied(), m.get(1).copied(), m.len()), (Some("zero"), Some("max"), None, 2));
        }

        #[test]
        fn owned_values_come_back() {
            let mut m = OpenMap::new();
            m.insert(1, String::from("one"));
            let old = m.insert(1, String::from("uno"));
            check!("insert 1 → \\"one\\", then 1 → \\"uno\\", then remove 1", (old, m.remove(1), m.len()), (Some("one".to_string()), Some("uno".to_string()), 0));
        }

        #[test]
        fn replace_after_resizes() {
            let mut m = OpenMap::new();
            for k in 0..100u64 {
                m.insert(k, k);
            }
            check!("insert 0..100, then insert 50 → 0", (m.insert(50, 0), m.get(50).copied(), m.len()), (Some(50), Some(0), 100));
        }

        #[test]
        fn reinsert_past_tombstones() {
            let mut m = OpenMap::new();
            for k in 0..100u64 {
                m.insert(k, k);
            }
            for k in (0..100u64).step_by(2) {
                m.remove(k);
            }
            let all_replaced = (1..100u64).step_by(2).all(|k| m.insert(k, k + 1000) == Some(k));
            let all_new = (1..100u64).step_by(2).all(|k| m.get(k) == Some(&(k + 1000)));
            check!("insert 0..100, remove the even keys, insert every odd key again", (m.len(), all_replaced, all_new), (50, true, true));
        }

        #[test]
        fn remove_everything() {
            let mut m = OpenMap::new();
            for k in 0..1000u64 {
                m.insert(k * 3, k);
            }
            let all_removed = (0..1000u64).all(|k| m.remove(k * 3) == Some(k));
            m.insert(5, 5);
            check!("insert 1000 keys, remove them all, insert 5", (all_removed, m.len(), m.get(0).copied(), m.get(5).copied()), (true, 1, None, Some(5)));
        }

        #[test]
        fn random_vs_hashmap() {
            let mut rng = anneal_prelude::Rng::new(4010);
            for _ in 0..200 {
                let mut m = OpenMap::new();
                let mut model = std::collections::HashMap::new();
                let mut log = Vec::new();
                let n = rng.below(40);
                for _ in 0..n {
                    let k = rng.below(12) as u64;
                    match rng.below(3) {
                        0 | 1 => {
                            let v = rng.below(100) as u32;
                            log.push(format!("insert {k} → {v}"));
                            check!(log.join(", "), m.insert(k, v), model.insert(k, v));
                        }
                        _ => {
                            log.push(format!("remove {k}"));
                            check!(log.join(", "), m.remove(k), model.remove(&k));
                        }
                    }
                    let q = rng.below(12) as u64;
                    check!(format!("{}; get {q}", log.join(", ")), (m.get(q).copied(), m.len()), (model.get(&q).copied(), model.len()));
                }
            }
        }

        #[test]
        fn scale_200k() {
            let mut m = OpenMap::new();
            for k in 0..200_000u64 {
                m.insert(k * 13, k);
            }
            let found = (0..200_000u64).filter(|&k| m.get(k * 13) == Some(&k)).count();
            let removed = (0..200_000u64).step_by(2).filter(|&k| m.remove(k * 13).is_some()).count();
            check!("insert 200000 keys, get each, remove half", (found, removed, m.len()), (200_000, 100_000, 100_000));
        }

        fn room_left<V>(m: &OpenMap<V>) -> bool {
            m.empty_slots() * 4 >= m.capacity()
        }

        #[test]
        fn tombstones_count_toward_the_load() {
            let mut m = OpenMap::new();
            for k in 0..10u64 {
                m.insert(k, k);
            }
            let mut first_bad = None;
            for k in 10..20_010u64 {
                m.insert(k, k);
                let after_insert = room_left(&m);
                m.remove(k);
                if first_bad.is_none() && !(after_insert && room_left(&m)) {
                    first_bad = Some(k);
                }
            }
            check!("keep 10 keys, insert and remove 20000 more: first key after which fewer than a quarter of the slots are Empty", (first_bad, m.len()), (None, 10));
        }

        #[test]
        fn churn_does_not_grow_the_table() {
            let mut m = OpenMap::new();
            for k in 0..100_000u64 {
                m.insert(k, k);
                m.remove(k);
            }
            let small = m.capacity() <= 16;
            check!("100000 rounds of insert k, remove k (largest len 1): capacity() <= 16, and a quarter empty", (small, room_left(&m), m.len()), (true, true, 0));
        }

        #[test]
        fn capacity_follows_the_largest_len() {
            let mut m = OpenMap::new();
            for k in 0..1000u64 {
                m.insert(k, k);
            }
            for k in 0..1000u64 {
                m.remove(k);
            }
            for k in 1000..50_000u64 {
                m.insert(k, k);
                m.remove(k);
            }
            let bounded = m.capacity() <= 16 * 1000;
            check!("insert 1000 keys, remove them, then 49000 insert/remove rounds: capacity() <= 16000, and a quarter empty", (bounded, room_left(&m), m.len()), (true, true, 0));
        }

        #[test]
        fn random_ops_keep_both_rules() {
            let mut rng = anneal_prelude::Rng::new(5101);
            for _ in 0..50 {
                let mut m = OpenMap::new();
                let mut peak = 0usize;
                let mut ok = true;
                let keys = 1 + rng.below(40) as u64;
                for _ in 0..400 {
                    let k = rng.below(keys as usize) as u64;
                    if rng.below(2) == 0 {
                        m.insert(k, k);
                    } else {
                        m.remove(k);
                    }
                    peak = peak.max(m.len());
                    ok &= room_left(&m) && m.capacity() <= 16 * peak.max(1);
                }
                check!(format!("400 random inserts and removes over {keys} keys: both rules after every call"), ok, true);
            }
        }
        """,
    ],
    hints=[("approach", "A key lives at its home slot or further along; a lookup stops at the first Empty slot."),
           ("approach", "Removing by writing Empty would cut the chain for keys that probed past it. Write a tombstone instead."),
           ("rust", "`std::mem::replace(self, Self::with_slots(new_n))` hands you the old table to reinsert from. Test a `used` count (Full plus Deleted), not `len`, before each insert, and double only if the live keys need it.")],
    notes=("Lookups skip tombstones; inserts reuse them. Counting tombstones toward the load factor stops a table full of tombstones from making every miss scan everything. Resizing drops the tombstones. When most used slots are tombstones, rebuild at the same size instead of doubling: otherwise insert-then-remove churn doubles the table forever while `len` stays at 1.", "O(1) expected", "O(n)"),
    follow_up="How does Robin Hood hashing or backward-shift deletion avoid tombstones?",
    related=["D14", "S8"],
))
# Wrong solutions are variations of the reference.
_open = P[-1]["solution"]
_insert_start, _insert_end = _open.index("            pub fn insert("), _open.index("            pub fn get(")
P[-1]["wrong"] = dict(
    ignores_tombstones_in_load=_open.replace("if (self.used + 1) * 4 > self.slots.len() * 3 {", "if (self.len + 1) * 4 > self.slots.len() * 3 {  // Wrong: tombstones don't count."),
    doubles_on_every_rebuild=_open.replace("let new_n = if (self.len + 1) * 2 > n { n * 2 } else { n };", "let new_n = n * 2;  // Wrong: churn doubles the table forever."),
    empty_on_remove=_open.replace("std::mem::replace(&mut self.slots[i], Slot::Deleted)", "std::mem::replace(&mut self.slots[i], Slot::Empty)"),
    insert_stops_at_tombstone=_open[:_insert_start] + """            pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
                if (self.used + 1) * 4 > self.slots.len() * 3 {
                    self.grow();
                }
                let mask = self.slots.len() - 1;
                let mut i = self.home(key);
                // Stops at the key, or at the first slot that isn't Full.
                loop {
                    match &mut self.slots[i] {
                        Slot::Full(k, v) if *k == key => return Some(std::mem::replace(v, value)),
                        Slot::Full(..) => i = (i + 1) & mask,
                        _ => break,
                    }
                }
                if let Slot::Empty = self.slots[i] {
                    self.used += 1;
                }
                self.slots[i] = Slot::Full(key, value);
                self.len += 1;
                None
            }

""" + _open[_insert_end:],
    vec_of_pairs="""
        pub struct OpenMap<V> {
            items: Vec<(u64, V)>,
        }

        impl<V> OpenMap<V> {
            pub fn new() -> Self {
                OpenMap { items: Vec::new() }
            }

            pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
                if let Some((_, v)) = self.items.iter_mut().find(|(k, _)| *k == key) {
                    return Some(std::mem::replace(v, value));
                }
                self.items.push((key, value));
                None
            }

            pub fn get(&self, key: u64) -> Option<&V> {
                self.items.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
            }

            pub fn remove(&mut self, key: u64) -> Option<V> {
                let i = self.items.iter().position(|(k, _)| *k == key)?;
                Some(self.items.swap_remove(i).1)
            }

            pub fn len(&self) -> usize {
                self.items.len()
            }

            pub fn capacity(&self) -> usize {
                self.items.capacity().max(8)
            }

            pub fn empty_slots(&self) -> usize {
                self.capacity() - self.items.len()
            }
        }
    """,
)

INTERVAL_WRONG = """
    use std::collections::BTreeMap;

    pub struct IntervalMap<V> {
        by_start: BTreeMap<u32, (u32, V)>,
    }

    impl<V> IntervalMap<V> {
        pub fn new() -> Self {
            IntervalMap { by_start: BTreeMap::new() }
        }

        pub fn insert(&mut self, start: u32, end: u32, value: V) -> Result<(), V> {
            if start >= end {
                return Err(value);
            }
            let before_overlaps = self.by_start.range(..=start).next_back().is_some_and(|(_, (e, _))| BEFORE);
            let after_overlaps = AFTER;
            if before_overlaps || after_overlaps {
                return Err(value);
            }
            self.by_start.insert(start, (end, value));
            Ok(())
        }

        pub fn get(&self, point: u32) -> Option<&V> {
            self.by_start.range(..=point).next_back().filter(|(_, (end, _))| GET).map(|(_, (_, v))| v)
        }
    }
"""

P.append(dict(
    slug="interval-map", title="An interval map on BTreeMap", level="hard", stage="build-it", tags=["BTreeMap::range", "intervals"],
    teaches=["Keying intervals by their start makes point lookup a floor query.", "Returning the value back in `Err` when an insert is refused."],
    statement="""
        `IntervalMap<V>` stores non-overlapping half-open intervals `[start, end)`. `insert` refuses empty
        or overlapping intervals and hands the value back in `Err`. `get(point)` returns the value of the
        interval containing `point`.
    """,
    starter="""
        use std::collections::BTreeMap;

        pub struct IntervalMap<V> {
            /// start → (end, value)
            by_start: BTreeMap<u32, (u32, V)>,
        }

        impl<V> IntervalMap<V> {
            pub fn new() -> Self {
                todo!()
            }

            pub fn insert(&mut self, start: u32, end: u32, value: V) -> Result<(), V> {
                todo!()
            }

            pub fn get(&self, point: u32) -> Option<&V> {
                todo!()
            }
        }
    """,
    solution="""
        use std::collections::BTreeMap;

        pub struct IntervalMap<V> {
            /// start → (end, value)
            by_start: BTreeMap<u32, (u32, V)>,
        }

        impl<V> IntervalMap<V> {
            pub fn new() -> Self {
                IntervalMap { by_start: BTreeMap::new() }
            }

            pub fn insert(&mut self, start: u32, end: u32, value: V) -> Result<(), V> {
                if start >= end {
                    return Err(value);
                }
                let before_overlaps = self.by_start.range(..=start).next_back().is_some_and(|(_, (e, _))| *e > start);
                let after_overlaps = self.by_start.range(start..).next().is_some_and(|(s, _)| *s < end);
                if before_overlaps || after_overlaps {
                    return Err(value);
                }
                self.by_start.insert(start, (end, value));
                Ok(())
            }

            pub fn get(&self, point: u32) -> Option<&V> {
                self.by_start
                    .range(..=point)
                    .next_back()
                    .filter(|(_, (end, _))| point < *end)
                    .map(|(_, (_, v))| v)
            }
        }

        impl<V> Default for IntervalMap<V> {
            fn default() -> Self {
                Self::new()
            }
        }
    """,
    visible=[
        T("lookup", "[0, 10) → \"a\", [20, 30) → \"b\"", '{ let mut m = IntervalMap::new(); m.insert(0, 10, "a").unwrap(); m.insert(20, 30, "b").unwrap(); (m.get(5).copied(), m.get(10).copied(), m.get(25).copied()) }', '(Some("a"), None, Some("b"))'),
        T("overlap_refused", "[0, 10) then [5, 15)", '{ let mut m = IntervalMap::new(); m.insert(0, 10, "a").unwrap(); m.insert(5, 15, "b") }', 'Err("b")'),
        T("touching_allowed", "[0, 10) then [10, 20)", '{ let mut m = IntervalMap::new(); m.insert(0, 10, 1).unwrap(); (m.insert(10, 20, 2), m.get(10).copied()) }', "(Ok(()), Some(2))"),
        T("empty_interval", "[3, 3)", "IntervalMap::new().insert(3, 3, 'x')", "Err('x')"),
        T("get_on_empty", "no intervals; get(0)", "IntervalMap::<u8>::new().get(0).copied()", "None"),
    ],
    hidden=[
        T("contains_existing", "[5, 6) then [0, 100)", '{ let mut m = IntervalMap::new(); m.insert(5, 6, 1).unwrap(); m.insert(0, 100, 2) }', "Err(2)"),
        T("inside_existing", "[0, 100) then [5, 6)", '{ let mut m = IntervalMap::new(); m.insert(0, 100, 1).unwrap(); m.insert(5, 6, 2) }', "Err(2)"),
        T("fits_before", "[10, 20) then [0, 10)", '{ let mut m = IntervalMap::new(); m.insert(10, 20, 1).unwrap(); (m.insert(0, 10, 2), m.get(9).copied(), m.get(10).copied()) }', "(Ok(()), Some(2), Some(1))"),
        T("overlaps_start", "[10, 20) then [0, 11)", '{ let mut m = IntervalMap::new(); m.insert(10, 20, 1).unwrap(); m.insert(0, 11, 2) }', "Err(2)"),
        T("reversed", "[5, 3)", "IntervalMap::new().insert(5, 3, 'x')", "Err('x')"),
        T("max_bounds", "[0, u32::MAX)", "{ let mut m = IntervalMap::new(); m.insert(0, u32::MAX, 'x').unwrap(); (m.get(0).copied(), m.get(u32::MAX - 1).copied(), m.get(u32::MAX).copied()) }", "(Some('x'), Some('x'), None)"),
        T("refused_changes_nothing", "[0, 10) → a; [5, 15) → b refused", '{ let mut m = IntervalMap::new(); m.insert(0, 10, "a").unwrap(); let r = m.insert(5, 15, "b"); (r, m.get(5).copied(), m.get(12).copied()) }', '(Err("b"), Some("a"), None)'),
        T("gap", "[0, 10), [20, 30); get(15)", "{ let mut m = IntervalMap::new(); m.insert(0, 10, 1).unwrap(); m.insert(20, 30, 2).unwrap(); m.get(15).copied() }", "None"),
        T("same_start", "[0, 10) then [0, 5)", "{ let mut m = IntervalMap::new(); m.insert(0, 10, 1).unwrap(); m.insert(0, 5, 2) }", "Err(2)"),
        T("fills_a_gap_exactly", "[0, 10), [20, 30), then [10, 20)", "{ let mut m = IntervalMap::new(); m.insert(0, 10, 1).unwrap(); m.insert(20, 30, 3).unwrap(); (m.insert(10, 20, 2), m.get(19).copied(), m.get(20).copied()) }", "(Ok(()), Some(2), Some(3))"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(4011);
            for _ in 0..300 {
                let mut m = IntervalMap::new();
                let mut model: Vec<(u32, u32, usize)> = Vec::new();
                let mut log = Vec::new();
                let n = rng.below(8);
                for id in 0..n {
                    let (a, b) = (rng.below(20) as u32, rng.below(20) as u32);
                    let ok = a < b && model.iter().all(|&(s, e, _)| e <= a || b <= s);
                    if ok {
                        model.push((a, b, id));
                    }
                    log.push(format!("[{a}, {b})"));
                    check!(log.join(", "), m.insert(a, b, id), if ok { Ok(()) } else { Err(id) });
                }
                for p in 0..21u32 {
                    let want = model.iter().find(|&&(s, e, _)| s <= p && p < e).map(|&(_, _, id)| id);
                    check!(format!("{}; get({p})", log.join(", ")), m.get(p).copied(), want);
                }
            }
        }

        #[test]
        fn scale_100k() {
            let mut m = IntervalMap::new();
            for i in (0..100_000u32).rev() {
                m.insert(i * 3, i * 3 + 2, i).unwrap();
            }
            let hits = (0..300_000u32).filter(|&p| m.get(p).is_some()).count();
            check!("100000 intervals [3i, 3i + 2); get every point below 300000", hits, 200_000);
        }
        """,
    ],
    hints=[("approach", "An interval containing `point` must be the one with the largest start ≤ point."),
           ("approach", "A new interval overlaps only if its predecessor ends after `start` or its successor starts before `end`.")],
    notes=("Two neighbour checks are enough because the stored intervals never overlap each other. `Err(value)` gives ownership back so the caller can retry without cloning.", "O(log n)", "O(n)"),
    follow_up="How would you support overlapping intervals and 'all intervals containing x'?",
    related=["D8", "D14"],
    wrong=dict(
        vec_scan="""
            pub struct IntervalMap<V> {
                items: Vec<(u32, u32, V)>,
            }

            impl<V> IntervalMap<V> {
                pub fn new() -> Self {
                    IntervalMap { items: Vec::new() }
                }

                pub fn insert(&mut self, start: u32, end: u32, value: V) -> Result<(), V> {
                    if start >= end || self.items.iter().any(|(s, e, _)| *s < end && start < *e) {
                        return Err(value);
                    }
                    self.items.push((start, end, value));
                    Ok(())
                }

                pub fn get(&self, point: u32) -> Option<&V> {
                    self.items.iter().find(|(s, e, _)| *s <= point && point < *e).map(|(_, _, v)| v)
                }
            }
        """,
        only_checks_the_one_before=INTERVAL_WRONG.replace("BEFORE", "*e > start").replace("AFTER", "false").replace("GET", "point < *end"),
        touching_counts=INTERVAL_WRONG.replace("BEFORE", "*e >= start").replace("AFTER", "self.by_start.range(start..).next().is_some_and(|(s, _)| *s <= end)").replace("GET", "point < *end"),
        end_is_inside=INTERVAL_WRONG.replace("BEFORE", "*e > start").replace("AFTER", "self.by_start.range(start..).next().is_some_and(|(s, _)| *s < end)").replace("GET", "point <= *end"),
    ),
))

STAGES = [("use-it", "Use it", "easy"), ("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("s4-maps-sets", "S4", "Maps & sets", "S", "core", 4,
                    "HashMap, HashSet, BTreeMap and BTreeSet: the entry API, ranges, Hash/Eq contracts, and a hash map from scratch.",
                    STAGES, P)
    print("S4", n)
