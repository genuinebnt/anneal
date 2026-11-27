A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`FrameDecoder` in `src/rust_primer/framing.rs`: bytes arrive from a socket or a log file in pieces of any size; each message (a **frame**) is a 2-byte big-endian length followed by that many payload bytes. `push` takes the next piece and returns the frames that are now complete; what is left over waits for the next call.

## Why

A read never promises to return a whole message: TCP splits and joins at will, and a log can end mid-record after a crash. Every protocol parser and every recovery routine has this loop in it. Getting it right means the *same* frames come out however the bytes were cut.

## The contract

- `push(bytes)` appends to what is buffered and returns every complete frame, in order.
- A frame announcing more than `max_frame` payload bytes is an error `TooLong { len }`; the decoder then forgets its buffered bytes.
- `buffered()` is the number of bytes held that are not yet part of a returned frame.

## Invariants

These must hold after every step, whatever the input:

- `buffered()` is always less than one whole frame (2 + length of the frame being waited for).
- Frames come out in the order their bytes went in; none is returned twice.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Cutting the same stream into pieces differently (one byte at a time, all at once, random cuts) gives the same frames.
- Pushing an empty slice changes nothing.
- The frames of a stream followed by more bytes start with the frames of the stream alone.

## Examples

Worked cases (the tests include them):

```text
[0,3,a,b,c] -> [abc]
[0,3,a] then [b,c,0,1,z] -> [] then [abc, z]
[0,0] -> one empty frame
[255,255,...] with max 100 -> TooLong
```

## What the tests check

- Whole frames, split frames, empty frames.
- A header split across two pushes.
- Too-long frames are an error and the decoder recovers.
- A property: any chunking gives the same frames.

## Done when

All the `sr_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Bytes](/cookbook/p/bytes-buf-buf-mut-framing): an incremental length-delimited decoder that keeps partial input between calls.
- [A length-prefixed TCP message server, built twice](/cookbook/p/tcp-framed-server): length-prefixed frames over TCP, where reads stop mid-frame.
- [RESP2, the Redis wire protocol](/cookbook/p/resp-protocol): an incremental parser that leaves a partial command buffered for the next read.
