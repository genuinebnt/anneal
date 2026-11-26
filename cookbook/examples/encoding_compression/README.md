# Encoding and Compression

serde, base64/hex/UTF-8, varints, LZ77/Huffman, bitmaps.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

4 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`serde_defaults`](serde_defaults.rs) | typed JSON parsing with defaults and strict rejection | simple | config/API payload parsing (serde is the default for JSON/TOML/YAML in nearly every Rust service); strict schemas reject typos early. | serde, serde-json |
| [`compression_lz77_huffman`](compression_lz77_huffman.rs) | a mini DEFLATE: LZ77 hash-chain matcher + canonical Huffman + CRC32 framing | advanced | gzip/zlib/PNG-style DEFLATE streams, HTTP Content-Encoding, backup/archive tools, columnar | std only |
| [`encodings_codecs`](encodings_codecs.rs) | from-scratch codecs: hex, base64/32, percent-encoding, UTF-8/16, varints, checksums | intermediate | base64 in JWTs, data: URLs, MIME and PEM; base32 in TOTP secrets and some content ids; | std only |
| [`serde_custom`](serde_custom.rs) | serde beyond derive: hand-written impls, attributes, borrowing, seeds, and a custom data format | advanced | human-friendly config values (durations, colours, sizes), versioned protocols with tagged enums, | serde, serde-json, thiserror |
