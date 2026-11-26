This service verifies signed webhooks. The header looks like `t=1700000000,v1=<64 hex digits>`, and the signature is
HMAC-SHA256 (provided as `hmac_sha256`, which is correct) with the shared secret over

```text
<t, exactly as written in the header> "." <body bytes>
```

A security review says the verifier **accepts forged and replayed requests**, and operations say rotating the secret
breaks deliveries. Fix `sign` and `verify`.

**`sign(secret, t, body)`** returns the header `t=<t>,v1=<lowercase hex>`.

**`verify(secret, header, body, now, tolerance)`** checks in this order:

1. **`Malformed`** if the header is not a comma-separated list of `key=value` pairs with exactly one `t` (ASCII digits
   that fit a `u64`) and at least one `v1`. Pairs with other keys are ignored. No spaces are trimmed.
2. **`BadSignature`** unless **at least one** `v1` is the hex (either case) of the **whole** 32-byte MAC. A `v1` that is
   not valid hex, or has another length, simply does not match.
3. **`Expired`** if `t` differs from `now` by **more than** `tolerance` seconds, in either direction.

Compare MACs without returning at the first differing byte. The tests cannot measure that, so it is on you.
