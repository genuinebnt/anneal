use solution::*;

fn sample() -> Index {
    let mut index = Index::new();
    index.add_batch(&["rust async runtime", "rust memory ordering"]);
    index.add_batch(&["go async broadcast", "ordering in memory"]);
    index
}

fn ask(index: &Index, query: &str) -> Vec<u32> {
    index.matches(&parse_query(query).expect("a good query"))
}

#[test]
fn and_or_not() {
    let i = sample();
    check!("rust async", ask(&i, "rust async"), vec![0]);
    check!("rust OR async", ask(&i, "rust OR async"), vec![0, 1, 2]);
    check!("NOT rust", ask(&i, "NOT rust"), vec![2, 3]);
    check!("async NOT rust", ask(&i, "async NOT rust"), vec![2]);
}

#[test]
fn phrases_and_patterns() {
    let i = sample();
    check!("\"memory ordering\"", ask(&i, "\"memory ordering\""), vec![1]);
    check!("*cast* OR *tim*", ask(&i, "*cast* OR *tim*"), vec![0, 2]);
}

#[test]
fn brackets() {
    check!("(rust OR go) async", ask(&sample(), "(rust OR go) async"), vec![0, 2]);
}

#[test]
fn nothing_and_its_opposite() {
    let i = sample();
    check!("\"\"", ask(&i, "\"\""), Vec::<u32>::new());
    check!("NOT \"\"", ask(&i, "NOT \"\""), vec![0, 1, 2, 3]);
}

#[test]
fn lists_with_no_items() {
    let i = sample();
    check!("And([])", i.matches(&Expr::And(vec![])), vec![0, 1, 2, 3]);
    check!("Or([])", i.matches(&Expr::Or(vec![])), Vec::<u32>::new());
}
