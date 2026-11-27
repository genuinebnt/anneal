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

/// A method that has been found: the class that provides it, and its name.
#[derive(Debug, Clone, Copy)]
struct Found<'a> {
    owner: usize,
    actions: &'a [Action],
}

#[derive(Debug, Clone, Copy)]
struct Bound<'a> {
    object: usize,
    method: Found<'a>,
}

struct Object<'a> {
    class: usize,
    fields: HashMap<&'a str, Bound<'a>>,
}

struct World<'a> {
    classes: &'a [Class],
    objects: Vec<Object<'a>>,
    out: Vec<String>,
}

impl<'a> World<'a> {
    /// The nearest class, from `from` upwards, that has the method.
    fn find(&self, from: Option<usize>, name: &str) -> Result<Found<'a>, Error> {
        let classes = self.classes;
        std::iter::successors(from, |&i| classes[i].parent)
            .find_map(|i| {
                let actions = classes[i].methods.get(name)?;
                Some(Found { owner: i, actions })
            })
            .ok_or_else(|| Error::NoMethod(name.to_string()))
    }

    fn invoke(&mut self, call: Bound<'a>, running: usize) -> Result<(), Error> {
        if running == MAX_RUNNING {
            return Err(Error::TooDeep);
        }
        let object = call.object;
        for action in call.method.actions {
            let own = Some(self.objects[object].class);
            let above = self.classes[self.objects[object].class].parent;
            match action {
                Action::Say(text) => self.out.push(text.clone()),
                Action::Own(name) | Action::Inherited(name) => {
                    let from = if matches!(action, Action::Own(_)) {
                        own
                    } else {
                        above
                    };
                    let method = self.find(from, name)?;
                    self.invoke(Bound { object, method }, running + 1)?;
                }
                Action::Keep { method, field } | Action::KeepInherited { method, field } => {
                    let from = if matches!(action, Action::Keep { .. }) {
                        own
                    } else {
                        above
                    };
                    let method = self.find(from, method)?;
                    self.objects[object]
                        .fields
                        .insert(field, Bound { object, method });
                }
            }
        }
        Ok(())
    }
}

/// Runs the commands in order. Returns everything said, and the first error, which ends the script.
pub fn play(classes: &[Class], script: &[Command]) -> (Vec<String>, Option<Error>) {
    let mut world = World {
        classes,
        objects: Vec::new(),
        out: Vec::new(),
    };
    for command in script {
        let result = match command {
            Command::New(class) => {
                world.objects.push(Object {
                    class: *class,
                    fields: HashMap::new(),
                });
                Ok(())
            }
            Command::Send(object, name) => {
                let class = Some(world.objects[*object].class);
                world.find(class, name).and_then(|method| {
                    let object = *object;
                    world.invoke(Bound { object, method }, 0)
                })
            }
            Command::Fire(object, field) => {
                match world.objects[*object].fields.get(field.as_str()).copied() {
                    Some(bound) => world.invoke(bound, 0),
                    None => Err(Error::EmptyField(field.clone())),
                }
            }
        };
        if let Err(error) = result {
            return (world.out, Some(error));
        }
    }
    (world.out, None)
}
