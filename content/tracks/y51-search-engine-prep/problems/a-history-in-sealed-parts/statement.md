A chat server keeps every message for ever. Messages arrive in batches. Readers want a **snapshot**: a view that stays as it is while new messages keep arriving.

Messages are numbered 0, 1, 2 and so on, in the order they arrive.

**Parts.** The history is kept as a row of parts. Each part holds a run of messages with neighbouring numbers. `part_sizes()` returns how many messages each part holds, oldest part first.

- `append(batch)` adds the batch as one new part at the end. An empty batch changes nothing.
- Then look at the newest part and the one before it. **While** the earlier one holds **fewer than twice** as many messages as the newest, join the two into one.

For example, batches of one message each give the sizes `[1]`, `[2]`, `[2, 1]`, `[4]`, `[4, 1]`.

**Questions.** Joining parts must never change any answer below.

- `len()`: how many messages there are.
- `message(n)`: the author and text of message `n`, or `None`.
- `by_author(author)`: the numbers of that author's messages, in rising order.
- `mean_length()`: the mean length of **all** messages, counted in characters, or `None` when there are none.
- `snapshot()`: a `History` that answers as this one does now. Later appends to either one do not change the other.

**A snapshot must not copy messages.** A test counts the bytes that `snapshot()` asks the allocator for. With 20 000 messages in the history, one call may ask for at most **4 096 bytes**.

Nothing else about speed or memory is graded. To see what appending costs, count how often each message is moved over 100 000 batches of one.
