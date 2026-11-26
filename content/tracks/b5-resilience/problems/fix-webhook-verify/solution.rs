use sha2::{Digest, Sha256};

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    Malformed,
    BadSignature,
    Expired,
}

/// HMAC-SHA256 as in RFC 2104. This part is correct.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // The expected length is public, so returning early on it tells an attacker nothing.
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The MAC over exactly what the sender signed: the timestamp's text, a dot, the body.
fn mac(secret: &[u8], t: &str, body: &[u8]) -> [u8; 32] {
    let message = [t.as_bytes(), b".", body].concat();
    hmac_sha256(secret, &message)
}

pub fn sign(secret: &[u8], t: u64, body: &[u8]) -> String {
    format!(
        "t={t},v1={}",
        hex::encode(mac(secret, &t.to_string(), body))
    )
}

pub fn verify(
    secret: &[u8],
    header: &str,
    body: &[u8],
    now: u64,
    tolerance: u64,
) -> Result<(), VerifyError> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for pair in header.split(',') {
        let (key, value) = pair.split_once('=').ok_or(VerifyError::Malformed)?;
        match key {
            "t" => {
                let digits = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
                let parsed = value
                    .parse::<u64>()
                    .ok()
                    .filter(|_| digits)
                    .ok_or(VerifyError::Malformed)?;
                if timestamp.replace((value, parsed)).is_some() {
                    return Err(VerifyError::Malformed);
                }
            }
            "v1" => signatures.push(value),
            _ => {}
        }
    }
    let Some((text, t)) = timestamp else {
        return Err(VerifyError::Malformed);
    };
    if signatures.is_empty() {
        return Err(VerifyError::Malformed);
    }
    let expected = mac(secret, text, body);
    if !signatures
        .iter()
        .any(|s| hex::decode(s).is_ok_and(|given| constant_time_eq(&given, &expected)))
    {
        return Err(VerifyError::BadSignature);
    }
    if now.abs_diff(t) > tolerance {
        return Err(VerifyError::Expired);
    }
    Ok(())
}
