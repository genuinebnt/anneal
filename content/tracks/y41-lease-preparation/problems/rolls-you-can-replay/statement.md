A simulation hits a rare bug on one run in ten thousand. That is only useful if you can run **that run again**. So all of its luck must come from one number, the seed.

No random number crate and no clock: you write the source of numbers.

## The raw rolls

`Dice::new(seed)` holds a 64-bit state that starts at `seed`. Each `roll()` does this, with wrapping arithmetic:

```text
state = state + 0x9E3779B97F4A7C15
z = state
z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
z = (z ^ (z >> 27)) * 0x94D049BB133111EB
return z ^ (z >> 31)
```

This recipe is fixed, so a seed means the same run on every machine. The rest is yours.

## Ranges and chances

- `between(lo, hi)` returns a value from `lo` to `hi`, **both included**. It must work for **every** pair with `lo <= hi`, up to `between(0, u64::MAX)`. It panics if `lo > hi`.
- Assume the raw rolls are perfectly random. Then every value of the range must be **equally** likely. "Nearly equally" is not enough, **even for huge ranges**.
- `between` may use more than one raw roll for one answer.
- `percent(p)` is `true` with probability `p` in 100. `percent(0)` is never true. With 100 or more it is always true.

## Streams

A simulation has several things that need luck: each machine, the network, and so on. If they share one `Dice`, one extra roll by one of them changes the luck of **all** the others.

`Dice::stream(seed, id)` gives each its own `Dice`:

- It depends only on `seed` and `id`. The same pair gives the same rolls.
- For different pairs, the streams must look **unrelated**. The tests compare the first 100 raw rolls of many pairs with small seeds and ids. No value may appear in two of those streams.
- So `stream(1, 2)` and `stream(2, 1)` differ. And no stream is another one **started a few rolls later**.

`stream` need not return any particular values.
