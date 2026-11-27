//! RECIPE (security): control what a program may fetch.
//! LEVEL: intermediate
//! Use in: any program that fetches an address it did not write.   Run: cargo run --example fetch_policy
//! COMMON USES: link previews, webhooks, import-from-address features, link checkers, collectors.
//!
//! Strict address checks, a list of allowed origins, a check of every address a name resolves to, a connection
//! pinned to the checked addresses, redirects followed by hand, and limits on bytes and total time.
//!
//! **LIMITS**: this is one layer, not a sandbox. It does not replace network rules (a firewall or an egress proxy that
//! you control). `is_public` is a list that can go out of date, and it cannot know that a public address belongs to
//! your own service. There is no TLS in this build, so nothing here tests `https` on the wire.
//! Every test uses local servers and a fake resolver. No real host is contacted.
//! Read: https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html
#![allow(dead_code)]

use reqwest::dns::{Name, Resolve, Resolving};
use reqwest::header::{CONTENT_ENCODING, LOCATION};
use reqwest::{Client, Url, redirect};
use std::collections::VecDeque;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------- the policy

/// One place the program may talk to. All three parts must match.
#[derive(Debug, Clone, PartialEq)]
struct Origin {
    scheme: &'static str,
    host: String,
    port: u16,
}

#[derive(Debug, PartialEq)]
enum Refused {
    Text,
    Parse,
    UserInfo,
    NotListed,
    Lookup,
    Address(IpAddr),
    Redirect,
    TooManyRedirects,
    Status(u16),
    Encoded,
    TooLarge,
    TooSlow,
    Transport,
}

struct Policy {
    allowed: Vec<Origin>,
    /// Which resolved addresses may be connected to. Real programs pass `is_public`.
    address_ok: fn(IpAddr) -> bool,
    max_redirects: usize,
    max_bytes: usize,
    deadline: Duration,
}

/// A name lookup the policy can be given. Tests pass a fake one.
trait Lookup {
    fn lookup(&self, host: &str) -> impl Future<Output = std::io::Result<Vec<IpAddr>>>;
}

/// The system's own lookup, for real programs. Nothing in this file calls it with a real name.
struct SystemLookup;
impl Lookup for SystemLookup {
    async fn lookup(&self, host: &str) -> std::io::Result<Vec<IpAddr>> {
        Ok(tokio::net::lookup_host((host, 0))
            .await?
            .map(|a| a.ip())
            .collect())
    }
}

/// Step 1: judge the text, then judge what the parser made of it.
fn check_url(policy: &Policy, text: &str) -> Result<Url, Refused> {
    // Refuse what different parsers read differently: spaces, control characters, non-ASCII and backslashes.
    if text.len() > 2048 || !text.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') {
        return Err(Refused::Text);
    }
    let rest = text
        .strip_prefix("http://")
        .or_else(|| text.strip_prefix("https://"))
        .ok_or(Refused::Text)?;
    if rest.starts_with('/') {
        return Err(Refused::Text); // the parser would skip extra slashes and read `http:///a` as host `a`
    }
    let url = Url::parse(text).map_err(|_| Refused::Parse)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Refused::UserInfo);
    }
    // From here on only the parsed parts count. The parser may have rewritten the host (`127.1` is `127.0.0.1`).
    let host = url.host_str().ok_or(Refused::Parse)?;
    let port = url.port_or_known_default().ok_or(Refused::Parse)?;
    let listed = policy
        .allowed
        .iter()
        .any(|o| o.scheme == url.scheme() && o.host == host && o.port == port);
    if listed {
        Ok(url)
    } else {
        Err(Refused::NotListed)
    }
}

/// Step 2: find every address behind the host, and refuse the lot if any one of them is not allowed.
async fn checked_addresses(
    policy: &Policy,
    resolver: &impl Lookup,
    url: &Url,
) -> Result<Vec<IpAddr>, Refused> {
    let host = url.host_str().ok_or(Refused::Parse)?;
    // An address written in the URL is never looked up, by us or by the client, so it is checked here.
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>();
    let addresses = match literal {
        Ok(ip) => vec![ip],
        Err(_) => resolver.lookup(host).await.map_err(|_| Refused::Lookup)?,
    };
    if addresses.is_empty() {
        return Err(Refused::Lookup);
    }
    match addresses.iter().find(|ip| !(policy.address_ok)(**ip)) {
        Some(bad) => Err(Refused::Address(*bad)),
        None => Ok(addresses),
    }
}

