# Practice: cryptography

How to use this page: run the recipe (`cargo run --release --example <name>`), answer Prove you got it, then do Try next in order.
These implementations are educational and not constant-time; never use them for real secrets. Check against published test vectors and against an audited crate.
No solutions here. S about 30 min, M about 2 h, A about a day. Recipe files are in `examples/cryptography/`.

## aes_modes
**Prove you got it**
1. Why does ECB leak structure and how does an image show it?
2. How does a padding oracle recover a CBC plaintext byte?

**Try next**
- [S] Check your AES against the FIPS-197 vector and 3 NIST vectors per mode.
- [M] Flip one ciphertext bit in CBC, CTR and GCM; document which plaintext bits change and which modes reject.
- [A] Cross-check 10k random (key, nonce, message) triples against the `aes-gcm` crate from [RustCrypto/AEADs](https://github.com/RustCrypto/AEADs).

**Use it in:** TLS, disk encryption. Real code: [RustCrypto/AEADs](https://github.com/RustCrypto/AEADs), [briansmith/ring](https://github.com/briansmith/ring).
**Pitfall:** reusing a GCM nonce under one key reveals the authentication key.

## chacha20_poly1305
**Prove you got it**
1. Why does nonce reuse break both confidentiality and authenticity?
2. What does the AEAD tag cover besides the ciphertext?

**Try next**
- [S] Pass the RFC 8439 test vectors.
- [M] Show the XOR of two ciphertexts under a reused nonce equals the XOR of the plaintexts, as a test.
- [A] Build a toy encrypted message protocol with a counter nonce, and test replay and reorder detection.

**Use it in:** TLS 1.3, WireGuard. Real code: [RustCrypto/AEADs](https://github.com/RustCrypto/AEADs).
**Pitfall:** random 96-bit nonces collide after about 2^32 messages per key.

## constant_time_side_channels
**Prove you got it**
1. Why does `==` on a MAC leak, and what does the attack recover per request?
2. Why can the compiler break a constant-time comparison?

**Try next**
- [S] Measure the timing difference of early-exit and constant-time compare with 1e6 iterations each.
- [M] Run the timing attack against the recipe's verifier with simulated noise and report the number of samples needed.
- [A] Port to `subtle` ([dalek-cryptography/subtle](https://github.com/dalek-cryptography/subtle)) and compare the generated assembly for a branch.

**Use it in:** MAC and token verification. Real code: [dalek-cryptography/subtle](https://github.com/dalek-cryptography/subtle).
**Pitfall:** secret-indexed table lookups leak through cache even if comparisons are constant time.

## hash_based_signatures
**Prove you got it**
1. Why is a Lamport key one-time only?
2. How does a Merkle tree turn many one-time keys into one public key?

**Try next**
- [S] Demonstrate forgery after signing two messages with one key.
- [M] Add state tracking so reuse is refused, with a test that crash-and-restart does not reuse an index.
- [A] Compare signature sizes of Lamport, WOTS and the tree scheme in a table.

**Use it in:** post-quantum signatures. Real code: [RustCrypto/hashes](https://github.com/RustCrypto/hashes) for primitives.
**Pitfall:** a stateful scheme with backup restore reuses keys.

## hmac_hkdf_pbkdf2
**Prove you got it**
1. Why does `H(key || msg)` suffer length extension and HMAC not?
2. What is the role of salt in HKDF and PBKDF2?

**Try next**
- [S] Verify RFC 4231 and RFC 5869 vectors.
- [M] Implement the length-extension attack on a naive MAC against your SHA-256 and show HMAC resists it.
- [A] Compare PBKDF2 speed to the `pbkdf2` crate in [RustCrypto/password-hashes](https://github.com/RustCrypto/password-hashes) and explain the gap.

**Use it in:** JWT HS256, key derivation. Real code: [RustCrypto/hashes](https://github.com/RustCrypto/hashes).
**Pitfall:** comparing the HMAC with `==`.

## merkle_tree
**Prove you got it**
1. Why do leaves and interior nodes use different prefixes?
2. How big is an inclusion proof for 1e6 leaves?

**Try next**
- [S] Flip one leaf and check proofs for the others still verify.
- [M] Implement a consistency proof between tree sizes n and m and test 100 pairs.
- [A] Build the append-only log with signed tree heads and a client that detects a rollback.

**Use it in:** certificate transparency, sync. Real code: [transparency-dev/merkle](https://github.com/transparency-dev/merkle).
**Pitfall:** second-preimage attack when leaf and node hashes are not domain separated.

## modular_arithmetic_dh_rsa
**Prove you got it**
1. Why does textbook RSA fail without padding, and how is malleability shown?
2. What does Miller-Rabin guarantee with k rounds?

**Try next**
- [S] Break the toy RSA by factoring n and show time versus bits.
- [M] Add a man-in-the-middle on DH and show authentication is required.
- [A] Implement Shamir 3-of-5 over a prime field and test every 3-subset recovers and every 2-subset reveals nothing (check by distribution on a tiny field).

**Use it in:** key exchange. Real code: [rustls/rustls](https://github.com/rustls/rustls).
**Pitfall:** small subgroup and unvalidated public keys.

## secure_random_and_passwords
**Prove you got it**
1. Compute the birthday bound for a 96-bit random nonce.
2. Why do password hashes need to be slow and memory-hard?

**Try next**
- [S] Print the entropy of a 12-character alphanumeric and a 4-word passphrase.
- [M] Add a UUIDv7 and test that ids sort by time and are unique over 1e6 ids.
- [A] Compare time per hash of your toy memory-hard function and `argon2` from [RustCrypto/password-hashes](https://github.com/RustCrypto/password-hashes) with equal memory.

**Use it in:** credential storage. Real code: [RustCrypto/password-hashes](https://github.com/RustCrypto/password-hashes), [uuid-rs/uuid](https://github.com/uuid-rs/uuid).
**Pitfall:** seeding a PRNG from the clock.

## Chapter projects

Other-chapter stems used: `webhooks_signatures`, `auth_basic_bearer_jwt` (web), `encodings_codecs` (encoding_compression), `record_batch_codec` (distributed), `mini_kv_store` (capstones).

### Small (about 2 h): signed tokens
Combine `hmac_hkdf_pbkdf2`, `constant_time_side_channels`, `encodings_codecs`.
Milestones: (1) token format; (2) sign and verify; (3) constant-time compare.
Acceptance: tampered tokens always rejected; timing difference not significant over 1e5 trials.
Stretch: key rotation with ids.

### Intermediate (a weekend): encrypted notes store
Combine `chacha20_poly1305`, `hmac_hkdf_pbkdf2`, `secure_random_and_passwords`, `mini_kv_store`.
Milestones: (1) derive keys from a password; (2) encrypt each value with a unique nonce; (3) store in the KV; (4) wrong password fails cleanly.
Acceptance: no nonce repeats in 1e6 writes; swapping two ciphertexts is detected (bind the key to AEAD data).
Stretch: key wrap and rotation.

### Advanced (a week): transparency log service
Combine `merkle_tree`, `hash_based_signatures`, `axum_router_state` (web), `record_batch_codec`.
Milestones: (1) append-only Merkle log over HTTP; (2) signed tree heads; (3) inclusion and consistency proofs; (4) auditing client that detects a fork.
Acceptance: a malicious server that rewrites history is caught by the client within one check; proofs verify for 1e5 entries.
Stretch: gossip heads between two auditors.
