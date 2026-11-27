use std::collections::HashMap;

/// One thing a method does, in order.
#[derive(Debug, Clone)]
pub enum Action {
    /// Adds the text to the output.
    Say(String),
    /// Sends a message to the same object.
    Own(String),
    /// Runs the version that the parent of this class provides.
    Inherited(String),
    /// Stores, under a field name, the method that `Own` would run now, together with this object.
    Keep { method: String, field: String },
    /// Stores the method that `Inherited` would run now, together with this object.
    KeepInherited { method: String, field: String },
}

#[derive(Debug, Clone)]
pub struct Class {
    pub name: String,
    /// The index of the parent in the list of classes. It is smaller than this class's own index.
    pub parent: Option<usize>,
    pub methods: HashMap<String, Vec<Action>>,
}

/// What a script does with its objects.
#[derive(Debug, Clone)]
pub enum Command {
    /// Makes a new object of the class with this index. Objects are numbered from 0.
    New(usize),
    /// Sends a message to an object.
    Send(usize, String),
    /// Runs what is stored in a field of an object.
    Fire(usize, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoMethod(String),
    EmptyField(String),
    TooDeep,
}

/// Methods that may be running at once.
pub const MAX_RUNNING: usize = 50;

/// Runs the commands in order. Returns everything said, and the first error, which ends the script.
pub fn play(classes: &[Class], script: &[Command]) -> (Vec<String>, Option<Error>) {
    todo!()
}
