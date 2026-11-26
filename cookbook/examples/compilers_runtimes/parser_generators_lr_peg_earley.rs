//! RECIPE 424 (parsing): three general parsing techniques on the same grammars: packrat PEG, SLR(1) LR generator, Earley.
//! LEVEL: advanced
//! Use in: language front ends, config/DSL parsers, tooling that must accept ambiguous or left-recursive grammars, grammar debugging.   Run: cargo run --release --example parser_generators_lr_peg_earley
#![allow(dead_code)]
//!
//! ONE GRAMMAR NOTATION, THREE ENGINES. Grammars are written `E->E+T|T;T->T*F|F;F->(E)|n` (uppercase = nonterminal, `_` = empty).
//! (a) PEG / PACKRAT: a grammar is a RECIPE, not a set of derivations: `/` is ORDERED choice (first success wins, no
//!     retry), plus greedy repetition and lookahead `&e` `!e`. Naive backtracking can be exponential; PACKRAT memoises
//!     (rule, position) -> result so each pair is computed once, giving O(n) time for O(n) memory. LEFT RECURSION
//!     (`E <- E '+' T`) re-enters the same (rule, position) forever; we detect it and show the standard rewrite
//!     `E <- T ('+' T)*`. Lookahead lets PEG recognise a^n b^n c^n, which is not even context-free.
//! (b) LR: from the grammar we compute NULLABLE/FIRST/FOLLOW, the LR(0) item-set automaton, and SLR(1) ACTION/GOTO tables
//!     (reduce on FOLLOW). Table cells holding two actions are CONFLICTS (shift/reduce, reduce/reduce) and are reported, not
//!     hidden. Yacc-style precedence/associativity resolves shift/reduce conflicts of the ambiguous `E->E+E|E*E|...`. The
//!     driver is the classic shift-reduce loop with a state stack and yields a parse tree.
//! (c) EARLEY: a chart of items `rule . dot @origin`; predictor/scanner/completer handle ANY context-free grammar (left
//!     recursive, ambiguous, nullable) in O(n^3) worst case, O(n^2) for unambiguous, O(n) for many LR grammars. Over the
//!     finished chart we count (or enumerate) parse trees, e.g. S -> S S | a has Catalan(n-1) trees for a^n.
//! WHY ALL THREE: they fail differently. PEG never reports ambiguity (the first match silently wins, so it can reject strings
//! the CFG generates, e.g. palindromes). LR gives the best speed and static ambiguity detection but rejects non-LR grammars at
//! generator time. Earley accepts everything context-free but is slower and leaves ambiguity to you.
//! COMMON USES: PEG: Python's CPython parser since 3.9, many hand-written DSL and Markdown/TOML parsers; LR: yacc/bison, most
//! language grammars in compiler courses and tools (LALR in practice); Earley: natural-language and ambiguous-grammar toolkits,
//! grammar-based fuzzers and syntax highlighters where the grammar is data supplied at runtime.
//! Read: Ford "Parsing Expression Grammars" https://bford.info/pub/lang/peg.pdf ; Aho, Lam, Sethi, Ullman "Compilers" ch. 4
//! (items, SLR); Earley "An Efficient Context-Free Parsing Algorithm" (CACM 1970);
//! https://en.wikipedia.org/wiki/Packrat_parser , https://en.wikipedia.org/wiki/Simple_LR_parser , https://en.wikipedia.org/wiki/Earley_parser
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ---------------------------------------------------------------- rng

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------- context-free grammars, FIRST and FOLLOW

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
enum Sym {
    T(char),
    N(usize),
}
#[derive(Clone, Debug)]
struct Rule {
    lhs: usize,
    rhs: Vec<Sym>,
}
#[derive(Clone, Debug)]
struct Grammar {
    names: Vec<char>,
    rules: Vec<Rule>,
    start: usize,
}
const EOF: char = '$';

