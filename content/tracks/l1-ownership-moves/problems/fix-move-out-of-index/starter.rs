#[derive(Debug, PartialEq)]
pub struct Queue {
    pub name: String,
    pub items: Vec<String>,
    pub current: Option<String>,
    pub done: Vec<String>,
}

impl Queue {
    pub fn new(name: &str, items: Vec<String>) -> Self {
        Queue {
            name: String::from(name),
            items,
            current: None,
            done: Vec::new(),
        }
    }

    /// Moves the first item out, leaving "" in its slot so the other items keep their positions.
    /// `items` is never empty when this is called.
    pub fn take_first(&mut self) -> String {
        let first = self.items[0];
        first
    }

    /// Makes the last item current and returns the job that was current before, if any.
    pub fn start_next(&mut self) -> Option<String> {
        let previous = self.current;
        self.current = self.items.pop();
        previous
    }

    /// Moves the current job to `done`. Returns whether there was one.
    pub fn finish(&mut self) -> bool {
        match self.current {
            Some(job) => {
                self.done.push(job);
                true
            }
            None => false,
        }
    }

    /// Hands over every finished job, oldest first, leaving `done` empty.
    pub fn drain_done(&mut self) -> Vec<String> {
        self.done
    }

    /// Swaps this queue's waiting items with `other`'s. Names, current jobs and finished jobs stay put.
    pub fn swap_items(&mut self, other: &mut Queue) {
        let mine = self.items;
        self.items = other.items;
        other.items = mine;
    }
}
