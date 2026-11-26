use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub async fn fan_out<I, T, E, F, Fut>(inputs: Vec<I>, limit: usize, mut f: F) -> Result<Vec<T>, E>
where
    F: FnMut(I) -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    assert!(limit > 0, "limit must be at least 1");
    let mut results: Vec<Option<T>> = inputs.iter().map(|_| None).collect();
    let slots = Arc::new(Semaphore::new(limit));
    let mut running = JoinSet::new();
    for (index, input) in inputs.into_iter().enumerate() {
        let work = f(input);
        let slots = slots.clone();
        running.spawn(async move {
            let _slot = slots.acquire_owned().await.unwrap();
            (index, work.await)
        });
    }
    while let Some(joined) = running.join_next().await {
        match joined {
            Ok((index, Ok(value))) => results[index] = Some(value),
            Ok((_, Err(error))) => {
                running.shutdown().await;
                return Err(error);
            }
            Err(failed) => std::panic::resume_unwind(failed.into_panic()),
        }
    }
    Ok(results
        .into_iter()
        .map(|r| r.expect("every task finished"))
        .collect())
}
