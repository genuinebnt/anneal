A photo contest shows a **shortlist**: up to `n` captions of entries that qualify. Every question about an entry is a call to an outside service, which costs money and can refuse. A refusal for an entry that the shortlist never needed must not happen at all.

Write `shortlist(entries, n, ranked, judge)`. The service is the `Judge` you are given:

- `qualifies(entry)`: may this entry be shown?
- `score(entry)`: its score. **Lower is better.**
- `caption(entry)`: the text to show.

**Make exactly the calls described here, in this order.** The tests record every call.

- If `n` is 0, make no call. The result is empty.
- **Not ranked:** take the entries in the order given. Ask `qualifies`. If the answer is yes, ask for the `caption` at once. Stop for good as soon as you have `n` captions. `score` is never called.
- **Ranked:** the shortlist is the `n` qualifying entries with the lowest scores, best first. Entries with equal scores keep the order they were given in. First go through **all** entries in order: ask `qualifies`, and for a yes ask `score` at once. Only after that, ask for the captions of the shortlisted entries, in shortlist order.

If any call returns an error, return that error at once and make no further call.

Entries are just numbers, and the same number may appear twice. There can be 200 000 of them.
