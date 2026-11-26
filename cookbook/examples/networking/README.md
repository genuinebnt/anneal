# Networking and Protocols

TCP servers, event loops, kqueue/epoll, io_uring model, HTTP/1.1, DNS, WebSocket, RESP.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

11 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`resp_protocol`](resp_protocol.rs) | RESP2, the Redis wire protocol: an incremental zero-copy parser, serializer, limits and a mini server core | advanced | Redis/Valkey clients and servers, protocol proxies and load balancers (e.g. connection-pool middleware), | std only |
| [`tcp_framed_server`](tcp_framed_server.rs) | a length-prefixed TCP message server, built twice: thread-per-connection (std) and task-per-connection (tokio) | advanced | Redis-style and Postgres-style wire protocols (length-prefixed messages), gRPC/HTTP2 frames, | tokio |
| [`mio_event_loop`](mio_event_loop.rs) | a single-threaded readiness event loop with `mio`: edge-triggered IO, per-connection state machines, a `Waker` | advanced | the reactor under tokio and other Rust async runtimes; Redis-like single-threaded servers; | mio |
| [`http1_parser`](http1_parser.rs) | HTTP/1.1 message parser and serializer: framing, request smuggling defence, pipelining, fuzzing | advanced | HTTP servers (hyper uses httparse), reverse proxies and load balancers, API gateways, WAF-style request | std only |
| [`dns_message_codec`](dns_message_codec.rs) | DNS wire format (RFC 1035): header, questions, A/AAAA/CNAME/NS/MX/TXT records, name compression | advanced | stub resolvers and recursive resolvers, authoritative servers, DNS-over-HTTPS/TLS proxies, mDNS/Bonjour | std only |
| [`websocket_frames`](websocket_frames.rs) | WebSocket framing (RFC 6455): handshake accept key (SHA-1 + base64), frames, masking, fragmentation | intermediate | browser-to-server live channels (chat, notifications, collaborative editing, dashboards), market-data and | std only |
| [`kqueue_epoll_raw`](kqueue_epoll_raw.rs) | a raw readiness poller over kqueue (macOS/BSD) and epoll (Linux) behind one trait; level vs edge; 20-client echo server | advanced | the event loop under nginx, Redis, libuv (Node.js), Netty, tokio and async-std/smol (via polling); game servers; load balancers. | libc |
| [`udp_sockets_reliable`](udp_sockets_reliable.rs) | real UDP on loopback, socket options, then RELIABLE delivery built on a lossy datagram channel (stop-and-wait, Go-Back-N, Selective Repeat) | advanced | QUIC (RFC 9000), DNS, DHCP, NTP, SNMP, WireGuard, game state sync, RTP/VoIP, service discovery | libc |
| [`tcp_congestion_control_sim`](tcp_congestion_control_sim.rs) | discrete-event simulation of TCP congestion control (Reno, NewReno, CUBIC, simplified BBR) over a drop-tail bottleneck | advanced | Linux CUBIC/BBR tuning, QUIC congestion controllers (quinn, Google's QUIC), datacenter variants | std only |
| [`packet_parsing_checksums`](packet_parsing_checksums.rs) | parsing and building packets from raw bytes: Ethernet/VLAN, ARP, IPv4/IPv6, TCP, UDP, ICMP, Internet checksum, fragmentation, pcap, flow table | advanced | tcpdump/Wireshark dissectors, Suricata/Zeek-style monitors, Linux netfilter conntrack, | std only |
| [`tls_handshake_x509_der`](tls_handshake_x509_der.rs) | ASN.1 DER, an X.509 certificate parser and chain validator, and a model of the TLS 1.3 handshake with its HKDF key schedule | advanced | every HTTPS connection; mTLS between services; certificate pinning and expiry monitoring; | std only |
