#[derive(Debug, PartialEq)]
pub struct Job {
    pub name: String,
    pub deadline: u64,
}

pub struct Queue {
    pub jobs: Vec<Job>,
}

impl Queue {
    /// Removes up to `limit` expired jobs (deadline before `now`), taking the earliest in the queue first, and
    /// returns them in queue order. Expired jobs past the limit stay queued where they are.
    pub fn take_expired(&mut self, now: u64, limit: usize) -> Vec<Job> {
        self.jobs
            .extract_if(.., |j| j.deadline < now)
            .take(limit)
            .collect()
    }

    /// Removes the first `n` jobs (all of them if there are fewer) and returns them in order.
    pub fn next_batch(&mut self, n: usize) -> Vec<Job> {
        let n = n.min(self.jobs.len());
        self.jobs.drain(..n).collect()
    }

    /// Moves the first job named `name` to the back of the queue, keeping the others in order. `false` if
    /// there's no such job.
    pub fn defer(&mut self, name: &str) -> bool {
        match self.jobs.iter().position(|j| j.name == name) {
            Some(i) => {
                self.jobs[i..].rotate_left(1);
                true
            }
            None => false,
        }
    }
}
