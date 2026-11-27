A debugger has to print the values of a running script. The script's lists are shared and can be changed, so a list can hold another list many times. It can even hold **itself**.

```rust
pub type List = Rc<RefCell<Vec<Item>>>;
pub enum Item { Number(i64), List(List) }
```

Write `show(item, limit)`. It returns the text of the item, or `None` if that text would be longer than `limit` bytes.

- A number is written as usual, such as `-7`.
- A list is `[`, its items joined by `, `, then `]`.
- A list that is **already being written**, because the current position is inside it, is written as `[...]`. So a list that holds only itself is `[[...]]`.
- Only that case is cut short. Two lists that look the same are different lists. A list that appears twice side by side is written in full both times.

**The limit.** `Some(text)` means the whole text has at most `limit` bytes. The text can be astronomically longer than the lists: a list that holds the next list twice, 60 times over, has more than 2^60 items to write. `show` must still answer at once when the limit is small.

**Depth.** Lists may be nested ten thousand deep or more. The deep tests run `show` on a thread with a **256 KB** stack, so the nesting must not use up the call stack.

`show` only reads. When it returns, no list is changed and no `RefCell` borrow is left behind.