/// A resolver that answers nothing. Only the pinned addresses can be used for a name.
struct NoLookups;
impl Resolve for NoLookups {
    fn resolve(&self, _: Name) -> Resolving {
        Box::pin(async { Err("this client may not look up names".into()) })
    }
}

/// Step 3: a client that connects only to the addresses that were checked.
fn pinned_client(host: &str, addresses: &[IpAddr]) -> Result<Client, Refused> {
    // port 0 means: use the port in the URL
    let pins: Vec<SocketAddr> = addresses.iter().map(|ip| SocketAddr::new(*ip, 0)).collect();
    Client::builder()
        .no_proxy() // a proxy would look the name up itself, so the pins would not apply
        .redirect(redirect::Policy::none())
        // no decoder may unpack the body, even if another part of a build switches one on
        .no_gzip()
        .no_brotli()
        .no_zstd()
        .no_deflate()
        .dns_resolver(Arc::new(NoLookups))
        .resolve_to_addrs(host, &pins)
        .build()
        .map_err(|_| Refused::Transport)
}

/// Where a redirect leads. The result is only text: it still has to pass `check_url`.
fn redirect_target(from: &Url, location: &str) -> Result<String, Refused> {
    // judge the header as sent: `join` would quietly remove tabs and turn `\` into `/`
    if !location.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') {
        return Err(Refused::Redirect);
    }
    let next = from.join(location).map_err(|_| Refused::Redirect)?;
    if from.scheme() == "https" && next.scheme() != "https" {
        return Err(Refused::Redirect);
    }
    Ok(next.into())
}

