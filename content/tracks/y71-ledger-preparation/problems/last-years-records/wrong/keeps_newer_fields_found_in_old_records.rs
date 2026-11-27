use serde_json::{Map, Value};

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

const CURRENT: u64 = 3;

struct Record {
    kind: String,
    data: Map<String, Value>,
}

fn take_str(data: &mut Map<String, Value>, key: &str) -> Result<String, ReadError> {
    match data.remove(key) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(ReadError::BadData),
    }
}

fn from_1(mut r: Record) -> Result<Record, ReadError> {
    if r.kind == "shipped" {
        r.kind = "sent".into();
        let to = take_str(&mut r.data, "to")?;
        let (name, city) = to.rsplit_once(", ").ok_or(ReadError::BadData)?;
        r.data.insert("name".into(), name.into());
        r.data.insert("city".into(), city.into());
    } else {
        let why = take_str(&mut r.data, "why")?;
        r.data.insert("note".into(), why.into());
    }
    Ok(r)
}

fn from_2(mut r: Record) -> Result<Record, ReadError> {
    if r.kind == "sent" {
        let grams = r.data.remove("grams").ok_or(ReadError::BadData)?;
        r.data.insert("weight_g".into(), grams);
        r.data
            .entry("carrier")
            .or_insert_with(|| "post".into());
    }
    Ok(r)
}

fn current(mut r: Record) -> Result<Parcel, ReadError> {
    if r.kind == "sent" {
        Ok(Parcel::Sent {
            name: take_str(&mut r.data, "name")?,
            city: take_str(&mut r.data, "city")?,
            weight_g: r
                .data
                .get("weight_g")
                .and_then(Value::as_u64)
                .ok_or(ReadError::BadData)?,
            carrier: take_str(&mut r.data, "carrier")?,
        })
    } else {
        Ok(Parcel::Lost {
            note: take_str(&mut r.data, "note")?,
        })
    }
}

fn knows(version: u64, kind: &str) -> bool {
    match kind {
        "lost" => true,
        "shipped" => version == 1,
        "sent" => version >= 2,
        _ => false,
    }
}

/// Reads a stored record, of any version this code knows.
pub fn read(stored: &str) -> Result<Parcel, ReadError> {
    let Ok(Value::Object(mut top)) = serde_json::from_str(stored) else {
        return Err(ReadError::NotARecord);
    };
    let version = match top.get("v") {
        None => 1,
        Some(v) => v.as_u64().ok_or(ReadError::NotARecord)?,
    };
    let Some(Value::String(kind)) = top.remove("kind") else {
        return Err(ReadError::NotARecord);
    };
    if version == 0 || version > CURRENT {
        return Err(ReadError::UnknownVersion(version));
    }
    if !knows(version, &kind) {
        return Err(ReadError::UnknownKind(kind));
    }
    let Some(Value::Object(data)) = top.remove("data") else {
        return Err(ReadError::BadData);
    };
    let mut record = Record { kind, data };
    if version <= 1 {
        record = from_1(record)?;
    }
    if version <= 2 {
        record = from_2(record)?;
    }
    current(record)
}
