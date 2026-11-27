use anneal_prelude::Rng;
use solution::Expr as E;
use solution::*;

fn col(name: &str) -> E {
    E::Column(name.to_string())
}

fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

fn not(a: E) -> E {
    E::Not(Box::new(a))
}

fn neg(a: E) -> E {
    E::Neg(Box::new(a))
}

fn is_null(a: E, negated: bool) -> E {
    E::IsNull { of: Box::new(a), negated }
}

fn condition(text: &str) -> Result<E, SyntaxError> {
    match parse(&format!("SELECT * FROM t WHERE {text}"))? {
        Statement::Select(s) => Ok(s.filter.expect("a filter")),
        other => panic!("not a select: {other:?}"),
    }
}

/// The mistake in `sql`, with the text it covers.
fn mistake(sql: &str) -> Option<(Problem, usize, &str)> {
    parse(sql).err().map(|e| (e.problem, e.start, &sql[e.start..e.end]))
}

#[test]
fn prefix_operators_and_strengths() {
    check!("- a * b", condition("- a * b"), Ok(bin(Op::Mul, neg(col("a")), col("b"))));
    check!("- - 5", condition("- -5"), Ok(neg(neg(int(5)))));
    check!("NOT NOT a AND b", condition("NOT NOT a AND b"), Ok(bin(Op::And, not(not(col("a"))), col("b"))));
    check!("NOT a IS NULL", condition("NOT a IS NULL"), Ok(not(is_null(col("a"), false))));
    check!("a + b IS NULL", condition("a + b IS NULL"), Ok(is_null(bin(Op::Add, col("a"), col("b")), false)));
    check!("a IS NULL = b", condition("a IS NULL = b"), Ok(bin(Op::Eq, is_null(col("a"), false), col("b"))));
    check!("a = b = c", condition("a = b = c"), Ok(bin(Op::Eq, bin(Op::Eq, col("a"), col("b")), col("c"))));
    check!("a OR NOT b", condition("a OR NOT b"), Ok(bin(Op::Or, col("a"), not(col("b")))));
    check!("(NOT a) = b", condition("(NOT a) = b"), Ok(bin(Op::Eq, not(col("a")), col("b"))));
    check!("!= and <>", condition("a != b"), condition("a <> b"));
    check!("no spaces: 1<=-2", condition("1<=-2"), Ok(bin(Op::Le, int(1), neg(int(2)))));
}

#[test]
fn not_cannot_follow_a_stronger_operator() {
    for (sql, at) in [("SELECT 1 + NOT a FROM t", 11), ("SELECT a < NOT b FROM t", 11), ("SELECT - NOT a FROM t", 9), ("SELECT 2 * NOT a FROM t", 11)] {
        check!(sql, mistake(sql), Some((Problem::Unexpected, at, "NOT")));
    }
}

#[test]
fn the_first_token_that_cannot_continue() {
    let cases = [
        ("DELETE FROM t", 0, "DELETE"),
        ("SELECT * FROM t t2", 16, "t2"),
        ("SELECT * FROM t;;", 16, ";"),
        ("SELECT * FROM select", 14, "select"),
        ("SELECT a FROM t WHERE a IS 5", 27, "5"),
        ("SELECT a FROM t ORDER BY a DESC ASC", 32, "ASC"),
        ("SELECT a FROM t ORDER BY a ASC DESC", 31, "DESC"),
        ("SELECT a FROM t ORDER BY a + 1", 27, "+"),
        ("SELECT a FROM t LIMIT -1", 22, "-"),
        ("SELECT a FROM t LIMIT 1 WHERE a", 24, "WHERE"),
        ("SELECT a FROM t WHERE a WHERE a", 24, "WHERE"),
        ("SELECT count(*) + 1 FROM t", 16, "+"),
        ("SELECT sum(a + 1) FROM t", 13, "+"),
        ("SELECT count(1) FROM t", 13, "1"),
        ("SELECT a FROM t WHERE count(a) > 1", 27, "("),
        ("CREATE TABLE t (a float)", 18, "float"),
        ("CREATE TABLE t ()", 16, ")"),
        ("CREATE TABLE order (a int)", 13, "order"),
        ("INSERT INTO t VALUES ()", 22, ")"),
        ("INSERT INTO t VALUES (1),", 25, ""),
        ("SELECT a, FROM t", 10, "FROM"),
        ("SELECT (a FROM t", 10, "FROM"),
        ("SELECT a) FROM t", 8, ")"),
        ("SELECT a AS b FROM t", 9, "AS"),
        ("   ", 3, ""),
    ];
    for (sql, start, text) in cases {
        check!(sql, mistake(sql), Some((Problem::Unexpected, start, text)));
    }
}

