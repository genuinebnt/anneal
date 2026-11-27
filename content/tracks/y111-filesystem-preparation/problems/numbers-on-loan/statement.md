A server hands out numbers: ports, desk numbers, ticket stubs. They come back later, in any order. The list of what is out must be small enough to save to disk, so it is **one bit for each number**.

`Lender` lends the numbers `0..count`. The first `reserved` of them are never lent. A `reserved` above `count` means all of them.

## The map

Bit `n % 8` of byte `n / 8` stands for number `n` (bit 0 is the lowest). It is set while `n` is lent or reserved. The map has exactly `count / 8` bytes, rounded up. Bits at `count` and above are 0.

## Calls

- `new(count, reserved)` starts with nothing lent.
- `take()` lends the **first free number at or after the one just after the last number lent**, going round to 0 once. Before anything is lent, the search starts at `reserved`. With nothing free it returns `None`.
- A number given back does **not** move the place where the search starts. So it is not lent again at once, and an old holder's late message is less likely to reach a new one.
- `give_back(n)` frees a lent number. Otherwise the error is the first that fits: `OutOfRange` (`n >= count`), `Reserved`, `NotLent`.
- `free()` is how many can be taken now. `bytes()` is the map.
- `from_bytes(saved, count, reserved)` loads a saved map, with the search at `reserved`. If `saved` is shorter than the map needs, return `None` **without making a map of that size**. Extra bytes are ignored, extra bits are cleared, and reserved bits are set whatever was saved.

Speed is not graded beyond finishing a million numbers.
