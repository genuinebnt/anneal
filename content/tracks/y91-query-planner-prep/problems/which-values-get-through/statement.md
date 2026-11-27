A ticket gate holds a list of rules, such as "age at least 18", "age below 30" and "zone exactly 4". A ticket passes when **every** rule holds. Before opening, the gate wants to know which values can pass at all.

**`allowed(rules)`** returns, for each field that has a rule, the smallest and the largest value that pass all of that field's rules. A field without rules is not listed. A missing bound is `i64::MIN` or `i64::MAX`. If **any** field has no value that passes, the whole result is `None`.

| Test | A value `x` passes when |
|---|---|
| `Above(v)` | `x > v` |
| `AtLeast(v)` | `x >= v` |
| `Below(v)` | `x < v` |
| `AtMost(v)` | `x <= v` |
| `Exactly(v)` | `x == v` |

**`between(entries, low, high)`** gets a list of `(value, id)` pairs sorted by value, then id. It returns the part of that list whose value is in `low..=high`, both ends included. If `low` is above `high`, that part is empty.

Any `i64` can appear anywhere, the smallest and the largest included. There can be 100 000 rules. Nothing may panic.