#[test]
fn words_that_are_ordinary_names() {
    let sql = "SELECT count, sum + 1, Max FROM int WHERE min > count ORDER BY text";
    check!(sql, parse(sql).is_ok(), true);
    check!("count (  * )", parse("SELECT count (  * ) FROM t").is_ok(), true);
    check!("an aggregate of `*` is read here, whatever it is", parse("SELECT sum(*), min(*) FROM t").is_ok(), true);
    let sql = "SELECT (count) FROM t";
    let got = match parse(sql) {
        Ok(Statement::Select(s)) => s.items,
        other => panic!("{other:?}"),
    };
    check!(sql, got, vec![Item::Expr { expr: col("count"), name: "count".into() }]);
}

#[test]
fn item_names_are_the_source_text() {
    let Ok(Statement::Select(s)) = parse("select (A+1)*2 , 'Ü' , NOT x,x IS NULL ,((y)) from t") else {
        panic!("it parses");
    };
    let names: Vec<String> = s
        .items
        .into_iter()
        .map(|i| match i {
            Item::Expr { name, .. } => name,
            other => panic!("{other:?}"),
        })
        .collect();
    check!("names", names, ["(A+1)*2", "'Ü'", "NOT x", "x IS NULL", "y"].map(String::from).to_vec());
}

#[test]
fn tokens() {
    check!("texts keep everything", condition("a = 'X ''y'' -- ( é\n'"), Ok(bin(Op::Eq, col("a"), E::Lit(Value::Text("X 'y' -- ( é\n".into())))));
    check!("empty text", condition("''"), Ok(E::Lit(Value::Text(String::new()))));
    check!("the largest number", condition("9223372036854775807"), Ok(int(i64::MAX)));
    check!("keywords in any case", parse("SeLeCt A fRoM T"), parse("select a from t"));
    check!("a number runs into a word", mistake("SELECT 1a FROM t"), Some((Problem::Unexpected, 8, "a")));
    check!("a lone !", mistake("SELECT a ! b FROM t"), Some((Problem::BadCharacter, 9, "!")));
    check!("a wide character", mistake("SELECT é FROM t"), Some((Problem::BadCharacter, 7, "é")));
    check!("a lone quote at the end", mistake("SELECT '"), Some((Problem::UnclosedText, 7, "'")));
    check!("two quotes then open", mistake("SELECT '''"), Some((Problem::UnclosedText, 7, "'''")));
}

#[test]
fn a_bad_token_wins_over_a_grammar_mistake() {
    check!("grammar first in the text", mistake("DELETE ?"), Some((Problem::BadCharacter, 7, "?")));
    check!("open text after a mistake", mistake("SELECT FROM 'x"), Some((Problem::UnclosedText, 12, "'x")));
    check!("number after a mistake", mistake("x 99999999999999999999").map(|m| m.0), Some(Problem::NumberTooLarge));
}

fn too_deep(sql: &str) -> bool {
    matches!(parse(sql), Err(SyntaxError { problem: Problem::TooDeep, start, end }) if start <= end && end <= sql.len())
}

