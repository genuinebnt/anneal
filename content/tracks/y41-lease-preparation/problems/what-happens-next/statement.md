You want to test hours of a system's life in milliseconds, with the same result every time. So the program gets its own clock. Nothing sleeps. An **agenda** holds the things that will happen, and the clock jumps from one to the next.

## `Agenda<E>`

An event is any type `E`. It need not be comparable, hashable or cloneable.

- `now()` starts at 0.
- `after(delay, event)` plans the event for the time `now() + delay` and returns a `Ticket`. A delay of 0 is allowed. If the time does not fit in a `u64`, it returns `None` and changes nothing.
- `next()` removes the pending event with the **earliest** time. Events with the same time come out **in the order they were planned**. `now()` becomes that time, and the pair `(time, event)` is returned. With nothing pending it returns `None` and the clock stays.
- `cancel(ticket)` removes that event if it is still pending and returns it. Otherwise it returns `None`: it already came out, or was cancelled. A cancelled event never moves the clock. (A ticket is only used with the agenda that made it.)
- `len()` counts the pending events.

## `run`

`run(agenda, max_steps, handle)` takes events with `next()` and calls `handle(agenda, time, event)` for each. The handler may plan and cancel events.

It stops in one of two ways:

- `Stop::Finished` when nothing is pending. This includes a run that used exactly `max_steps` steps and left nothing.
- `Stop::OutOfSteps` when it has handled `max_steps` events and something is still pending. That event is **not** taken out.

The budget is how a test survives a system that never stops, such as two machines answering each other for ever.

## Size

Up to 200,000 events can be pending at once, and any of them may be cancelled. The tests do that in a debug build, so no operation may look through all pending events.