impl Grammar {
    fn parse(spec: &str) -> Grammar {
        let lines: Vec<String> = spec
            .split(';')
            .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
            .filter(|s| !s.is_empty())
            .collect();
        let mut names: Vec<char> = vec![];
        for l in &lines {
            let c = l.chars().next().unwrap();
            if !names.contains(&c) {
                names.push(c);
            }
        }
        let mut rules = vec![];
        for l in &lines {
            let (lhs, rhs) = l.split_once("->").expect("rule needs ->");
            let li = names
                .iter()
                .position(|&c| Some(c) == lhs.chars().next())
                .unwrap();
            for alt in rhs.split('|') {
                let syms = alt
                    .chars()
                    .filter(|&c| c != '_')
                    .map(|c| {
                        if c.is_ascii_uppercase() {
                            Sym::N(
                                names
                                    .iter()
                                    .position(|&x| x == c)
                                    .expect("undefined nonterminal"),
                            )
                        } else {
                            Sym::T(c)
                        }
                    })
                    .collect();
                rules.push(Rule { lhs: li, rhs: syms });
            }
        }
        Grammar {
            names,
            rules,
            start: 0,
        }
    }
    fn rule_text(&self, r: usize) -> String {
        let rhs: String = self.rules[r]
            .rhs
            .iter()
            .map(|s| self.sym_char(*s))
            .collect();
        format!(
            "{} -> {}",
            self.names[self.rules[r].lhs],
            if rhs.is_empty() {
                "ε".to_string()
            } else {
                rhs
            }
        )
    }
    fn sym_char(&self, s: Sym) -> char {
        match s {
            Sym::T(c) => c,
            Sym::N(n) => self.names[n],
        }
    }
    fn rules_of(&self, nt: usize) -> Vec<usize> {
        (0..self.rules.len())
            .filter(|&r| self.rules[r].lhs == nt)
            .collect()
    }
    fn nullable(&self) -> Vec<bool> {
        let mut nu = vec![false; self.names.len()];
        let mut changed = true;
        while changed {
            changed = false;
            for r in &self.rules {
                if !nu[r.lhs] && r.rhs.iter().all(|s| matches!(s, Sym::N(n) if nu[*n])) {
                    nu[r.lhs] = true;
                    changed = true;
                }
            }
        }
        nu
    }
    /// FIRST of a symbol string, plus whether the whole string can vanish.
    fn first_seq(
        &self,
        syms: &[Sym],
        nullable: &[bool],
        first: &[BTreeSet<char>],
    ) -> (BTreeSet<char>, bool) {
        let mut out = BTreeSet::new();
        for s in syms {
            match s {
                Sym::T(c) => {
                    out.insert(*c);
                    return (out, false);
                }
                Sym::N(n) => {
                    out.extend(first[*n].iter().copied());
                    if !nullable[*n] {
                        return (out, false);
                    }
                }
            }
        }
        (out, true)
    }
    fn first(&self, nullable: &[bool]) -> Vec<BTreeSet<char>> {
        let mut first = vec![BTreeSet::new(); self.names.len()];
        let mut changed = true;
        while changed {
            changed = false;
            for r in &self.rules {
                let (f, _) = self.first_seq(&r.rhs, nullable, &first);
                for c in f {
                    changed |= first[r.lhs].insert(c);
                }
            }
        }
        first
    }
    /// FOLLOW(B) gets FIRST(beta) for every `A -> alpha B beta`, and FOLLOW(A) when beta can vanish.
    fn follow(&self, nullable: &[bool], first: &[BTreeSet<char>]) -> Vec<BTreeSet<char>> {
        let mut follow = vec![BTreeSet::new(); self.names.len()];
        follow[self.start].insert(EOF);
        let mut changed = true;
        while changed {
            changed = false;
            for r in &self.rules {
                for (i, s) in r.rhs.iter().enumerate() {
                    if let Sym::N(b) = s {
                        let (f, all_null) = self.first_seq(&r.rhs[i + 1..], nullable, first);
                        let mut add: BTreeSet<char> = f;
                        if all_null {
                            add.extend(follow[r.lhs].iter().copied());
                        }
                        for c in add {
                            changed |= follow[*b].insert(c);
                        }
                    }
                }
            }
        }
        follow
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tree {
    Leaf(char),
    Node(usize, Vec<Tree>),
}
impl Tree {
    fn show(&self, g: &Grammar) -> String {
        match self {
            Tree::Leaf(c) => c.to_string(),
            Tree::Node(nt, ch) => {
                let kids: Vec<String> = ch.iter().map(|c| c.show(g)).collect();
                if kids.is_empty() {
                    format!("({})", g.names[*nt])
                } else {
                    format!("({} {})", g.names[*nt], kids.join(" "))
                }
            }
        }
    }
    /// Operator-tree shape: `(l op r)`, ignoring unit rules and parentheses. Equal shapes = same parse structure.
    fn shape(&self) -> String {
        match self {
            Tree::Leaf(c) => c.to_string(),
            Tree::Node(_, ch) if ch.len() == 1 => ch[0].shape(),
            Tree::Node(_, ch) if ch.len() == 3 => match &ch[0] {
                Tree::Leaf('(') => ch[1].shape(),
                _ => format!("({} {} {})", ch[0].shape(), ch[1].shape(), ch[2].shape()),
            },
            Tree::Node(..) => "?".into(),
        }
    }
}

// ---------------------------------------------------------------- (a) PEG with packrat memoisation

#[derive(Clone)]
enum Peg {
    Eps,
    End,
    Lit(char),
    Seq(Vec<Peg>),
    Alt(Vec<Peg>),
    Star(Box<Peg>),
    Not(Box<Peg>),
    And(Box<Peg>),
    Rule(usize),
}
fn lit(c: char) -> Peg {
    Peg::Lit(c)
}
fn seq(v: Vec<Peg>) -> Peg {
    Peg::Seq(v)
}
fn alt(v: Vec<Peg>) -> Peg {
    Peg::Alt(v)
}
fn star(p: Peg) -> Peg {
    Peg::Star(Box::new(p))
}
fn rule(i: usize) -> Peg {
    Peg::Rule(i)
}

struct PegGrammar {
    names: Vec<String>,
    rules: Vec<Peg>,
}

#[derive(Clone, Copy)]
enum Cell {
    Unknown,
    Done(Option<usize>), // end position of the match, or failure
}

struct Packrat<'a> {
    g: &'a PegGrammar,
    input: &'a [char],
    memo: Vec<Cell>,
    active: Vec<bool>,
    memoize: bool,
    steps: u64,
    hits: u64,
}

impl<'a> Packrat<'a> {
    fn new(g: &'a PegGrammar, input: &'a [char], memoize: bool) -> Self {
        let cells = g.rules.len() * (input.len() + 1);
        Packrat {
            g,
            input,
            memo: vec![Cell::Unknown; cells],
            active: vec![false; cells],
            memoize,
            steps: 0,
            hits: 0,
        }
    }
    /// Evaluates one rule at a position. The memo makes each (rule, position) pair cost its body once, ever.
    fn call(&mut self, r: usize, pos: usize) -> Result<Option<usize>, String> {
        let idx = r * (self.input.len() + 1) + pos;
        if self.memoize {
            if let Cell::Done(v) = self.memo[idx] {
                self.hits += 1;
                return Ok(v);
            }
        }
        if self.active[idx] {
            return Err(format!(
                "left recursion: rule `{}` re-entered at position {pos} without consuming input",
                self.g.names[r]
            ));
        }
        self.active[idx] = true;
        let res = self.eval(&self.g.rules[r], pos);
        self.active[idx] = false;
        let res = res?;
        if self.memoize {
            self.memo[idx] = Cell::Done(res);
        }
        Ok(res)
    }
    fn eval(&mut self, e: &Peg, pos: usize) -> Result<Option<usize>, String> {
        self.steps += 1;
        Ok(match e {
            Peg::Eps => Some(pos),
            Peg::End => (pos == self.input.len()).then_some(pos),
            Peg::Lit(c) => (self.input.get(pos) == Some(c)).then_some(pos + 1),
            Peg::Seq(items) => {
                let mut p = pos;
                for it in items {
                    match self.eval(it, p)? {
                        Some(n) => p = n,
                        None => return Ok(None),
                    }
                }
                Some(p)
            }
            Peg::Alt(items) => {
                // ORDERED choice: the first alternative that succeeds wins; later ones are never tried afterwards.
                for it in items {
                    if let Some(n) = self.eval(it, pos)? {
                        return Ok(Some(n));
                    }
                }
                None
            }
            Peg::Star(inner) => {
                let mut p = pos;
                while let Some(n) = self.eval(inner, p)? {
                    if n == p {
                        break; // no progress: stop instead of looping forever
                    }
                    p = n;
                }
                Some(p)
            }
            Peg::Not(inner) => self.eval(inner, pos)?.is_none().then_some(pos),
            Peg::And(inner) => self.eval(inner, pos)?.map(|_| pos),
            Peg::Rule(r) => self.call(*r, pos)?,
        })
    }
}

struct PegRun {
    accepted: Result<bool, String>,
    steps: u64,
    hits: u64,
}
fn peg_run(g: &PegGrammar, input: &str, memoize: bool) -> PegRun {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Packrat::new(g, &chars, memoize);
    let accepted = p.call(0, 0).map(|r| r == Some(chars.len()));
    PegRun {
        accepted,
        steps: p.steps,
        hits: p.hits,
    }
}

fn peg_names(n: &[&str]) -> Vec<String> {
    n.iter().map(|s| s.to_string()).collect()
}
fn peg_parens() -> PegGrammar {
    PegGrammar {
        names: peg_names(&["S"]),
        rules: vec![alt(vec![
            seq(vec![lit('('), rule(0), lit(')'), rule(0)]),
            Peg::Eps,
        ])],
    }
}
/// E <- T ('+' T)* ; T <- F ('*' F)* ; F <- '(' E ')' / 'n'   (repetition instead of left recursion)
fn peg_arith() -> PegGrammar {
    PegGrammar {
        names: peg_names(&["E", "T", "F"]),
        rules: vec![
            seq(vec![rule(1), star(seq(vec![lit('+'), rule(1)]))]),
            seq(vec![rule(2), star(seq(vec![lit('*'), rule(2)]))]),
            alt(vec![seq(vec![lit('('), rule(0), lit(')')]), lit('n')]),
        ],
    }
}
/// The textbook CFG transcribed literally: E <- E '+' T / T. Left recursive, so PEG cannot run it.
fn peg_arith_left_recursive() -> PegGrammar {
    PegGrammar {
        names: peg_names(&["E", "T", "F"]),
        rules: vec![
            alt(vec![seq(vec![rule(0), lit('+'), rule(1)]), rule(1)]),
            alt(vec![seq(vec![rule(1), lit('*'), rule(2)]), rule(2)]),
            alt(vec![seq(vec![lit('('), rule(0), lit(')')]), lit('n')]),
        ],
    }
}
/// S <- a S a / b S b / a / b / ε : ordered choice commits to `a` too early for some palindromes.
fn peg_palindrome() -> PegGrammar {
    PegGrammar {
        names: peg_names(&["S"]),
        rules: vec![alt(vec![
            seq(vec![lit('a'), rule(0), lit('a')]),
            seq(vec![lit('b'), rule(0), lit('b')]),
            lit('a'),
            lit('b'),
            Peg::Eps,
        ])],
    }
}
/// a^n b^n c^n via lookahead: S <- &(A 'c') a* B !.   A <- a A b / a b   B <- b B c / b c (n >= 1). Not context-free!
fn peg_anbncn() -> PegGrammar {
    PegGrammar {
        names: peg_names(&["S", "A", "B"]),
        rules: vec![
            seq(vec![
                Peg::And(Box::new(seq(vec![rule(1), lit('c')]))),
                star(lit('a')),
                rule(2),
                Peg::End,
            ]),
            alt(vec![
                seq(vec![lit('a'), rule(1), lit('b')]),
                seq(vec![lit('a'), lit('b')]),
            ]),
            alt(vec![
                seq(vec![lit('b'), rule(2), lit('c')]),
                seq(vec![lit('b'), lit('c')]),
            ]),
        ],
    }
}
/// R_i <- R_{i+1} 'a' / R_{i+1} 'b' ; R_n <- 'z'. Input `z b^n`: naive parsing re-parses R_{i+1} twice per level = 2^n.
fn peg_chain(n: usize) -> PegGrammar {
    let mut rules: Vec<Peg> = (0..n)
        .map(|i| {
            alt(vec![
                seq(vec![rule(i + 1), lit('a')]),
                seq(vec![rule(i + 1), lit('b')]),
            ])
        })
        .collect();
    rules.push(lit('z'));
    PegGrammar {
        names: (0..=n).map(|i| format!("R{i}")).collect(),
        rules,
    }
}

// ---------------------------------------------------------------- (b) LR: items, SLR(1) tables, conflicts, driver

type Item = (usize, usize); // (rule, dot)

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Act {
    Shift(usize),
    Reduce(usize),
    Accept,
}
#[derive(Clone, Copy, PartialEq)]
enum Assoc {
    Left,
    Right,
}
struct Prec(HashMap<char, (u8, Assoc)>);

#[derive(Debug, Clone)]
struct Conflict {
    state: usize,
    token: char,
    kind: &'static str,
    acts: Vec<Act>,
}

struct Lr {
    g: Grammar, // augmented with S' -> S as the last rule
    action: Vec<BTreeMap<char, Act>>,
    goto: Vec<BTreeMap<usize, usize>>,
    conflicts: Vec<Conflict>,
    resolved_by_precedence: usize,
}

fn closure(g: &Grammar, items: &BTreeSet<Item>) -> BTreeSet<Item> {
    let mut set = items.clone();
    let mut work: Vec<Item> = set.iter().copied().collect();
    while let Some((r, d)) = work.pop() {
        if let Some(Sym::N(n)) = g.rules[r].rhs.get(d) {
            for r2 in g.rules_of(*n) {
                if set.insert((r2, 0)) {
                    work.push((r2, 0));
                }
            }
        }
    }
    set
}

fn goto_set(g: &Grammar, items: &BTreeSet<Item>, s: Sym) -> BTreeSet<Item> {
    let moved: BTreeSet<Item> = items
        .iter()
        .filter(|&&(r, d)| g.rules[r].rhs.get(d) == Some(&s))
        .map(|&(r, d)| (r, d + 1))
        .collect();
    closure(g, &moved)
}

/// The LR(0) automaton: states are closed item sets, edges are grammar symbols.
fn build_states(g: &Grammar) -> (Vec<BTreeSet<Item>>, BTreeMap<(usize, Sym), usize>) {
    let aug = g.rules.len() - 1;
    let mut states = vec![closure(g, &BTreeSet::from([(aug, 0)]))];
    let mut index: HashMap<BTreeSet<Item>, usize> = HashMap::from([(states[0].clone(), 0)]);
    let mut edges = BTreeMap::new();
    let mut i = 0;
    while i < states.len() {
        let syms: BTreeSet<Sym> = states[i]
            .iter()
            .filter_map(|&(r, d)| g.rules[r].rhs.get(d).copied())
            .collect();
        for s in syms {
            let target = goto_set(g, &states[i], s);
            let next = states.len();
            let id = *index.entry(target.clone()).or_insert(next);
            if id == next {
                states.push(target);
            }
            edges.insert((i, s), id);
        }
        i += 1;
    }
    (states, edges)
}

fn build_slr(user: &Grammar, prec: Option<&Prec>) -> Lr {
    let mut g = user.clone();
    let aug_nt = g.names.len();
    g.names.push('\'');
    g.rules.push(Rule {
        lhs: aug_nt,
        rhs: vec![Sym::N(user.start)],
    });
    g.start = aug_nt;
    let aug = g.rules.len() - 1;
    let nullable = g.nullable();
    let first = g.first(&nullable);
    let follow = g.follow(&nullable, &first);
    let (states, edges) = build_states(&g);

    let mut action = vec![];
    let mut goto = vec![BTreeMap::new(); states.len()];
    let (mut conflicts, mut resolved) = (vec![], 0);
    for (&(s, sym), &t) in &edges {
        if let Sym::N(n) = sym {
            goto[s].insert(n, t);
        }
    }
    for (s, items) in states.iter().enumerate() {
        let mut cells: BTreeMap<char, BTreeSet<Act>> = BTreeMap::new();
        for &(r, d) in items {
            match g.rules[r].rhs.get(d) {
                Some(Sym::T(c)) => {
                    cells
                        .entry(*c)
                        .or_default()
                        .insert(Act::Shift(edges[&(s, Sym::T(*c))]));
                }
                Some(Sym::N(_)) => {}
                None if r == aug => {
                    cells.entry(EOF).or_default().insert(Act::Accept);
                }
                None => {
                    for &c in &follow[g.rules[r].lhs] {
                        cells.entry(c).or_default().insert(Act::Reduce(r));
                    }
                }
            }
        }
        let mut row = BTreeMap::new();
        for (tok, acts) in cells {
            if acts.len() == 1 {
                row.insert(tok, *acts.iter().next().unwrap());
                continue;
            }
            let shift = acts.iter().find(|a| matches!(a, Act::Shift(_))).copied();
            let reduces: Vec<Act> = acts
                .iter()
                .filter(|a| matches!(a, Act::Reduce(_)))
                .copied()
                .collect();
            // Yacc rule: a shift/reduce conflict is resolved by comparing the rule's precedence (its last terminal) with the token's.
            if let (Some(sh), [Act::Reduce(r)], Some(p)) = (shift, reduces.as_slice(), prec) {
                let rule_tok = g.rules[*r]
                    .rhs
                    .iter()
                    .rev()
                    .find_map(|s| if let Sym::T(c) = s { Some(*c) } else { None });
                if let (Some((rp, ra)), Some((tp, _))) =
                    (rule_tok.and_then(|c| p.0.get(&c)), p.0.get(&tok))
                {
                    let reduce_wins = rp > tp || (rp == tp && *ra == Assoc::Left);
                    row.insert(tok, if reduce_wins { Act::Reduce(*r) } else { sh });
                    resolved += 1;
                    continue;
                }
            }
            let kind = if shift.is_some() {
                "shift/reduce"
            } else {
                "reduce/reduce"
            };
            conflicts.push(Conflict {
                state: s,
                token: tok,
                kind,
                acts: acts.iter().copied().collect(),
            });
            row.insert(tok, shift.unwrap_or(reduces[0])); // yacc defaults: prefer shift, else the earlier rule
        }
        action.push(row);
    }
    Lr {
        g,
        action,
        goto,
        conflicts,
        resolved_by_precedence: resolved,
    }
}

fn lr_parse(lr: &Lr, input: &str) -> Result<Tree, String> {
    let chars: Vec<char> = input.chars().chain([EOF]).collect();
    let mut stack: Vec<(usize, Option<Tree>)> = vec![(0, None)];
    let mut pos = 0;
    loop {
        let state = stack.last().unwrap().0;
        match lr.action[state].get(&chars[pos]) {
            None => {
                return Err(format!(
                    "syntax error at position {pos} (unexpected `{}`)",
                    chars[pos]
                ));
            }
            Some(Act::Shift(t)) => {
                stack.push((*t, Some(Tree::Leaf(chars[pos]))));
                pos += 1;
            }
            Some(Act::Reduce(r)) => {
                let rule = &lr.g.rules[*r];
                let kids: Vec<Tree> = stack
                    .split_off(stack.len() - rule.rhs.len())
                    .into_iter()
                    .map(|(_, t)| t.unwrap())
                    .collect();
                let top = stack.last().unwrap().0;
                stack.push((lr.goto[top][&rule.lhs], Some(Tree::Node(rule.lhs, kids))));
            }
            Some(Act::Accept) => return Ok(stack.pop().unwrap().1.unwrap()),
        }
    }
}

fn print_table(lr: &Lr) {
    let mut terms: BTreeSet<char> = lr.action.iter().flat_map(|r| r.keys().copied()).collect();
    terms.remove(&EOF);
    let cols: Vec<char> = terms.into_iter().chain([EOF]).collect();
    let nts: Vec<usize> = (0..lr.g.names.len() - 1).collect();
    print!("      ACTION:");
    cols.iter().for_each(|c| print!("{c:>5}"));
    print!("  | GOTO:");
    nts.iter().for_each(|&n| print!("{:>4}", lr.g.names[n]));
    println!();
    for s in 0..lr.action.len() {
        print!("      state {s:>2}:");
        for c in &cols {
            let cell = match lr.action[s].get(c) {
                Some(Act::Shift(t)) => format!("s{t}"),
                Some(Act::Reduce(r)) => format!("r{r}"),
                Some(Act::Accept) => "acc".into(),
                None => ".".into(),
            };
            print!("{cell:>5}");
        }
        print!("  |       ");
        for n in &nts {
            print!(
                "{:>4}",
                lr.goto[s]
                    .get(n)
                    .map(|t| t.to_string())
                    .unwrap_or(".".into())
            );
        }
        println!();
    }
}

// ---------------------------------------------------------------- (c) Earley recogniser, tree counting and enumeration

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct EItem {
    rule: usize,
    dot: usize,
    origin: usize,
}

struct Chart {
    sets: Vec<Vec<EItem>>,
    done: HashSet<(usize, usize, usize)>, // completed (nonterminal, from, to)
    accepted: bool,
    ops: u64,
}

fn earley(g: &Grammar, input: &[char]) -> Chart {
    let n = input.len();
    let nullable = g.nullable();
    let mut sets: Vec<Vec<EItem>> = vec![vec![]; n + 1];
    let mut seen: Vec<HashSet<EItem>> = vec![HashSet::new(); n + 1];
    let mut ops = 0u64;
    let mut add = |sets: &mut Vec<Vec<EItem>>, i: usize, it: EItem| {
        if seen[i].insert(it) {
            sets[i].push(it);
        }
    };
    for r in g.rules_of(g.start) {
        add(
            &mut sets,
            0,
            EItem {
                rule: r,
                dot: 0,
                origin: 0,
            },
        );
    }
    for i in 0..=n {
        let mut k = 0;
        while k < sets[i].len() {
            let it = sets[i][k];
            k += 1;
            ops += 1;
            let rule = &g.rules[it.rule];
            let adv = EItem {
                dot: it.dot + 1,
                ..it
            };
            match rule.rhs.get(it.dot) {
                None => {
                    // COMPLETER: everything that was waiting for this nonterminal at `origin` can advance.
                    let mut j = 0;
                    while j < sets[it.origin].len() {
                        ops += 1;
                        let w = sets[it.origin][j];
                        j += 1;
                        if g.rules[w.rule].rhs.get(w.dot) == Some(&Sym::N(rule.lhs)) {
                            add(
                                &mut sets,
                                i,
                                EItem {
                                    dot: w.dot + 1,
                                    ..w
                                },
                            );
                        }
                    }
                }
                Some(Sym::N(nt)) => {
                    // PREDICTOR (with the nullable shortcut so empty derivations are not missed).
                    for r2 in g.rules_of(*nt) {
                        add(
                            &mut sets,
                            i,
                            EItem {
                                rule: r2,
                                dot: 0,
                                origin: i,
                            },
                        );
                    }
                    if nullable[*nt] {
                        add(&mut sets, i, adv);
                    }
                }
                Some(Sym::T(c)) => {
                    // SCANNER
                    if input.get(i) == Some(c) {
                        add(&mut sets, i + 1, adv);
                    }
                }
            }
        }
    }
    let mut done = HashSet::new();
    for (j, set) in sets.iter().enumerate() {
        for it in set {
            if it.dot == g.rules[it.rule].rhs.len() {
                done.insert((g.rules[it.rule].lhs, it.origin, j));
            }
        }
    }
    let accepted = done.contains(&(g.start, 0, n));
    Chart {
        sets,
        done,
        accepted,
        ops,
    }
}

/// Number of distinct parse trees of `nt` over input[i..j], by memoised search through the completed-span set.
struct Counter<'a> {
    g: &'a Grammar,
    input: &'a [char],
    done: &'a HashSet<(usize, usize, usize)>,
    memo_nt: HashMap<(usize, usize, usize), u128>,
    memo_seq: HashMap<(usize, usize, usize, usize), u128>,
}
impl Counter<'_> {
    fn trees(&mut self, nt: usize, i: usize, j: usize) -> u128 {
        if !self.done.contains(&(nt, i, j)) {
            return 0;
        }
        if let Some(&v) = self.memo_nt.get(&(nt, i, j)) {
            return v;
        }
        self.memo_nt.insert((nt, i, j), 0); // cycle guard: assumes no cyclic derivations (A =>+ A)
        let total = self
            .g
            .rules_of(nt)
            .into_iter()
            .map(|r| self.seq(r, 0, i, j))
            .sum();
        self.memo_nt.insert((nt, i, j), total);
        total
    }
    fn seq(&mut self, r: usize, d: usize, i: usize, j: usize) -> u128 {
        let g = self.g;
        match g.rules[r].rhs.get(d) {
            None => (i == j) as u128,
            Some(Sym::T(c)) => {
                (i < j && self.input[i] == *c) as u128 * self.seq(r, d + 1, i + 1, j)
            }
            Some(Sym::N(n)) => {
                if let Some(&v) = self.memo_seq.get(&(r, d, i, j)) {
                    return v;
                }
                let v = (i..=j)
                    .map(|k| match self.trees(*n, i, k) {
                        0 => 0,
                        heads => heads * self.seq(r, d + 1, k, j),
                    })
                    .sum();
                self.memo_seq.insert((r, d, i, j), v);
                v
            }
        }
    }
}
fn count_trees(g: &Grammar, input: &[char], chart: &Chart) -> u128 {
    let mut c = Counter {
        g,
        input,
        done: &chart.done,
        memo_nt: HashMap::new(),
        memo_seq: HashMap::new(),
    };
    c.trees(g.start, 0, input.len())
}

