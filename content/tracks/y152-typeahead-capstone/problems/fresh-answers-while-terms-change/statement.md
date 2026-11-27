> **Spoiler before you open the editor.** The starter holds reference code for steps 1 and 2: the index, `classify` and the search. 
>
> It also holds the small cache `Recent`, and the answers to **Light up what the user typed** and **How many slips of the finger?** Solve those first.
>
> Solve these two as well: **An O(1) LRU cache without Rc or unsafe** and **Never serve an answer from before the change**. The code gives away much of the first. You need the idea of the second here.

**Capstone, step 3 of 4.** Many threads now share one service. Most keystrokes repeat, so answers are cached. Terms change all the time, and no user may see an answer from before a change.

`Service` is shared by many threads, so every method takes `&self`.

- `new(capacity, judge)`. `judge(typed_key, term_key)` gives the verdict of the matching rule, as `classify` does. It is the **only** way your service may decide whether a term matches, both when searching and when a write arrives. It never calls back into the service.
- `add` and `choose` work as before. `len` is the number of terms.
- `suggest(typed, limit, now)` returns `Answer { cached, items }`. The items are what `search` of step 2 returns. Their scores are the scores **at this call's `now`**, also for a cached answer.

**Fresh answers.** Each answer must equal a fresh search of the terms as they were at some moment between the start and the end of the call. So after `add` or `choose` returns, every later answer shows the change.

**The cache** holds at most `capacity` answers. A `capacity` of 0 caches nothing.

- An answer is stored under the key of `typed` and `min(limit, 20)`. Two spellings with one key share it. Two limits do not.
- `cached` is `true` when the answer came from the cache. Such a call does not call `judge`.
- **An answer must be stored** when no write ran or started while it was worked out.
- **A stored answer must stay** when no later write names a term whose key matches its typed key. "Names" means each pair of an `add` with a non-empty key, and each successful `choose`. "Matches" means `judge` says `Some`. A write that names no such term is no reason to drop it, and neither is a failed `choose`. When some named term does match, you may drop the answer or keep a correct one.
- When the cache is full, storing a new key drops the one used longest ago. Both a hit and a store are uses.
- A `limit` of 0 returns no items. Whether it is cached is not graded.

**Threads.** Some tests make a call of `judge` wait for up to a second while other threads use the service. Your service may make those threads wait. It must not deadlock once `judge` returns, and it must not serve an old answer afterwards.

**What no test can force.** A write may land in the instant after your search ends and before its answer is stored. The tests cannot stop a thread there. A design that leaves a gap there can pass and still serve an old answer, so check it by reading your code.

**What is not graded.** How long threads wait for each other, and which of two designs is faster. Measure that on your machine if you care. Panics inside `judge` are not tested.
