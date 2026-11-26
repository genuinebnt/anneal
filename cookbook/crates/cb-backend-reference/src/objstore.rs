//! BLOCK 10: object storage port (local disk + in-memory), content sniffing, presigned download URLs.
//!
//! Production alternatives: `object_store` crate (S3/GCS/Azure/local behind one trait), `aws-sdk-s3`.
//! Pitfalls handled here:
//! * path traversal: keys are validated, never joined blindly (`../../etc/passwd`);
//! * torn files: the disk writer writes to `*.part` and renames on `finish`, so readers never see half a file;
//! * trusting `Content-Type`: the client declares it, we sniff magic bytes and store what we SAW;
//! * download links: HMAC-signed `key + expiry`, constant-time verified, no DB lookup needed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio::io::AsyncWriteExt;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error("invalid object key")]
    BadKey,
    #[error("object not found")]
    NotFound,
    #[error("storage io: {0}")]
    Io(String),
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound { Self::NotFound } else { Self::Io(e.to_string()) }
    }
}

/// A streaming upload in progress. Dropping without `finish` must leave nothing visible.
#[async_trait]
pub trait ObjectWriter: Send {
    async fn write(&mut self, chunk: Bytes) -> Result<(), StoreError>;
    async fn finish(self: Box<Self>) -> Result<(), StoreError>;
    async fn abort(self: Box<Self>);
}

#[async_trait]
pub trait ObjectStore: Send + Sync + 'static {
    async fn begin_put(&self, key: &str) -> Result<Box<dyn ObjectWriter>, StoreError>;
    async fn get(&self, key: &str) -> Result<Bytes, StoreError>;
    async fn delete(&self, key: &str) -> Result<(), StoreError>;
}

/// Allow only `[A-Za-z0-9_.-]` segments separated by `/`, no empty or dot-only segments.
pub fn validate_key(key: &str) -> Result<(), StoreError> {
    let ok = !key.is_empty()
        && key.len() <= 200
        && key.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        });
    if ok { Ok(()) } else { Err(StoreError::BadKey) }
}

// ---------------------------------------------------------------- in-memory

#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    objects: Arc<Mutex<HashMap<String, Bytes>>>,
}

