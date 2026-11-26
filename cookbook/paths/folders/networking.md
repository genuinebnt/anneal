# Chapter `networking`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`websocket_frames`](../../examples/networking/websocket_frames.rs) | intermediate | WebSocket framing (RFC 6455): handshake accept key (SHA-1 + base64), frames, masking, fragmentation. |
| 2 | [`dns_message_codec`](../../examples/networking/dns_message_codec.rs) | advanced | DNS wire format (RFC 1035): header, questions, A/AAAA/CNAME/NS/MX/TXT records, name compression. |
| 3 | [`http1_parser`](../../examples/networking/http1_parser.rs) | advanced | HTTP/1.1 message parser and serializer: framing, request smuggling defence, pipelining, fuzzing. |
| 4 | [`kqueue_epoll_raw`](../../examples/networking/kqueue_epoll_raw.rs) | advanced | a raw readiness poller over kqueue (macOS/BSD) and epoll (Linux) behind one trait; level vs edge; 20-client echo server. |
| 5 | [`mio_event_loop`](../../examples/networking/mio_event_loop.rs) | advanced | a single-threaded readiness event loop with `mio`: edge-triggered IO, per-connection state machines, a `Waker` |
| 6 | [`packet_parsing_checksums`](../../examples/networking/packet_parsing_checksums.rs) | advanced | parsing and building packets from raw bytes: Ethernet/VLAN, ARP, IPv4/IPv6, TCP, UDP, ICMP, Internet checksum, fragmentation, pcap, flow table |
| 7 | [`resp_protocol`](../../examples/networking/resp_protocol.rs) | advanced | RESP2, the Redis wire protocol: an incremental zero-copy parser, serializer, limits and a mini server core. |
| 8 | [`tcp_congestion_control_sim`](../../examples/networking/tcp_congestion_control_sim.rs) | advanced | discrete-event simulation of TCP congestion control (Reno, NewReno, CUBIC, simplified BBR) over a drop-tail bottleneck |
| 9 | [`tcp_framed_server`](../../examples/networking/tcp_framed_server.rs) | advanced | a length-prefixed TCP message server, built twice: thread-per-connection (std) and task-per-connection (tokio) |
| 10 | [`tls_handshake_x509_der`](../../examples/networking/tls_handshake_x509_der.rs) | advanced | ASN.1 DER, an X.509 certificate parser and chain validator, and a model of the TLS 1.3 handshake with its HKDF key schedule |
| 11 | [`udp_sockets_reliable`](../../examples/networking/udp_sockets_reliable.rs) | advanced | real UDP on loopback, socket options, then RELIABLE delivery built on a lossy datagram channel (stop-and-wait, Go-Back-N, Selective Repeat) |
