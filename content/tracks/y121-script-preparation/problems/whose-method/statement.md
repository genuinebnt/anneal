A game defines its creatures as kinds with parents, such as `Dragon` under `Flyer` under `Creature`. A kind can replace what its parent does, or add to it. Designers keep finding that the wrong version runs. Write `play(classes, script)`.

**Classes.** `classes[i].parent` is `None` or an index smaller than `i`. A class **provides** its own methods and inherits the rest. Looking for a method "from class c" means: c itself, then its parent, and so on. The first class that has it wins.

Chains of parents may be tens of thousands long. The long-chain test runs `play` on a thread with a **256 KB** stack.

**Objects** have a class and some fields. `New` makes one; objects are numbered from 0. Every index in a script is valid.

**Running a method** performs its actions in order, for one object.

| Action | Meaning |
|---|---|
| `Say(text)` | add the text to the output |
| `Own(m)` | find `m` from **the object's class**, and run it for the same object |
| `Inherited(m)` | find `m` from the parent of **the class that provides the method now running**, and run it for the same object |
| `Keep { method, field }` | find the method as `Own` would, and store it in the object's field, **with** the object. Do not run it |
| `KeepInherited { method, field }` | the same, finding it as `Inherited` would |

A stored method is a finished choice. `Fire(o, field)` runs what is stored in `o`'s field: the same method, for the same object. 

It does not search again, and it behaves exactly as it would have if it had run where it was found. Storing again replaces what was there. Each object has its own fields.

**Commands.** `Send(o, m)` finds `m` from `o`'s class and runs it.

**Errors** end the script. The output keeps what was said before.

- `NoMethod(m)`: a search found nothing. A class without a parent finds nothing with `Inherited`.
- `EmptyField(f)`: `Fire` on a field that holds nothing.
- `TooDeep`: a method would start while `MAX_RUNNING` (50) methods are already running.
