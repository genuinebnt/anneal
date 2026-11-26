# Chapter `cryptography`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`constant_time_side_channels`](../../examples/cryptography/constant_time_side_channels.rs) | intermediate | constant-time code and side channels: timing leaks in `==`, a timing attack, ct_select/ct_eq, cache leaks, zeroization. |
| 2 | [`secure_random_and_passwords`](../../examples/cryptography/secure_random_and_passwords.rs) | intermediate | secure randomness, nonces and the birthday bound, UUID v4/v7, password hashing (PBKDF2, memory-hard toy), entropy. |
| 3 | [`aes_modes`](../../examples/cryptography/aes_modes.rs) | advanced | AES-128 from scratch with ECB, CBC (+padding oracle), CTR and GCM |
| 4 | [`chacha20_poly1305`](../../examples/cryptography/chacha20_poly1305.rs) | advanced | ChaCha20, Poly1305 and the RFC 8439 AEAD, plus nonce-reuse and tamper demos |
| 5 | [`hash_based_signatures`](../../examples/cryptography/hash_based_signatures.rs) | advanced | hash-based signatures: Lamport, Winternitz (WOTS, w=16), and a Merkle signature scheme (XMSS-like). |
| 6 | [`hmac_hkdf_pbkdf2`](../../examples/cryptography/hmac_hkdf_pbkdf2.rs) | advanced | SHA-256 from scratch, HMAC, HKDF, PBKDF2, and the length-extension attack |
| 7 | [`merkle_tree`](../../examples/cryptography/merkle_tree.rs) | advanced | SHA-256 from scratch + Merkle tree, inclusion proofs, append-only log, anti-entropy. |
| 8 | [`modular_arithmetic_dh_rsa`](../../examples/cryptography/modular_arithmetic_dh_rsa.rs) | advanced | modular arithmetic, Miller-Rabin, Diffie-Hellman, toy RSA (+CRT, malleability), Shamir secret sharing. |
