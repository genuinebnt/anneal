use solution::*;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn n(x: i64) -> Expr {
        Expr::Num(x)
    }
    pub fn v(x: &str) -> Expr {
        Expr::Var(x.into())
    }
    pub fn add(a: Expr, b: Expr) -> Expr {
        Expr::Add(Box::new(a), Box::new(b))
    }
    pub fn less(a: Expr, b: Expr) -> Expr {
        Expr::Less(Box::new(a), Box::new(b))
    }
    pub fn iff(c: Expr, a: Expr, b: Expr) -> Expr {
        Expr::If(Box::new(c), Box::new(a), Box::new(b))
    }
    pub fn le(x: &str, value: Expr, body: Expr) -> Expr {
        Expr::Let(x.into(), Box::new(value), Box::new(body))
    }
    pub fn set(x: &str, value: Expr) -> Expr {
        Expr::Set(x.into(), Box::new(value))
    }
    pub fn then(a: Expr, b: Expr) -> Expr {
        Expr::Then(Box::new(a), Box::new(b))
    }
    fn names(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }
    pub fn fun(params: &[&str], body: Expr) -> Expr {
        Expr::Function(names(params), std::rc::Rc::new(body))
    }
    pub fn rec(name: &str, params: &[&str], body: Expr, rest: Expr) -> Expr {
        Expr::Recursive(
            name.into(),
            names(params),
            std::rc::Rc::new(body),
            Box::new(rest),
        )
    }
    pub fn call(f: Expr, args: Vec<Expr>) -> Expr {
        Expr::Call(Box::new(f), args)
    }
}

/// `run` on a thread with room for deep recursion. The tree is built there too.
#[allow(dead_code)]
fn big(make: impl FnOnce() -> Expr + Send + 'static) -> Result<i64, EvalError> {
    let thread = std::thread::Builder::new().stack_size(64 << 20);
    thread
        .spawn(move || run(&make()))
        .expect("spawn")
        .join()
        .expect("run panicked")
}

#[test]
fn every_error() {
    use build::*;
    use EvalError::*;
    let cases: Vec<(&str, fn() -> Expr, EvalError)> = vec![
        ("y", || v("y"), Unknown("y".into())),
        ("set y = 1", || set("y", n(1)), Unknown("y".into())),
        ("set y = z", || set("y", v("z")), Unknown("z".into())),
        (
            "let x = x; 0",
            || le("x", v("x"), n(0)),
            Unknown("x".into()),
        ),
        ("fn() 0", || fun(&[], n(0)), NotANumber),
        ("fn + 1", || add(fun(&[], n(0)), n(1)), NotANumber),
        (
            "fn + y",
            || add(fun(&[], n(0)), v("y")),
            Unknown("y".into()),
        ),
        (
            "fn < y",
            || less(fun(&[], n(0)), v("y")),
            Unknown("y".into()),
        ),
        ("if fn", || iff(fun(&[], n(0)), n(1), n(2)), NotANumber),
        ("1()", || call(n(1), vec![]), NotAFunction),
        ("1(y)", || call(n(1), vec![v("y")]), Unknown("y".into())),
        (
            "(fn(a) a)()",
            || call(fun(&["a"], v("a")), vec![]),
            WrongCount,
        ),
        (
            "(fn() 0)(1)",
            || call(fun(&[], n(0)), vec![n(1)]),
            WrongCount,
        ),
        (
            "first error wins",
            || add(v("p"), v("q")),
            Unknown("p".into()),
        ),
        ("then stops", || then(v("p"), v("q")), Unknown("p".into())),
    ];
    for (text, program, error) in cases {
        check!(text, big(program), Err(error));
    }
}

#[test]
fn only_the_chosen_branch_runs() {
    use build::*;
    check!("if 1", big(|| iff(n(1), n(5), v("nope"))), Ok(5));
    check!("if 0", big(|| iff(n(0), v("nope"), n(6))), Ok(6));
    check!("if -3", big(|| iff(n(-3), n(7), n(8))), Ok(7));
}

#[test]
fn the_caller_does_not_lend_its_variables() {
    use build::*;
    let p = || {
        le(
            "peek",
            fun(&[], v("secret")),
            le("secret", n(1), call(v("peek"), vec![])),
        )
    };
    check!(
        "made before secret existed",
        big(p),
        Err(EvalError::Unknown("secret".into()))
    );
    let p = || {
        let caller = fun(&["f"], le("a", n(100), call(v("f"), vec![])));
        le(
            "a",
            n(1),
            le("f", fun(&[], v("a")), call(caller, vec![v("f")])),
        )
    };
    check!("the caller has its own a", big(p), Ok(1));
}

#[test]
fn set_changes_the_nearest_and_the_hidden_one_stays() {
    use build::*;
    let p = || le("x", n(1), then(le("x", n(10), set("x", n(20))), v("x")));
    check!("inner set", big(p), Ok(1));
    let p = || {
        le(
            "x",
            n(1),
            then(call(fun(&["x"], set("x", n(9))), vec![v("x")]), v("x")),
        )
    };
    check!("a parameter is its own variable", big(p), Ok(1));
    let p = || {
        le(
            "x",
            n(1),
            then(call(fun(&[], set("x", n(9))), vec![]), v("x")),
        )
    };
    check!("set from inside a function", big(p), Ok(9));
}

