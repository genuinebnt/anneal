A help centre has thousands of pages. People search it by naming words that a page must have, may have and must not have.

A **word** of a page is a longest run of letters and digits (`char::is_alphanumeric`), lowercased with `str::to_lowercase`. So `Wi-Fi isn't on!` holds `wi`, `fi`, `isn`, `t` and `on`.

- `add(text)` stores the next page and returns its number: 0, 1, 2 and so on.
- `pages_with(word)` returns the numbers of the pages that hold the word, in rising order, each once.
- `find(all, any, none)` returns, in rising order, the pages that:
  - hold **every** word in `all`,
  - **and** hold **at least one** word in `any`, unless `any` is empty,
  - **and** hold **no** word in `none`.

An empty list asks for nothing, so `find(&[], &[], &[])` returns every page.

Each word in a question is one word of letters and digits, in any case. Lowercase it the same way. The same word may be given twice.

**The costly case.** A question often joins a word that is on three pages with words that are on every page. The tests grade this case on its own function:

`in_every_list(lists)` returns, rising, the numbers that are in **every** list. Each list rises with no repeats, and can be read only through `len()` (free) and `get(i)` (counted). No lists give no numbers. The lists come in any order.

**The budget.** Let `w` be the number of lists, `s` the length of the shortest and `L` the length of the longest. Let `bits(L)` be the number of binary digits of `L` (`usize::BITS - L.leading_zeros()`). One call may use `get` at most `w * (s + 1) * (2 * bits(L) + 4)` times. A test list stops the test as soon as the budget is passed. `L` can be a thousand million.

Only these reads are graded. The speed of `Shelf` is not. To see what your `find` costs, time it with one rare and two very common words over 100 000 pages. `find` need not call `in_every_list`.
