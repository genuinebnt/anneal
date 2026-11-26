use std::future::Future;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};
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
    let mut report = Report {
        completed: Vec::new(),
        interrupted: None,
        unstarted: Vec::new(),
    };
    // Set once, when the shutdown signal is first seen.
    let mut deadline: Option<Instant> = None;
    loop {
        let next = match deadline {
            None => tokio::select! {
                biased;
                () = shutdown.cancelled() => {
                    jobs.close();
                    deadline = Some(Instant::now() + grace);
                    continue;
                }
                job = jobs.recv() => job,
            },
            Some(end) if Instant::now() >= end => break,
            Some(end) => match tokio::time::timeout_at(end, jobs.recv()).await {
                Ok(job) => job,
                Err(_) => break,
            },
        };
        let Some(job) = next else { break };
        let work = handle(job.clone());
        tokio::pin!(work);
        let finished = loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled(), if deadline.is_none() => {
                    jobs.close();
                    deadline = Some(Instant::now() + grace);
                }
                () = &mut work => break true,
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => break false,
            }
        };
        if finished {
            report.completed.push(job);
        } else {
            report.interrupted = Some(job);
            break;
        }
    }
    jobs.close();
    while let Ok(job) = jobs.try_recv() {
        report.unstarted.push(job);
    }
    report
}
