Implement the framing layer used by almost every binary protocol (Redis-style, Kafka, gRPC's message prefix, `tokio_util`'s
`LengthDelimitedCodec`): each message is `[len: u32 big-endian][payload of len bytes]`.

`encode_frame` writes one frame. `Decoder` is fed **arbitrary chunks** with `feed` and hands out complete frames with
`next_frame`: `Ok(Some(payload))` for the next whole frame, `Ok(None)` when it needs more bytes (and keeps what it has), and
`Err(TooLarge)` as soon as the 4-byte header announces more than `max_frame` bytes: **before** any of that payload is
buffered. Several frames may arrive in one chunk, and a frame may arrive one byte at a time. A zero-length frame is valid.
