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
        #[serde(with = "digits")]
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

/// A `u64` written as a string of ASCII digits.
mod digits {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        // `parse` alone would accept a leading `+`.
        if !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(D::Error::custom("amount must contain only digits"));
        }
        text.parse().map_err(D::Error::custom)
    }
}