fn all_trees(
    g: &Grammar,
    input: &[char],
    done: &HashSet<(usize, usize, usize)>,
    nt: usize,
    i: usize,
    j: usize,
) -> Vec<Tree> {
    if !done.contains(&(nt, i, j)) {
        return vec![];
    }
    let mut out = vec![];
    for r in g.rules_of(nt) {
        for kids in seq_trees(g, input, done, r, 0, i, j) {
            out.push(Tree::Node(nt, kids));
        }
    }
    out
}
fn seq_trees(
    g: &Grammar,
    input: &[char],
    done: &HashSet<(usize, usize, usize)>,
    r: usize,
    d: usize,
    i: usize,
    j: usize,
) -> Vec<Vec<Tree>> {
    match g.rules[r].rhs.get(d) {
        None => {
            if i == j {
                vec![vec![]]
            } else {
                vec![]
            }
        }
        Some(Sym::T(c)) => {
            if i < j && input[i] == *c {
                seq_trees(g, input, done, r, d + 1, i + 1, j)
                    .into_iter()
                    .map(|mut t| {
                        t.insert(0, Tree::Leaf(*c));
                        t
                    })
                    .collect()
            } else {
                vec![]
            }
        }
        Some(Sym::N(n)) => {
            let mut out = vec![];
            for k in i..=j {
                // Prune with the chart, and compute tails BEFORE heads: with k == j the head would re-derive the
                // same span (S -> S S) and recurse forever, but its tail is then empty so we never get there.
                if !done.contains(&(*n, i, k)) {
                    continue;
                }
                let tails = seq_trees(g, input, done, r, d + 1, k, j);
                if tails.is_empty() {
                    continue;
                }
                for head in all_trees(g, input, done, *n, i, k) {
                    for tail in &tails {
                        let mut v = vec![head.clone()];
                        v.extend(tail.iter().cloned());
                        out.push(v);
                    }
                }
            }
            out
        }
    }
}