async fn fetch_hops(
    policy: &Policy,
    resolver: &impl Lookup,
    start: &str,
) -> Result<Vec<u8>, Refused> {
    let mut text = start.to_string();
    for _hop in 0..=policy.max_redirects {
        // every hop gets all the checks, not only the first
        let url = check_url(policy, &text)?;
        let addresses = checked_addresses(policy, resolver, &url).await?;
        let client = pinned_client(url.host_str().ok_or(Refused::Parse)?, &addresses)?;
        let mut response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|_| Refused::Transport)?;
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(Refused::Redirect)?;
            text = redirect_target(&url, location)?;
            continue;
        }
        if status != 200 {
            return Err(Refused::Status(status));
        }
        if response
            .headers()
            .get(CONTENT_ENCODING)
            .is_some_and(|v| v != "identity")
        {
            return Err(Refused::Encoded);
        }
        // the announced length can refuse early, but only the count of real bytes can accept
        if response
            .content_length()
            .is_some_and(|n| n > policy.max_bytes as u64)
        {
            return Err(Refused::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Refused::Transport)? {
            if chunk.len() > policy.max_bytes - body.len() {
                return Err(Refused::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        return Ok(body);
    }
    Err(Refused::TooManyRedirects)
}

/// One time limit around everything: lookups, connections, every redirect and the body.
async fn fetch(policy: &Policy, resolver: &impl Lookup, url: &str) -> Result<Vec<u8>, Refused> {
    tokio::time::timeout(policy.deadline, fetch_hops(policy, resolver, url))
        .await
        .unwrap_or(Err(Refused::TooSlow))
}

// ---------------------------------------------------------------- which addresses are public

fn is_public(ip: IpAddr) -> bool {
    // `::ffff:127.0.0.1` is 127.0.0.1 in IPv6 form
    match ip.to_canonical() {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let special = matches!(
        (a, b, c),
        (0 | 10 | 127, _, _)
            | (100, 64..=127, _)           // shared space behind carrier NAT
            | (169, 254, _)                // link-local, with many cloud metadata services
            | (172, 16..=31, _)
            | (192, 0, 0 | 2)
            | (192, 31, 196)
            | (192, 52, 193)
            | (192, 88, 99)
            | (192, 168, _)
            | (192, 175, 48)
            | (198, 18 | 19, _)
            | (198, 51, 100)
            | (203, 0, 113)
            | (224..=255, _, _) // multicast, reserved and broadcast
    );
    !special
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let [s0, s1, ..] = ip.segments();
    // accept only global unicast, 2000::/3, and then remove the special blocks inside it
    let global_unicast = s0 & 0xe000 == 0x2000;
    let special = (s0 == 0x2001 && s1 < 0x0200) // protocol assignments, with Teredo
        || (s0 == 0x2001 && s1 == 0x0db8) // documentation
        || s0 == 0x2002 // 6to4 carries an IPv4 address inside
        || (s0 == 0x3fff && s1 < 0x1000); // documentation
    global_unicast && !special
}

// ---------------------------------------------------------------- the broken version, for the experiment

/// VULNERABLE, on purpose: checks one lookup and lets the client make another.
async fn vuln_check_then_fetch(
    policy: &Policy,
    resolver: Arc<Scripted>,
    text: &str,
) -> Result<String, Refused> {
    let url = check_url(policy, text)?;
    checked_addresses(policy, resolver.as_ref(), &url).await?;
    let client = Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(AsReqwest(resolver)))
        .build()
        .map_err(|_| Refused::Transport)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| Refused::Transport)?;
    response.text().await.map_err(|_| Refused::Transport)
}

// ---------------------------------------------------------------- a fake resolver and fake servers

/// Gives the scripted answers in order and repeats the last one. It never touches the network.
struct Scripted {
    host: &'static str,
    answers: Mutex<VecDeque<Vec<IpAddr>>>,
    asked: AtomicUsize,
}

impl Scripted {
    fn new(host: &'static str, answers: &[&[&str]]) -> Arc<Scripted> {
        let parse = |list: &&[&str]| list.iter().map(|s| s.parse().unwrap()).collect();
        Arc::new(Scripted {
            host,
            answers: Mutex::new(answers.iter().map(parse).collect()),
            asked: AtomicUsize::new(0),
        })
    }
    fn answer(&self, host: &str) -> std::io::Result<Vec<IpAddr>> {
        if host != self.host {
            return Err(std::io::Error::other("unknown name"));
        }
        self.asked.fetch_add(1, Ordering::SeqCst);
        let mut answers = self.answers.lock().unwrap();
        let next = if answers.len() > 1 {
            answers.pop_front()
        } else {
            answers.front().cloned()
        };
        Ok(next.unwrap_or_default())
    }
}

impl Lookup for Scripted {
    async fn lookup(&self, host: &str) -> std::io::Result<Vec<IpAddr>> {
        self.answer(host)
    }
}

struct AsReqwest(Arc<Scripted>);
impl Resolve for AsReqwest {
    fn resolve(&self, name: Name) -> Resolving {
        let found = self.0.answer(name.as_str());
        Box::pin(async move {
            let addresses = found?.into_iter().map(|ip| SocketAddr::new(ip, 0));
            Ok(Box::new(addresses) as Box<dyn Iterator<Item = SocketAddr> + Send>)
        })
    }
}

#[derive(Clone)]
enum Reply {
    Bytes(Vec<u8>),
    /// sends the head, then one byte every 20 ms for ever
    Drip,
    Silent,
}

struct Server {
    address: SocketAddr,
    hits: Arc<AtomicUsize>,
}

async fn serve_on(listener: TcpListener, reply: Reply) -> Server {
    let address = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            counted.fetch_add(1, Ordering::SeqCst);
            let reply = reply.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buffer).await {
                        Ok(n) if n > 0 && request.len() < 16 * 1024 => {
                            request.extend_from_slice(&buffer[..n])
                        }
                        _ => return,
                    }
                }
                match reply {
                    Reply::Bytes(bytes) => {
                        let _ = socket.write_all(&bytes).await;
                    }
                    Reply::Drip => {
                        let _ = socket
                            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                            .await;
                        while socket.write_all(b"x").await.is_ok() {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    }
                    Reply::Silent => tokio::time::sleep(Duration::from_secs(60)).await,
                }
                let _ = socket.shutdown().await;
            });
        }
    });
    Server { address, hits }
}

async fn serve(reply: Reply) -> Server {
    serve_on(TcpListener::bind("127.0.0.1:0").await.unwrap(), reply).await
}

fn response(status: &str, headers: &str, body: &str) -> Reply {
    let length = body.len();
    Reply::Bytes(format!("HTTP/1.1 {status}\r\n{headers}Content-Length: {length}\r\nConnection: close\r\n\r\n{body}").into_bytes())
}

