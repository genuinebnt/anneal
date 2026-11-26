You are writing the part of a library that imports a stock file. Each non-empty line is `sku,quantity`. Callers need to
**match** on what went wrong, print the **whole chain** of causes, and decide whether trying again could help.

Finish the two error enums (the variants are given; `thiserror` is available, and the hand-written trait impls in the
starter are only there so it compiles), then write the functions.

- `parse_row(line)` parses one line. Split at the **first** comma; trim both parts.
  - No comma, or an empty SKU: `MissingField("sku")` for an empty SKU, `MissingField("quantity")` when there is no comma
    or nothing after it. Check the SKU first.
  - A quantity that is not a `u32`: `BadQuantity`, keeping the parse error as the **source**.
  - A quantity of `0`: `ZeroQuantity`.
- `import(path, read)` calls `read(path)` once and parses every line. Lines that are blank after trimming are skipped.
  Line numbers start at 1 and count **every** line, blank ones included. It stops at the first bad line.
  - `read` failing gives `ImportError::Read`; a bad line gives `ImportError::BadRow`.
- `chain(err)` returns the message of `err` and of every source below it, outermost first.
- `is_retryable(err)` is true only for a `Read` error whose `io::ErrorKind` is `Interrupted`, `TimedOut` or
  `WouldBlock`.

The messages are exact, and **a message never repeats its source**:

| Error | `Display` |
|---|---|
| `RowError::MissingField("sku")` | `missing sku` |
| `RowError::BadQuantity { text: "x".into(), .. }` | `quantity "x" is not a number` |
| `RowError::ZeroQuantity` | `quantity must be at least 1` |
| `ImportError::Read { path: "a.csv".into(), .. }` | `cannot read a.csv` |
| `ImportError::BadRow { line: 3, .. }` | `line 3 is invalid` |

`source()` returns the `io::Error` for `Read`, the `RowError` for `BadRow`, the `ParseIntError` for `BadQuantity`, and
nothing for the other two.
