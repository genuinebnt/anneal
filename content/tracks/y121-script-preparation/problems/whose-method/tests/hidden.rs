use solution::*;

#[allow(dead_code)]
mod build {
    use solution::*;
    pub fn say(t: &str) -> Action {
        Action::Say(t.into())
    }
    pub fn own(m: &str) -> Action {
        Action::Own(m.into())
    }
    pub fn up(m: &str) -> Action {
        Action::Inherited(m.into())
    }
    pub fn keep(m: &str, f: &str) -> Action {
        Action::Keep {
            method: m.into(),
            field: f.into(),
        }
    }
    pub fn keep_up(m: &str, f: &str) -> Action {
        Action::KeepInherited {
            method: m.into(),
            field: f.into(),
        }
    }
    pub fn class(name: &str, parent: Option<usize>, methods: Vec<(&str, Vec<Action>)>) -> Class {
        Class {
            name: name.into(),
            parent,
            methods: methods
                .into_iter()
                .map(|(n, a)| (n.to_string(), a))
                .collect(),
        }
    }
    pub fn send(o: usize, m: &str) -> Command {
        Command::Send(o, m.into())
    }
    pub fn fire(o: usize, f: &str) -> Command {
        Command::Fire(o, f.into())
    }
}

#[allow(dead_code)]
fn said(words: &[&str]) -> (Vec<String>, Option<Error>) {
    (words.iter().map(|w| w.to_string()).collect(), None)
}

#[allow(dead_code)]
fn failed(words: &[&str], error: Error) -> (Vec<String>, Option<Error>) {
    (words.iter().map(|w| w.to_string()).collect(), Some(error))
}

/// A, B under A, C under B. Each class says its letter in `who`.
#[allow(dead_code)]
fn family() -> Vec<Class> {
    use build::*;
    vec![
        class(
            "A",
            None,
            vec![
                ("who", vec![say("A")]),
                ("hello", vec![say("hi"), own("who")]),
            ],
        ),
        class(
            "B",
            Some(0),
            vec![("who", vec![say("B")]), ("ask", vec![up("who")])],
        ),
        class("C", Some(1), vec![("who", vec![say("C")])]),
    ]
}

#[test]
fn empty_fields_and_separate_objects() {
    use build::*;
    let classes = vec![class(
        "Bell",
        None,
        vec![
            ("ring", vec![say("ding")]),
            ("arm", vec![keep("ring", "alarm")]),
        ],
    )];
    check!(
        "nothing stored",
        play(&classes, &[Command::New(0), fire(0, "alarm")]),
        failed(&[], Error::EmptyField("alarm".into()))
    );
    let script = [
        Command::New(0),
        Command::New(0),
        send(0, "arm"),
        fire(1, "alarm"),
    ];
    check!(
        "the other object",
        play(&classes, &script),
        failed(&[], Error::EmptyField("alarm".into()))
    );
    check!("empty script", play(&classes, &[]), said(&[]));
}

#[test]
fn storing_a_method_does_not_run_it() {
    use build::*;
    let classes = vec![class(
        "X",
        None,
        vec![
            ("a", vec![keep("b", "f"), say("kept")]),
            ("b", vec![say("b")]),
        ],
    )];
    check!(
        "keep only",
        play(&classes, &[Command::New(0), send(0, "a")]),
        said(&["kept"])
    );
    let classes = vec![class(
        "X",
        None,
        vec![("a", vec![say("1"), keep("nothing", "f"), say("2")])],
    )];
    check!(
        "keep a missing one",
        play(&classes, &[Command::New(0), send(0, "a")]),
        failed(&["1"], Error::NoMethod("nothing".into()))
    );
}

#[test]
fn a_stored_method_does_not_search_again() {
    use build::*;
    let classes = vec![
        class("A", None, vec![("who", vec![say("A")])]),
        class(
            "B",
            Some(0),
            vec![
                ("who", vec![say("B")]),
                ("grab", vec![keep_up("who", "f")]),
                ("mine", vec![keep("who", "f")]),
            ],
        ),
    ];
    let script = [
        Command::New(1),
        send(0, "grab"),
        fire(0, "f"),
        send(0, "mine"),
        fire(0, "f"),
    ];
    check!(
        "the parent's, then its own",
        play(&classes, &script),
        said(&["A", "B"])
    );
}

#[test]
fn a_stored_method_still_belongs_to_its_provider() {
    use build::*;
    // B provides `go`, which goes to A's `step`. Stored on a C and fired later, it still must.
    let classes = vec![
        class(
            "A",
            None,
            vec![
                ("step", vec![say("A.step"), own("name")]),
                ("name", vec![say("A")]),
            ],
        ),
        class(
            "B",
            Some(0),
            vec![("step", vec![say("B.step")]), ("go", vec![up("step")])],
        ),
        class(
            "C",
            Some(1),
            vec![
                ("step", vec![say("C.step")]),
                ("name", vec![say("C")]),
                ("save", vec![keep_up("go", "g")]),
                ("save2", vec![keep("go", "h")]),
            ],
        ),
    ];
    let script = [
        Command::New(2),
        send(0, "save"),
        fire(0, "g"),
        send(0, "save2"),
        fire(0, "h"),
    ];
    check!(
        "both go to A.step",
        play(&classes, &script),
        said(&["A.step", "C", "A.step", "C"])
    );
}

