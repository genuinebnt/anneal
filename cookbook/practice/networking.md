# Practice: networking

**How to use this page.** Run the recipe, then answer "Prove you got it" using real sockets on loopback and a real tool (`nc`, `curl`, `dig`, `tcpdump` or Wireshark) as the second opinion; if your code and the tool disagree, the tool is usually right.
Every parser task ends in the same three tests: feed the input whole, feed it one byte at a time, and feed it seeded random garbage (must never panic or allocate unboundedly).
Network tests must be deterministic: bind port 0, read the assigned port back, use channels or barriers instead of sleeps. FlowForge REST API stage: attempt your stage first; these use other protocols.

## dns_message_codec

**Prove you got it**
1. Predict the encoded size of a response with three records for `www.example.com`, `mail.example.com`, `example.com` with and without compression; print both and verify with the compression map in `Encoder`.
2. Predict what a decoder does with a name pointer that points to itself, or forward past its own position. Build the bytes by hand and check that your decoder returns an error rather than looping.

**Try next**
- [S] Add `SRV` and `SOA` record types; round-trip test with 1000 seeded random messages.
- [M] Add a defence: limit total decoded name length (255) and pointer hops (for example 16); craft three hostile packets and assert distinct error variants.
- [A] Write a tiny authoritative UDP server for a zone you define in code; query it with `dig @127.0.0.1 -p PORT` from a test (skip when `dig` is missing) and assert answers and NXDOMAIN; compare ideas with [hickory-dns](https://github.com/hickory-dns/hickory-dns).

**Use it in**: stub resolvers and service discovery. Real code: [hickory-dns](https://github.com/hickory-dns/hickory-dns) (formerly trust-dns).

**Pitfall**: compression pointers make parsers vulnerable to loops and to amplification; bound hops and total size.

## http1_parser

**Prove you got it**
1. Predict the outcome of a request carrying BOTH `Content-Length: 5` and `Transfer-Encoding: chunked`, and of two `Content-Length` headers with different values. Look up the smuggling rules in RFC 9112, then check what your parser does.
2. Predict how many parse calls and how much re-scanning occur when a 1 MB request arrives one byte at a time with the "re-parse from the start" strategy; measure the quadratic cost, then think about how a resumable parser avoids it.

**Try next**
- [S] Add a limit on header count and header size; assert the exact error for 101 headers and 8 KiB+1 line.
- [M] Add chunked trailers and chunk extensions; test the byte-at-a-time equivalence property on 10k seeded random valid requests.
- [A] Build a minimal HTTP server on std threads using your parser: keep-alive, pipelining, 400 on poisoned stream, `Connection: close`; test with `curl --http1.1` and with a raw TCP client sending 3 pipelined requests in one write. Compare with [httparse](https://github.com/seanmonstar/httparse) on speed for 1M header blocks.

**Use it in**: proxies and gateways (compose with `web/reverse_proxy_load_balancer`). Real code: [httparse](https://github.com/seanmonstar/httparse) and [hyper](https://github.com/hyperium/hyper).

**Pitfall**: disagreeing about where a message ends is the root of request smuggling; reject ambiguity instead of guessing.

## kqueue_epoll_raw

**Prove you got it**
1. Predict how many events a level-triggered poller reports for a socket with unread data if you ignore it for 3 loop turns versus edge-triggered (EV_CLEAR or EPOLLET). Verify with a counter.
2. Predict what happens to the 20-client echo server if one client sends 10 MB and never reads the echo; where does the server stall and what must it do (write buffering plus interest on WRITABLE)?

**Try next**
- [S] Add `delete` on close and assert the poller never returns an event for a removed token.
- [M] Add a timer wheel integration: the poller `wait` timeout equals the next deadline (compose with `os_and_storage/timing_wheel`); test that an idle connection is closed within 50 ms of its timeout.
- [A] Implement a minimal reactor plus `Future`-based `TcpStream::read` on top of the poller (compose with `async/waker_executor`) and run an echo server with 5000 connections; compare with [mio](https://github.com/tokio-rs/mio).

**Use it in**: event loops in proxies and databases. Real code: [mio](https://github.com/tokio-rs/mio), and std's Unix socket layer in [`sys`](https://github.com/rust-lang/rust/tree/master/library/std/src/sys/net).

**Pitfall**: edge-triggered without reading until `WouldBlock` loses events permanently.

## mio_event_loop

**Prove you got it**
1. Predict what happens if the loop reads only one `read()` per readable event on an edge-triggered registration with 100 KB pending; run the experiment with a client that sends 100 KB once and then waits.
2. Predict `Waker` behaviour: a worker thread calls `waker.wake()` 1000 times quickly; how many wake events does the loop see (coalescing)? Count.

**Try next**
- [S] Add per-connection idle timeouts using a `BinaryHeap` of deadlines and `Poll::poll(Some(timeout))`.
- [M] Add backpressure: stop reading from a connection while its outbound buffer exceeds 64 KiB; assert memory stays bounded when a client never reads (measure the buffer length).
- [A] Turn it into a multi-loop server (N threads, each with its own `Poll`, `SO_REUSEPORT` listeners); show throughput scaling with a client load generator and compare with the single loop.

**Use it in**: high-connection servers. Real code: [mio](https://github.com/tokio-rs/mio) is the base of tokio's IO driver in [tokio net](https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime/io).

**Pitfall**: forgetting to re-register or to drain writes leaves connections hanging until the next unrelated event.

## packet_parsing_checksums

**Prove you got it**
1. Predict the Internet checksum of a header you build by hand (use the RFC 1071 example), and what `l4_verify` returns when you flip any one bit; then update incrementally with `csum_update` and predict equality with a full recomputation.
2. Predict what the parser does with an IPv4 header whose IHL is 4, whose total length exceeds the buffer, or whose fragment offset overlaps; check every one yields a `ParseError` and no panic.

**Try next**
- [S] Property test: for 100k random byte strings, parse never panics (compose with the 30,000-input fuzz style in the recipe, but scale to 1M).
- [M] Add IPv4 fragment reassembly with a timeout and overlap policy; test that overlapping fragments with different data are handled deterministically.
- [A] Write a flow table (5-tuple to stats) reading a pcap file you generate and then one captured with `tcpdump -w`; print top talkers; verify counts against `tcpdump -r` or `tshark`. Study how [smoltcp](https://github.com/smoltcp-rs/smoltcp) structures its wire module.

**Use it in**: firewalls, IDS and userspace stacks. Real code: [smoltcp](https://github.com/smoltcp-rs/smoltcp).

**Pitfall**: checksums are optional or offloaded on real NICs, so captured packets often have "bad" checksums that are fine.

## resp_protocol

**Prove you got it**
1. Predict the result of parsing `*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n` split at every possible byte position: each prefix must return `Ok(None)` and the full buffer a frame. Run the sweep.
2. Predict how the parser reacts to `$9999999999\r\n` (huge bulk length) and `*-5`; confirm the `Limits` reject before allocating.

**Try next**
- [S] Add pipelining test: 1000 commands in one buffer yield 1000 frames, `consumed` sums to the buffer length.
- [M] Implement `SET`/`GET`/`DEL`/`INCR` with `EX` expiry on top of the parser using your timer wheel or a heap; test with `redis-cli` or a [redis-rs](https://github.com/redis-rs/redis-rs) client if installed.
- [A] Add RESP3 types (maps, sets, doubles, null) with a version-negotiating `HELLO`; build a differential fuzzer between your parser and the redis-rs parser (feed random mutations of valid frames; both must accept/reject the same).

**Use it in**: Redis-compatible servers and clients. Real code: [mini-redis](https://github.com/tokio-rs/mini-redis) and [redis-rs](https://github.com/redis-rs/redis-rs).

**Pitfall**: trusting length prefixes lets one packet allocate gigabytes.

## tcp_congestion_control_sim

**Prove you got it**
1. Predict the shape of cwnd over time for Reno on the drop-tail bottleneck (slow start, sawtooth) and how the queue size (0.25 x BDP vs 1 x BDP vs 4 x BDP) changes utilisation and delay. Print and check.
2. Predict how two Reno flows share the link, and one Reno versus one CUBIC; compute Jain's fairness index from the output.

**Try next**
- [S] Add random loss (1 percent) and show which algorithm degrades most; explain why BBR is less affected.
- [M] Add a second bottleneck or a different RTT per flow and demonstrate RTT unfairness numerically.
- [A] Implement a congestion controller for the reliable-UDP protocol from `udp_sockets_reliable` (AIMD window) and test over a simulated lossy link; compare goodput with fixed windows. Read the congestion code in [quinn](https://github.com/quinn-rs/quinn) afterwards.

**Use it in**: transport tuning and QUIC stacks. Real code: [quinn](https://github.com/quinn-rs/quinn) and [quiche](https://github.com/cloudflare/quiche) implement pluggable controllers.

**Pitfall**: loss-based algorithms fill buffers (bufferbloat): high throughput and high latency at the same time.

## tcp_framed_server

**Prove you got it**
1. Predict what happens when a client sends the 4-byte length and then the payload with a 500 ms pause inside the payload, with a 200 ms read timeout on the server: does the connection survive? Run it on both server variants.
2. Predict the max memory use if 1000 clients each declare a 100 MiB frame; check the frame cap rejects before allocating.

**Try next**
- [S] Add a connection-count limit and an idle timeout; test that the 11th connection is refused when the limit is 10.
- [M] Add request pipelining and out-of-order responses with request ids; assert each response matches its request over 10k seeded interleavings.
- [A] Add TLS using [rustls](https://github.com/rustls/rustls) with a self-signed certificate generated in the test, plus graceful shutdown (compose with `async/graceful_shutdown`).

**Use it in**: custom RPC and database protocols. Real code: [tokio's framing utilities](https://github.com/tokio-rs/tokio/tree/master/tokio/src/io) and [mini-redis](https://github.com/tokio-rs/mini-redis).

**Pitfall**: `read` returning fewer bytes than asked is normal on TCP; always loop (`read_full`).

## tls_handshake_x509_der

**Prove you got it**
1. Predict that the DER decoder rejects a non-minimal length (`0x81 0x05` for a 5-byte value) and an indefinite length; craft both and check. Say why strictness matters for signatures.
2. Predict the HKDF-derived keys for the RFC 8448 test vectors if the recipe includes them (check the main output); if you change one byte of the transcript hash, predict how many output bytes change (all).

**Try next**
- [S] Add certificate validity-window checks with an injectable clock and test three cases (not yet valid, valid, expired).
- [M] Add name constraints or SAN matching including wildcard rules (`*.example.com` matches `a.example.com` but not `a.b.example.com`); table-driven tests with 20 cases.
- [A] Write a certificate generator (self-signed CA plus leaf) using DER encoding by hand and validate your own chain with your validator, then with `openssl verify` (skip when openssl is absent). Compare structure with [rustls](https://github.com/rustls/rustls).

**Use it in**: mTLS service identity. Real code: [rustls](https://github.com/rustls/rustls).

**Pitfall**: validating a signature without checking name, validity and key usage is not validation.

## udp_sockets_reliable

**Prove you got it**
1. Predict what `recv_from` returns when the buffer is 4 bytes and the datagram is 10 bytes (truncation), and whether the rest remains queued. Check.
2. Predict goodput of stop-and-wait versus Go-Back-N versus Selective Repeat at 10 percent loss and window 8; run the simulation and compare with the formula window/RTT.

**Try next**
- [S] Add sequence-number wraparound (8-bit sequence) and assert correct delivery over 10k packets.
- [M] Add duplicate and reordering injection (not just loss) to the channel; assert exactly-once, in-order delivery in all three protocols.
- [A] Build a file transfer over real UDP loopback sockets using your Selective Repeat with a loss/reorder shim socket in between; assert SHA-256 equality of a 10 MB file for 20 seeded loss patterns. Compare design choices with [quinn](https://github.com/quinn-rs/quinn).

**Use it in**: game netcode and QUIC-like protocols.

**Pitfall**: UDP gives you no backpressure; a full receive buffer silently drops datagrams.

## websocket_frames

**Prove you got it**
1. Predict the `Sec-WebSocket-Accept` value for the RFC 6455 sample key (`dGhlIHNhbXBsZSBub25jZQ==` gives `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`), and then check your function against it.
2. Predict what the decoder does with an unmasked client frame, a control frame longer than 125 bytes, and a fragmented ping; decide which the RFC forbids and verify.

**Try next**
- [S] Add UTF-8 validation of text messages including across fragments; test with a multi-byte char split between fragments.
- [M] Add a message-assembly layer with a max message size and interleaved control frames; test ping inside a fragmented message.
- [A] Build a chat server on your frames (compose with `async/bounded_channel` and `concurrency/actor_model`), testable by a browser or the [tungstenite](https://github.com/snapview/tungstenite-rs) client in a test; assert slow clients are dropped without blocking others.

**Use it in**: live-update servers. Real code: [tungstenite-rs](https://github.com/snapview/tungstenite-rs).

**Pitfall**: forgetting that client-to-server frames must be masked, or that server frames must not be.

## Chapter projects

### Small (about 2 h): a robust protocol fuzz harness
Combine `resp_protocol`, `http1_parser`, `dns_message_codec`, and `dev_tools/testing_patterns`.
- M1: define a `trait Parser` with `parse(&[u8]) -> Result<Option<(Frame, usize)>>` and wrap the three.
- M2: shared property tests: byte-at-a-time equals whole, random split points equal whole, random garbage never panics.
- Accept: 1M seeded inputs per parser pass in under a minute; deliberately introduce an off-by-one in one parser and show the harness catches it.
- Stretch: add a shrinker (compose with `dev_tools/shrinking`) that reduces a failing input to a minimal one.

### Intermediate (a weekend): a Redis-compatible mini server with expiry and persistence
Combine `resp_protocol`, `mio_event_loop` or `tcp_framed_server`, `os_and_storage/timing_wheel`, `os_and_storage/wal_append_log`, and `data_structures/lru_cache`.
- M1: single-thread event loop serving GET/SET/DEL/INCR/EXPIRE/TTL.
- M2: expiry using the timer wheel; maxmemory with LRU eviction.
- M3: append-only-file persistence and recovery.
- Accept: `redis-cli` (or redis-rs) scripted session works; `redis-benchmark` (if installed) reaches at least 50k ops/s with pipelining; kill -9 recovery returns every acknowledged write; memory never exceeds the cap by more than 1 percent in a 1M-op seeded test.
- Stretch: replication stream to a replica with a sequence offset (compose with `distributed/kafka_log_model`).

### Advanced (a week): an HTTP/1.1 reverse proxy and load balancer from scratch
Combine `http1_parser`, `kqueue_epoll_raw` or `mio_event_loop`, `tcp_congestion_control_sim` (for tuning thinking), `tls_handshake_x509_der` (use rustls in practice), `concurrency/snapshot_cow_rcu` for hot-reloaded routes, `distributed/consistent_hashing`, `distributed/token_bucket`, and `web/reverse_proxy_load_balancer`.
- M1: event loop with connection state machines for client and upstream sides, keep-alive pooling to upstreams.
- M2: routing table behind an RCU snapshot, reloaded on SIGHUP.
- M3: health checks, consistent-hash and least-connections balancing, per-client token bucket.
- M4: graceful drain and a trace-id header propagated end to end.
- Accept: 50 mock upstreams (some slow, some dead, some malformed) and 5k concurrent clients; every request is answered exactly once (correct 502/504 on failures); smuggling test corpus (CL.TE, TE.CL, duplicate CL) rejected; route reload under load drops zero requests; a latency histogram shows p99 within 3x of p50 at 70 percent load.
- Stretch: TLS termination via rustls and HTTP/2 to upstream with [h2/hyper](https://github.com/hyperium/hyper) as the reference.
