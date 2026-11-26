use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, PartialEq, Eq)]
pub struct Report<J> {
    pub completed: Vec<J>,
    pub interrupted: Option<J>,
    pub unstarted: Vec<J>,
}

pub async fn run_worker<J, F, Fut>(
    mut jobs: mpsc::Receiver<J>,
    shutdown: CancellationToken,
    grace: Duration,
    mut handle: F,
) -> Report<J>
where
    J: Clone,
    F: FnMut(J) -> Fut,
    Fut: Future<Output = ()>,
{
    todo!()
}
