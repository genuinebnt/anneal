//! Outgoing webhooks: URL policy, HMAC signature, delivery.
//!
//! Signature header: `t=<unix secs>,v1=<hex HMAC-SHA256(secret, "<t>.<body>")>`. The timestamp is INSIDE
//! the MAC so a captured request cannot be replayed later; the receiver rejects old timestamps and
//! dedupes on the delivery id (delivery is at-least-once).
//! SSRF: a webhook URL is attacker-chosen and WE make the request. In prod we require https, refuse
//! private/loopback address literals, and never follow redirects. NOT covered: a public DNS name that
//! resolves to a private address (DNS rebinding). That needs checking the resolved address at connect
//! time, or an egress proxy such as Smokescreen.
//! Recipe: `examples/web/webhooks_signatures.rs`.

use std::net::IpAddr;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const SIGNATURE_HEADER: &str = "x-signature";
pub const DELIVERY_HEADER: &str = "x-delivery-id";

fn mac(secret: &str, ts: i64, body: &[u8]) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    m.update(format!("{ts}.").as_bytes());
    m.update(body);
    m
}

#[must_use]
pub fn sign(secret: &str, ts: i64, body: &[u8]) -> String {
    format!("t={ts},v1={}", hex::encode(mac(secret, ts, body).finalize().into_bytes()))
}

/// What a receiver does. Exposed so tests (and the demo receiver) verify exactly as a customer would.
#[must_use]
pub fn verify(secret: &str, header: &str, body: &[u8], now_secs: i64, tolerance_secs: i64) -> bool {
    let Some((t, v1)) = header.split_once(',') else { return false };
    let Some(ts) = t.strip_prefix("t=").and_then(|s| s.parse::<i64>().ok()) else { return false };
    let Some(sig) = v1.strip_prefix("v1=").and_then(|s| hex::decode(s).ok()) else { return false };
    if now_secs.abs_diff(ts) > tolerance_secs.unsigned_abs() {
        return false;
    }
    mac(secret, ts, body).verify_slice(&sig).is_ok()
}

fn is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_internal(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || v6.is_unspecified() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// `strict` is on in prod. Dev allows plain http to loopback so a local receiver works.
pub fn validate_url(raw: &str, strict: bool) -> Result<(), &'static str> {
    if raw.len() > 2_000 {
        return Err("url is too long");
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "not a valid absolute URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("scheme must be http or https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("credentials in the URL are not allowed");
    }
    let host = url.host_str().ok_or("url has no host")?;
    if !strict {
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err("https is required");
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.');
    if bare == "localhost" || bare.ends_with(".localhost") || bare.parse().is_ok_and(is_internal) {
        return Err("internal addresses are not allowed");
    }
    Ok(())
}

pub fn client(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        // A 302 to http://169.254.169.254/ would bypass the URL check made at registration.
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

pub struct Delivery<'a> {
    pub url: &'a str,
    pub secret: &'a str,
    pub delivery_id: &'a str,
    pub body: String,
    pub now_secs: i64,
}

/// Any non-2xx answer (including redirects) is a failure, to be retried by the queue.
pub async fn deliver(http: &reqwest::Client, d: Delivery<'_>) -> Result<(), String> {
    let res = http
        .post(d.url)
        .header("content-type", "application/json")
        .header(SIGNATURE_HEADER, sign(d.secret, d.now_secs, d.body.as_bytes()))
        .header(DELIVERY_HEADER, d.delivery_id)
        .body(d.body)
        .send()
        .await
        // `without_url`: the URL may carry a capability token that must not reach logs or the jobs table.
        .map_err(|e| e.without_url().to_string())?;
    if res.status().is_success() { Ok(()) } else { Err(format!("receiver answered {}", res.status())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_verifies_and_rejects_tampering_and_replay() {
        let h = sign("s3cret", 1_000, b"{\"a\":1}");
        assert!(verify("s3cret", &h, b"{\"a\":1}", 1_100, 300));
        assert!(!verify("s3cret", &h, b"{\"a\":2}", 1_100, 300), "body changed");
        assert!(!verify("other", &h, b"{\"a\":1}", 1_100, 300), "wrong secret");
        assert!(!verify("s3cret", &h, b"{\"a\":1}", 1_301, 300), "too old");
        let moved = h.replace("t=1000", "t=1300");
        assert!(!verify("s3cret", &moved, b"{\"a\":1}", 1_301, 300), "timestamp is covered by the MAC");
        for junk in ["", "t=1", "t=x,v1=00", "t=1000,v1=zz", "v1=00,t=1000"] {
            assert!(!verify("s3cret", junk, b"", 1_000, 300), "{junk}");
        }
    }

    #[test]
    fn strict_mode_blocks_ssrf_targets() {
        for bad in [
            "http://example.com/h",
            "https://localhost/h",
            "https://api.localhost./h",
            "https://127.0.0.1/h",
            "https://10.1.2.3/h",
            "https://192.168.0.1/h",
            "https://169.254.169.254/latest/meta-data",
            "https://2130706433/h", // decimal form of 127.0.0.1
            "https://[::1]/h",
            "https://[fd00::1]/h",
            "https://[fe80::1]/h",
            "https://[::ffff:127.0.0.1]/h",
            "https://user:pw@example.com/h",
            "ftp://example.com/h",
            "/relative",
        ] {
            assert!(validate_url(bad, true).is_err(), "{bad} should be rejected");
        }
        assert!(validate_url("https://hooks.example.com/x", true).is_ok());
        assert!(validate_url("http://127.0.0.1:9/x", false).is_ok(), "dev allows local receivers");
    }
}
