use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChargeError {
    Declined(String),
    KeyReused,
    InProgress,
}

struct Slot {
    amount: u64,
    /// `None` while the payment is on its way.
    receipt: Option<String>,
}

#[derive(Default)]
pub struct Payments {
    slots: Mutex<HashMap<String, Slot>>,
}

/// Frees the key when dropped, unless the payment succeeded: after an error, a panic or a cancelled call.
struct Claim<'a> {
    payments: &'a Payments,
    key: &'a str,
    keep: bool,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        if !self.keep {
            self.payments.slots().remove(self.key);
        }
    }
}

impl Payments {
    pub fn new() -> Self {
        Self::default()
    }

    fn slots(&self) -> MutexGuard<'_, HashMap<String, Slot>> {
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
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
        // Looking up and claiming happen under one lock, which is released before anything slow.
        {
            let mut slots = self.slots();
            match slots.get(key) {
                Some(slot) if slot.amount != amount => return Err(ChargeError::KeyReused),
                Some(slot) => return slot.receipt.clone().ok_or(ChargeError::InProgress),
                None => {
                    let slot = Slot {
                        amount,
                        receipt: None,
                    };
                    slots.insert(key.to_string(), slot);
                }
            }
        }
        let mut claim = Claim {
            payments: self,
            key,
            keep: false,
        };
        let receipt = match send(amount).await {
            Ok(r) => r,
            Err(e) => {
                claim.keep = true;
                if let Some(slot) = self.slots().get_mut(key) {
                    slot.receipt = Some(e.clone());
                }
                return Err(ChargeError::Declined(e));
            }
        };
        if let Some(slot) = self.slots().get_mut(key) {
            slot.receipt = Some(receipt.clone());
        }
        claim.keep = true;
        Ok(receipt)
    }
}
