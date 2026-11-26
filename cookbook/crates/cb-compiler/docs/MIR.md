# MIR: the mid-level IR

`src/mir/` lowers the typed HIR to a **monomorphic, typed control-flow graph**. Everything after it (optimiser, bytecode) never sees generics, patterns, `match`, `while`, `&&`, or method sugar.

```text
HIR (typed tree) --mir::lower--> MIR (CFG, monomorphic) --opt passes--> MIR --bc::codegen--> bytecode
```

## Shape

```rust
Func   { name, inst_name, params, locals: Vec<LocalDecl{ty}>, blocks: Vec<Block>, ret, scopes, is_const }
Block  { stmts: Vec<Stmt>, term: Terminator }          // block 0 is the entry
Stmt   = Assign(local, Rvalue) | SetField(local, i, op) | SetIndex(arr, idx, val, checked)
Rvalue = Use | Unary | Binary | Agg(kind, ops) | Field | Index(arr, idx, checked) | Len | Tag | Builtin | Cast
Terminator = Goto | Branch{cond,then,else} | Switch{value,cases,default}
           | Call{callee, args, dest, target} | Return | Unreachable
Operand = Const(Int|Bool|Unit|Str|Fn(FuncId)) | Local(id)
```

* **Locals are typed and mutable** (not SSA): `let mut`, loop variables and `match` bindings are just assigned slots, so lowering stays small. Passes use dataflow (constant/copy propagation, liveness, definite initialisation) instead of phi nodes. SSA is an optional second form used by -O3 only: `opt::ssa::to_ssa` sets `Func::ssa`, blocks then carry `phis`, every local has one definition that dominates its uses, `SetField` is written `Rvalue::WithField`, and the verifier checks those rules instead of definite initialisation (see `docs/OPTIMISATIONS.md`).
* **Places.** A place is a local; stores into aggregates are `SetField(local, i, v)` (in-place, copy-on-write at run time) and stores into arrays are `SetIndex`. `p.a.b = v` lowers to `t = p.a; t.b = v; p.a = t` (value semantics); `xs[i].f = v` to `t = xs[i]; t.f = v; xs[i] = t`. Index sub-expressions are evaluated exactly once, left to right, after the right-hand side (as the interpreter does).
* **Calls are terminators** (`Call` ends a block and names its continuation), so the CFG shows every point where control leaves the function. Built-ins (`print`, `len`, `push`, ...) are rvalues because they never call back into user code.
* **Short-circuit `&&` / `||`** become branches that assign one `bool` temporary. `break`/`continue` jump to the loop's exit/header blocks; `break value` assigns the loop's result temporary first. `return` ends the block with `Return`. Code after a diverging expression is never emitted (lowering tracks "current block = none").
* **Match lowering.** The scrutinee is copied to a temporary. For enums a `Switch` on `Tag(scrutinee)` jumps to the first arm that can match each variant; each arm then tests its remaining sub-patterns (literals, nested variants) top-down and falls through to the next arm on failure. Bindings are plain `Assign`s along the success path; or-patterns try alternatives in order (they bind the same locals); guards branch to the next arm when false. Arms are tried in source order, so first-match semantics need no reasoning about overlaps; the checker already guarantees exhaustiveness (the last fall-through block is `Unreachable`).
* **Aggregates.** `Agg(Tuple|Array|Struct(def)|Variant(def, i), operands)`; fields are in declaration order. `Field(op, i)` reads a tuple/struct field or an enum payload slot; `Tag` reads the variant index.
* **Memory model.** No explicit drops. Values are reference counted in the VM (aggregates copy-on-write, arrays shared handles, strings immutable). Documented in `src/mir/mod.rs` and `docs/BYTECODE.md`.
* **Source spans.** Every `Stmt` and every `Call` terminator carries the `Span` of the HIR expression it was lowered from (index stores and loads use the span of the *place* expression, compound assignments the span of the whole assignment, exactly the spans the interpreter reports). Passes that rewrite a statement in place keep its span; the passes that create statements (inlining's argument and result moves, tail-call parameter moves) stamp them with the span of the call. `Scope::call_site` records where an inlined frame was called from.
* **Scopes.** Each statement carries a scope id. Scope 0 is the function itself; the inliner appends the callee's scopes below the call-site scope, so run-time stack traces list inlined frames exactly like the unoptimised program.

## Monomorphisation

A memoised worklist: `main` is instance `(main, [])`; lowering a body substitutes the instance's type arguments into every type, and each `Fn(def, targs)` use asks for the instance `(def, targs')`, creating (and queueing) it the first time only. Constants are instances too (`is_const`, evaluated once by the VM and cached). Output: `MirProgram.stats` (total and per-generic-function instance counts), printed as the header of `pebblec dump-mir`:

```text
// monomorphisation: 7 function instances
//   generic `id`: 4 instance(s)
//   generic `pair`: 2 instance(s)
```

Polymorphic recursion (`fn grow<T>(x: T) { grow([x]) }`) would instantiate forever, so the type depth of an instance is limited to 24 and the program to 4000 instances; exceeding either is a `LowerError` ("polymorphic recursion: ..."), tested in `tests/backend.rs`. Functions unreachable from `main` are never instantiated.

## The verifier (`mir::verify`)

Run after lowering and after every optimisation pass (`OptOptions.verify`, always on in tests; the CLI enables it for `dump-*` and in debug builds). It checks:

1. every reachable block's successors exist; the function has blocks; scopes are well formed;
2. no local has a non-monomorphic type (`Param`, `Var`, `Error`);
3. operand and result types of every statement and terminator (arithmetic on ints, `+` on strings, comparisons, aggregate shapes via `TypeEnv`, field/index/len/tag operand kinds, built-in signatures, call arity and argument types, branch on `bool`, switch on `int`, `return` type);
4. **definite initialisation**: a forward must-analysis; reading a local that is not assigned on *every* path from the entry is an error.

Errors are strings of the form `function (fn#N): bbK: message`; `tests/verify.rs` corrupts MIR in fourteen ways and checks each is rejected without a panic.

## Worked example

```pebble
fn sum(xs: [int]) -> int {
    let mut i = 0;
    let mut s = 0;
    while i < len(xs) {
        s += xs[i];
        i += 1;
    }
    s
}
```

**HIR** (condensed; `pebblec dump-hir`): `Block { Let(i = 0); Let(s = 0); While(Binary(Lt, Local i, Builtin(Len, Local xs)), Block { Assign(+=, Local s, Index(Local xs, Local i)); Assign(+=, Local i, 1) }); tail Local s }`.

**MIR at -O0** (`pebblec dump-mir`):

```text
fn sum(_0: [int]) -> int {
  bb0:                           // entry
    _1 = 0                       // i
    _2 = 0                       // s
    goto bb1
  bb1:                           // loop header: condition
    _3 = len(_0)
    _4 = _1 < _3
    branch _4 -> bb2, bb3
  bb2:                           // body
    _5 = _0[_1]
    _2 = _2 + _5
    _1 = _1 + 1
    goto bb1
  bb3:
    return _2
}
```

**Optimised MIR** (-O3, after `sum` was inlined into its single call site in `main`): the same loop with `_7 = _0[_3] !nocheck`; the bounds-check eliminator proved `0 <= i < len(xs)` from the condition `i < len(xs)` (`i` starts at 0 and only grows) and no `push`/`pop`/call sits between the length read and the index. The rest of the program is unchanged because every remaining statement can trap or has an effect.

**Bytecode** (`pebblec dump-bc --opt-level=0`, function `sum`; `-O3` fuses and checks less):

```text
  0: loadi r1, 0
  1: loadi r2, 0
  2: len r3, r0
  3: jmplt r1, r3, @5      ; fused  _4 = _1 < _3; branch _4
  4: ret r2
  5: index r3, r0, r1      ; -O3: indexu
  6: add r2, r2, r3
  7: addi r1, r1, 1
  8: jmp @2
```

Register allocation: `_0` is the parameter (r0); `_1`, `_2` live across the loop (r1, r2); `_3`, `_5` have disjoint intervals and share r3; `_4` disappeared into `jmplt`.

## Reading order

`mir/mod.rs` (data types and helpers) -> `mir/lower.rs` -> `mir/print.rs` -> `mir/verify.rs`, then `docs/OPTIMISATIONS.md`.