// ---------------------------------------------------------------- oracles and random strings

fn oracle_parens(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => return false,
        };
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}
/// Independent hand-written recursive descent for E = T (+ T)*, T = F (* F)*, F = ( E ) | n.
fn oracle_arith(s: &str) -> bool {
    fn e(b: &[u8], p: usize) -> Option<usize> {
        let mut p = t(b, p)?;
        while b.get(p) == Some(&b'+') {
            p = t(b, p + 1)?;
        }
        Some(p)
    }
    fn t(b: &[u8], p: usize) -> Option<usize> {
        let mut p = f(b, p)?;
        while b.get(p) == Some(&b'*') {
            p = f(b, p + 1)?;
        }
        Some(p)
    }
    fn f(b: &[u8], p: usize) -> Option<usize> {
        match b.get(p) {
            Some(b'n') => Some(p + 1),
            Some(b'(') => {
                let q = e(b, p + 1)?;
                (b.get(q) == Some(&b')')).then_some(q + 1)
            }
            _ => None,
        }
    }
    e(s.as_bytes(), 0) == Some(s.len())
}
fn oracle_palindrome(s: &str) -> bool {
    s.chars().all(|c| c == 'a' || c == 'b') && s.chars().eq(s.chars().rev())
}
fn oracle_anbncn(s: &str) -> bool {
    let n = s.len() / 3;
    n > 0 && s == format!("{}{}{}", "a".repeat(n), "b".repeat(n), "c".repeat(n))
}

