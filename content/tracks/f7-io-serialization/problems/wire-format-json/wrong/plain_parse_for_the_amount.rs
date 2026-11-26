use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub created: u64,
    #[serde(flatten)]
    pub payload: Payload,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Payload {
    #[serde(rename = "charge.succeeded", rename_all = "camelCase")]
    Charge {
        #[serde(with = "text_number")]
        amount_cents: u64,
        currency: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    #[serde(rename = "charge.refunded", rename_all = "camelCase")]
    Refund { charge_id: String, reason: Reason },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Duplicate,
    Fraudulent,
    RequestedByCustomer,
    #[serde(other)]
    Other,
}

mod text_number {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        String::deserialize(deserializer)?
            .trim()
            .parse()
            .map_err(D::Error::custom)
    }
}
