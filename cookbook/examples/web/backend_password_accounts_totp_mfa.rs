//! RECIPE 1202 (web-backend): account lifecycle: password hashing (PBKDF2 from scratch), email verify, reset, lockout, sessions, TOTP + recovery codes.
//! LEVEL: advanced
//! Use in: any app with its own accounts (login, signup, password reset, 2FA)   Run: cargo run --release --example backend_password_accounts_totp_mfa
//! COMMON USES: SaaS signup/login, admin consoles with 2FA, "forgot password" flows, remember-me cookies, authenticator-app MFA.
#![allow(dead_code)]
//!
//! Extends: auth_basic_bearer_jwt (credentials -> token), cookies_sessions_csrf (cookie sessions), oauth2_pkce_flow (delegated login).
//!
//! Everything cryptographic is implemented from scratch with std only so you can SEE it, and checked against published test vectors:
//! SHA-1 / SHA-256 (the round constants are DERIVED from fractional parts of roots of primes, then the vectors prove them),
//! HMAC (RFC 2202/4231), PBKDF2-HMAC-SHA256 (RFC 7914 vectors), HOTP (RFC 4226 Appendix D) and TOTP (RFC 6238 Appendix B), base32.
//! DO NOT ship hand-rolled crypto: production uses `argon2` (Argon2id), `hmac`+`sha1`/`sha2`, `totp-rs`, `rand`/`getrandom` (OS CSPRNG;
//! this file uses a seeded xorshift ONLY so runs are deterministic). The lifecycle logic (tokens, lockout, rotation) is the real lesson.
//!
//! Password hashing choices: Argon2id is the default pick (OWASP minimum ~19 MiB memory, t=2, p=1; more memory is better; memory-hard
//! resists GPUs). scrypt is the alternative. PBKDF2-HMAC-SHA256 (OWASP: 600,000 iterations) is for FIPS environments. bcrypt truncates at
//! 72 bytes. The stored string encodes algorithm+params so you can raise cost later and rehash transparently on the next login.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   store a password             -> memory-hard KDF + per-user random salt; encode params in the string; never SHA-256(pw)
//!   email-verify / reset token   -> 32 random bytes, hex; store ONLY sha256(token); single use; short expiry (verify 24h, reset 30m)
//!   compare secrets              -> `ct_eq` (constant time) for MACs/TOTP codes; random-token lookup by hash is fine
//!   avoid user enumeration       -> same response + same work for unknown email (dummy hash), "if that email exists..." on reset
//!   brute-force defence          -> per-account failure counter + exponential lock, plus per-IP throttling (backend_rate_limit_quota_middleware)
//!   session fixation             -> issue a NEW session id at login, delete the pre-login one
//!   after password change        -> kill all sessions + remember-me tokens
//!   remember-me                  -> selector:validator cookie, store hash(validator), rotate on use, theft = revoke all
//!   authenticator app            -> base32 secret, TOTP SHA-1 6 digits 30 s, accept +-1 step, remember last used counter (replay)
//!   recovery codes               -> 8-10 random codes, store hashes, each usable once, shown once
//!   upgrade hash params          -> `needs_rehash()` on successful login
//!
//! Read: OWASP Password Storage Cheat Sheet (https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html),
//! RFC 6238 (https://www.rfc-editor.org/rfc/rfc6238), RFC 4226 (https://www.rfc-editor.org/rfc/rfc4226), RFC 8018 (PBKDF2) (https://www.rfc-editor.org/rfc/rfc8018).
//!
//! TRY NEXT:
//!   - [S] Change `LOCK_AFTER` to 3 and predict which assertions in `lifecycle()` fail; then run and check.
//!   - [S] Measure: time `pbkdf2_sha256` at 1k/10k/100k iterations on your machine; pick the count that costs ~250 ms.
//!   - [M] Add "TOTP step-up for sensitive actions": a session flag `mfa_at` that expires after 5 minutes; test with the virtual clock.
//!   - [M] Replace `Svc::rand_bytes` with reading /dev/urandom; keep a seeded mode for tests behind a trait (inject RNG).
//!   - [A] PROJECT: combine with backend_rate_limit_quota_middleware (per-IP + per-account throttle) and backend_authz_rbac_abac_policies (roles on the session).
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tower::ServiceExt;