fn derive(g: &Grammar, nt: usize, depth: usize, rng: &mut Rng, out: &mut String) {
    let rules = g.rules_of(nt);
    let pick = if depth > 6 {
        // Out of budget: take the rule with the fewest nonterminals so the derivation ends.
        *rules
            .iter()
            .min_by_key(|&&r| {
                g.rules[r]
                    .rhs
                    .iter()
                    .filter(|s| matches!(s, Sym::N(_)))
                    .count()
            })
            .unwrap()
    } else {
        rules[rng.below(rules.len())]
    };
    for s in &g.rules[pick].rhs {
        match s {
            Sym::T(c) => out.push(*c),
            Sym::N(n) => derive(g, *n, depth + 1, rng, out),
        }
    }
}
/// Half valid sentences (random derivations), half those with 1-2 random edits, plus pure noise.
fn sample_strings(g: &Grammar, alphabet: &[char], rng: &mut Rng, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            let mut s = String::new();
            if i % 4 == 3 {
                (0..rng.below(9)).for_each(|_| s.push(alphabet[rng.below(alphabet.len())]));
                return s;
            }
            derive(g, g.start, 0, rng, &mut s);
            s.truncate(40);
            if i % 4 != 0 {
                let mut cs: Vec<char> = s.chars().collect();
                for _ in 0..1 + rng.below(2) {
                    let at = rng.below(cs.len() + 1);
                    match rng.below(3) {
                        0 if at < cs.len() => cs[at] = alphabet[rng.below(alphabet.len())],
                        1 if at < cs.len() => {
                            cs.remove(at);
                        }
                        _ => cs.insert(at, alphabet[rng.below(alphabet.len())]),
                    }
                }
                s = cs.into_iter().collect();
            }
            s
        })
        .collect()
}

