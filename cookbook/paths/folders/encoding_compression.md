# Chapter `encoding_compression`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`serde_defaults`](../../examples/encoding_compression/serde_defaults.rs) | simple | typed JSON parsing with defaults and strict rejection. |
| 2 | [`encodings_codecs`](../../examples/encoding_compression/encodings_codecs.rs) | intermediate | from-scratch codecs: hex, base64/32, percent-encoding, UTF-8/16, varints, checksums. |
| 3 | [`compression_lz77_huffman`](../../examples/encoding_compression/compression_lz77_huffman.rs) | advanced | a mini DEFLATE: LZ77 hash-chain matcher + canonical Huffman + CRC32 framing. |
| 4 | [`serde_custom`](../../examples/encoding_compression/serde_custom.rs) | advanced | serde beyond derive: hand-written impls, attributes, borrowing, seeds, and a custom data format |