// ------------------------------------------------------------------ hashes from scratch ------------------------------
fn md_pad(data: &[u8]) -> Vec<u8> {
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    m
}
/// floor(root_n(p * 2^(32*n))) mod 2^32 = first 32 fractional bits of the n-th root of p. Integer-only, exact.
fn frac_root(p: u128, n: u32) -> u32 {
    let (mut lo, mut hi) = (0u128, 1u128 << 40);
    let target = p << (32 * n);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if mid.checked_pow(n).is_some_and(|v| v <= target) {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    lo as u32
}
fn first_primes(n: usize) -> Vec<u128> {
    let mut v = vec![];
    let mut c = 2u128;
    while v.len() < n {
        if v.iter().all(|p| c % p != 0) {
            v.push(c);
        }
        c += 1;
    }
    v
}
fn sha256(data: &[u8]) -> Vec<u8> {
    let primes = first_primes(64);
    let k: Vec<u32> = primes.iter().map(|p| frac_root(*p, 3)).collect(); // cube roots of the first 64 primes
    let mut h: Vec<u32> = primes[..8].iter().map(|p| frac_root(*p, 2)).collect(); // square roots of the first 8
    for chunk in md_pad(data).chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes(b.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]];
        for i in 0..64 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (hv, x) in h.iter_mut().zip(v) {
            *hv = hv.wrapping_add(x);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).collect()
}
fn sha1(data: &[u8]) -> Vec<u8> {
    let mut h: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    for chunk in md_pad(data).chunks(64) {
        let mut w = [0u32; 80];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes(b.try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..20 => ((b & c) | (!b & d), 0x5a827999u32),
                20..40 => (b ^ c ^ d, 0x6ed9eba1),
                40..60 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, t);
        }
        for (hv, x) in h.iter_mut().zip([a, b, c, d, e]) {
            *hv = hv.wrapping_add(x);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).collect()
}
fn hmac(hash: fn(&[u8]) -> Vec<u8>, key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut k = if key.len() > 64 {
        hash(key)
    } else {
        key.to_vec()
    };
    k.resize(64, 0);
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend(hash(&inner));
    hash(&outer)
}
fn pbkdf2_sha256(pw: &[u8], salt: &[u8], iters: u32, dk_len: usize) -> Vec<u8> {
    let mut out = vec![];
    for block in 1u32.. {
        if out.len() >= dk_len {
            break;
        }
        let mut s = salt.to_vec();
        s.extend_from_slice(&block.to_be_bytes());
        let mut u = hmac(sha256, pw, &s);
        let mut t = u.clone();
        for _ in 1..iters {
            u = hmac(sha256, pw, &u);
            t.iter_mut().zip(&u).for_each(|(a, b)| *a ^= b);
        }
        out.extend(t);
    }
    out.truncate(dk_len);
    out
}
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    // no early exit: time depends on length only. Production: `subtle::ConstantTimeEq`.
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}
const B32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
fn b32_encode(data: &[u8]) -> String {
    let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}