#[test]
fn hostile_depth() {
    let n = 100_000;
    let hostile = [
        ("brackets", format!("SELECT {}1{} FROM t", "(".repeat(n), ")".repeat(n))),
        ("open brackets only", format!("SELECT {}", "(".repeat(n))),
        ("NOT", format!("SELECT {}a FROM t", "NOT ".repeat(n))),
        ("minus", format!("SELECT {}1 FROM t", "- ".repeat(n))),
        ("a chain of +", format!("SELECT 1{} FROM t", " + 1".repeat(n))),
        ("a chain of OR", format!("SELECT * FROM t WHERE a{}", " OR a".repeat(n))),
        ("a chain of IS NULL", format!("SELECT a{} FROM t", " IS NULL".repeat(n))),
        ("in a tuple", format!("INSERT INTO t VALUES (1{})", " * 1".repeat(n))),
    ];
    for (what, sql) in hostile {
        check!(what, too_deep(&sql), true);
    }
}

#[test]
fn the_limits_are_exact() {
    let chain = |nodes: usize| format!("SELECT 1{} FROM t", " + 1".repeat(nodes - 1));
    check!("100 tall", parse(&chain(MAX_HEIGHT)).is_ok(), true);
    check!("101 tall", too_deep(&chain(MAX_HEIGHT + 1)), true);
    let nots = |nodes: usize| format!("SELECT {}a FROM t", "NOT ".repeat(nodes - 1));
    check!("99 NOTs", parse(&nots(MAX_HEIGHT)).is_ok(), true);
    check!("100 NOTs", too_deep(&nots(MAX_HEIGHT + 1)), true);
    let brackets = |n: usize| format!("SELECT {}1{} FROM t", "(".repeat(n), ")".repeat(n));
    check!("100 brackets add no height", parse(&brackets(MAX_OPEN)).is_ok(), true);
    check!("101 brackets", too_deep(&brackets(MAX_OPEN + 1)), true);
    let both = format!("SELECT {}1{} FROM t", "(- ".repeat(99), ")".repeat(99));
    check!("99 brackets and 99 minus signs", parse(&both).is_ok(), true);
    let nested = format!("SELECT {}1{} FROM t", "(1 + ".repeat(99), ")".repeat(99));
    check!("nested on the right, 100 tall", parse(&nested).is_ok(), true);
    // a tall left side under a chain counts too
    let left = format!("SELECT ({}) * 2 * 2 FROM t", vec!["1"; 99].join(" + "));
    check!("99 + 2 more levels", too_deep(&left), true);
    // the limit is per path: many shallow expressions are fine
    let wide = format!("SELECT {} FROM t", vec![format!("({})", vec!["1"; 90].join(" + ")); 50].join(", "));
    check!("fifty tall items side by side", parse(&wide).is_ok(), true);
    let brackets_in_a_row = format!("SELECT {} FROM t", vec!["(1)"; 300].join(", "));
    check!("closed brackets no longer count", parse(&brackets_in_a_row).is_ok(), true);
}

#[test]
fn a_large_insert() {
    let sql = format!("INSERT INTO t VALUES {}", vec!["(1, 'abc', NULL)"; 20_000].join(","));
    let rows = match parse(&sql) {
        Ok(Statement::Insert { rows, .. }) => rows.len(),
        other => panic!("{:?}", other.map(|_| ())),
    };
    check!("20 000 rows", rows, 20_000);
    let long = format!("SELECT '{}' FROM t", "x''".repeat(100_000));
    check!("a long text with many quotes", parse(&long).is_ok(), true);
}

const NAMES: [&str; 5] = ["a", "b_2", "count", "int", "_x"];
const OPS: [(Op, &str); 12] = [
    (Op::Or, "OR"),
    (Op::And, "AND"),
    (Op::Eq, "="),
    (Op::Ne, "<>"),
    (Op::Lt, "<"),
    (Op::Le, "<="),
    (Op::Gt, ">"),
    (Op::Ge, ">="),
    (Op::Add, "+"),
    (Op::Sub, "-"),
    (Op::Mul, "*"),
    (Op::Div, "/"),
];

