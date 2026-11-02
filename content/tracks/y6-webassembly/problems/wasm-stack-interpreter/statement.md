Write the heart of a WebAssembly engine for the `i32` subset: an interpreter over **raw instruction bytes**, the way the
spec describes it. A `Func` has `params`, `results`, extra zero-initialised `locals`, and `code` ending in `end` (`0x0B`).
`run(funcs, entry, args, fuel)` calls function `entry` and returns its results, or a `Trap`.

Instructions (opcode, immediate): `unreachable` 00, `nop` 01, `block bt` 02, `loop bt` 03, `if bt` 04, `else` 05, `end` 0B,
`br l` 0C, `br_if l` 0D, `return` 0F, `call f` 10, `drop` 1A, `select` 1B (pops `c`, then `b`, then `a`; pushes `a` if
`c != 0`, else `b`), `local.get/set/tee i` 20/21/22, `i32.const v` 41 (**signed** LEB128), `eqz` 45, `eq ne lt_s lt_u gt_s
gt_u le_s le_u ge_s ge_u` 46..=4F, `add sub mul` 6A 6B 6C, `div_s` 6D, `rem_s` 6F, `and or xor` 71 72 73, `shl shr_s shr_u`
74 75 76. A block type `bt` is `0x40` (no result) or `0x7F` (one `i32`). Helpers to read the LEB immediates are provided.

Rules that are easy to get subtly wrong: arithmetic **wraps**; shift counts are taken **mod 32**; `div_s` and `rem_s` trap with
`DivideByZero` on a zero divisor, `div_s` also traps `IntegerOverflow` for `i32::MIN / -1`, while `i32::MIN % -1` is `0`.
`br l` to a `block`/`if` jumps past its `end`; to a `loop` it jumps back to the loop's start (loops take no values); either way
the stack is cut back to the height at block entry plus the values the block yields. The function body is itself the outermost
block, so `br` to it and `return` both finish the function. `if` with a false condition and no `else` skips to `end`.

Every executed instruction costs one unit of fuel (including `end` and `else`); with none left the next instruction traps
`OutOfFuel`. More than 100 active calls (the entry counts as one) is `StackOverflow`. All functions are scanned before running:
an unknown opcode, an unbalanced `block`/`end`, bytes after the final `end`, a bad local or label index, a stack underflow or
a wrong argument count is `Trap::Malformed`; calling a function that does not exist is `BadFunction(index)`.
