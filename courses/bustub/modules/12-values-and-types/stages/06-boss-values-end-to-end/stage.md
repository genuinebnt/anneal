**Where this fits.** The end of the values module: the pieces together, then BusTub's own `type_test`.

> [!CHECK] A value is computed (`a * b`), stored in a page, read back and compared with the value you started from. Name two bugs in different stages of this module that this one round trip can reveal even though each stage's own tests passed.
> ||Each stage's tests check one function against its own oracle, so two functions can each pass and still disagree about a value: a cast that produces NULL in a form the comparison does not treat as NULL, or a comparison that is wrong for one pair of types, makes the final `compare_equals` disagree although every step passed its own tests. Stage tests check one function at a time against its own oracle; the chain checks that the functions agree on what a value of a type *is*.||
>
> - Which function produces the type the reader is told?
> - Which function decides the final answer?
> - Why use `compare_equals` rather than `==` on the enum?

## The task

Nothing new to design.

- **A chain property.** For random integers `a`, `b` and each of the five operators: if the arithmetic succeeds, serialise the result, deserialise it with its own type, and `compare_equals` it with the result: `True`.
- **Text and back.** A random integer cast to text and back to its own type compares equal to itself.
- **BusTub's `type_test`** (`type_test.rs`): the invalid type, the types' coercion from themselves, max and min values, comparing a string with an integer, and a generic struct that compares two `Value`s.

## Your freedom

Everything: the earlier stages' code is all you use here.

## The Rust toolbox

**Reading a property failure.** The shrunk counterexample prints the operator and the two numbers; compute it by hand with `i128` and see which of cast, arithmetic, comparison or storage gave a different answer.

**`println!` in a test.** `cargo test -- --nocapture` shows what your code printed; use `eprintln!("{v:?}")` to see the value at each stage.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- The arithmetic, storage and comparison chain.
- Cast to text and back.
- BusTub's `type_test`.

## Hints

### The chain fails and every stage passed

Print the value after each step. The type of the result is the first thing to check: `TINYINT * TINYINT` must be `TINYINT`, and the reader is told that type.

## Performance

None specific: the module is cheap. Run `cargo test --release` once and note the time of the properties (a few thousand cases each); a module you will call millions of times per second should be tested well.

## Experiment

Optional. Predict first, then run.

1. **Wider chains.** Add a cast to a wider type before storing. Does the property still hold? Why must it, and what would break it?
2. **A value type of your own.** Add a `Date` variant (days since an epoch) end to end: which matches does the compiler show you, and how many tests of your own do you write?

## Other designs

None for this stage. The *Other designs* sections of 3a-01 to 3a-05 list the alternatives to compare with yours.

## In BusTub

`type_test.cpp` is small (BusTub leaves most of the value tests to the students): the invalid type, `GetInstance`, `MaxValue`, `MinValue`, a string equal to an integer and a template test. Your module must also work as the foundation of every executor in Project 3.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Type::GetInstance(type_id)` | not needed: type-level functions live on `TypeId` |
| `ASSERT_DEATH` for invalid type | `assert!(type_id.type_size().is_err())` |
| `template <class K, class V> class BPlusTreePage` in the test | a generic struct with a method over `Value`s |

**Port rule:** a singleton `Type` object per type becomes methods on a `Copy` enum.

## Learn more

- BusTub's [`type_test.cpp`](https://github.com/cmu-db/bustub/blob/master/test/type/type_test.cpp)
