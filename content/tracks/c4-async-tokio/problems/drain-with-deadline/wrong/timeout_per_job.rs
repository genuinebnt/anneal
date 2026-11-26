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
    let mut report = Report {
        completed: Vec::new(),
        interrupted: None,
        unstarted: Vec::new(),
    };
    loop {
        let next = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            job = jobs.recv() => job,
        };
        let Some(job) = next else { return report };
        let work = handle(job.clone());
        tokio::pin!(work);
        tokio::select! {
            biased;
            () = shutdown.cancelled() => {
                jobs.close();
                if tokio::time::timeout(grace, &mut work).await.is_ok() {
                    report.completed.push(job);
                } else {
                    report.interrupted = Some(job);
                }
                break;
            }
            () = &mut work => report.completed.push(job),
        }
    }
    jobs.close();
    while report.interrupted.is_none() {
        let Ok(job) = jobs.try_recv() else { break };
        if tokio::time::timeout(grace, handle(job.clone()))
            .await
            .is_ok()
        {
            report.completed.push(job);
        } else {
            report.interrupted = Some(job);
        }
    }
    while let Ok(job) = jobs.try_recv() {
        report.unstarted.push(job);
    }
    report
}
