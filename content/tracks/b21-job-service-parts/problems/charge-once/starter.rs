use std::future::Future;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChargeError {
    Declined(String),
    KeyReused,
    InProgress,
}

pub struct Payments {
    // TODO: your fields
}

impl Payments {
    pub fn new() -> Self {
        todo!()
    }

    pub async fn charge<F, Fut>(
        &self,
        key: &str,
        amount: u64,
        send: F,
    ) -> Result<String, ChargeError>
    where
        F: FnOnce(u64) -> Fut,
        Fut: Future<Output = Result<String, String>>,
    {
        todo!()
    }
}
