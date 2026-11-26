//! Argon2id password hashing, off the async worker threads.
//!
//! WHY `spawn_blocking`: one hash costs tens of milliseconds of CPU and (at the OWASP baseline) 19 MiB.
//! Run on a runtime worker, a handful of concurrent logins would stall every other request.
//! Pitfall: unknown emails must cost as much as wrong passwords, or response time reveals which accounts
//! exist. `Hasher` keeps a dummy hash made with the same parameters for that path.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

use crate::config::AuthConfig;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Hasher {
    params: Params,
    dummy: String,
}

fn argon(params: Params) -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn hash_blocking(params: Params, password: &str) -> Result<String, String> {
    // A v4 UUID is 16 bytes from the OS CSPRNG (6 bits fixed): ample for a salt.
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).map_err(|e| e.to_string())?;
    argon(params).hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| e.to_string())
}

impl Hasher {
    pub fn new(cfg: &AuthConfig) -> anyhow::Result<Self> {
        let params = Params::new(cfg.argon2_m_kib, cfg.argon2_t, 1, None)
            .map_err(|e| anyhow::anyhow!("bad argon2 parameters: {e}"))?;
        let dummy =
            hash_blocking(params.clone(), "dummy password for unknown accounts").map_err(|e| anyhow::anyhow!(e))?;
        Ok(Self { params, dummy })
    }

    pub async fn hash(&self, password: String) -> AppResult<String> {
        let params = self.params.clone();
        tokio::task::spawn_blocking(move || hash_blocking(params, &password))
            .await
            .map_err(AppError::internal)?
            .map_err(|e| AppError::internal(anyhow::anyhow!(e)))
    }

    /// `stored = None` means "no such user": verify against the dummy and always answer `false`.
    pub async fn verify(&self, password: String, stored: Option<String>) -> AppResult<bool> {
        let exists = stored.is_some();
        let phc = stored.unwrap_or_else(|| self.dummy.clone());
        let params = self.params.clone();
        let matched = tokio::task::spawn_blocking(move || {
            // The parameters used to VERIFY come from the stored PHC string, so old hashes keep working
            // after the configured cost is raised.
            PasswordHash::new(&phc).is_ok_and(|h| argon(params).verify_password(password.as_bytes(), &h).is_ok())
        })
        .await
        .map_err(AppError::internal)?;
        Ok(matched && exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cheap() -> Hasher {
        Hasher::new(&AuthConfig { argon2_m_kib: 64, argon2_t: 1, ..AuthConfig::default() }).unwrap()
    }

    #[tokio::test]
    async fn hashes_are_salted_argon2id_and_verify() {
        let h = cheap();
        let a = h.hash("correct horse".into()).await.unwrap();
        let b = h.hash("correct horse".into()).await.unwrap();
        assert!(a.starts_with("$argon2id$"), "{a}");
        assert_ne!(a, b, "same password must get different salts");
        assert!(h.verify("correct horse".into(), Some(a.clone())).await.unwrap());
        assert!(!h.verify("wrong horse".into(), Some(a)).await.unwrap());
    }

    #[tokio::test]
    async fn unknown_user_never_verifies_even_with_the_dummy_password() {
        let h = cheap();
        let ok = h.verify("dummy password for unknown accounts".into(), None).await.unwrap();
        assert!(!ok);
    }

    #[tokio::test]
    async fn corrupt_stored_hash_fails_closed() {
        assert!(!cheap().verify("x".into(), Some("not-a-phc-string".into())).await.unwrap());
    }
}
