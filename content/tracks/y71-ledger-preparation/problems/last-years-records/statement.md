A parcel company has stored three years of tracking records as JSON. The shape changed twice. The old records are evidence in disputes, so **nobody may rewrite them**. The new code must read them as they are.

A record looks like this:

```
{"v": 3, "kind": "sent", "data": {...}}
```

Write `read(stored)`. It returns the record as a `Parcel`, the **current** shape.

**Version 3**, the current one, has two kinds:

- `"sent"`: `name` and `city` (strings), `weight_g` (whole number from 0 to `u64::MAX`), `carrier` (string).
- `"lost"`: `note` (string).

**Version 2** is like version 3 with two differences. The weight is called `grams`. There is no carrier: every parcel went by `"post"`.

**Version 1** is like version 2 with these differences:

- `"sent"` is called `"shipped"`.
- Instead of `name` and `city`, one string `to` holds both, as `"name, city"`.
- Split `to` at the **last** `", "`, because a name may hold commas. A `to` without `", "` is bad.
- In `"lost"`, the `note` is called `why`.

A record **without** `v` is version 1.

**Each version knows only its own fields.** Any other field is ignored, even when a later version gives that name a meaning. A kind that its version does not define is unknown. So `"shipped"` is unknown in versions 2 and 3.

**Errors**, checked in this order:

1. `NotARecord`: the text is not a JSON object, or its `v` is not a whole number from 0 up, or `kind` is not a string.
2. `UnknownVersion(v)`: `v` is 0 or greater than 3.
3. `UnknownKind(kind)`: the kind as it is written in the record.
4. `BadData`: `data` is missing or not an object, or a field the version needs is missing or has the wrong type.
