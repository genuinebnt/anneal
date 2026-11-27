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
fn let_and_arithmetic() {
    use build::*;
    check!(
        "let x = 2; x + x",
        big(|| le("x", n(2), add(v("x"), v("x")))),
        Ok(4)
    );
}

#[test]
fn a_function_sees_where_it_was_made() {
    use build::*;
    // let a = 1; let get = fn() a; let a = 2; get()
    let p = || {
        le(
            "a",
            n(1),
            le(
                "get",
                fun(&[], v("a")),
                le("a", n(2), call(v("get"), vec![])),
            ),
        )
    };
    check!("let a = 1; get = fn() a; let a = 2; get()", big(p), Ok(1));
}

#[test]
fn a_variable_is_shared_not_copied() {
    use build::*;
    // let a = 1; let get = fn() a; set a = 7; get()
    let p = || {
        le(
            "a",
            n(1),
            le(
                "get",
                fun(&[], v("a")),
                then(set("a", n(7)), call(v("get"), vec![])),
            ),
        )
    };
    check!("set after the function was made", big(p), Ok(7));
}

#[test]
fn a_counter_outlives_its_maker() {
    use build::*;
    // let make = fn() (let c = 0; fn() set c = c + 1); let tick = make(); tick(); tick()
    let p = || {
        let make = fun(&[], le("c", n(0), fun(&[], set("c", add(v("c"), n(1))))));
        le(
            "make",
            make,
            le(
                "tick",
                call(v("make"), vec![]),
                then(call(v("tick"), vec![]), call(v("tick"), vec![])),
            ),
        )
    };
    check!("two ticks", big(p), Ok(2));
}

#[test]
fn a_function_that_calls_itself() {
    use build::*;
    let p = || {
        let body = iff(
            less(v("k"), n(1)),
            n(0),
            add(v("k"), call(v("sum"), vec![add(v("k"), n(-1))])),
        );
        rec("sum", &["k"], body, call(v("sum"), vec![n(100)]))
    };
    check!("sum(100)", big(p), Ok(5050));
}