/// Tests only: lets 127.0.0.1 stand for "a public address", so a local server can play a public one.
fn test_only_ipv4_loopback(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn origin(host: &str, port: u16) -> Origin {
    Origin {
        scheme: "http",
        host: host.to_string(),
        port,
    }
}

fn test_policy(allowed: Vec<Origin>) -> Policy {
    Policy {
        allowed,
        address_ok: test_only_ipv4_loopback,
        max_redirects: 3,
        max_bytes: 1000,
        deadline: Duration::from_secs(5),
    }
}

// ---------------------------------------------------------------- checks

fn address_text_checks() {
    let policy = test_policy(vec![
        origin("good.test", 80),
        Origin {
            scheme: "https",
            host: "good.test".into(),
            port: 443,
        },
    ]);
    for ok in [
        "http://good.test/",
        "http://good.test:80/a?b=1",
        "https://good.test/x",
        "http://GOOD.test/",
    ] {
        assert!(check_url(&policy, ok).is_ok(), "{ok} must pass");
    }
    let refused: &[(&str, Refused)] = &[
        ("http://good.test\\@evil.test/", Refused::Text),
        ("http://good.test/a b", Refused::Text),
        (" http://good.test/", Refused::Text),
        ("http://go\tod.test/", Refused::Text),
        ("http://go\nod.test/", Refused::Text),
        ("http://good。test/", Refused::Text),
        ("http:/good.test/", Refused::Text),
        ("http:good.test", Refused::Text),
        ("http:///good.test", Refused::Text),
        ("//good.test/", Refused::Text),
        ("file:///etc/hosts", Refused::Text),
        ("ftp://good.test/", Refused::Text),
        ("http://good.test:80:90/", Refused::Parse),
        ("http://good.test:65536/", Refused::Parse),
        ("http://", Refused::Parse),
        ("http://user@good.test/", Refused::UserInfo),
        ("http://:secret@good.test/", Refused::UserInfo),
        ("http://good.test@evil.test/", Refused::UserInfo),
        ("http://evil.test/", Refused::NotListed),
        ("http://good.test.evil.test/", Refused::NotListed),
        ("http://good.test%2eevil.test/", Refused::NotListed),
        ("http://evil.test#@good.test/", Refused::NotListed),
        ("http://good.test./", Refused::NotListed),
        ("http://good.test:8080/", Refused::NotListed),
        ("http://good.test:22/", Refused::NotListed),
        ("http://good.test:443/", Refused::NotListed),
        ("https://good.test:80/", Refused::NotListed),
        ("http://127.1/", Refused::NotListed),
        ("http://[::1]/", Refused::NotListed),
    ];
    for (text, why) in refused {
        assert_eq!(check_url(&policy, text).unwrap_err(), *why, "{text:?}");
    }
    assert_eq!(
        check_url(&policy, &format!("http://good.test/{}", "a".repeat(3000))).unwrap_err(),
        Refused::Text
    );
    // the parser rewrites these spellings, which is why a rule must look at the parsed host
    for spelling in [
        "http://127.1/",
        "http://0x7f000001/",
        "http://2130706433/",
        "http://0177.0.0.1/",
    ] {
        assert_eq!(
            Url::parse(spelling).unwrap().host_str(),
            Some("127.0.0.1"),
            "{spelling}"
        );
    }
    println!("address text: ok");
}

/// Hostile text must never panic, and whatever passes must have exactly the listed parts.
fn random_address_checks() {
    let policy = test_policy(vec![origin("good.test", 80)]);
    let pieces = [
        "http://",
        "https://",
        "good.test",
        "evil.test",
        "@",
        ":",
        "/",
        "//",
        "\\",
        "#",
        "?",
        ".",
        "%",
        "%2e",
        "%00",
        "[",
        "]",
        "::1",
        "127.0.0.1",
        "80",
        "8080",
        " ",
        "\t",
        "\0",
        "é",
        "。",
        "xn--",
        "..",
        "[::",
        "99999999999",
    ];
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut random = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut passed = 0;
    for _ in 0..60_000 {
        let mut text = String::from(if random(4) == 0 { "" } else { "http://" });
        if random(2) == 0 {
            text.push_str("good.test");
        }
        for _ in 0..random(7) {
            text.push_str(pieces[random(pieces.len())]);
        }
        if let Ok(url) = check_url(&policy, &text) {
            passed += 1;
            // a second, naive reading of the text: the host part ends at the first / ? or #
            let rest = text.strip_prefix("http://").expect("only http is listed");
            let authority = rest
                .split(['/', '?', '#'])
                .next()
                .unwrap()
                .to_ascii_lowercase();
            // an empty `@` or `:@` before the host carries no user name and passes
            let authority = authority.trim_start_matches([':', '@']).replace("%2e", ".");
            let (host, port) = authority.split_once(':').unwrap_or((&authority, ""));
            assert_eq!(host, "good.test", "{text:?}");
            assert!(
                port.is_empty() || port.trim_start_matches('0') == "80",
                "{text:?}"
            );
            assert_eq!(url.host_str(), Some("good.test"));
            assert_eq!(url.port_or_known_default(), Some(80));
        }
    }
    assert!(
        passed > 1000,
        "only {passed} random addresses passed, so the check above proves little"
    );
    println!("random addresses: ok ({passed} of 60000 passed)");
}

/// The same address rules written a second way, as a table of blocks, and compared on many addresses.
fn public_address_checks() {
    let refused = [
        "0.0.0.0",
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "100.127.255.255",
        "192.0.2.1",
        "198.18.0.1",
        "198.19.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::127.0.0.1",
        "fe80::1",
        "fc00::1",
        "fd00:ec2::254",
        "ff02::1",
        "64:ff9b::7f00:1",
        "64:ff9b:1::1",
        "2001::1",
        "2001:db8::1",
        "2002:7f00:1::",
        "3fff::1",
        "100::1",
        "5f00::1",
    ];
    for text in refused {
        assert!(!is_public(text.parse().unwrap()), "{text} must be refused");
    }
    for text in [
        "1.1.1.1",
        "100.63.255.255",
        "100.128.0.0",
        "172.15.0.1",
        "172.32.0.1",
        "2606:4700::1111",
        "::ffff:1.1.1.1",
    ] {
        assert!(is_public(text.parse().unwrap()), "{text} must pass");
    }
    // Not special in any registry, so the list lets it through. One cloud uses it for a platform service.
    assert!(is_public("168.63.129.16".parse().unwrap()));

    let v4: &[(&str, u32)] = &[
        ("0.0.0.0", 8),
        ("10.0.0.0", 8),
        ("100.64.0.0", 10),
        ("127.0.0.0", 8),
        ("169.254.0.0", 16),
        ("172.16.0.0", 12),
        ("192.0.0.0", 24),
        ("192.0.2.0", 24),
        ("192.31.196.0", 24),
        ("192.52.193.0", 24),
        ("192.88.99.0", 24),
        ("192.168.0.0", 16),
        ("192.175.48.0", 24),
        ("198.18.0.0", 15),
        ("198.51.100.0", 24),
        ("203.0.113.0", 24),
        ("224.0.0.0", 4),
        ("240.0.0.0", 4),
    ];
    let v6: &[(&str, u32)] = &[
        ("2001::", 23),
        ("2001:db8::", 32),
        ("2002::", 16),
        ("3fff::", 20),
    ];
    let in_v4 = |ip: u32, base: &str, bits: u32| {
        ip >> (32 - bits) == u32::from(base.parse::<Ipv4Addr>().unwrap()) >> (32 - bits)
    };
    let in_v6 = |ip: u128, base: &str, bits: u32| {
        ip >> (128 - bits) == u128::from(base.parse::<Ipv6Addr>().unwrap()) >> (128 - bits)
    };
    let mut seed = 42u64;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..200_000u32 {
        // half the time, land on or just beside a block's first or last address
        let (base, bits) = v4[random() as usize % v4.len()];
        let first = u32::from(base.parse::<Ipv4Addr>().unwrap());
        let last = first | (u32::MAX >> bits);
        let edge =
            [first, first.wrapping_sub(1), last, last.wrapping_add(1)][random() as usize % 4];
        let ip = if round % 2 == 0 {
            edge
        } else {
            random() as u32
        };
        let oracle = !v4.iter().any(|(base, bits)| in_v4(ip, base, *bits));
        assert_eq!(
            is_public(IpAddr::V4(ip.into())),
            oracle,
            "{}",
            Ipv4Addr::from(ip)
        );

        let (base, bits) = v6[random() as usize % v6.len()];
        let first = u128::from(base.parse::<Ipv6Addr>().unwrap());
        let last = first | (u128::MAX >> bits);
        let edge = [first, first - 1, last, last + 1][random() as usize % 4];
        let ip = if round % 2 == 0 {
            edge
        } else {
            (u128::from(random()) << 64 | u128::from(random())) >> (round % 4)
        };
        if Ipv6Addr::from(ip).to_ipv4_mapped().is_none() {
            let oracle =
                in_v6(ip, "2000::", 3) && !v6.iter().any(|(base, bits)| in_v6(ip, base, *bits));
            assert_eq!(
                is_public(IpAddr::V6(ip.into())),
                oracle,
                "{}",
                Ipv6Addr::from(ip)
            );
        }
    }
    println!("address ranges: ok");
}

async fn lookup_checks() {
    let mut policy = test_policy(vec![
        origin("good.test", 80),
        origin("10.0.0.7", 80),
        origin("[::1]", 80),
        origin("127.0.0.1", 80),
    ]);
    policy.address_ok = is_public;
    let url = check_url(&policy, "http://good.test/").unwrap();
    // one bad address in the answer refuses the whole answer
    let mixed = Scripted::new("good.test", &[&["1.1.1.1", "10.0.0.7"]]);
    let got = checked_addresses(&policy, mixed.as_ref(), &url).await;
    assert_eq!(
        got.unwrap_err(),
        Refused::Address("10.0.0.7".parse().unwrap())
    );
    let none = Scripted::new("good.test", &[&[]]);
    assert_eq!(
        checked_addresses(&policy, none.as_ref(), &url)
            .await
            .unwrap_err(),
        Refused::Lookup
    );
    // an address in the URL is checked too, although nothing looks it up, and being listed does not excuse it
    let unused = Scripted::new("nothing.test", &[&[]]);
    for text in ["http://10.0.0.7/", "http://[::1]/", "http://127.1/"] {
        let url = check_url(&policy, text).unwrap();
        assert!(
            matches!(
                checked_addresses(&policy, unused.as_ref(), &url).await,
                Err(Refused::Address(_))
            ),
            "{text}"
        );
    }
    assert_eq!(unused.asked.load(Ordering::SeqCst), 0);
    println!("lookups: ok");
}

async fn pinning_checks() {
    // With the real rule, a name that resolves to this machine is refused and the server never hears of it.
    let server = serve(response("200 OK", "", "hello")).await;
    let port = server.address.port();
    let url = format!("http://good.test:{port}/");
    let mut strict = test_policy(vec![origin("good.test", port)]);
    strict.address_ok = is_public;
    let local = Scripted::new("good.test", &[&["127.0.0.1"]]);
    assert_eq!(
        fetch(&strict, local.as_ref(), &url).await,
        Err(Refused::Address("127.0.0.1".parse().unwrap()))
    );
    assert_eq!(server.hits.load(Ordering::SeqCst), 0);

    // With the test rule, the same request goes through, after exactly one lookup.
    let policy = test_policy(vec![origin("good.test", port)]);
    let local = Scripted::new("good.test", &[&["127.0.0.1"]]);
    assert_eq!(
        fetch(&policy, local.as_ref(), &url).await,
        Ok(b"hello".to_vec())
    );
    assert_eq!(local.asked.load(Ordering::SeqCst), 1);

    // A listed address written in the URL works too, and nothing is looked up.
    let literal = test_policy(vec![origin("127.0.0.1", port)]);
    let unused = Scripted::new("nothing.test", &[&[]]);
    assert_eq!(
        fetch(&literal, unused.as_ref(), &format!("http://127.1:{port}/")).await,
        Ok(b"hello".to_vec())
    );

    // A client of ours cannot look up a name that was not pinned.
    let client = pinned_client("good.test", &["127.0.0.1".parse().unwrap()]).unwrap();
    assert!(
        client
            .get(format!("http://localhost:{port}/"))
            .send()
            .await
            .is_err()
    );
    println!("pinning: ok");
}

/// Two servers share a port: one on 127.0.0.1 (allowed here) and one on ::1 (not allowed).
async fn two_servers() -> Option<(Server, Server)> {
    for _ in 0..20 {
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = first.local_addr().unwrap().port();
        if let Ok(second) = TcpListener::bind(("::1", port)).await {
            let open = serve_on(first, response("200 OK", "", "public page")).await;
            let secret = serve_on(second, response("200 OK", "", "internal secret")).await;
            return Some((open, secret));
        }
    }
    None
}

async fn rebinding_checks() {
    let Some((open, secret)) = two_servers().await else {
        println!("name changes between check and use: SKIPPED, this machine has no IPv6 loopback");
        return;
    };
    let port = open.address.port();
    let url = format!("http://good.test:{port}/");
    let policy = test_policy(vec![origin("good.test", port)]);

    // the name answers with an allowed address once, then with a forbidden one
    let changing = Scripted::new("good.test", &[&["127.0.0.1"], &["::1"]]);
    let got = vuln_check_then_fetch(&policy, changing.clone(), &url).await;
    assert_eq!(
        got,
        Ok("internal secret".to_string()),
        "the attack must work on the broken version"
    );
    assert_eq!(changing.asked.load(Ordering::SeqCst), 2);

    let before = secret.hits.load(Ordering::SeqCst);
    let changing = Scripted::new("good.test", &[&["127.0.0.1"], &["::1"]]);
    assert_eq!(
        fetch(&policy, changing.as_ref(), &url).await,
        Ok(b"public page".to_vec())
    );
    assert_eq!(changing.asked.load(Ordering::SeqCst), 1);
    assert_eq!(secret.hits.load(Ordering::SeqCst), before);
    println!("name changes between check and use: ok");
}

async fn redirect_checks() {
    let secret = serve(response("200 OK", "", "internal secret")).await;
    let secret_port = secret.address.port();
    let nowhere = Scripted::new("good.test", &[&["127.0.0.1"]]);

    // to an origin that is not listed: by address, scheme-relative, and by another name
    for location in [
        format!("http://127.0.0.1:{secret_port}/"),
        format!("//127.0.0.1:{secret_port}/"),
        format!("http://evil.test:{secret_port}/"),
    ] {
        let server = serve(response(
            "302 Found",
            &format!("Location: {location}\r\n"),
            "",
        ))
        .await;
        let port = server.address.port();
        let policy = test_policy(vec![origin("good.test", port)]);
        let got = fetch(
            &policy,
            nowhere.as_ref(),
            &format!("http://good.test:{port}/"),
        )
        .await;
        assert_eq!(got, Err(Refused::NotListed), "{location}");
    }
    assert_eq!(secret.hits.load(Ordering::SeqCst), 0);

    // to a listed name: the new hop is looked up and checked again
    let location = format!("Location: http://other.test:{secret_port}/\r\n");
    let server = serve(response("307 Temporary Redirect", &location, "")).await;
    let port = server.address.port();
    let policy = test_policy(vec![
        origin("good.test", port),
        origin("other.test", secret_port),
    ]);
    struct Two;
    impl Lookup for Two {
        async fn lookup(&self, host: &str) -> std::io::Result<Vec<IpAddr>> {
            let text = if host == "good.test" {
                "127.0.0.1"
            } else {
                "10.0.0.7"
            };
            Ok(vec![text.parse().unwrap()])
        }
    }
    let got = fetch(&policy, &Two, &format!("http://good.test:{port}/")).await;
    assert_eq!(got, Err(Refused::Address("10.0.0.7".parse().unwrap())));

    // a loop stops at the limit; a relative target that is allowed works
    let looping = serve(response(
        "301 Moved Permanently",
        "Location: /again\r\n",
        "",
    ))
    .await;
    let port = looping.address.port();
    let policy = test_policy(vec![origin("good.test", port)]);
    let got = fetch(
        &policy,
        nowhere.as_ref(),
        &format!("http://good.test:{port}/"),
    )
    .await;
    assert_eq!(got, Err(Refused::TooManyRedirects));
    assert_eq!(
        looping.hits.load(Ordering::SeqCst),
        4,
        "the first request and three redirects"
    );

    let from = Url::parse("https://good.test/a/b").unwrap();
    assert_eq!(
        redirect_target(&from, "c"),
        Ok("https://good.test/a/c".to_string())
    );
    assert_eq!(
        redirect_target(&from, "http://good.test/"),
        Err(Refused::Redirect)
    );
    assert_eq!(
        redirect_target(&from, "/\\evil.test"),
        Err(Refused::Redirect)
    );
    assert_eq!(
        redirect_target(&from, "/\t/evil.test"),
        Err(Refused::Redirect)
    );
    println!("redirects: ok");
}

async fn limit_checks() {
    let nowhere = Scripted::new("good.test", &[&["127.0.0.1"]]);
    let get = async |reply: Reply, deadline_ms: u64| {
        let server = serve(reply).await;
        let port = server.address.port();
        let mut policy = test_policy(vec![origin("good.test", port)]);
        policy.deadline = Duration::from_millis(deadline_ms);
        fetch(
            &policy,
            nowhere.as_ref(),
            &format!("http://good.test:{port}/"),
        )
        .await
    };
    let exact = "a".repeat(1000);
    assert_eq!(
        get(response("200 OK", "", &exact), 5000)
            .await
            .map(|b| b.len()),
        Ok(1000)
    );
    assert_eq!(
        get(response("200 OK", "", &"a".repeat(1001)), 5000).await,
        Err(Refused::TooLarge)
    );
    // a huge announced length is refused before the body arrives
    let huge = Reply::Bytes(b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n".to_vec());
    assert_eq!(get(huge, 5000).await, Err(Refused::TooLarge));
    // no announced length: only counting stops it
    let mut endless = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    endless.extend(std::iter::repeat_n(b'a', 100_000));
    assert_eq!(
        get(Reply::Bytes(endless), 5000).await,
        Err(Refused::TooLarge)
    );
    assert_eq!(
        get(response("200 OK", "Content-Encoding: gzip\r\n", "zz"), 5000).await,
        Err(Refused::Encoded)
    );
    assert_eq!(
        get(response("404 Not Found", "", "no"), 5000).await,
        Err(Refused::Status(404))
    );
    assert_eq!(
        get(Reply::Bytes(b"not http at all\r\n\r\n".to_vec()), 5000).await,
        Err(Refused::Transport)
    );
    assert_eq!(
        get(
            Reply::Bytes(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\nshort".to_vec()),
            5000
        )
        .await,
        Err(Refused::Transport)
    );
    // each byte arrives in time, but the whole answer never does
    for slow in [Reply::Drip, Reply::Silent] {
        let started = Instant::now();
        assert_eq!(get(slow, 300).await, Err(Refused::TooSlow));
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }
    println!("size and time limits: ok");
}

/// Runs in a child process whose `HTTP_PROXY` points at a local fake proxy.
async fn proxy_child(port: u16) {
    let url = format!("http://good.test:{port}/");
    let policy = test_policy(vec![origin("good.test", port)]);
    let resolver = Scripted::new("good.test", &[&["127.0.0.1"]]);
    let ours = fetch(&policy, resolver.as_ref(), &url).await.unwrap();
    // the same pins without `no_proxy`
    let careless = Client::builder()
        .dns_resolver(Arc::new(NoLookups))
        .resolve("good.test", "127.0.0.1:0".parse().unwrap())
        .build()
        .unwrap();
    let theirs = careless
        .get(url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    println!("{} / {theirs}", String::from_utf8(ours).unwrap());
}

async fn proxy_checks() {
    let target = serve(response("200 OK", "", "from the pinned address")).await;
    let proxy = serve(response("200 OK", "", "from the proxy")).await;
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
    child
        .arg("proxy-child")
        .arg(target.address.port().to_string())
        .env_clear();
    child
        .env("HTTP_PROXY", format!("http://{}", proxy.address))
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(20), child.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "from the pinned address / from the proxy"
    );
    assert_eq!(
        (
            target.hits.load(Ordering::SeqCst),
            proxy.hits.load(Ordering::SeqCst)
        ),
        (1, 1)
    );
    println!("environment proxy: ok");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "proxy-child") {
        return proxy_child(args[2].parse().unwrap()).await;
    }
    address_text_checks();
    random_address_checks();
    public_address_checks();
    lookup_checks().await;
    pinning_checks().await;
    rebinding_checks().await;
    redirect_checks().await;
    limit_checks().await;
    proxy_checks().await;
    println!("all fetch policy checks passed");
}
