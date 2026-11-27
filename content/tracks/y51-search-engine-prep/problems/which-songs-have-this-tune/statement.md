Someone hums a few notes and wants to know every song that has them. Notes are numbers.

- `add(notes)` stores the next song and returns its number: 0, 1, 2 and so on. A song may be empty.
- `find(tune)` returns every pair `(song, start)` where the notes of `tune` appear in that song **one straight after another**, in order, beginning at place `start`. Places count from 0.

The pairs come sorted by song, then by start. All of them are returned, overlapping ones included: in the song `7 7 7`, the tune `7 7` starts at 0 and at 1.

A tune lies inside one song. It never runs from the end of one song into the beginning of the next. An empty tune gives an empty answer.

**The costly case.** Most notes of a tune are common, and one may be rare. It can be **anywhere** in the tune. The tests grade this case on its own function:

`starts(notes)` works for one tune. `notes[i]` lists every `(song, place)` where note `i` of the tune is, rising, with no repeats. The function returns, rising, every `(song, start)` such that for **each** `i`, `notes[i]` holds `(song, start + i)`. An empty `notes` gives nothing. A list can be read only through `len()` (free) and `get(i)` (counted).

**The budget.** Let `w` be the number of lists, `s` the length of the shortest and `L` the length of the longest. Let `bits(L)` be the number of binary digits of `L` (`usize::BITS - L.leading_zeros()`). One call may use `get` at most `w * (s + 1) * (2 * bits(L) + 4)` times. A test list stops the test as soon as the budget is passed. `L` can be a thousand million.

Only these reads are graded. The speed of `Songbook` is not. To see what your `find` costs, time it over two million notes, with the rare note last in the tune. `find` need not call `starts`.