/// A random tree and its text with a bracket around every operator, so no strength rule is needed to read it.
fn tree(rng: &mut Rng, depth: u32) -> (E, String) {
    if depth == 0 || rng.below(3) == 0 {
        return match rng.below(5) {
            0 => {
                let n = rng.int(0, 50);
                (int(n), n.to_string())
            }
            1 => {
                let text = rng.pick(&["", "it's", "a b", "NULL"]).to_string();
                let sql = format!("'{}'", text.replace('\'', "''"));
                (E::Lit(Value::Text(text)), sql)
            }
            2 => {
                let b = rng.bool();
                (E::Lit(Value::Bool(b)), b.to_string())
            }
            3 => (E::Lit(Value::Null), "null".to_string()),
            _ => {
                let name = *rng.pick(&NAMES);
                (col(name), name.to_string())
            }
        };
    }
    let (a, left) = tree(rng, depth - 1);
    match rng.below(5) {
        0 => (not(a), format!("(NOT {left})")),
        1 => (neg(a), format!("(- {left})")),
        2 => {
            let negated = rng.bool();
            (is_null(a, negated), format!("({left} IS {}NULL)", if negated { "NOT " } else { "" }))
        }
        _ => {
            let (b, right) = tree(rng, depth - 1);
            let (op, symbol) = *rng.pick(&OPS);
            (bin(op, a, b), format!("({left} {symbol} {right})"))
        }
    }
}

/// Changes capitals outside texts and stretches the spaces.
fn restyle(rng: &mut Rng, sql: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for c in sql.chars() {
        quoted ^= c == '\'';
        if quoted {
            out.push(c);
        } else if c == ' ' {
            out.push_str(rng.pick(&[" ", "  ", "\n", " \t"]));
        } else {
            out.push(if rng.bool() { c.to_ascii_uppercase() } else { c });
        }
    }
    out
}

/// A random statement: its text in a plain style, and its tree.
fn statement(rng: &mut Rng) -> (String, Statement) {
    let table = rng.pick(&["t", "emp", "max"]).to_string();
    match rng.below(4) {
        0 => {
            let types = [(Type::Int, "int"), (Type::Text, "text"), (Type::Bool, "bool")];
            let columns: Vec<(String, (Type, &str))> = (0..rng.int(1, 4)).map(|_| (rng.pick(&NAMES).to_string(), *rng.pick(&types))).collect();
            let sql: Vec<String> = columns.iter().map(|(n, t)| format!("{n} {}", t.1)).collect();
            let columns = columns.into_iter().map(|(n, t)| (n, t.0)).collect();
            (format!("create table {table} ({})", sql.join(", ")), Statement::CreateTable { table, columns })
        }
        1 => {
            let mut rows = Vec::new();
            let mut sql = Vec::new();
            for _ in 0..rng.int(1, 3) {
                let (row, text): (Vec<E>, Vec<String>) = (0..rng.int(1, 3)).map(|_| tree(rng, 2)).unzip();
                rows.push(row);
                sql.push(format!("({})", text.join(", ")));
            }
            (format!("insert into {table} values {}", sql.join(", ")), Statement::Insert { table, rows })
        }
        _ => {
            let mut items = Vec::new();
            let mut sql = Vec::new();
            for _ in 0..rng.int(1, 4) {
                match rng.below(4) {
                    0 => {
                        items.push(Item::Star);
                        sql.push("*".to_string());
                    }
                    1 => {
                        let all = [(Aggregate::Count, "count"), (Aggregate::Sum, "sum"), (Aggregate::Min, "min"), (Aggregate::Max, "max")];
                        let (of, word) = *rng.pick(&all);
                        let column = rng.bool().then(|| rng.pick(&NAMES).to_string());
                        sql.push(format!("{word}({})", column.as_deref().unwrap_or("*")));
                        items.push(Item::Aggregate { of, column });
                    }
                    _ => {
                        // a column name is a name; anything else is called by its text, which restyling changes, so
                        // those names are checked elsewhere
                        let name = rng.pick(&NAMES).to_string();
                        sql.push(name.clone());
                        items.push(Item::Expr { expr: col(&name), name });
                    }
                }
            }
            let mut text = format!("select {} from {table}", sql.join(", "));
            let filter = rng.bool().then(|| tree(rng, 4)).map(|(e, s)| {
                text.push_str(&format!(" where {s}"));
                e
            });
            let group_by = rng.bool().then(|| rng.pick(&NAMES).to_string());
            if let Some(g) = &group_by {
                text.push_str(&format!(" group by {g}"));
            }
            let order_by = rng.bool().then(|| OrderBy { column: rng.pick(&NAMES).to_string(), descending: rng.bool() });
            if let Some(o) = &order_by {
                let direction = if o.descending { " desc" } else { *rng.pick(&["", " asc"]) };
                text.push_str(&format!(" order by {}{direction}", o.column));
            }
            let limit = rng.bool().then(|| rng.below(100) as u64);
            if let Some(n) = limit {
                text.push_str(&format!(" limit {n}"));
            }
            (text, Statement::Select(Select { items, table, filter, group_by, order_by, limit }))
        }
    }
}