impl MemoryStore {
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.lock().unwrap().len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

struct MemoryWriter {
    key: String,
    buf: Vec<u8>,
    target: Arc<Mutex<HashMap<String, Bytes>>>,
}

#[async_trait]
impl ObjectWriter for MemoryWriter {
    async fn write(&mut self, chunk: Bytes) -> Result<(), StoreError> {
        self.buf.extend_from_slice(&chunk);
        Ok(())
    }
    async fn finish(self: Box<Self>) -> Result<(), StoreError> {
        self.target.lock().unwrap().insert(self.key, Bytes::from(self.buf));
        Ok(())
    }
    async fn abort(self: Box<Self>) {}
}

#[async_trait]
impl ObjectStore for MemoryStore {
    async fn begin_put(&self, key: &str) -> Result<Box<dyn ObjectWriter>, StoreError> {
        validate_key(key)?;
        Ok(Box::new(MemoryWriter { key: key.into(), buf: vec![], target: self.objects.clone() }))
    }
    async fn get(&self, key: &str) -> Result<Bytes, StoreError> {
        validate_key(key)?;
        self.objects.lock().unwrap().get(key).cloned().ok_or(StoreError::NotFound)
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        validate_key(key)?;
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
}

// ---------------------------------------------------------------- local disk

#[derive(Debug, Clone)]
pub struct LocalDiskStore {
    root: PathBuf,
}

impl LocalDiskStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    fn path(&self, key: &str) -> Result<PathBuf, StoreError> {
        validate_key(key)?;
        Ok(self.root.join(key))
    }
}

/// Distinguishes concurrent writers. Two keys differing only by extension (`a.png`, `a.jpg`) or two
/// uploads of one key must never share a temp file.
static PART_SEQ: AtomicU64 = AtomicU64::new(0);

struct DiskWriter {
    file: tokio::fs::File,
    part: PathBuf,
    dest: PathBuf,
}

/// A request can be cancelled (timeout, disconnect) between `write` and `finish`: the future is just
/// dropped, so `abort` never runs. `Drop` is the only hook that always does.
impl Drop for DiskWriter {
    fn drop(&mut self) {
        // After a successful rename the path no longer exists, so this is a no-op.
        let _ = std::fs::remove_file(&self.part);
    }
}

#[async_trait]
impl ObjectWriter for DiskWriter {
    async fn write(&mut self, chunk: Bytes) -> Result<(), StoreError> {
        Ok(self.file.write_all(&chunk).await?)
    }
    async fn finish(mut self: Box<Self>) -> Result<(), StoreError> {
        self.file.flush().await?;
        self.file.sync_all().await?;
        // rename is atomic on POSIX filesystems: readers see the whole object or none of it.
        Ok(tokio::fs::rename(&self.part, &self.dest).await?)
    }
    async fn abort(self: Box<Self>) {
        // Dropping the box removes the temp file.
    }
}

#[async_trait]
impl ObjectStore for LocalDiskStore {
    async fn begin_put(&self, key: &str) -> Result<Box<dyn ObjectWriter>, StoreError> {
        let dest = self.path(key)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // `~` is rejected by `validate_key`, so a half-written file is unreachable through `get`.
        let mut name = dest.file_name().ok_or(StoreError::BadKey)?.to_os_string();
        name.push(format!("~{}", PART_SEQ.fetch_add(1, Ordering::Relaxed)));
        let part = dest.with_file_name(name);
        let file = tokio::fs::File::create(&part).await?;
        Ok(Box::new(DiskWriter { file, part, dest }))
    }
    async fn get(&self, key: &str) -> Result<Bytes, StoreError> {
        Ok(Bytes::from(tokio::fs::read(self.path(key)?).await?))
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        match tokio::fs::remove_file(self.path(key)?).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- sniffing

/// Content types we accept, decided from the first bytes. Returns `None` for anything else.
#[must_use]
pub fn sniff(head: &[u8]) -> Option<&'static str> {
    const MAGIC: &[(&[u8], &str)] = &[
        (b"\x89PNG\r\n\x1a\n", "image/png"),
        (b"\xff\xd8\xff", "image/jpeg"),
        (b"GIF87a", "image/gif"),
        (b"GIF89a", "image/gif"),
        (b"%PDF-", "application/pdf"),
        (b"PK\x03\x04", "application/zip"),
    ];
    if let Some((_, ct)) = MAGIC.iter().find(|(m, _)| head.starts_with(m)) {
        return Some(ct);
    }
    // Plain text heuristic: valid UTF-8 (allowing a cut multi-byte tail) with no control bytes except whitespace.
    let text = match std::str::from_utf8(head) {
        Ok(s) => s,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).ok()?,
        Err(_) => return None,
    };
    let clean = !head.is_empty() && text.chars().all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'));
    clean.then_some("text/plain")
}

/// Strip any path and odd characters from a client-supplied filename (it ends up in headers and logs).
#[must_use]
pub fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .take(100)
        .collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() { "file".into() } else { cleaned }
}

// ---------------------------------------------------------------- presigned URLs

type HmacSha256 = Hmac<Sha256>;

/// Signs `key + expiry`. The same trick as S3 presigned URLs, minus the canonical-request ceremony.
#[derive(Clone)]
pub struct Presigner {
    secret: Vec<u8>,
}

impl Presigner {
    #[must_use]
    pub fn new(secret: &str) -> Self {
        Self { secret: secret.as_bytes().to_vec() }
    }

    fn mac(&self, key: &str, exp_ms: i64) -> HmacSha256 {
        let mut m = HmacSha256::new_from_slice(&self.secret).expect("hmac accepts any key length");
        m.update(key.as_bytes());
        m.update(b"\n");
        m.update(exp_ms.to_string().as_bytes());
        m
    }

    #[must_use]
    pub fn sign(&self, key: &str, exp_ms: i64) -> String {
        hex::encode(self.mac(key, exp_ms).finalize().into_bytes())
    }

