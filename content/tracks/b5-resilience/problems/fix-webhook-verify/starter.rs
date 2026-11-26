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
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn sign(secret: &[u8], t: u64, body: &[u8]) -> String {
    format!("t={t},v1={}", hex::encode(hmac_sha256(secret, body)))
}

pub fn verify(
    secret: &[u8],
    header: &str,
    body: &[u8],
    now: u64,
    tolerance: u64,
) -> Result<(), VerifyError> {
    let mut timestamp = None;
    let mut signature = None;
    for pair in header.split(',') {
        let (key, value) = pair.split_once('=').ok_or(VerifyError::Malformed)?;
        match key {
            "t" => timestamp = Some(value.parse::<u64>().map_err(|_| VerifyError::Malformed)?),
            "v1" if signature.is_none() => signature = Some(value),
            _ => {}
        }
    }
    let (Some(t), Some(signature)) = (timestamp, signature) else {
        return Err(VerifyError::Malformed);
    };
    if now.saturating_sub(t) > tolerance {
        return Err(VerifyError::Expired);
    }
    let given = hex::decode(signature).map_err(|_| VerifyError::BadSignature)?;
    if !constant_time_eq(&given, &hmac_sha256(secret, body)) {
        return Err(VerifyError::BadSignature);
    }
    Ok(())
}
