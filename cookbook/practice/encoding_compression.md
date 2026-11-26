# Practice: encoding_compression

How to use this page: run the recipe (`cargo run --release --example <name>`), answer Prove you got it, then do Try next in order.
The best oracle here is a round trip plus a real tool (`gzip`, `base64`, `xxd`); write it first.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/encoding_compression/`.

## compression_lz77_huffman
**Prove you got it**
1. Why does Huffman lose to entropy only by a bounded amount, and what does LZ77 remove first?
2. What makes a Huffman code canonical and why does that save header space?

**Try next**
- [S] Print compression ratio for text, random bytes and a repeated pattern; explain each.
- [M] Add a window size parameter and a lazy matching mode; assert round trip on 1000 random inputs and report ratio change.
- [A] Emit a real gzip stream and check that `gzip -dc` accepts it.

**Use it in:** gzip, PNG, HTTP compression. Real code: [Frommi/miniz_oxide](https://github.com/Frommi/miniz_oxide), [madler/zlib](https://github.com/madler/zlib).
**Pitfall:** random data expands; always allow a stored block.

## encodings_codecs
**Prove you got it**
1. Why is strict decoding (`encode(decode(s)) == s`) a security property?
2. How does a varint encode 300?

**Try next**
- [S] Add base64url without padding and the RFC vectors.
- [M] Add overlong UTF-8 rejection and test with the classic bad sequences.
- [A] Compare with the `base64` and `hex` crates on 1000 random inputs for equality and speed.

**Use it in:** tokens and wire formats. Real code: [uuid-rs/uuid](https://github.com/uuid-rs/uuid) for hex handling; [tokio-rs/prost](https://github.com/tokio-rs/prost) for varints.
**Pitfall:** decoders that accept non-canonical forms enable smuggling.

## serde_custom
**Prove you got it**
1. When do you need a `Visitor`, and what does it receive?
2. What does `deny_unknown_fields` do with `flatten`?

**Try next**
- [S] Add a `"2h30m"` duration test and a failing case with an error that names the field.
- [M] Make a type deserialise from either a string or an object (untagged) and test both plus a bad input.
- [A] Write a tiny custom data format (serializer and deserializer) for key=value lines and round trip a nested struct.

**Use it in:** config and APIs. Real code: [serde-rs/serde](https://github.com/serde-rs/serde), [serde-rs/json](https://github.com/serde-rs/json).
**Pitfall:** `untagged` produces poor errors.

## serde_defaults
**Prove you got it**
1. What does a missing field do with and without `#[serde(default)]`?
2. Why is `u32` better than checking `>= 0` by hand?

**Try next**
- [S] Add a typo test showing `deny_unknown_fields` error text.
- [M] Add a default that depends on another field using a function.
- [A] Generate a JSON schema (by hand or with `schemars`) and validate samples against it.

**Use it in:** config files. Real code: [serde-rs/serde](https://github.com/serde-rs/serde).
**Pitfall:** defaults hiding a misspelt key without `deny_unknown_fields`.

## Chapter projects

Other-chapter stems used: `merkle_tree`, `hmac_hkdf_pbkdf2` (cryptography), `record_batch_codec` (distributed), `json_parser` (text_and_parsing), `axum_errors` (web).

### Small (about 2 h): checked codec toolbox
Combine `encodings_codecs`, `serde_defaults`, `hmac_hkdf_pbkdf2`.
Milestones: (1) config struct with strict parsing; (2) token = base64url(payload) + HMAC; (3) verification.
Acceptance: tampering any byte fails; every bad-config case yields a distinct error.
Stretch: key rotation.

### Intermediate (a weekend): archive tool
Combine `compression_lz77_huffman`, `binary_format_parsing`, `merkle_tree`.
Milestones: (1) container with entries and CRCs; (2) per-entry compression; (3) Merkle root of contents; (4) extract one file by proof.
Acceptance: `gzip -dc` can read a single-entry output; bit flips are always detected; list is O(entries).
Stretch: dedup chunks.

### Advanced (a week): wire protocol with schema evolution
Combine `serde_custom`, `record_batch_codec`, `compression_lz77_huffman`, `axum_errors`.
Milestones: (1) versioned message format; (2) old and new readers interoperate; (3) compressed batches; (4) HTTP endpoint with typed errors.
Acceptance: golden bytes for 3 versions; forward and backward compatibility tests; fuzz gives no panics.
Stretch: write a schema-compatibility checker.
