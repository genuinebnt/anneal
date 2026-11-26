A payment provider sends webhook events as JSON. Their format is fixed. Make the Rust types in the starter read and
write **exactly** that format, using `serde` attributes (and a little code where no attribute fits). Keep the names and
types of the Rust fields and variants: the tests build and compare these values.

A charge:

```json
{"id":"evt_1","created":1700000000,"type":"charge.succeeded",
 "data":{"amountCents":"1250","currency":"EUR","note":"gift"},
 "tags":["web"]}
```

The other two kinds:

```json
{"id":"evt_2","created":5,"type":"charge.refunded","data":{"chargeId":"ch_9","reason":"requested_by_customer"}}
{"id":"evt_3","created":6,"type":"ping"}
```

- `amountCents` is a `u64` sent as a **string** of ASCII digits. A JSON number, an empty string, a sign, spaces or a
  value too large for `u64` is an error.
- `note` may be absent or `null` when reading; when it is `None`, it is **left out** when writing.
- `reason` is `duplicate`, `fraudulent` or `requested_by_customer`. Any **other string** reads as `Reason::Other`,
  which is written as `other`.
- `tags` may be absent when reading, and is left out when empty.
- Unknown fields, at the top level or inside `data`, are **ignored**: the provider adds fields without warning.
- An unknown `type`, or a missing `id`, `created`, `data` field or required field inside `data`, is an error.

Object key order does not matter.
