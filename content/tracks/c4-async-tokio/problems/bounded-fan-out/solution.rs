use std::future::Future;
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
    let mut pending = inputs.into_iter().enumerate();
    let mut running = JoinSet::new();
    loop {
        while running.len() < limit {
            let Some((index, input)) = pending.next() else {
                break;
            };
            let work = f(input);
            running.spawn(async move { (index, work.await) });
        }
        let Some(joined) = running.join_next().await else {
            break;
        };
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
