`n.to_string()` allocates. The `itoa` crate (used by `serde_json`, `tracing` and most fast formatters) writes digits into a
caller-provided stack buffer instead. Implement three functions that each return a `&str` pointing into `buf`:

- `fmt_u64`: decimal digits of a `u64`. A buffer of 20 bytes is always enough.
- `fmt_i64`: the same for an `i64`, with `-` for negatives; this must work for `i64::MIN` (`-9223372036854775808`, 20 characters).
- `fmt_grouped`: a `u64` with a comma between each group of three digits (`1234567` becomes `1,234,567`, `999` stays `999`).

No heap allocation, no `format!`. Zero prints as `0`.
