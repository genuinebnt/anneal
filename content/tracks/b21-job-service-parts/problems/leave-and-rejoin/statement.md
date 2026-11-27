A deploy tool prints progress lines. People open the page late, lose their connection, and come back. Each of them wants every line they have not seen, in order, followed by new lines as they appear.

Write `Feed`, a growing list of text entries that can be followed. Clones of a `Feed` are handles to the **same** feed.

- `publish(text)` appends an entry and returns its id. Ids are 1, 2, 3 and so on. After the feed is closed it returns `Err(Closed)` and adds nothing.
- `close()` ends the feed. Closing twice is fine.
- `since(after)` returns the entries with an id greater than `after`, oldest first. It never waits.
- `follow(after)` returns a stream of those entries, followed by every later one.

**What a follower sees:**

- each entry with an id greater than `after`, **exactly once**, in id order, whether it was published before or after the call;
- this holds however far behind the follower falls, and while other threads publish;
- when `after` is beyond the newest id, nothing arrives until an id above `after` exists;
- once the feed is closed and the follower has had every entry, the stream **ends**;
- dropping **all** `Feed` handles also ends the feed. A follower still gets the entries it has not had, and then its stream ends.

**While it waits,** a follower must not spin. Its task is woken when something happens, not before. Followers never slow `publish` down, and a follower that is never polled affects nobody.

`publish`, `close`, `since` and `follow` are ordinary (not `async`) functions. They may be called from any thread. The stream is `Send`, does not borrow the `Feed`, and can be polled without a Tokio runtime. The `futures` and `tokio` crates are available.
