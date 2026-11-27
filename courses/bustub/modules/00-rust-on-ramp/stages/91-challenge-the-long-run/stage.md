A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/rust_primer/rle.rs` has a complete run-length codec: `aaab` becomes the pairs (3, a) (1, b). It looks right, and it has one bug. Find it with the tests, and fix it.

## Why

Most real debugging starts from a failing test and a counterexample, not from reading code. A property test that fails prints the *smallest* input it could find that breaks the rule, and that input is usually the whole bug report.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||A run longer than 255 was written as one pair with `run as u8`, which keeps only the low byte, so a run of 256 bytes came back as a run of 0. A long run has to be split into pairs of at most 255. Hand-written inputs never repeat a byte 256 times; a property test's generator does.||

## The contract

- `rle_encode` turns data into (count, byte) pairs with counts from 1 to 255.
- `rle_decode` turns them back and reports a count of 0 or a missing byte as an error.

## Invariants

These must hold after every step, whatever the input:

- Every count in the encoding is between 1 and 255.
- The counts add up to the length of the data.
- No pair is wider than two bytes.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `decode(encode(x)) == x` for every `x`.
- The encoding is never more than twice the length of the data.
- Encoding data made of one repeated byte gives `ceil(len / 255)` pairs.

## Examples

Worked cases (the tests include them):

```text
[] -> []
"aaab" -> [3, a, 1, b]
[7; 255] -> [255, 7]
[7; 256] -> [255, 7, 1, 7]
```

## What the tests check

- The exact pairs for small inputs and for the longest run one pair can hold.
- Round trips of random data, including long runs.
- A well-formedness property: counts and lengths.
- Bad input to the decoder is an error.

## Done when

All the `sr_c2` tests pass, and you can say in one sentence what the bug was.
