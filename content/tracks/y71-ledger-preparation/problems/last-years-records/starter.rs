/// A tracking record in today's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parcel {
    Sent {
        name: String,
        city: String,
        weight_g: u64,
        carrier: String,
    },
    Lost {
        note: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    NotARecord,
    UnknownVersion(u64),
    UnknownKind(String),
    BadData,
}

/// Reads a stored record, of any version this code knows.
pub fn read(stored: &str) -> Result<Parcel, ReadError> {
    todo!()
}