fn catalan(n: u32) -> u128 {
    (0..n).fold(1u128, |c, k| c * 2 * (2 * k as u128 + 1) / (k as u128 + 2))
}

// ---------------------------------------------------------------- the demo and the tests

fn main() {
    let arith = Grammar::parse("E->E+T|T;T->T*F|F;F->(E)|n");
    let parens = Grammar::parse("S->(S)S|_");
    let palin = Grammar::parse("S->aSa|bSb|a|b|_");
    let ambig = Grammar::parse("S->SS|a");
    let dangling = Grammar::parse("S->iS|iSeS|a");
    let ambig_expr = Grammar::parse("E->E+E|E-E|E*E|E^E|(E)|n");
    let plain_expr = Grammar::parse("E->E+E|E*E|(E)|n");
    let prec_full = Prec(HashMap::from([
        ('+', (1, Assoc::Left)),
        ('-', (1, Assoc::Left)),
        ('*', (2, Assoc::Left)),
        ('^', (3, Assoc::Right)),
    ]));

    // --- FIRST / FOLLOW for the arithmetic grammar
    let nu = arith.nullable();
    let (fi, fo) = (arith.first(&nu), arith.follow(&nu, &arith.first(&nu)));
    println!("FIRST / FOLLOW for E->E+T|T; T->T*F|F; F->(E)|n");
    for n in 0..arith.names.len() {
        println!(
            "  {}: nullable={:<5} FIRST={:?} FOLLOW={:?}",
            arith.names[n], nu[n], fi[n], fo[n]
        );
    }
    assert_eq!(fi[0], BTreeSet::from(['(', 'n']));
    assert_eq!(fo[2], BTreeSet::from(['$', ')', '*', '+']));

    // --- (b) LR tables: print a small one, then the real ones
    let tiny = build_slr(&Grammar::parse("E->E+n|n"), None);
    println!(
        "\nSLR(1) table for E->E+n|n ({} states, {} conflicts); s=shift r=reduce(rule#):",
        tiny.action.len(),
        tiny.conflicts.len()
    );
    for r in 0..tiny.g.rules.len() {
        println!("    rule {r}: {}", tiny.g.rule_text(r));
    }
    print_table(&tiny);
    assert!(tiny.conflicts.is_empty());

    let lr_arith = build_slr(&arith, None);
    let lr_parens = build_slr(&parens, None);
    assert!(lr_arith.conflicts.is_empty() && lr_parens.conflicts.is_empty());
    println!(
        "\nLR automaton sizes: arithmetic {} states, balanced parens {} states; both conflict-free (SLR(1))",
        lr_arith.action.len(),
        lr_parens.action.len()
    );
    let t = lr_parse(&lr_arith, "n+n*n").unwrap();
    println!("parse tree of n+n*n: {}", t.show(&arith));
    assert_eq!(t.shape(), "(n + (n * n))");

    // Precedence and associativity from an AMBIGUOUS grammar, resolved yacc-style.
    let raw = build_slr(&ambig_expr, None);
    let lr_prec = build_slr(&ambig_expr, Some(&prec_full));
    println!(
        "\nE->E+E|E-E|E*E|E^E|(E)|n: {} unresolved conflicts without precedence; with it: {} resolved, {} left",
        raw.conflicts.len(),
        lr_prec.resolved_by_precedence,
        lr_prec.conflicts.len()
    );
    assert!(raw.conflicts.iter().all(|c| c.kind == "shift/reduce") && !raw.conflicts.is_empty());
    assert!(lr_prec.conflicts.is_empty());
    for (s, want) in [
        ("n+n*n", "(n + (n * n))"),
        ("n*n+n", "((n * n) + n)"),
        ("n-n-n", "((n - n) - n)"),
        ("n^n^n", "(n ^ (n ^ n))"),
        ("(n+n)*n", "((n + n) * n)"),
        ("n+n*n^n-n", "((n + (n * (n ^ n))) - n)"),
    ] {
        let got = lr_parse(&lr_prec, s).unwrap().shape();
        println!("  {s:<10} => {got}");
        assert_eq!(got, want, "{s}");
    }

    // Conflict reporting on grammars that are not SLR(1).
    let lr_dangling = build_slr(&dangling, None);
    println!(
        "\ndangling else S->iS|iSeS|a: {} conflict(s):",
        lr_dangling.conflicts.len()
    );
    for c in &lr_dangling.conflicts {
        println!(
            "  state {} on `{}`: {} {:?}",
            c.state, c.token, c.kind, c.acts
        );
    }
    assert!(
        lr_dangling
            .conflicts
            .iter()
            .any(|c| c.token == 'e' && c.kind == "shift/reduce")
    );
    // Default (shift) binds `else` to the nearest `if`.
    let t = lr_parse(&lr_dangling, "iiaea").unwrap();
    println!("  default-shift parse of iiaea: {}", t.show(&dangling));
    assert_eq!(t.show(&dangling), "(S i (S i (S a) e (S a)))");
    let lr_palin = build_slr(&palin, None);
    let lr_ambig = build_slr(&ambig, None);
    println!(
        "palindromes S->aSa|bSb|a|b|_ : {} conflicts (the middle is unknowable with 1 token of lookahead)",
        lr_palin.conflicts.len()
    );
    println!(
        "ambiguous  S->SS|a           : {} conflicts ({})",
        lr_ambig.conflicts.len(),
        lr_ambig
            .conflicts
            .iter()
            .map(|c| c.kind)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(!lr_palin.conflicts.is_empty() && !lr_ambig.conflicts.is_empty());

    // --- (c) Earley: ambiguity counting
    println!("\nEarley on S->SS|a: parse trees of a^n = Catalan(n-1)");
    print!("  n:      ");
    (1..=12).for_each(|n| print!("{n:>6}"));
    print!("\n  trees:  ");
    for n in 1..=12 {
        let input: Vec<char> = "a".repeat(n).chars().collect();
        let chart = earley(&ambig, &input);
        let cnt = count_trees(&ambig, &input, &chart);
        assert!(chart.accepted);
        assert_eq!(cnt, catalan(n as u32 - 1), "a^{n}");
        print!("{cnt:>6}");
    }
    println!();
    let big: Vec<char> = "a".repeat(18).chars().collect();
    let cnt = count_trees(&ambig, &big, &earley(&ambig, &big));
    assert_eq!(cnt, 129644790); // Catalan(17)
    println!("  n=18: {cnt} trees counted without enumerating them");
    let input: Vec<char> = "aaaa".chars().collect();
    let chart = earley(&ambig, &input);
    let trees = all_trees(&ambig, &input, &chart.done, 0, 0, 4);
    assert_eq!(trees.len(), 5);
    println!(
        "  all 5 trees of aaaa: {}",
        trees
            .iter()
            .map(|t| t.show(&ambig))
            .collect::<Vec<_>>()
            .join("  ")
    );
    let input: Vec<char> = "iiaea".chars().collect();
    let chart = earley(&dangling, &input);
    let trees = all_trees(&dangling, &input, &chart.done, 0, 0, 5);
    assert_eq!(trees.len(), 2);
    println!("  dangling else, iiaea has {} parses:", trees.len());
    trees
        .iter()
        .for_each(|t| println!("    {}", t.show(&dangling)));
    assert!(
        trees
            .iter()
            .any(|t| t.show(&dangling) == "(S i (S i (S a) e (S a)))"),
        "the LR default-shift parse is one of them"
    );

    // Cubic growth for ambiguous grammars, near-linear for LR-friendly ones.
    println!("\nEarley work (chart items processed + completer steps):");
    let mut prev = 0u64;
    for n in [20usize, 40, 80, 160] {
        let ops = earley(&ambig, &vec!['a'; n]).ops;
        let ratio = if prev > 0 {
            format!("x{:.1} per doubling", ops as f64 / prev as f64)
        } else {
            String::new()
        };
        println!("  S->SS|a   n={n:<4} ops={ops:<10} {ratio}");
        if prev > 0 {
            assert!(
                ops as f64 / prev as f64 > 5.0,
                "ambiguous grammar should be about cubic"
            );
        }
        prev = ops;
    }
    let mut prev = 0u64;
    for reps in [10usize, 20, 40, 80] {
        let s: Vec<char> = format!("n{}", "+n*n".repeat(reps)).chars().collect();
        let ops = earley(&arith, &s).ops;
        println!(
            "  arith     len={:<4} ops={ops:<10} {}",
            s.len(),
            if prev > 0 {
                format!("x{:.1} per doubling", ops as f64 / prev as f64)
            } else {
                String::new()
            }
        );
        if prev > 0 {
            assert!(
                (ops as f64 / prev as f64) < 3.0,
                "unambiguous LR-friendly input should be near linear"
            );
        }
        prev = ops;
    }

    // --- (a) PEG: packrat linearity versus exponential backtracking
    println!(
        "\nPEG R_i <- R_(i+1) 'a' / R_(i+1) 'b' on input z b^n (adversarial for backtracking):"
    );
    println!(
        "  {:>3} {:>12} {:>12} {:>10}",
        "n", "naive steps", "packrat", "memo hits"
    );
    for n in [8usize, 12, 16, 20] {
        let g = peg_chain(n);
        let input = format!("z{}", "b".repeat(n));
        let (naive, memo) = (peg_run(&g, &input, false), peg_run(&g, &input, true));
        println!(
            "  {n:>3} {:>12} {:>12} {:>10}",
            naive.steps, memo.steps, memo.hits
        );
        assert_eq!(naive.accepted, Ok(true));
        assert_eq!(memo.accepted, Ok(true));
        assert!(
            naive.steps >= 1 << n,
            "naive backtracking is exponential: 2^{n}"
        );
        assert!(memo.steps <= 12 * n as u64 + 12, "packrat is linear");
        assert!(memo.hits as usize >= n);
    }
    println!("\nPEG arithmetic E <- T ('+' T)*: steps grow linearly with input length");
    let pa = peg_arith();
    let mut prev = 0u64;
    for reps in [100usize, 1000, 10000] {
        let s = format!("n{}", "+n*(n+n)".repeat(reps));
        let r = peg_run(&pa, &s, true);
        assert_eq!(r.accepted, Ok(true));
        println!(
            "  len={:<7} steps={:<8} steps/char={:.2}",
            s.len(),
            r.steps,
            r.steps as f64 / s.len() as f64
        );
        assert!(r.steps <= 12 * s.len() as u64);
        if prev > 0 {
            assert!((r.steps as f64 / prev as f64) < 11.0);
        }
        prev = r.steps;
    }
    let lr_text = peg_run(&peg_arith_left_recursive(), "n+n", true).accepted;
    println!(
        "\nleft recursion E <- E '+' T / T:  {}",
        lr_text.as_ref().unwrap_err()
    );
    assert!(lr_text.is_err());
    assert_eq!(
        peg_run(&peg_arith(), "n+n", true).accepted,
        Ok(true),
        "the rewrite works"
    );

    // Lookahead gives PEG more than context-free power.
    let g = peg_anbncn();
    let mut checked = 0;
    for len in 0..=9u32 {
        for code in 0..3u32.pow(len) {
            let s: String = (0..len)
                .map(|i| ['a', 'b', 'c'][((code / 3u32.pow(i)) % 3) as usize])
                .collect();
            assert_eq!(
                peg_run(&g, &s, true).accepted,
                Ok(oracle_anbncn(&s)),
                "a^n b^n c^n on {s:?}"
            );
            checked += 1;
        }
    }
    println!(
        "PEG with &-lookahead recognises a^n b^n c^n: all {checked} strings over {{a,b,c}} up to length 9 agree with the oracle"
    );

    // --- agreement of all engines on random strings
    let mut rng = Rng(0xDEC0DED_5EED);
    let engines = |g: &Grammar, lr: &Lr, pg: &PegGrammar, s: &str| -> (bool, Option<bool>, bool) {
        let chars: Vec<char> = s.chars().collect();
        let e = earley(g, &chars).accepted;
        let l = if lr.conflicts.is_empty() {
            Some(lr_parse(lr, s).is_ok())
        } else {
            None
        };
        let p = peg_run(pg, s, true).accepted.unwrap();
        (e, l, p)
    };
    let mut total = 0;
    for (name, g, lr, pg, alphabet, oracle) in [
        (
            "balanced parens",
            &parens,
            &lr_parens,
            &peg_parens(),
            vec!['(', ')'],
            oracle_parens as fn(&str) -> bool,
        ),
        (
            "arithmetic",
            &arith,
            &lr_arith,
            &peg_arith(),
            vec!['n', '+', '*', '(', ')'],
            oracle_arith,
        ),
    ] {
        let strings = sample_strings(g, &alphabet, &mut rng, 1000);
        let (mut yes, mut tree_checks) = (0, 0);
        for s in &strings {
            let want = oracle(s);
            let (e, l, p) = engines(g, lr, pg, s);
            assert_eq!(
                (e, l, p),
                (want, Some(want), want),
                "{name}: engines disagree on {s:?}"
            );
            if want {
                yes += 1;
                if s.len() <= 24 {
                    // For an unambiguous grammar the single Earley tree must equal the LR parse tree.
                    let chars: Vec<char> = s.chars().collect();
                    let chart = earley(g, &chars);
                    let trees = all_trees(g, &chars, &chart.done, g.start, 0, chars.len());
                    assert_eq!(trees.len() as u128, count_trees(g, &chars, &chart));
                    assert_eq!(
                        trees,
                        vec![lr_parse(lr, s).unwrap()],
                        "{name}: LR and Earley trees differ on {s:?}"
                    );
                    tree_checks += 1;
                }
            }
        }
        total += strings.len();
        println!(
            "{name:<16}: PEG, SLR(1), Earley and the oracle agree on {} strings ({yes} members; {tree_checks} LR tree == Earley tree)",
            strings.len()
        );
    }
    // Precedence-resolved ambiguous grammar parses to the same operator tree as the stratified grammar.
    let lr_plain_prec = build_slr(
        &plain_expr,
        Some(&Prec(HashMap::from([
            ('+', (1, Assoc::Left)),
            ('*', (2, Assoc::Left)),
        ]))),
    );
    assert!(lr_plain_prec.conflicts.is_empty());
    let mut same = 0;
    for s in sample_strings(&arith, &['n', '+', '*', '(', ')'], &mut rng, 600) {
        if oracle_arith(&s) {
            assert_eq!(
                lr_parse(&lr_plain_prec, &s).unwrap().shape(),
                lr_parse(&lr_arith, &s).unwrap().shape(),
                "{s}"
            );
            same += 1;
        }
    }
    println!(
        "precedence-resolved E->E+E|E*E|... gives the same operator tree as the stratified grammar on {same} sentences"
    );
    assert!(total >= 2000 && same > 50);

    // Palindromes: Earley matches the oracle; PEG is sound but incomplete; SLR(1) cannot even build a table.
    let strings = sample_strings(&palin, &['a', 'b'], &mut rng, 600);
    let (mut members, mut peg_missed) = (0, 0);
    for s in &strings {
        let want = oracle_palindrome(s);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(earley(&palin, &chars).accepted, want, "palindrome {s:?}");
        let p = peg_run(&peg_palindrome(), s, true).accepted.unwrap();
        assert!(!p || want, "PEG accepted a non-palindrome {s:?}");
        members += want as usize;
        peg_missed += (want && !p) as usize;
    }
    assert!(
        peg_missed > 0,
        "ordered choice should miss some palindromes"
    );
    println!(
        "palindromes     : Earley agrees with the oracle on {} strings ({members} members); PEG missed {peg_missed} of them (ordered choice commits to `a`); SLR(1) has {} conflicts",
        strings.len(),
        lr_palin.conflicts.len()
    );
    assert_eq!(peg_run(&peg_palindrome(), "abba", true).accepted, Ok(false));
    assert!(oracle_palindrome("abba"));

    // --- summary table, computed from the experiments above
    println!("\nWhich technique handles which grammar class (measured above):");
    println!(
        "  {:<34} {:<32} {:<32} {:<12}",
        "grammar", "PEG / packrat", "SLR(1) tables", "Earley"
    );
    let rows: Vec<(&str, String, String, String)> = vec![
        (
            "balanced parens",
            "ok".into(),
            format!("ok ({} states)", lr_parens.action.len()),
            "ok".into(),
        ),
        (
            "arithmetic, left-recursive CFG",
            "loops; needs rewrite to (..)*".into(),
            format!("ok ({} states)", lr_arith.action.len()),
            "ok".into(),
        ),
        (
            "ambiguous E->E+E|... + precedence",
            "n/a (use precedence climbing)".into(),
            format!(
                "{} conflicts, {} resolved",
                raw.conflicts.len(),
                lr_prec.resolved_by_precedence
            ),
            "all trees".into(),
        ),
        (
            "ambiguous S->SS|a",
            "left recursive, not expressible".into(),
            format!("{} conflicts", lr_ambig.conflicts.len()),
            "Catalan counts".into(),
        ),
        (
            "dangling else",
            "first match wins silently".into(),
            format!(
                "{} S/R conflict (shift default)",
                lr_dangling.conflicts.len()
            ),
            "both parses".into(),
        ),
        (
            "palindromes",
            format!("subset only ({peg_missed} misses)"),
            format!("{} conflicts", lr_palin.conflicts.len()),
            "ok".into(),
        ),
        (
            "a^n b^n c^n (not context-free)",
            "ok via lookahead".into(),
            "impossible".into(),
            "impossible".into(),
        ),
    ];
    for (g, p, l, e) in rows {
        println!("  {g:<34} {p:<26} {l:<22} {e:<12}");
    }
    println!(
        "all assertions passed ({total} random strings x 3 engines + exhaustive and structural checks)"
    );
}
