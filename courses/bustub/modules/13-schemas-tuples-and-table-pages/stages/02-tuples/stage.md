A **tuple** is a row as bytes: that is what goes into a page, so a row of values must be encoded under the schema and read back. The values already know how to serialise themselves (module 3a); the tuple's job is the *arrangement*: fixed columns at their schema offsets, and each text value placed in a variable part with its slot pointing to it. NULLs need no extra space: a NULL is a reserved pattern in its value's own bytes.

> [!CHECK] A schema has two `VARCHAR` columns and a row has both of them NULL. How many bytes does the tuple need, and what do the two 4-byte slots hold? Could the reader of column 2 tell NULL from an empty string, and from a string that happens to be at offset 0?
> ||The fixed part is two 4-byte slots, and each NULL string takes a 4-byte marker (`u32::MAX`) in the variable part, so the tuple is 16 bytes: each slot holds the offset of its marker. The reader follows the slot to the stored value, reads the length, and finds `u32::MAX` (NULL), or `1` and a zero byte (the empty string), or `n + 1` and `n` bytes (text). NULL, empty and non-empty are distinct *at the value's own location*, which is why a slot value of "offset 8" is never confused with a flag.||
>
> - Where does the second text start?
> - What decides the size of the tuple?
> - What does reading a NULL integer look like in the bytes?

## The task

`Tuple::new(values, schema)` builds the tuple holding one value per column (panics if there is not one value of the right type per column); `get_value(schema, i)` reads column `i` (an inlined column where the schema says it is; a text column through the offset its slot holds); `is_null`; `key_from_tuple(schema, key_schema, key_attrs)` (an index key: the chosen columns' values arranged under the key schema); `to_string(schema)` (`(1, hello, <NULL>)`: values separated by `, `, a NULL as `<NULL>`); `serialize_to(storage)` (a 4-byte little-endian length, then the bytes) and `deserialize_from(storage)`; the constructors `from_bytes(rid, data)` and `empty`, and `get_length`.

The tuple's **size**: no more than the schema's fixed part plus each text value's stored size, and no less than the fixed part. The exact arrangement of the variable part is yours (BusTub's is in the notes and works); the tests only read values back.

Tests: a property over random schemas (up to eight columns of every type, varchar lengths 1 to 39) and random rows (NULLs, multi-byte characters, empty strings): every value comes back with its type, `is_null` is right, the length is within bounds, a key tuple holds the chosen columns, a serialised tuple has its length prefix and comes back byte for byte; plus the panics, the printed form and `from_bytes`.

## Your freedom

How the variable part is arranged (BusTub's: each text after the fixed part in column order; you may also pack them differently so long as `get_value` finds them), how `Tuple::new` sizes the buffer, and how you share code between building and reading.

## The Rust toolbox

**Size first, then fill.** `let mut data = vec![0u8; size];` then `value.serialize_to(&mut data[offset..])` for each: the value writes at the start of the slice it is given. Computing the size in one pass and writing in a second is simpler than growing a `Vec`.

**`copy_from_slice` for a slot.** `data[at..at + 4].copy_from_slice(&(text_at as u32).to_le_bytes())`; the reverse is `u32::from_le_bytes(data[at..at + 4].try_into().unwrap())`.

**Iterate with an index and a value.** `values.iter().enumerate()` and `schema.column(i as u32)`: the schema's accessors take `u32`; cast at the boundary.

**Asserting the contract.** `assert_eq!(values.len(), schema.column_count() as usize, "a tuple needs one value per column")` and check each value's `type_id()` against its column's.

**`key_from_tuple` by composition.** Read the chosen values with `get_value`, then `Tuple::new(&values, key_schema)`: no new byte logic.

**`String::from_utf8_lossy` is in module 3a.** Text round-trips because `Value::deserialize_from` does.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `vec![0; n]`, slicing with `[a..b]`, `copy_from_slice`, `try_into`.
- [F7 I/O & serialization](/t/f7-io-serialization): fixed and variable parts, length prefixes.
- [L2 Borrowing](/t/l2-borrowing): writing into disjoint parts of one buffer.
- L7 Enums & pattern matching: Patterns in depth: matching on column types.
- [S2 Strings & text](/t/s2-strings-text): Understand: strings stored in a page.

## Tests

- Every value of a random row comes back from its tuple with its type.
- A tuple is the fixed part plus its text, within bounds.
- A key tuple holds the chosen columns.
- A serialised tuple has its length prefix and comes back byte for byte.
- Wrong numbers or types of values panic; printing; `from_bytes`.

## Hints

### Do the fixed schema first

A schema of only fixed columns needs no variable part: write `Tuple::new` for that and the round-trip property passes for it. Then add one text column, then two.

### Offsets are from the start of the tuple

The slot holds the offset from byte 0 of the tuple, not from the end of the fixed part. Print the bytes of a small tuple and check by eye.

### A NULL text

It occupies the 4-byte marker in the variable part like any text. Do not skip it: the slot must point at something.

## Performance

A tuple is built once per insert and read once per scan: building is two passes over the values; `get_value` is a slot read and a decode. A scan that reads one column of a wide tuple should not decode the others, which is why `get_value` goes straight to the column.

**Measure it.** Build and read one million 5-column tuples and see how much time goes to allocation (`vec![0; n]`): predict, then measure; the next experiment shows how to avoid it.

## Experiment

Optional. Predict first, then run.

1. **A reused buffer.** Make `Tuple::new_into(buf: &mut Vec<u8>, ..)` write into a buffer the caller keeps. How much faster is a loop of a million rows?
2. **Pack the text first.** Put text values before the fixed part. What changes in `get_value`, and does the schema's offsets rule survive?

## Other designs

- **Fixed part then variable part (ours, BusTub's).**
- **A slot directory per tuple** with offset and length for every column: random access to every column, more bytes.
- **Null bitmap + packed values** (PostgreSQL): no reserved patterns, and every number usable.
- **Prefix-compressed or dictionary-encoded text:** columnar engines.

## In BusTub

`Column`, `Schema` and `Tuple` are classes in `src/catalog` and `src/storage/table`; `TablePage` is the slotted page in `src/storage/page/table_page.{h,cpp}`. BusTub's `TableHeap` (module 3c) links `TablePage`s into a table.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `Tuple(std::vector<Value> values, const Schema *schema)` | `Tuple::new(&[Value], &Schema)` |
| `*reinterpret_cast<uint32_t *>(data_ + offset)` | `u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())` |
| `std::memcpy(dest, src, n)` | `dest[..n].copy_from_slice(&src[..n])` |
| `Tuple::GetValue(const Schema *, uint32_t)` | `get_value(&self, &Schema, u32) -> Value` |

**Port rule:** pointer arithmetic into a byte buffer becomes slicing with a range, which is bounds-checked.

## Learn more

- [`<[T]>::copy_from_slice`](https://doc.rust-lang.org/std/primitive.slice.html#method.copy_from_slice) · PostgreSQL's [heap tuple format](https://www.postgresql.org/docs/current/storage-page-layout.html#STORAGE-TUPLE-LAYOUT)