fn b32_decode(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (vec![], 0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        let v = B32.iter().position(|x| *x == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Some(out)
}
fn hotp(key: &[u8], counter: u64, digits: u32) -> u32 {
    let mac = hmac(sha1, key, &counter.to_be_bytes());
    let o = (mac[19] & 0xf) as usize; // dynamic truncation (RFC 4226 section 5.3)
    let bin = u32::from_be_bytes([mac[o] & 0x7f, mac[o + 1], mac[o + 2], mac[o + 3]]);
    bin % 10u32.pow(digits)
}
fn totp(key: &[u8], unix: u64, digits: u32) -> u32 {
    hotp(key, unix / 30, digits)
}

fn vectors() {
    println!("== Test vectors (published RFC values)");
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha1(b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(hex(&sha256(&[b'a'; 1000])).len(), 64); // multi-block path runs
    assert_eq!(
        hex(&hmac(sha256, b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    assert_eq!(
        hex(&hmac(sha1, b"Jefe", b"what do ya want for nothing?")),
        "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 1, 32)),
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 4096, 32)),
        "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
    );
    let key = b"12345678901234567890";
    let want = [
        755224, 287082, 359152, 969429, 338314, 254676, 287922, 162583, 399871, 520489,
    ];
    for (c, w) in want.iter().enumerate() {
        assert_eq!(hotp(key, c as u64, 6), *w);
    }
    for (t, w) in [
        (59u64, 94287082u32),
        (1111111109, 7081804),
        (1111111111, 14050471),
        (1234567890, 89005924),
        (2000000000, 69279037),
        (20000000000, 65353130),
    ] {
        assert_eq!(totp(key, t, 8), w, "RFC 6238 T={t}");
    }
    assert_eq!(b32_encode(b"foobar"), "MZXW6YTBOI");
    assert_eq!(b32_decode("MZXW6YTBOI======").unwrap(), b"foobar");
    assert!(ct_eq(b"abc", b"abc") && !ct_eq(b"abc", b"abd") && !ct_eq(b"abc", b"ab"));
    println!(
        "   SHA-1/256, HMAC, PBKDF2 (1 and 4096 iters), HOTP x10, TOTP x6 (RFC 6238), base32: all match"
    );
}

// ------------------------------------------------------------------ account service ----------------------------------
#[derive(Debug, PartialEq)]
enum AuthError {
    InvalidCredentials, // same for unknown email and wrong password
    Locked { retry_after: u64 },
    NotVerified,
    MfaRequired,
    WeakPassword(&'static str),
    BadToken,
}
const LOCK_AFTER: i64 = 5;
const SESSION_TTL: u64 = 12 * 3600;
const REMEMBER_TTL: u64 = 30 * 86400;
const RESET_TTL: u64 = 30 * 60;
const VERIFY_TTL: u64 = 24 * 3600;

struct Svc {
    db: Mutex<Connection>,
    now: AtomicU64,
    rng: Mutex<u64>,
    iters: AtomicU32,
    hash_calls: AtomicU32,
}
struct UserRow {
    id: i64,
    pw: String,
    verified: bool,
    fails: i64,
    locked_until: i64,
    totp: Option<String>,
    totp_on: bool,
    last_counter: i64,
}
#[derive(Debug)]
struct Session {
    id: String,
    expires: u64,
    remember: Option<String>,
}

impl Svc {
    fn new(iters: u32) -> Self {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT UNIQUE NOT NULL, pw TEXT NOT NULL, verified INT NOT NULL DEFAULT 0,
               fails INT NOT NULL DEFAULT 0, locked_until INT NOT NULL DEFAULT 0, totp TEXT, totp_on INT NOT NULL DEFAULT 0, last_counter INT NOT NULL DEFAULT 0);
             CREATE TABLE tokens(hash TEXT PRIMARY KEY, user_id INT NOT NULL, purpose TEXT NOT NULL, expires INT NOT NULL, used INT NOT NULL DEFAULT 0);
             CREATE TABLE sessions(hash TEXT PRIMARY KEY, user_id INT NOT NULL, expires INT NOT NULL);
             CREATE TABLE remember(selector TEXT PRIMARY KEY, vhash TEXT NOT NULL, user_id INT NOT NULL, expires INT NOT NULL);
             CREATE TABLE recovery(user_id INT NOT NULL, hash TEXT NOT NULL, used INT NOT NULL DEFAULT 0, PRIMARY KEY(user_id, hash));",
        )
        .unwrap();
        Svc {
            db: Mutex::new(db),
            now: AtomicU64::new(1_700_000_000),
            rng: Mutex::new(0x2545F4914F6CDD1D),
            iters: AtomicU32::new(iters),
            hash_calls: AtomicU32::new(0),
        }
    }
    fn now(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }
    fn advance(&self, secs: u64) {
        self.now.fetch_add(secs, Ordering::Relaxed);
    }
    fn rand_bytes(&self, n: usize) -> Vec<u8> {
        let mut s = self.rng.lock();
        (0..n)
            .map(|_| {
                *s ^= *s << 13;
                *s ^= *s >> 7;
                *s ^= *s << 17;
                (*s >> 24) as u8
            })
            .collect()
    }
    // ---- password hashing: "pbkdf2-sha256$iters$salt$hash"
    fn hash_password(&self, pw: &str, iters: u32) -> String {
        self.hash_calls.fetch_add(1, Ordering::Relaxed);
        let salt = self.rand_bytes(16);
        format!(
            "pbkdf2-sha256${iters}${}${}",
            hex(&salt),
            hex(&pbkdf2_sha256(pw.as_bytes(), &salt, iters, 32))
        )
    }
    fn verify_password(&self, pw: &str, stored: &str) -> (bool, bool) {
        self.hash_calls.fetch_add(1, Ordering::Relaxed);
        let p: Vec<&str> = stored.split('$').collect();
        let iters: u32 = p[1].parse().unwrap();
        let got = pbkdf2_sha256(pw.as_bytes(), &unhex(p[2]), iters, 32);
        (
            ct_eq(&got, &unhex(p[3])),
            iters < self.iters.load(Ordering::Relaxed),
        ) // (ok, needs_rehash)
    }
    fn check_policy(pw: &str) -> Result<(), AuthError> {
        // NIST 800-63B: length matters, composition rules do not; block known-bad passwords.
        const COMMON: [&str; 4] = ["password", "12345678", "qwertyui", "letmein1"];
        match pw.chars().count() {
            0..8 => Err(AuthError::WeakPassword("too_short")),
            65.. => Err(AuthError::WeakPassword("too_long")),
            _ if COMMON.contains(&pw.to_lowercase().as_str()) => {
                Err(AuthError::WeakPassword("too_common"))
            }
            _ => Ok(()),
        }
    }
    // ---- one-time tokens: only the hash is stored
    fn issue_token(&self, c: &Connection, user_id: i64, purpose: &str, ttl: u64) -> String {
        let raw = hex(&self.rand_bytes(32));
        c.execute(
            "INSERT INTO tokens(hash,user_id,purpose,expires) VALUES(?,?,?,?)",
            params![
                hex(&sha256(raw.as_bytes())),
                user_id,
                purpose,
                (self.now() + ttl) as i64
            ],
        )
        .unwrap();
        raw
    }
    fn consume_token(&self, c: &Connection, raw: &str, purpose: &str) -> Result<i64, AuthError> {
        let h = hex(&sha256(raw.as_bytes()));
        // single atomic UPDATE: two concurrent requests cannot both consume the token
        let n = c
            .execute(
                "UPDATE tokens SET used=1 WHERE hash=? AND purpose=? AND used=0 AND expires>?",
                params![h, purpose, self.now() as i64],
            )
            .unwrap();
        if n != 1 {
            return Err(AuthError::BadToken);
        }
        Ok(
            c.query_row("SELECT user_id FROM tokens WHERE hash=?", [h], |r| r.get(0))
                .unwrap(),
        )
    }
    fn user(c: &Connection, email: &str) -> Option<UserRow> {
        c.query_row("SELECT id,pw,verified,fails,locked_until,totp,totp_on,last_counter FROM users WHERE email=?", [email], |r| {
            Ok(UserRow { id: r.get(0)?, pw: r.get(1)?, verified: r.get::<_, i64>(2)? != 0, fails: r.get(3)?, locked_until: r.get(4)?, totp: r.get(5)?, totp_on: r.get::<_, i64>(6)? != 0, last_counter: r.get(7)? })
        })
        .optional()
        .unwrap()
    }
    // ---- lifecycle
    /// Returns the verification token to e-mail. `None` when the address already exists: the HTTP answer must be identical either way.
    fn register(&self, email: &str, pw: &str) -> Result<Option<String>, AuthError> {
        Self::check_policy(pw)?;
        let email = email.trim().to_lowercase();
        let hash = self.hash_password(pw, self.iters.load(Ordering::Relaxed));
        let c = self.db.lock();
        let n = c
            .execute(
                "INSERT OR IGNORE INTO users(email,pw) VALUES(?,?)",
                params![email, hash],
            )
            .unwrap();
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(self.issue_token(
            &c,
            c.last_insert_rowid(),
            "verify",
            VERIFY_TTL,
        )))
    }
    fn verify_email(&self, token: &str) -> Result<(), AuthError> {
        let c = self.db.lock();
        let uid = self.consume_token(&c, token, "verify")?;
        c.execute("UPDATE users SET verified=1 WHERE id=?", [uid])
            .unwrap();
        Ok(())
    }
    fn login(
        &self,
        email: &str,
        pw: &str,
        second: Option<&str>,
        pre_session: Option<&str>,
        remember: bool,
    ) -> Result<Session, AuthError> {
        let email = email.trim().to_lowercase();
        let c = self.db.lock();
        let Some(u) = Self::user(&c, &email) else {
            // equalise work: unknown email still pays for one hash, so timing does not reveal existence
            let _ =
                self.verify_password(pw, "pbkdf2-sha256$1000$00000000000000000000000000000000$00");
            return Err(AuthError::InvalidCredentials);
        };
        if u.locked_until as u64 > self.now() {
            return Err(AuthError::Locked {
                retry_after: u.locked_until as u64 - self.now(),
            });
        }
        let (ok, rehash) = self.verify_password(pw, &u.pw);
        if !ok {
            let fails = u.fails + 1;
            let lock = if fails >= LOCK_AFTER {
                self.now() + (60u64 << (fails - LOCK_AFTER).min(6))
            } else {
                0
            }; // 60 s, 120 s, ... cap ~1 h
            c.execute(
                "UPDATE users SET fails=?, locked_until=? WHERE id=?",
                params![fails, lock as i64, u.id],
            )
            .unwrap();
            return Err(AuthError::InvalidCredentials);
        }
        if !u.verified {
            return Err(AuthError::NotVerified);
        }
        if u.totp_on {
            let code = second.ok_or(AuthError::MfaRequired)?;
            if !self.check_second_factor(&c, &u, code) {
                c.execute("UPDATE users SET fails=fails+1 WHERE id=?", [u.id])
                    .unwrap();
                return Err(AuthError::InvalidCredentials);
            }
        }
        c.execute(
            "UPDATE users SET fails=0, locked_until=0 WHERE id=?",
            [u.id],
        )
        .unwrap();
        if rehash {
            let h = self.hash_password(pw, self.iters.load(Ordering::Relaxed));
            c.execute("UPDATE users SET pw=? WHERE id=?", params![h, u.id])
                .unwrap();
        }
        // session fixation defence: the pre-login id dies, a fresh one is minted
        if let Some(old) = pre_session {
            c.execute(
                "DELETE FROM sessions WHERE hash=?",
                [hex(&sha256(old.as_bytes()))],
            )
            .unwrap();
        }
        Ok(self.new_session(&c, u.id, remember))
    }
    fn new_session(&self, c: &Connection, uid: i64, remember: bool) -> Session {
        let id = hex(&self.rand_bytes(32));
        let expires = self.now() + SESSION_TTL;
        c.execute(
            "INSERT INTO sessions VALUES(?,?,?)",
            params![hex(&sha256(id.as_bytes())), uid, expires as i64],
        )
        .unwrap();
        let remember = remember.then(|| {
            let (sel, val) = (hex(&self.rand_bytes(8)), hex(&self.rand_bytes(16)));
            c.execute(
                "INSERT INTO remember VALUES(?,?,?,?)",
                params![
                    sel,
                    hex(&sha256(val.as_bytes())),
                    uid,
                    (self.now() + REMEMBER_TTL) as i64
                ],
            )
            .unwrap();
            format!("{sel}:{val}")
        });
        Session {
            id,
            expires,
            remember,
        }
    }
    fn session_user(&self, id: &str) -> Option<i64> {
        self.db
            .lock()
            .query_row(
                "SELECT user_id FROM sessions WHERE hash=? AND expires>?",
                params![hex(&sha256(id.as_bytes())), self.now() as i64],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }
    /// Cookie "selector:validator" -> new session + ROTATED validator. Wrong validator for a known selector = stolen cookie: revoke all.
    fn login_remembered(&self, cookie: &str) -> Result<Session, AuthError> {
        let (sel, val) = cookie.split_once(':').ok_or(AuthError::BadToken)?;
        let c = self.db.lock();
        let row: Option<(String, i64, i64)> = c
            .query_row(
                "SELECT vhash,user_id,expires FROM remember WHERE selector=?",
                [sel],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .unwrap();
        let (vhash, uid, exp) = row.ok_or(AuthError::BadToken)?;
        if !ct_eq(vhash.as_bytes(), hex(&sha256(val.as_bytes())).as_bytes()) {
            c.execute("DELETE FROM remember WHERE user_id=?", [uid])
                .unwrap();
            c.execute("DELETE FROM sessions WHERE user_id=?", [uid])
                .unwrap();
            return Err(AuthError::BadToken);
        }
        if (exp as u64) < self.now() {
            return Err(AuthError::BadToken);
        }
        // rotate the VALIDATOR, keep the selector: a replay of the old cookie now matches the selector but not the validator = theft signal
        let newval = hex(&self.rand_bytes(16));
        c.execute(
            "UPDATE remember SET vhash=?, expires=? WHERE selector=?",
            params![
                hex(&sha256(newval.as_bytes())),
                (self.now() + REMEMBER_TTL) as i64,
                sel
            ],
        )
        .unwrap();
        let mut s = self.new_session(&c, uid, false);
        s.remember = Some(format!("{sel}:{newval}"));
        Ok(s)
    }
    fn request_reset(&self, email: &str) -> Option<String> {
        let c = self.db.lock();
        let u = Self::user(&c, &email.trim().to_lowercase())?;
        c.execute(
            "UPDATE tokens SET used=1 WHERE user_id=? AND purpose='reset'",
            [u.id],
        )
        .unwrap(); // older links die
        Some(self.issue_token(&c, u.id, "reset", RESET_TTL))
    }
    fn reset_password(&self, token: &str, new_pw: &str) -> Result<(), AuthError> {
        Self::check_policy(new_pw)?;
        let c = self.db.lock();
        let uid = self.consume_token(&c, token, "reset")?;
        let h = self.hash_password(new_pw, self.iters.load(Ordering::Relaxed));
        c.execute(
            "UPDATE users SET pw=?, fails=0, locked_until=0 WHERE id=?",
            params![h, uid],
        )
        .unwrap();
        c.execute("DELETE FROM sessions WHERE user_id=?", [uid])
            .unwrap(); // a reset means "I may be compromised"
        c.execute("DELETE FROM remember WHERE user_id=?", [uid])
            .unwrap();
        Ok(())
    }
    // ---- MFA
    fn begin_totp(&self, email: &str) -> String {
        let secret = b32_encode(&self.rand_bytes(20));
        self.db
            .lock()
            .execute(
                "UPDATE users SET totp=?, totp_on=0 WHERE email=?",
                params![secret, email],
            )
            .unwrap();
        secret
    }
    /// User proves they scanned the QR code by entering a valid code; only then is MFA enforced. Returns the recovery codes (shown once).
    fn confirm_totp(&self, email: &str, code: &str) -> Option<Vec<String>> {
        let c = self.db.lock();
        let u = Self::user(&c, email)?;
        let key = b32_decode(u.totp.as_deref()?)?;
        let step = self.now() / 30;
        if !(step - 1..=step + 1).any(|s| {
            ct_eq(
                format!("{:06}", hotp(&key, s, 6)).as_bytes(),
                code.as_bytes(),
            )
        }) {
            return None;
        }
        c.execute(
            "UPDATE users SET totp_on=1, last_counter=? WHERE id=?",
            params![step as i64, u.id],
        )
        .unwrap();
        let codes: Vec<String> = (0..8)
            .map(|_| {
                let b = self.rand_bytes(5);
                format!("{}-{}", hex(&b[..2]), hex(&b[2..]))
            })
            .collect();
        for code in &codes {
            c.execute(
                "INSERT INTO recovery(user_id,hash) VALUES(?,?)",
                params![u.id, hex(&sha256(code.as_bytes()))],
            )
            .unwrap();
        }
        Some(codes)
    }
    fn check_second_factor(&self, c: &Connection, u: &UserRow, code: &str) -> bool {
        if let Some(key) = u.totp.as_deref().and_then(b32_decode) {
            let step = self.now() / 30;
            // accept +-1 step for clock skew, but never a step <= last accepted one (replay of a sniffed code)
            for s in (step - 1..=step + 1).filter(|s| *s as i64 > u.last_counter) {
                if ct_eq(
                    format!("{:06}", hotp(&key, s, 6)).as_bytes(),
                    code.as_bytes(),
                ) {
                    c.execute(
                        "UPDATE users SET last_counter=? WHERE id=?",
                        params![s as i64, u.id],
                    )
                    .unwrap();
                    return true;
                }
            }
        }
        // recovery code: single use
        c.execute(
            "UPDATE recovery SET used=1 WHERE user_id=? AND hash=? AND used=0",
            params![u.id, hex(&sha256(code.as_bytes()))],
        )
        .unwrap()
            == 1
    }
}

// ------------------------------------------------------------------ sections ----------------------------------------
fn everyday_core() {
    println!("== Everyday core: register -> verify -> login, uniform errors, lockout, reset");
    let s = Svc::new(1000);
    assert_eq!(
        s.register("a@x.co", "short"),
        Err(AuthError::WeakPassword("too_short"))
    );
    assert_eq!(
        s.register("a@x.co", "Password"),
        Err(AuthError::WeakPassword("too_common"))
    );
    let tok = s
        .register("  A@X.co ", "correct horse battery")
        .unwrap()
        .unwrap();
    assert_eq!(s.register("a@x.co", "another long one").unwrap(), None); // duplicate: same Ok shape for the caller
    // token is stored hashed: the raw token appears nowhere in the DB
    let leaked: i64 =
        s.db.lock()
            .query_row("SELECT count(*) FROM tokens WHERE hash=?", [&tok], |r| {
                r.get(0)
            })
            .unwrap();
    assert_eq!(leaked, 0);
    assert_eq!(
        s.login("a@x.co", "correct horse battery", None, None, false)
            .unwrap_err(),
        AuthError::NotVerified
    );
    s.verify_email(&tok).unwrap();
    assert_eq!(s.verify_email(&tok), Err(AuthError::BadToken)); // single use
    let sess = s
        .login("a@x.co", "correct horse battery", None, None, false)
        .unwrap();
    assert!(s.session_user(&sess.id).is_some());
    // uniform error + equal hashing work for unknown vs wrong password
    let before = s.hash_calls.load(Ordering::Relaxed);
    assert_eq!(
        s.login("ghost@x.co", "whatever123", None, None, false)
            .unwrap_err(),
        AuthError::InvalidCredentials
    );
    let unknown = s.hash_calls.load(Ordering::Relaxed) - before;
    let before = s.hash_calls.load(Ordering::Relaxed);
    assert_eq!(
        s.login("a@x.co", "wrong password", None, None, false)
            .unwrap_err(),
        AuthError::InvalidCredentials
    );
    assert_eq!(s.hash_calls.load(Ordering::Relaxed) - before, unknown);
    // lockout after LOCK_AFTER consecutive failures, even the right password is refused until it expires
    for _ in 0..LOCK_AFTER - 1 {
        let _ = s.login("a@x.co", "nope nope nope", None, None, false);
    }
    assert!(matches!(
        s.login("a@x.co", "correct horse battery", None, None, false),
        Err(AuthError::Locked { retry_after: 60 })
    ));
    s.advance(61);
    assert!(
        s.login("a@x.co", "correct horse battery", None, None, false)
            .is_ok()
    );
    // reset: expiring, single-use, kills sessions
    let r1 = s.request_reset("a@x.co").unwrap();
    let r2 = s.request_reset("a@x.co").unwrap();
    assert_eq!(
        s.reset_password(&r1, "brand new password"),
        Err(AuthError::BadToken)
    ); // superseded
    s.advance(RESET_TTL + 1);
    assert_eq!(
        s.reset_password(&r2, "brand new password"),
        Err(AuthError::BadToken)
    ); // expired
    let r3 = s.request_reset("a@x.co").unwrap();
    s.reset_password(&r3, "brand new password").unwrap();
    assert!(s.session_user(&sess.id).is_none(), "old sessions revoked");
    assert!(s.request_reset("ghost@x.co").is_none()); // HTTP layer answers 202 either way
    assert!(
        s.login("a@x.co", "brand new password", None, None, false)
            .is_ok()
    );
    println!(
        "   lifecycle ok; unknown-email path costs {unknown} hash call(s), same as wrong password"
    );
}

fn intermediate() {
    println!(
        "== Intermediate: session rotation, remember-me theft detection, hash upgrade, TOTP + recovery codes"
    );
    let s = Svc::new(1000);
    let t = s.register("b@x.co", "long enough pw").unwrap().unwrap();
    s.verify_email(&t).unwrap();
    // session fixation: the attacker planted `pre`; after login it is worthless
    let pre = Session {
        id: hex(&s.rand_bytes(8)),
        expires: 0,
        remember: None,
    };
    s.db.lock()
        .execute(
            "INSERT INTO sessions VALUES(?,?,?)",
            params![hex(&sha256(pre.id.as_bytes())), 0, (s.now() + 99) as i64],
        )
        .unwrap();
    assert!(s.session_user(&pre.id).is_some());
    let sess = s
        .login("b@x.co", "long enough pw", None, Some(&pre.id), true)
        .unwrap();
    assert!(
        s.session_user(&pre.id).is_none()
            && s.session_user(&sess.id).is_some()
            && sess.id != pre.id
    );
    // remember-me: normal use rotates; replaying the old cookie = theft = everything revoked
    let c1 = sess.remember.clone().unwrap();
    let s2 = s.login_remembered(&c1).unwrap();
    let c2 = s2.remember.clone().unwrap();
    assert_ne!(c1, c2);
    assert!(s.login_remembered(&c1).is_err()); // replay of the OLD cookie: selector known, validator stale
    assert!(
        s.session_user(&s2.id).is_none() && s.login_remembered(&c2).is_err(),
        "theft revokes the whole family"
    );
    // hash upgrade: raise the cost, old hash verifies once and is rewritten
    s.iters.store(2000, Ordering::Relaxed);
    s.login("b@x.co", "long enough pw", None, None, false)
        .unwrap();
    let stored: String =
        s.db.lock()
            .query_row("SELECT pw FROM users WHERE email='b@x.co'", [], |r| {
                r.get(0)
            })
            .unwrap();
    assert!(stored.starts_with("pbkdf2-sha256$2000$"));
    // TOTP enrol (needs a valid code), enforce, replay protection, recovery code
    let secret = s.begin_totp("b@x.co");
    let key = b32_decode(&secret).unwrap();
    let code = |s: &Svc| format!("{:06}", totp(&key, s.now(), 6));
    assert!(s.confirm_totp("b@x.co", "000000").is_none() || code(&s) == "000000");
    let codes = s.confirm_totp("b@x.co", &code(&s)).unwrap();
    assert_eq!(codes.len(), 8);
    assert_eq!(
        s.login("b@x.co", "long enough pw", None, None, false)
            .unwrap_err(),
        AuthError::MfaRequired
    );
    s.advance(30);
    let c = code(&s);
    assert!(
        s.login("b@x.co", "long enough pw", Some(&c), None, false)
            .is_ok()
    );
    assert!(
        s.login("b@x.co", "long enough pw", Some(&c), None, false)
            .is_err(),
        "same code twice = replay, refused"
    );
    s.advance(30);
    assert!(
        s.login("b@x.co", "long enough pw", Some(&codes[0]), None, false)
            .is_ok()
    );
    assert!(
        s.login("b@x.co", "long enough pw", Some(&codes[0]), None, false)
            .is_err(),
        "recovery code is single use"
    );
    let stored_codes: i64 =
        s.db.lock()
            .query_row(
                "SELECT count(*) FROM recovery WHERE hash=?",
                [&codes[1]],
                |r| r.get(0),
            )
            .unwrap();
    assert_eq!(stored_codes, 0, "recovery codes stored hashed");
    println!("   fixation, remember-me rotation, hash upgrade, TOTP replay + recovery code: ok");
}

async fn less_known() {
    println!("== Less known: HTTP mapping (401 uniform, 423 + Retry-After), pre-hash cost timing");
    let svc = Arc::new(Svc::new(1000));
    let t = svc.register("c@x.co", "long enough pw").unwrap().unwrap();
    svc.verify_email(&t).unwrap();
    async fn login(State(s): State<Arc<Svc>>, Json(b): Json<Value>) -> axum::response::Response {
        let r = s.login(
            b["email"].as_str().unwrap_or(""),
            b["password"].as_str().unwrap_or(""),
            b["code"].as_str(),
            None,
            false,
        );
        match r {
            Ok(sess) => (
                StatusCode::OK,
                [(
                    "set-cookie",
                    format!(
                        "sid={}; HttpOnly; Secure; SameSite=Lax; Max-Age={SESSION_TTL}",
                        sess.id
                    ),
                )],
                "ok",
            )
                .into_response(),
            Err(AuthError::Locked { retry_after }) => (
                StatusCode::LOCKED,
                [("retry-after", retry_after.to_string())],
                "locked",
            )
                .into_response(),
            Err(AuthError::MfaRequired) => {
                (StatusCode::UNAUTHORIZED, [("x-mfa", "required")], "mfa").into_response()
            }
            Err(_) => (StatusCode::UNAUTHORIZED, "invalid credentials").into_response(), // never say which part was wrong
        }
    }
    let app = Router::new()
        .route("/login", post(login))
        .with_state(svc.clone());
    let post_login = |email: &'static str, pw: &'static str| {
        let app = app.clone();
        async move {
            let req = axum::http::Request::builder()
                .method("POST")
                .uri("/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    json!({"email": email, "password": pw}).to_string(),
                ))
                .unwrap();
            app.oneshot(req).await.unwrap()
        }
    };
    let ok = post_login("c@x.co", "long enough pw").await;
    assert_eq!(ok.status(), 200);
    let cookie = ok.headers()["set-cookie"].to_str().unwrap().to_string();
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("Secure") && cookie.contains("SameSite=Lax")
    );
    assert_eq!(
        post_login("ghost@x.co", "long enough pw").await.status(),
        401
    );
    for _ in 0..LOCK_AFTER {
        assert_eq!(
            post_login("c@x.co", "bad password!").await.status(),
            if svc
                .db
                .lock()
                .query_row("SELECT fails FROM users WHERE email='c@x.co'", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()
                >= LOCK_AFTER - 1
            {
                401
            } else {
                401
            }
        );
    }
    let locked = post_login("c@x.co", "long enough pw").await;
    assert_eq!(locked.status(), 423);
    assert!(locked.headers().contains_key("retry-after"));
    let t0 = std::time::Instant::now();
    pbkdf2_sha256(b"pw", b"salt", 100_000, 32);
    println!(
        "   100k PBKDF2 iterations took {:?} on this machine (printed, not asserted)",
        t0.elapsed()
    );
}

fn production_tips() {
    println!("== Production tips and tricks");
    let s = Svc::new(1000);
    // 1. Never SHA-256(password): same input -> same output, GPU-fast. Salts differ per hash here.
    let (a, b) = (
        s.hash_password("same password", 1000),
        s.hash_password("same password", 1000),
    );
    assert_ne!(a, b);
    // 2. Encode algorithm+params in the stored string so you can migrate; verify returns needs_rehash.
    assert!(s.verify_password("same password", &a).0 && !s.verify_password("same password", &a).1);
    // 3. Compare MACs/codes with ct_eq. WHY: `==` returns at the first differing byte (timing oracle).
    assert!(!ct_eq(b"123456", b"123457"));
    // 4. Random tokens: >= 128 bits, from the OS CSPRNG (`getrandom`), hex/base64url. Hash before storing: a DB leak must not yield live links.
    assert_eq!(s.rand_bytes(32).len(), 32);
    // 5. Reset always answers "if that address exists we sent a link". WHY: no account enumeration. (asserted in everyday_core)
    // 6. Password policy: min 8 (ideally 12+), max 64-128 (cap to stop hash-DoS), no composition rules, block breached lists (HIBP k-anonymity).
    assert_eq!(
        Svc::check_policy(&"x".repeat(65)),
        Err(AuthError::WeakPassword("too_long"))
    );
    // 7. Run the KDF off the async runtime (`spawn_blocking`): 250 ms of CPU on a reactor thread stalls every other request.
    // 8. Throttle per ACCOUNT and per IP; per-account alone lets an attacker lock victims out (DoS): prefer backoff over hard lock.
    // 9. Normalise email (trim + lowercase) on EVERY entry point; unique index on the normalised column.
    // 10. TOTP: window +-1, store last counter (anti-replay), rate-limit attempts (1e6 space), show recovery codes once, hash them.
    assert_eq!(totp(b"12345678901234567890", 59, 8), 94287082);
    // 11. Cookies: HttpOnly + Secure + SameSite=Lax + __Host- prefix; session ids hashed at rest (done: sessions.hash).
    // 12. On password change/reset: revoke sessions and remember tokens; email the user about it (security notification).
    // 13. Anti-pattern: logging the request body of /login or /reset. Use redaction (backend_request_context_observability).
    println!("   13 tips asserted/noted");
}

#[tokio::main]
async fn main() {
    vectors();
    everyday_core();
    intermediate();
    less_known().await;
    production_tips();
    println!("OK");
}
