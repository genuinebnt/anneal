use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub created: u64,
    pub payload: Payload,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payload {
    Charge {
        amount_cents: u64,
        currency: String,
        note: Option<String>,
    },
    Refund {
        charge_id: String,
        reason: Reason,
    },
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    Duplicate,
    Fraudulent,
    RequestedByCustomer,
    Other,
}