    /// `verify_slice` compares in constant time (no timing oracle on the signature).
    #[must_use]
    pub fn verify(&self, key: &str, exp_ms: i64, sig: &str, now_ms: i64) -> bool {
        if now_ms > exp_ms {
            return false;
        }
        hex::decode(sig).is_ok_and(|raw| self.mac(key, exp_ms).verify_slice(&raw).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_reject_traversal() {
        for bad in ["", "../x", "a/../b", "/abs", "a//b", "a/./b", "sp ace", "nul\0"] {
            assert!(validate_key(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(validate_key("org-1/att_2.png").is_ok());
    }

    #[tokio::test]
    async fn disk_store_round_trip_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalDiskStore::new(dir.path());
        let mut w = s.begin_put("o/k1").await.unwrap();
        w.write(Bytes::from_static(b"hello ")).await.unwrap();
        assert!(matches!(s.get("o/k1").await, Err(StoreError::NotFound)), "invisible before finish");
        w.write(Bytes::from_static(b"world")).await.unwrap();
        w.finish().await.unwrap();
        assert_eq!(&s.get("o/k1").await.unwrap()[..], b"hello world");
        s.delete("o/k1").await.unwrap();
        s.delete("o/k1").await.unwrap(); // idempotent
        assert!(matches!(s.get("o/k1").await, Err(StoreError::NotFound)));
    }

    #[tokio::test]
    async fn aborted_upload_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalDiskStore::new(dir.path());
        let mut w = s.begin_put("o/k2").await.unwrap();
        w.write(Bytes::from_static(b"partial")).await.unwrap();
        w.abort().await;
        assert!(s.get("o/k2").await.is_err());
        assert_eq!(std::fs::read_dir(dir.path().join("o")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn concurrent_writers_do_not_share_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalDiskStore::new(dir.path());
        let mut png = s.begin_put("o/a.png").await.unwrap();
        let mut jpg = s.begin_put("o/a.jpg").await.unwrap();
        png.write(Bytes::from_static(b"PNG")).await.unwrap();
        jpg.write(Bytes::from_static(b"JPEG")).await.unwrap();
        png.finish().await.unwrap();
        jpg.finish().await.unwrap();
        assert_eq!(&s.get("o/a.png").await.unwrap()[..], b"PNG");
        assert_eq!(&s.get("o/a.jpg").await.unwrap()[..], b"JPEG");
    }

    #[tokio::test]
    async fn dropped_upload_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalDiskStore::new(dir.path());
        let mut w = s.begin_put("o/k3").await.unwrap();
        w.write(Bytes::from_static(b"partial")).await.unwrap();
        drop(w); // what a cancelled request does
        assert_eq!(std::fs::read_dir(dir.path().join("o")).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn memory_store_matches_disk_semantics() {
        let s = MemoryStore::default();
        let mut w = s.begin_put("a/b").await.unwrap();
        w.write(Bytes::from_static(b"x")).await.unwrap();
        assert!(s.is_empty());
        w.finish().await.unwrap();
        assert_eq!(s.len(), 1);
        assert!(s.begin_put("../bad").await.is_err());
    }

    #[test]
    fn sniffing_trusts_bytes_not_names() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(sniff(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(sniff("héllo\n".as_bytes()), Some("text/plain"));
        assert_eq!(sniff(b"MZ\x90\x00\x03\x00"), None, "executables are not on the allow-list");
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn filenames_are_neutralised() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("C:\\dir\\evil name?.exe"), "evil_name_.exe");
        assert_eq!(sanitize_filename("..."), "file");
    }

    #[test]
    fn presigned_urls_expire_and_resist_tampering() {
        let p = Presigner::new("s3cret-s3cret-s3cret-s3cret-s3cret");
        let sig = p.sign("org/att", 10_000);
        assert!(p.verify("org/att", 10_000, &sig, 9_999));
        assert!(!p.verify("org/att", 10_000, &sig, 10_001), "expired");
        assert!(!p.verify("org/other", 10_000, &sig, 0), "different key");
        assert!(!p.verify("org/att", 99_999, &sig, 0), "extended expiry");
        assert!(!p.verify("org/att", 10_000, "zz", 0), "garbage signature");
    }
}
