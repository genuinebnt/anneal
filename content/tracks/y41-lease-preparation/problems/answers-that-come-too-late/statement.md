A program asks a server a question over a network. Messages can be **slow**, **lost** or **delivered twice**. So the program sets a timer, and asks again if the timer goes off first.

The trap is what comes **afterwards**. An answer or a timer you stopped waiting for still arrives. Your `Caller` must never mistake it for a current one.

## What to write

The `Caller` does no input or output itself. It has no clock and no socket. Each call of `handle(input)` takes one thing that happened and returns the things to do, **in order**. That way a test controls every message.

**Timers cannot be cancelled.** Every timer you set will go off sooner or later, perhaps long after it matters.

**Attempts.** Each message sent has an attempt number. Numbers go 1, 2, 3, and so on over the **whole life** of the caller. A number is never used again.

One new attempt means two outputs: `Send { attempt, request }`, then `SetTimer { attempt, wait_ms }`.

## The rules

- **`Ask`** while no question is open: the question is now open. Make its first attempt.
- **`Ask`** while a question is open: return only `Busy { request }` with the **new** request, and change nothing.
- **`TimeUp { attempt }`** counts only if `attempt` is the **newest** attempt of the open question. If the question has had fewer than `max_attempts` attempts, make another. If not, return `Failed { request }` and close the question.
- **`Reply { attempt, answer }`** counts if `attempt` is **any** attempt of the open question: a slow answer to an earlier try is still an answer. Return `Answered { request, answer }` and close the question.
- **`GiveUp`** closes the open question, if there is one, with no output.
- Every other input returns nothing and changes nothing. This includes numbers that were never sent.

**Waiting longer.** The first attempt of a question waits `first_wait_ms`. Each further attempt of the same question waits twice as long as the one before. No wait is ever above `max_wait_ms`, the first one included. Any `u64` and any `u32` can be given to `new`, and nothing may overflow.

`new` panics if `max_attempts` is 0.
