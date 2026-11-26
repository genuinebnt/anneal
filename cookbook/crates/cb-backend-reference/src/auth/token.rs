//! Access tokens: `v1.<base64url(claims JSON)>.<base64url(HMAC-SHA256)>`.
//!
//! WHY hand-rolled instead of `jsonwebtoken`: this app mints and verifies its own tokens with one key and
//! one algorithm. JWT's flexibility is where its classic bugs live (`alg: none`, RS/HS confusion, `kid`
//! injection). With a fixed format there is no header to trust. Use `jsonwebtoken` (or PASETO) when a
//! THIRD PARTY must verify your tokens: you then want asymmetric keys, JWKS and a standard format.
//! Pitfalls: verify the MAC before parsing the payload; compare in constant time (`verify_slice`);
//! treat `exp` as exclusive; never put secrets in the claims (they are only encoded, not encrypted).
//!
//! Opaque secrets (refresh tokens, API keys, email tokens) are different: they are random, so they are
//! stored as a plain SHA-256. A slow hash is only needed for LOW-entropy secrets such as passwords.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: String,
    /// Refresh-token family (the login session). Revoking the family kills its access tokens too.
    pub sid: String,
    /// Expiry, unix seconds.
    pub exp: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    BadSignature,
    #[error("token expired")]
    Expired,
}

#[derive(Clone)]
pub struct TokenSigner {
    key: Vec<u8>,
}

impl std::fmt::Debug for TokenSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSigner(***)")
    }
}

impl TokenSigner {
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self { key: key.as_bytes().to_vec() }
    }

    fn mac(&self, signed: &str) -> Hmac<Sha256> {
        let mut m = Hmac::<Sha256>::new_from_slice(&self.key).expect("hmac accepts any key length");
        m.update(signed.as_bytes());
        m
    }

    #[must_use]
    pub fn mint(&self, claims: &Claims) -> String {
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap_or_default());
        let signed = format!("v1.{body}");
        let sig = URL_SAFE_NO_PAD.encode(self.mac(&signed).finalize().into_bytes());
        format!("{signed}.{sig}")
    }

    pub fn verify(&self, token: &str, now_secs: i64) -> Result<Claims, TokenError> {
        let (signed, sig) = token.rsplit_once('.').ok_or(TokenError::Malformed)?;
        let body = signed.strip_prefix("v1.").ok_or(TokenError::Malformed)?;
        let sig = URL_SAFE_NO_PAD.decode(sig).map_err(|_| TokenError::Malformed)?;
        self.mac(signed).verify_slice(&sig).map_err(|_| TokenError::BadSignature)?;
        let raw = URL_SAFE_NO_PAD.decode(body).map_err(|_| TokenError::Malformed)?;
        let claims: Claims = serde_json::from_slice(&raw).map_err(|_| TokenError::Malformed)?;
        if now_secs >= claims.exp {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }
}

/// How opaque secrets are stored and looked up.
#[must_use]
pub fn hash_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> Claims {
        Claims { sub: "u1".into(), sid: "f1".into(), exp: 100 }
    }

    #[test]
    fn round_trip_and_exclusive_expiry() {
        let s = TokenSigner::new("k".repeat(32).as_str());
        let t = s.mint(&claims());
        assert_eq!(s.verify(&t, 99).unwrap(), claims());
        assert_eq!(s.verify(&t, 100), Err(TokenError::Expired));
    }

    #[test]
    fn every_single_byte_mutation_is_rejected() {
        let s = TokenSigner::new("k".repeat(32).as_str());
        let t = s.mint(&claims());
        for i in 0..t.len() {
            let mut bytes = t.clone().into_bytes();
            bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
            let mutated = String::from_utf8(bytes).unwrap();
            assert!(s.verify(&mutated, 0).is_err(), "position {i}: {mutated}");
        }
    }

    #[test]
    fn forged_claims_with_another_key_are_rejected() {
        let mallory = TokenSigner::new("m".repeat(32).as_str());
        let forged = mallory.mint(&Claims { sub: "admin".into(), sid: "f".into(), exp: i64::MAX });
        let s = TokenSigner::new("k".repeat(32).as_str());
        assert_eq!(s.verify(&forged, 0), Err(TokenError::BadSignature));
        for junk in ["", ".", "v1..", "a.b.c", "v1.e30"] {
            assert!(s.verify(junk, 0).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn secret_hash_is_stable_and_not_the_secret() {
        assert_eq!(hash_secret("abc"), hash_secret("abc"));
        assert_ne!(hash_secret("abc"), hash_secret("abd"));
        assert!(!hash_secret("abc").contains("abc"));
    }
}
