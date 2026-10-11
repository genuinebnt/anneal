**Where this fits.** The end of the module. Nothing new to build: four layers fail at once, on four threads, each with its own context; then random stacks of context are checked against a model of what they must do; then everything a client may be shown is checked for leaks.

> [!CHECK] A property test adds a random number of context layers to a base error and checks four things afterwards: the report, the number of elements in the chain, the code and the root cause. Which of the four would still pass if your `context` replaced the cause instead of wrapping it, and which would catch it?
> ||The code and the root cause would be wrong, and so would the chain length and the report: a layer that replaces its cause loses the original. If the cause is *kept* but `code()` reads the outermost layer, only the code check fails. The four checks together pin down what context must do: it adds exactly one element, one sentence, and nothing else changes. A test of one of them alone leaves room for a wrong design that happens to satisfy it.
>
> - Which check catches a lost cause, and which a wrong code?
> - What is the smallest base error that distinguishes the wrong designs?
> - Why is a model (the expected report built independently) stronger than comparing two of your own functions?

## The task

Nothing new to implement. The tests run what you built:

- **Four layers on four threads.** Each fails differently (a syntax error, an unknown table, a permission problem, a division by zero), each adds its own context, and the errors travel to the main thread over a channel. Codes and reports must arrive intact.
- **A property over random stacks of context.** For any base error and any stack of messages: the report is the layers outermost first with the cause once; the chain has one element per layer more; the code is the base's; the root cause is the base.
- **The client's view.** Internal failures show only `internal error`, whatever their messages and context said; a client's own mistake shows its message.
- **A long chain.** 2 000 layers are walked and reported, and the code is still the cause's.
- **The standard contract.** A function that returns `Box<dyn Error + Send + Sync>` and uses `?` on a `DbResult` hands back an error from which the `DbError` can be taken out again.

## Your freedom

The same as before. A failure here may be a design decision of an earlier stage showing its cost: a code stored at the wrong layer, a report that rescans, a cause that was flattened.

## The Rust toolbox

**A model beside the code.** The property test builds the expected report from the list of messages, independently of your `report()`. When the two disagree, one of them is wrong, and the shrunk counterexample says which.

**Channels carry errors.** `mpsc::channel` sends any `Send` value: a `DbError` is one, which is the whole reason for the bounds of the first stage.

**Reading a shrunk failure.** The property test prints the smallest input that fails. Reproduce it by hand with two or three context layers before changing anything.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- Four layers, four threads: the right codes and reports arrive.
- Random stacks of context: report, chain length, code and root cause match the model.
- Internal failures reveal nothing; a client's mistakes show their message.
- A 2 000-layer chain and a boxed round trip.

## Hints

### A property fails with two layers

Add exactly that many by hand and print the chain: which element is missing, or doubled? A cause that was flattened and a cause that was wrapped twice look different.

### The report repeats a sentence

An error made by `From` says what its cause says. If the report shows it twice, the repeated-message rule is missing or compares the wrong things.

### A thread test hangs

A channel's receiver ends when every sender is dropped. If the main thread waits forever, a clone of the sender is still alive.

## Performance

The long-chain test is also a quadratic detector: a `report()` that rescans the chain for every layer takes seconds on 2 000 layers instead of microseconds.

**Measure it.** Time the long-chain test in release mode with 2 000 and with 20 000 layers. Doubling the layers should about double the time.

## Experiment

Optional. Predict first, then run.

1. **Make `context` replace the cause.** Which properties fail? Which one tells you first?
2. **Show the root cause's message to the client for class XX.** Which test catches the leak?

## Other designs

None for this stage. The *Other designs* sections of 3k-01 to 3k-04 list the alternatives.

## In BusTub

Not part of BusTub. You now have the shape of an error type that a database could expose.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a test that throws in a thread and checks `std::exception_ptr` | an error sent over a channel and checked by value |
| a fuzzer over exception chains | a property test against a model |

**Port rule:** test an error type as a contract, not as an example.

## Learn more

- [`proptest`](https://docs.rs/proptest) · [`std::sync::mpsc`](https://doc.rust-lang.org/std/sync/mpsc/index.html)
