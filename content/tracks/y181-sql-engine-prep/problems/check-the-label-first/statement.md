A warehouse prints parcel labels from a table of orders. Staff write a **formula** for each line of the label, such as "if there are more than one items, join the item count and the word *parcels*". A formula with a mistake must be refused **before** printing starts, not at row 5 000 of a run.

Write `check(formula, fields)`. It gets the formula as a tree, and the table's fields as `(name, kind)` pairs. It gets no rows. Field names are unique, and match exactly.

**Kinds.** A value is a `Number`, `Words` or `YesNo`. `Blank` stands for an empty value, which fits **any** kind. So the kind of a formula is an `Option<Kind>`: `None` means "fits anything".

| Formula | Its operands must be | Its kind |
|---|---|---|
| `Number`, `Words`, `YesNo` | | its own |
| `Blank` | | `None` |
| `Field(name)` | | the field's |
| `Add(a, b)` | numbers | `Number` |
| `Join(parts)` | each one words; there may be none | `Words` |
| `Same(a, b)` | two of one kind | `YesNo` |
| `Pick(test, a, b)` | `test` yes/no; `a` and `b` of one kind | the kind of `a`, or of `b` if `a` fits anything |

An operand that fits anything is accepted everywhere.

**The result** is the same tree as a `Checked` value, with every field name replaced by the field's index in `fields`, together with the formula's kind.

**The first mistake.** Take the operands of a node from left to right. For each one, first check everything inside it, then check its kind against what this node needs. Only then go on to the next operand. Return the first mistake found:

- `NoSuchField(name)`;
- `WrongKind { needed, found }`, where `found` is the operand's kind. When two operands must be of one kind, the earlier one with a known kind sets what the later one `needed`.

A formula is at most 100 nodes deep.
