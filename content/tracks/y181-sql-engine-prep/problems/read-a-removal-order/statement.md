A removal firm takes orders as short text messages, such as `move 2 chairs, 1 piano from attic to garage by fri urgent`. Staff type them on their phones, so mistakes happen. A bad message must come back with the exact place that is wrong.

Write `read(message)`. The starter's `pieces` function has already cut the message into words and commas, each with the bytes it covers.

**The form** of an order. Parts in `[ ]` may be left out, and `{ }` means "any number of times":

```text
order := "move" item { "," item } "from" PLACE "to" PLACE [ "by" DAY ] [ "urgent" ]
item  := COUNT THING
```

- Fixed words (`move`, `from`, `to`, `by`, `urgent`) may be written in any mix of capitals and small letters.
- `COUNT` is made of digits only, and its value is from 1 to `u32::MAX`.
- `THING` and `PLACE` are made of ASCII letters only. None of the five fixed words can be a thing or a place. They are returned in small letters.
- `DAY` is `mon`, `tue`, `wed`, `thu`, `fri`, `sat` or `sun`, in any capitals. A day name is allowed as a thing or a place.

**Mistakes.** Read from left to right. The mistake is at the first piece that could not be part of any correct order starting with the pieces before it. `start` and `end` are the bytes that piece covers. If the message stops too early, both are the length of the message in bytes.

`wanted` says what should have been there:

| `wanted` | When |
|---|---|
| `Move` | the order does not start with `move` |
| `Count` | an item should start (at the beginning, or after a comma) |
| `Thing` | a count is not followed by a thing |
| `From` | an item is followed by neither a comma nor `from` |
| `Place` | `from` or `to` is not followed by a place |
| `To` | the first place is not followed by `to` |
| `Day` | `by` is not followed by a day |
| `End` | the order is complete, but something follows that cannot belong to it |

Messages can have 200 000 pieces. Nothing may panic.