#[test]
fn two_levels_of_inherited() {
    use build::*;
    let classes = vec![
        class("A", None, vec![("m", vec![say("A")])]),
        class(
            "B",
            Some(0),
            vec![("m", vec![say("B("), up("m"), say(")")])],
        ),
        class(
            "C",
            Some(1),
            vec![("m", vec![say("C("), up("m"), say(")")])],
        ),
        class("D", Some(2), vec![]),
    ];
    check!(
        "D m",
        play(&classes, &[Command::New(3), send(0, "m")]),
        said(&["C(", "B(", "A", ")", ")"])
    );
}

#[test]
fn the_parent_may_get_it_from_further_up() {
    use build::*;
    let classes = vec![
        class("A", None, vec![("m", vec![say("A")])]),
        class("B", Some(0), vec![]),
        class("C", Some(1), vec![("m", vec![up("m")])]),
        class("Other", None, vec![("m", vec![up("m")])]),
    ];
    check!(
        "C m",
        play(&classes, &[Command::New(2), send(0, "m")]),
        said(&["A"])
    );
    check!(
        "no parent",
        play(&classes, &[Command::New(3), send(0, "m")]),
        failed(&[], Error::NoMethod("m".into()))
    );
}

#[test]
fn fifty_methods_may_run_at_once() {
    use build::*;
    fn countdown(n: usize) -> Vec<Class> {
        use build::*;
        let names: Vec<String> = (0..n).map(|i| format!("m{i}")).collect();
        let methods = (0..n)
            .map(|i| {
                let actions = if i + 1 < n {
                    vec![own(&names[i + 1])]
                } else {
                    vec![say("end")]
                };
                (names[i].as_str(), actions)
            })
            .collect();
        vec![class("X", None, methods)]
    }
    check!(
        "50",
        play(&countdown(50), &[Command::New(0), send(0, "m0")]),
        said(&["end"])
    );
    check!(
        "51",
        play(&countdown(51), &[Command::New(0), send(0, "m0")]),
        failed(&[], Error::TooDeep)
    );
    let loops = vec![class("L", None, vec![("a", vec![own("a")])])];
    check!(
        "for ever",
        play(&loops, &[Command::New(0), send(0, "a")]),
        failed(&[], Error::TooDeep)
    );
    let many = vec![class(
        "L",
        None,
        vec![("a", vec![say(".")]), ("b", vec![own("a"); 200])],
    )];
    check!(
        "one after another",
        play(&many, &[Command::New(0), send(0, "b")]).1,
        None::<Error>
    );
}

#[test]
fn a_very_long_chain_of_parents() {
    use build::*;
    let mut classes = vec![class("c0", None, vec![("deep", vec![say("root")])])];
    for i in 1..60_000 {
        classes.push(class("c", Some(i - 1), vec![]));
    }
    let thread = std::thread::Builder::new().stack_size(256 << 10);
    let work = move || play(&classes, &[Command::New(59_999), send(0, "deep")]);
    let got = thread
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("play panicked");
    check!("60 000 classes", got, said(&["root"]));
}

// Random families in which every method says who provides it, and may then pass the same message upwards. What is
// said for one message is therefore the list of providers on the way up.

#[test]
fn random_families() {
    use build::*;
    let mut rng = anneal_prelude::Rng::new(6601);
    for _ in 0..500 {
        let count = 1 + rng.below(7);
        let mut classes = Vec::new();
        let mut passes = Vec::new();
        for i in 0..count {
            let parent = if i > 0 && rng.below(5) > 0 {
                Some(rng.below(i))
            } else {
                None
            };
            let has = rng.below(3) > 0;
            let goes_up = rng.bool();
            let mut methods = Vec::new();
            if has {
                let mut actions = vec![say(&format!("c{i}"))];
                if goes_up {
                    actions.push(up("m"));
                }
                methods.push(("m", actions));
            }
            passes.push((parent, has, goes_up));
            classes.push(class("k", parent, methods));
        }
        let target = rng.below(count);
        let (mut want, mut error) = (Vec::new(), None);
        let mut at = Some(target);
        loop {
            while at.is_some_and(|i| !passes[i].1) {
                at = passes[at.unwrap()].0;
            }
            let Some(i) = at else {
                error = Some(Error::NoMethod("m".into()));
                break;
            };
            want.push(format!("c{i}"));
            if !passes[i].2 {
                break;
            }
            at = passes[i].0;
        }
        let text = format!("classes (parent, has m, goes up) {passes:?}, send to class {target}");
        check!(
            text,
            play(&classes, &[Command::New(target), send(0, "m")]),
            (want, error)
        );
    }
}
