Two bank clerks each read a balance, add 10 and write it back. Usually the account gains 20. Sometimes it gains only 10. It depends on **who moved when**, and testing by running real threads may never show it.

So take the threads away. Each **actor** is a list of steps. A **schedule** says who moves next. Now every possible run can be named, searched for and run again.

## What to write

A `Step<S>` changes the shared state `S`. A step cannot fail and can be taken at any time. Anything an actor remembers between its steps lives inside `S`. Steps are plain functions: the same step on equal states gives equal states.

A **schedule** is a list of actor numbers. Each entry makes that actor take **its next** step. So `[0, 1, 1, 0]` runs actor 0's first step, actor 1's first two steps, then actor 0's second. A schedule may stop before the actors are done.

- `replay(start, actors, schedule)` returns every state along the way: the start, then one state after each entry. It returns `None` if any entry names no actor, or an actor with no step left.
- `shortest_failure(start, actors, ok)` looks for a schedule after which `ok` is **false**. It may fail **in the middle** of a run, even if later steps would repair the state.

Which failing schedule it returns is fixed, so that a bug report never changes:

1. one with the **fewest** entries;
2. among those, the **first in dictionary order**: compare entry by entry, and the smaller actor number wins.

It returns the empty schedule if the start already fails, and `None` if no schedule can fail. `ok` depends only on the state it is given.

## Size

The tests run in a debug build with up to 6 actors of 6 steps each. That is about 2,670,000,000,000,000,000,000,000 complete schedules, so you cannot try them one by one. In the large tests the steps lead to **few different states**: fewer than 200,000 combinations of a state and how far each actor has got.
