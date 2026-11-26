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
        std::mem::take(&mut self.items[0])
    }

    /// Makes the last item current and returns the job that was current before, if any.
    pub fn start_next(&mut self) -> Option<String> {
        let next = self.items.pop();
        std::mem::replace(&mut self.current, next)
    }

    /// Moves the current job to `done`. Returns whether there was one.
    pub fn finish(&mut self) -> bool {
        match self.current.take() {
            Some(job) => {
                self.done.push(job);
                true
            }
            None => false,
        }
    }

    /// Hands over every finished job, oldest first, leaving `done` empty.
    pub fn drain_done(&mut self) -> Vec<String> {
        std::mem::take(&mut self.done)
    }

    /// Swaps this queue's waiting items with `other`'s. Names, current jobs and finished jobs stay put.
    pub fn swap_items(&mut self, other: &mut Queue) {
        std::mem::swap(&mut self.items, &mut other.items);
    }
}
