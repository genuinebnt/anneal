A device moves between positions numbered from 0. You cannot see where it is. You only hear signals.

- A **signal move** `(from, signal, to)` happens when the device makes that signal: it goes from `from` to `to`. Several moves may share a start and a signal, and then the device takes any one of them.
- A **silent move** `(from, to)` makes no sound. The device may take any number of silent moves, or none, at any time.

Write `Tracker`:

- `Tracker::new(&machine, start)`: the device starts at `start`.
- `observe(signal)`: one signal has been heard.
- `possible()`: every position the device could be in **now**, given everything heard so far, sorted and without repeats.

If no history could have made the signals heard, `possible()` is empty, and stays empty whatever is heard next.

All positions used are below `machine.positions`. Silent moves may form loops.

**Sizes.** A machine may have hundreds of thousands of positions, with silent chains that long. Tens of thousands of signals may arrive. The number of different histories can grow without any useful limit.
