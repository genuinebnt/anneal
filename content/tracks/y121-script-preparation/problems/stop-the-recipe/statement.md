A test bench runs scripted procedures against a device. Each script is a list of nested steps, and the engineers want to know exactly where a run stopped. Write `run(procedures, fuel)`. It runs the procedure called `main`.

**Steps** run in order.

| Step | Meaning |
|---|---|
| `Log(c)` | add `c` to the log |
| `Repeat(n, steps)` | run `steps` up to `n` times; each time is a **round**, numbered from 1 |
| `InRound(r, step)` | run `step` only if the current round of the nearest repeat is `r` |
| `NextRound` | leave the rest of this round of the nearest repeat |
| `EndRepeat` | leave the nearest repeat altogether |
| `Run(name)` | run another procedure, then go on |
| `Return` | leave the current procedure |
| `Alarm(code)` | stop everything |

"The nearest repeat" means the nearest one **around the step in the same procedure**. It may be many levels of `InRound` away. A repeat in the caller does not count. `Return` leaves only its own procedure, from inside any number of repeats.

**Failures** stop the whole run. The log keeps what was written before.

| `Cause` | When |
|---|---|
| `Alarm(code)` | an alarm step runs |
| `UnknownProcedure(name)` | `Run` names a procedure that does not exist (or there is no `main`) |
| `NoRepeat` | `NextRound`, `EndRepeat` or `InRound` runs with no repeat around it in its procedure |
| `TooDeep` | a `Run` would make `MAX_RUNNING + 1` procedures run at once; `main` counts |
| `OutOfTime` | the fuel ran out |

For `Run`, an unknown name is checked before the depth. A step that is not reached is never an error. `InRound` with no repeat around it fails whether or not its number would match.

**The trace** lists the procedures that were running at the moment of the failure, innermost first, ending with `main`. Procedures that had returned are not in it. For `UnknownProcedure` and `TooDeep`, the procedure that could not start is not in it. With no `main` the trace is empty.

**Fuel.** Every step that runs costs 1, and so does every round of a repeat, even an empty one. When a cost cannot be paid, the run stops with `OutOfTime`. So `fuel` bounds the whole run.

Steps are nested at most 100 deep. The tests run `run` on a thread with a 64 MB stack.
