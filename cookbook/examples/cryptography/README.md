# Cryptography (educational)

Hashes, MACs, AEADs, AES, RSA, signatures, constant-time code, password storage, Merkle trees. Not for production.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

8 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`merkle_tree`](merkle_tree.rs) | SHA-256 from scratch + Merkle tree, inclusion proofs, append-only log, anti-entropy | advanced | Git and Mercurial-style content-addressed object graphs (hash-linked trees), Bitcoin block | std only |
| [`hmac_hkdf_pbkdf2`](hmac_hkdf_pbkdf2.rs) | SHA-256 from scratch, HMAC, HKDF, PBKDF2, and the length-extension attack | advanced | HMAC in TLS 1.2 PRF/records and JWT HS256 signatures; HKDF in TLS 1.3 key schedule, Signal, Noise; | std only |
| [`chacha20_poly1305`](chacha20_poly1305.rs) | ChaCha20, Poly1305 and the RFC 8439 AEAD, plus nonce-reuse and tamper demos | advanced | TLS 1.3 cipher suite TLS_CHACHA20_POLY1305_SHA256, OpenSSH chacha20-poly1305@openssh.com, | std only |
| [`aes_modes`](aes_modes.rs) | AES-128 from scratch with ECB, CBC (+padding oracle), CTR and GCM | advanced | AES-GCM in TLS 1.2/1.3 and QUIC, AES-CTR in SSH, AES-CBC in legacy TLS and old file formats | std only |
| [`modular_arithmetic_dh_rsa`](modular_arithmetic_dh_rsa.rs) | modular arithmetic, Miller-Rabin, Diffie-Hellman, toy RSA (+CRT, malleability), Shamir secret sharing | advanced | Miller-Rabin in key generation (RSA/DH primes); Diffie-Hellman (finite-field and its elliptic-curve variant) | std only |
| [`constant_time_side_channels`](constant_time_side_channels.rs) | constant-time code and side channels: timing leaks in `==`, a timing attack, ct_select/ct_eq, cache leaks, zeroization | intermediate | constant-time HMAC/tag comparison (TLS, JWT and webhook-signature verification), password-hash verification, | std only |
| [`hash_based_signatures`](hash_based_signatures.rs) | hash-based signatures: Lamport, Winternitz (WOTS, w=16), and a Merkle signature scheme (XMSS-like) | advanced | XMSS (RFC 8391) and LMS/HSS (RFC 8554) for firmware and boot-image signing where the signer is a controlled HSM; | std only |
| [`secure_random_and_passwords`](secure_random_and_passwords.rs) | secure randomness, nonces and the birthday bound, UUID v4/v7, password hashing (PBKDF2, memory-hard toy), entropy | intermediate | session tokens and API keys from the OS RNG; AEAD nonce management in TLS 1.3 (sequence-number-derived nonces) and | std only |
