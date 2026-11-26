use std::future::Future;

pub async fn fan_out<I, T, E, F, Fut>(inputs: Vec<I>, limit: usize, f: F) -> Result<Vec<T>, E>
where
    F: FnMut(I) -> Fut,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: Send + 'static,
{
    todo!()
}
