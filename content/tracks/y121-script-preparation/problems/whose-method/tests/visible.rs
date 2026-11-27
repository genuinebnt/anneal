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
fn a_child_replaces_a_method() {
    use build::*;
    check!(
        "C who",
        play(&family(), &[Command::New(2), send(0, "who")]),
        said(&["C"])
    );
}

#[test]
fn an_inherited_method_asks_the_object() {
    use build::*;
    check!(
        "C hello",
        play(&family(), &[Command::New(2), send(0, "hello")]),
        said(&["hi", "C"])
    );
}

#[test]
fn the_parent_is_the_parent_of_the_provider() {
    use build::*;
    // `ask` is provided by B, so it goes to A, even for a C.
    check!(
        "C ask",
        play(&family(), &[Command::New(2), send(0, "ask")]),
        said(&["A"])
    );
}

#[test]
fn a_missing_method() {
    use build::*;
    check!(
        "A ask",
        play(
            &family(),
            &[Command::New(0), send(0, "who"), send(0, "ask")]
        ),
        failed(&["A"], Error::NoMethod("ask".into()))
    );
}

#[test]
fn a_stored_method_keeps_its_object() {
    use build::*;
    let classes = vec![class(
        "Bell",
        None,
        vec![
            ("ring", vec![say("ding")]),
            ("arm", vec![keep("ring", "alarm")]),
        ],
    )];
    let script = [
        Command::New(0),
        send(0, "arm"),
        fire(0, "alarm"),
        fire(0, "alarm"),
    ];
    check!(
        "fire twice",
        play(&classes, &script),
        said(&["ding", "ding"])
    );
}