#[test]
fn two_functions_share_one_variable() {
    use build::*;
    let p = || {
        let both = le(
            "c",
            n(0),
            le(
                "bump",
                fun(&[], set("c", add(v("c"), n(1)))),
                fun(&["which"], iff(v("which"), call(v("bump"), vec![]), v("c"))),
            ),
        );
        le(
            "o",
            both,
            then(
                call(v("o"), vec![n(1)]),
                then(call(v("o"), vec![n(1)]), call(v("o"), vec![n(0)])),
            ),
        )
    };
    check!("bump, bump, read", big(p), Ok(2));
}

#[test]
fn each_call_makes_new_variables() {
    use build::*;
    let p = || {
        let make = fun(
            &["start"],
            fun(&["d"], set("start", add(v("start"), v("d")))),
        );
        le(
            "make",
            make,
            le(
                "a",
                call(v("make"), vec![n(100)]),
                le(
                    "b",
                    call(v("make"), vec![n(500)]),
                    then(
                        call(v("a"), vec![n(1)]),
                        then(
                            call(v("b"), vec![n(2)]),
                            add(call(v("a"), vec![n(0)]), call(v("b"), vec![n(0)])),
                        ),
                    ),
                ),
            ),
        )
    };
    check!("two accounts", big(p), Ok(603));
}

#[test]
fn a_recursive_function_can_be_handed_out() {
    use build::*;
    let p = || {
        let body = iff(
            less(v("k"), n(1)),
            v("base"),
            call(v("down"), vec![add(v("k"), n(-1))]),
        );
        let maker = fun(&["base"], rec("down", &["k"], body, v("down")));
        le(
            "f",
            call(maker, vec![n(42)]),
            le("down", n(0), call(v("f"), vec![n(10)])),
        )
    };
    check!("returned from its maker", big(p), Ok(42));
    let p = || {
        let body = call(v("g"), vec![]);
        rec(
            "g",
            &[],
            n(1),
            then(le("keep", v("g"), n(0)), call(fun(&[], body), vec![])),
        )
    };
    check!("used from a function in the rest", big(p), Ok(1));
}

#[test]
fn two_hundred_calls_and_no_more() {
    use build::*;
    fn depth(k: i64) -> Expr {
        use build::*;
        let body = iff(
            less(v("k"), n(2)),
            n(1),
            call(v("d"), vec![add(v("k"), n(-1))]),
        );
        rec("d", &["k"], body, call(v("d"), vec![n(k)]))
    }
    check!("200 deep", big(|| depth(200)), Ok(1));
    check!("201 deep", big(|| depth(201)), Err(EvalError::TooDeep));
    let forever = || rec("f", &[], call(v("f"), vec![]), call(v("f"), vec![]));
    check!("for ever", big(forever), Err(EvalError::TooDeep));
    // Calls that have returned do not count.
    let many = || {
        let body = iff(
            less(v("k"), n(2)),
            n(1),
            add(
                call(v("fib"), vec![add(v("k"), n(-1))]),
                call(v("fib"), vec![add(v("k"), n(-2))]),
            ),
        );
        rec("fib", &["k"], body, call(v("fib"), vec![n(15)]))
    };
    check!("fib(15)", big(many), Ok(987));
}

#[test]
fn addition_wraps() {
    use build::*;
    check!("max + 1", big(|| add(n(i64::MAX), n(1))), Ok(i64::MIN));
}

// A bank of accounts, each a function holding its own balance, driven at random and compared with an array.

#[allow(dead_code)]
fn bank(deposits: &[(usize, i64)], accounts: usize, ask: usize) -> Expr {
    use build::*;
    let mut body = call(v(&format!("a{ask}")), vec![n(0)]);
    for &(who, amount) in deposits.iter().rev() {
        body = then(call(v(&format!("a{who}")), vec![n(amount)]), body);
    }
    for i in (0..accounts).rev() {
        body = le(
            &format!("a{i}"),
            call(v("open"), vec![n(i as i64 * 1000)]),
            body,
        );
    }
    let account = le(
        "balance",
        v("start"),
        fun(&["d"], set("balance", add(v("balance"), v("d")))),
    );
    le("open", fun(&["start"], account), body)
}

#[test]
fn random_accounts() {
    let mut rng = anneal_prelude::Rng::new(6401);
    for _ in 0..150 {
        let accounts = 1 + rng.below(5);
        let deposits: Vec<(usize, i64)> = (0..rng.below(40))
            .map(|_| {
                let who = rng.below(accounts);
                (who, rng.int(-50, 50))
            })
            .collect();
        let ask = rng.below(accounts);
        let want: i64 = ask as i64 * 1000
            + deposits
                .iter()
                .filter(|d| d.0 == ask)
                .map(|d| d.1)
                .sum::<i64>();
        let text = format!("{accounts} accounts, deposits {deposits:?}, ask {ask}");
        let moves = deposits.clone();
        check!(text, big(move || bank(&moves, accounts, ask)), Ok(want));
    }
}
