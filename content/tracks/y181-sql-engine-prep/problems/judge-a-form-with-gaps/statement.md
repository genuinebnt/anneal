A grant office decides applications with a **rule** over the answers on a form. Applicants leave questions blank. The old program read a blank as 0 and turned good applications down.

A form is a list of answers: `answers[q]` is the answer to question `q`. It is **missing** when it is `None`, or when the list is too short to hold it.

Write `judge(rule, answers)`. The verdict is `Yes`, `No` or `Unknown`.

| Rule | Verdict |
|---|---|
| `AtLeast(q, n)` | `Unknown` if answer `q` is missing, else whether it is `>= n` |
| `Same(a, b)` | `Unknown` if either answer is missing, else whether they are equal |
| `Missing(q)` | whether answer `q` is missing. Never `Unknown` |
| `Not(r)` | `Yes` and `No` swap; `Unknown` stays |
| `All(rs)` | `No` if any is `No`. Otherwise `Unknown` if any is `Unknown`. Otherwise `Yes` |
| `Any(rs)` | `Yes` if any is `Yes`. Otherwise `Unknown` if any is `Unknown`. Otherwise `No` |

Each part is judged by these rules alone. The program does not reason about whether some answer could ever make a part `Yes`. So `Same(0, 0)` with answer 0 missing is `Unknown`.

Then write `sort_forms(rule, forms)`. It returns two lists of form numbers, each in order: the forms to **accept** (the verdict is `Yes`) and the forms to **send back** for the missing answers (`Unknown`). A form judged `No` is in neither.

A rule is at most 100 deep. `All` and `Any` may have no parts.
