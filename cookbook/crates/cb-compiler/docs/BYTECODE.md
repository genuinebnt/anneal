# PEBBLE bytecode and VM

The back end of `pebblec`: a **register-based** bytecode generated from optimised MIR and executed by the VM in `src/vm/mod.rs`. Code lives in `src/bc/` (`mod.rs` instruction set, `codegen.rs`, `text.rs` assembler/disassembler, `verify.rs`).

```text
MIR --codegen--> Chunk { funcs, consts, main } --bc::verify--> vm::run
```

## Why registers

A stack machine needs about one instruction per operand push; `a = b + c` is `push b; push c; add; pop a` (4 dispatches). A register machine encodes the same as `add ra, rb, rc` (1 dispatch). Dispatch (the `match` on the opcode) dominates a safe-Rust interpreter, so fewer, fatter instructions win. The cost is wider instructions and the need for register allocation, which MIR's typed locals make easy. (Lua 5 made the same switch; see "The Implementation of Lua 5.0".)

## Chunk layout

* `Chunk { funcs: Vec<BcFunc>, consts: Vec<BConst>, main }`. `consts` is one program-wide pool of strings and integers that do not fit a 32-bit immediate (`loadk`); small integers use `loadi`.
* `BcFunc { name, nregs, nparams, code, lists, switches, scopes, pc_scope, pc_span, is_const }`.
  * `code: Vec<Instr>`. `Instr` is a Rust enum (`u16` registers, `u32` jump targets), about 16 bytes; decoding is a `match`, no bit twiddling.
  * `lists`: argument and element register lists (`mkagg`, `mkarray`, `call`). Keeping them out of the instruction keeps `Instr` small and fixed-size.
  * `switches`: `(value, target)` tables for `switch`.
  * `scopes` and `pc_scope`: debug info. Scope 0 is the function; inlining adds child scopes so a trap inside inlined code still prints `at callee`, `at caller` frames (identical to the interpreter's traces).
  * `pc_span`: the source span of every instruction (`Span::default()` = unknown), copied from `mir::Stmt::span` / the `Call` terminator span by codegen. A VM trap reports `pc_span[pc]` as the `-->` location; each physical frame's call site is the `pc_span` of the caller's call instruction, and an inlined frame's call site is `Scope::call_site`. The text listing does not carry spans (an assembled chunk has an empty `pc_span` and prints no `-->`); the verifier accepts an empty `pc_span` or one entry per instruction.
  * `is_const`: a `const` initialiser, evaluated at most once and hidden from traces.
* Parameters occupy registers `0..nparams` on entry; the callee frame is `nregs` slots.

## Frames, calls, values

One shared register stack (`Vec<Value>`); a frame is the window `base .. base + nregs`, the callee window starts right after the caller's. `call d, f, [args]` copies the argument registers into the callee's first registers, pushes a `Frame { func, pc, base, dest, memo }` and continues at pc 0; `ret a` stores the value in the caller's `dest` register. Native recursion is never used, so a PEBBLE recursion limit (10 000 frames, as in the interpreter) is a clean `RuntimeError`. A second guard bounds the total register slots (16 M) so hostile bytecode cannot exhaust memory.

`Value` is a 24-byte tagged enum: `Int(i64) | Bool | Unit | Str(Rc<str>) | Agg(Rc<Agg>) | Array(Rc<RefCell<Vec<Value>>>) | Fn(u32)`. Justification: PEBBLE has no floats, so NaN-boxing (8-byte values, needs `unsafe` and a separate heap) would buy memory density but not much speed, and would cost the "no unsafe" property. Aggregates are `Rc` with copy-on-write (`Rc::make_mut` in `setfield`), which gives structs, tuples and enums value semantics; arrays are shared handles (reference semantics), exactly as in the language definition.

**Heap.** Reference counting. Cycles can only be built through arrays of recursive enums (`push(a, E::Arr(a))`); they leak until exit and cannot cause unsafety. Dropping a very long nested chain recurses natively, so the CLI runs the VM on a large-stack thread.

**Traps** (same messages and kinds as `interp`): `integer overflow in addition|subtraction|multiplication|division|negation`, `division by zero`, `remainder by zero`, `index out of bounds: the length is L but the index is I`, `pop from an empty array`, `assertion failed[: msg]`, `panic: msg`, `recursion depth limit (N) exceeded`, `step limit exceeded (possible infinite loop)`. Fuel counts executed instructions. Output is buffered and written when the program ends, also on error.

## Instruction set

Operand kinds: `r` register, `k` constant-pool index, `imm` 32-bit signed immediate, `@t` instruction index, `f` function index, `[..]` register list, `{..}` switch table.

| Mnemonic | Operands | Meaning |
|---|---|---|
| `move` | d:reg, s:reg | d = s |
| `loadk` | d:reg, k:kst | d = pool constant (string or integer that does not fit 32 bits) |
| `loadi` | d:reg, v:imm | d = 32-bit immediate |
| `loadb` | d:reg, v:flag | d = bool |
| `loadunit` | d:reg | d = () |
| `loadfn` | d:reg, f:fid | d = function value f |
| `add` | d:reg, a:reg, b:reg | d = a + b, traps on overflow |
| `sub` | d:reg, a:reg, b:reg | d = a - b, traps on overflow |
| `mul` | d:reg, a:reg, b:reg | d = a * b, traps on overflow |
| `div` | d:reg, a:reg, b:reg | d = a / b; traps on zero and MIN / -1 |
| `rem` | d:reg, a:reg, b:reg | d = a % b; same traps as div |
| `addi` | d:reg, a:reg, v:imm | d = a + imm (trap message: addition) |
| `subi` | d:reg, a:reg, v:imm | d = a - imm (trap message: subtraction) |
| `muli` | d:reg, a:reg, v:imm | d = a * imm (trap message: multiplication) |
| `neg` | d:reg, a:reg | d = -a, traps on MIN |
| `eqi` | d:reg, a:reg, b:reg | d = a == b on ints |
| `nei` | d:reg, a:reg, b:reg | d = a != b on ints |
| `lti` | d:reg, a:reg, b:reg | d = a < b |
| `lei` | d:reg, a:reg, b:reg | d = a <= b |
| `gti` | d:reg, a:reg, b:reg | d = a > b |
| `gei` | d:reg, a:reg, b:reg | d = a >= b |
| `lts` | d:reg, a:reg, b:reg | string a < b (bytewise) |
| `les` | d:reg, a:reg, b:reg | string a <= b |
| `gts` | d:reg, a:reg, b:reg | string a > b |
| `ges` | d:reg, a:reg, b:reg | string a >= b |
| `eq` | d:reg, a:reg, b:reg | structural equality on any two values of one type |
| `ne` | d:reg, a:reg, b:reg | structural inequality |
| `not` | d:reg, a:reg | d = !a |
| `cast` | d:reg, a:reg | d = a as int (bool to 0/1) |
| `concat` | d:reg, a:reg, b:reg | d = a + b on strings |
| `mkagg` | d:reg, tag:idx, list:lst | build a tuple/struct (tag 0) or enum variant (tag = variant index) from the registers of the list |
| `mkarray` | d:reg, list:lst | build an array (reference value) from the list |
| `getfield` | d:reg, a:reg, i:idx | d = field i of an aggregate |
| `setfield` | r:reg, i:idx, v:reg | copy-on-write store into the aggregate held in r |
| `tag` | d:reg, a:reg | d = variant index of an enum value |
| `index` | d:reg, a:reg, i:reg | d = a[i], bounds-checked |
| `indexu` | d:reg, a:reg, i:reg | d = a[i], bounds proved by the optimiser (a miss is an internal error, never UB) |
| `setindex` | a:reg, i:reg, v:reg | a[i] = v, bounds-checked |
| `setindexu` | a:reg, i:reg, v:reg | a[i] = v, proved in bounds |
| `len` | d:reg, a:reg | d = length of array or byte length of string |
| `push` | a:reg, v:reg | append v to array a |
| `pop` | d:reg, a:reg | d = remove last element; traps when empty |
| `chars` | d:reg, a:reg | d = array of one-scalar strings |
| `tostr` | d:reg, a:reg | d = display of int/bool/str/() |
| `print` | a:reg | write a without newline |
| `println` | a:reg | write a and a newline |
| `println0` | - | write a newline |
| `assert` | c:reg | trap if c is false |
| `assertmsg` | c:reg, m:reg | trap with message m if c is false |
| `panic` | m:reg | trap with message m |
| `jmp` | t:tgt | pc = t |
| `jmpif` | c:reg, t:tgt | if c then pc = t |
| `jmpifnot` | c:reg, t:tgt | if !c then pc = t |
| `jmplt` | a:reg, b:reg, t:tgt | if a < b then pc = t (ints) |
| `jmpge` | a:reg, b:reg, t:tgt | if a >= b then pc = t |
| `jmplti` | a:reg, v:imm, t:tgt | if a < imm |
| `jmpgei` | a:reg, v:imm, t:tgt | if a >= imm |
| `jmpgti` | a:reg, v:imm, t:tgt | if a > imm |
| `jmplei` | a:reg, v:imm, t:tgt | if a <= imm |
| `switch` | r:reg, s:swt | pc = target of the int in r (table), else default |
| `call` | d:reg, f:fid, args:lst | d = call function f with the registers of the list as arguments |
| `callind` | d:reg, fr:reg, args:lst | d = call the function value in fr |
| `ret` | a:reg | return a |
| `unreachable` | - | trap (internal error) |

Fused compare-and-branch instructions (`jmplt`, `jmpge`, the `...i` immediate forms) replace the pair `t = a < b; jmpif t` when the temporary has no other use, saving a dispatch per loop iteration. Immediate arithmetic keeps its own mnemonic (`addi`/`subi`/`muli`) so the trap message still says "addition" / "subtraction" / "multiplication".

## Register allocation

`codegen::allocate`: MIR locals get one *interval* each: the hull of every position where the local is defined, used, or live across a block boundary (from the liveness analysis in `opt/live.rs`). A linear scan walks intervals in start order, frees a register when its owner's interval has strictly ended, and takes the lowest free register otherwise. Parameters are pinned to registers `0..n`. Constants an instruction needs in a register are loaded into scratch slots above the allocated ones (`nregs = allocated + max scratch`). Hull intervals are conservative (a loop-carried value occupies its register for the whole loop), never unsound.

## Text format (round trips)

`pebblec dump-bc FILE` prints it; `bc::text::assemble` reads it back (`assemble(disassemble(c))` disassembles to the identical text; a test checks every run test and example at -O0 and -O3):

```text
main f0
const k0 = str "hi"
func f1 "sum" regs=5 params=1 const=false
  scope 0 "sum" -
  0: loadi r1, 0                  ; s0
  1: loadi r2, 0                  ; s0
  2: len r3, r0                   ; s0
  3: jmplt r1, r3, @5             ; s0
  4: ret r2                       ; s0
  5: index r3, r0, r1             ; s0
  6: add r2, r2, r3               ; s0
  7: addi r1, r1, 1               ; s0
  8: jmp @2                       ; s0
end
```

`; sN` is the scope of the instruction (debug info for traces).

## The bytecode verifier

`bc::verify::verify` checks, for every function: register operands `< nregs` (also inside lists), constant/function/list/switch indices exist, jump and switch targets are inside the function, `nparams <= nregs`, `pc_scope` matches `code`, scope parents are valid, direct calls pass exactly the callee's parameter count, `const` initialisers are not used as values, and the last instruction cannot fall off the end. A verified chunk cannot index out of range in the VM. *Types* are checked dynamically: an `add` on a string yields an `Internal` runtime error, not a panic. `tests/verify.rs` mutates real listings and generates random bytecode (thousands of cases): the verifier and VM never panic and every run ends within the fuel limit.

## Unsafe

None. `indexu`/`setindexu` ("bounds proved") still use `Vec::get`; the optimiser's proof removes the *language-level* check and error construction, and a wrong proof surfaces as an internal error instead of undefined behaviour. Measured benefit of going `unsafe` here was judged too small to justify it.