#[test]
fn random_statements_read_back() {
    let mut rng = Rng::new(18201);
    for _ in 0..1000 {
        let (plain, want) = statement(&mut rng);
        let mut sql = restyle(&mut rng, &plain);
        if rng.bool() {
            sql.push(';');
        }
        check!(format!("{sql:?}"), parse(&sql), Ok(want));
    }
}

#[test]
fn a_statement_cut_short_is_whole_or_stops_too_early() {
    let mut rng = Rng::new(18202);
    for _ in 0..500 {
        let (sql, _) = statement(&mut rng);
        // cut at a space outside any text
        let mut quoted = false;
        let cuts: Vec<usize> = sql.char_indices().filter(|&(_, c)| {
            quoted ^= c == '\'';
            !quoted && c == ' '
        }).map(|(i, _)| i).collect();
        let cut = &sql[..*rng.pick(&cuts)];
        if let Err(e) = parse(cut) {
            check!(format!("{cut:?}"), (e.problem, e.start, e.end), (Problem::Unexpected, cut.len(), cut.len()));
        }
    }
}

/// Reads a chain with no brackets by splitting it at the weakest operator, the rightmost of equals.
fn split(operands: &[i64], ops: &[(Op, &str)]) -> E {
    let strength = |op: Op| match op {
        Op::Or => 1,
        Op::And => 2,
        Op::Add | Op::Sub => 5,
        Op::Mul | Op::Div => 6,
        _ => 4,
    };
    let weakest = ops.iter().enumerate().rev().min_by_key(|(_, o)| strength(o.0));
    match weakest {
        None => int(operands[0]),
        Some((at, &(op, _))) => bin(op, split(&operands[..=at], &ops[..at]), split(&operands[at + 1..], &ops[at + 1..])),
    }
}

#[test]
fn random_chains_without_brackets() {
    let mut rng = Rng::new(18203);
    for _ in 0..800 {
        let len = rng.int(0, 7) as usize;
        let ops: Vec<(Op, &str)> = (0..len).map(|_| *rng.pick(&OPS)).collect();
        let operands: Vec<i64> = (0..=len).map(|_| rng.int(0, 9)).collect();
        let mut sql = operands[0].to_string();
        for (op, n) in ops.iter().zip(&operands[1..]) {
            sql.push_str(&format!(" {} {n}", op.1));
        }
        check!(sql.clone(), condition(&sql), Ok(split(&operands, &ops)));
    }
}

#[test]
fn any_text_at_all() {
    let mut rng = Rng::new(18204);
    let bits = ["select", "from", "where", "insert", "into", "values", "create", "table", "(", ")", ",", "*", "-", "=", "'", "''", " ", " ", "a", "1", "not", "is", "null", "é", "!", "<", ">", ";", "count", "order", "by", "limit"];
    for _ in 0..5000 {
        let len = rng.below(14);
        let sql: String = (0..len).map(|_| *rng.pick(&bits)).collect::<Vec<_>>().join(if rng.bool() { " " } else { "" });
        if let Err(e) = parse(&sql) {
            let inside = e.start <= e.end && e.end <= sql.len() && sql.is_char_boundary(e.start) && sql.is_char_boundary(e.end);
            check!(format!("{sql:?} points inside the text"), inside, true);
        }
    }
}
