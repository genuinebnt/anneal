# Optimisations

`pebblec --opt-level=N` selects a pipeline (`opt::pipeline`); `--dump-after=PASS` prints the MIR after every run of a pass (`all` for every pass). After **every** pass the MIR verifier runs in tests (`OptOptions.verify`), so a buggy pass is named in the error. `tests/backend.rs` also runs *each pass alone* over all run tests and all examples and compares output and errors with the interpreter.

| Level | Pipeline |
|---|---|
| 0 | none (lowering already removes unreachable blocks) |
| 1 | simplify-cfg, const-prop, copy-prop, dce, simplify-cfg |
| 2 | 1 + cse, strength, tco, bce (with clean-ups in between) |
| 3 | inline first, then level 2, then `ssa, sccp, gvn, licm, out-of-ssa`, then a last const-prop/copy-prop/cse/dce round |

**Trap-preserving rule.** PEBBLE traps on overflow, division by zero, bounds, empty `pop`, assertions. A pass may *remove* a computation only if it cannot trap (`Rvalue::may_trap_or_effect`); `x = a + b` with `x` unused is kept because the overflow is observable. Folding never evaluates an operation that would trap: `1 / 0` and `MAX + 1` stay as instructions and trap at run time with the original message.

## simplify-cfg (`opt/cfg.rs`)

*What:* fold branches on constants and `branch c -> b, b`, thread jumps through empty `goto` blocks, merge `a: goto b` when `b` has one predecessor, drop unreachable blocks, renumber. *Why:* the other passes (and lowering) leave a lot of glue blocks; fewer blocks means fewer jumps and better register allocation. *Example:* `if 1 < 2 { A } else { B }` becomes the body of `A` in the same block (`tests/mir/dead_branch.mir`). *Pitfall:* `Call` terminators cannot be merged into their continuation (a call ends a block); block 0 must never be merged away.

## const-prop (`opt/prop.rs`)

*What:* a forward dataflow over per-local lattice `Top > Const(c) > Bot` replaces uses by constants, folds `Binary/Unary/Cast/Len(str)` on constants, and turns constant `branch`/`switch` into `goto`; repeated until stable (folding a branch exposes more constants). *Why:* the basis of dead-branch removal; propagating across blocks needs dataflow because locals are mutable. *Example:* `let x = 2 + 3; let y = if x > 4 { x * 2 } else { 0 }; y + x` folds to `return 15` (`const_fold_across_blocks.mir`). *Pitfalls:* never fold what would trap (`checked_*` returns `None`); string concatenation folding is capped at 4 KiB; at the loop header the meet of the entry and back-edge values must be computed to a fixed point or loop variables would be treated as constants.

## copy-prop (same engine, `Copy(y)` lattice value)

*What:* after `x = y`, uses of `x` read `y` while neither is reassigned. A copy is killed when `x` or `y` is assigned (including `SetField`, which redefines the aggregate). *Why:* lowering creates many temporaries (`_5 = _1; ... use _5`); this makes them dead for `dce`. *Pitfall:* aggregates have value semantics; `x = y; y.f = 1` must not make `x` see the update, which is why `SetField(y, ..)` kills `Copy(y)` entries. Arrays alias anyway, so copying an array handle is always safe.

## cse (`opt/cse.rs`): local value numbering

*What:* within a block, rvalues with the same operator and the same operand *value numbers* reuse the first result (commutative operators are canonicalised; `+` on strings is not commutative). Array reads (`Index`, `Len`) are forgotten at every `SetIndex`/`push`/`pop`. *Why:* repeated `a * b`, `xs[i]`, `len(xs)`, field reads. *Example:* `cse_local.mir`: the second `a * b` becomes a copy. *Pitfalls:* aggregate construction and `chars` allocate fresh identity and are never merged; replacing a *trapping* op by a copy is sound because the first occurrence already ran. The cross-block version is `gvn` below.

## strength (`opt/strength.rs`)

*What:* `x+0`, `x-0`, `x*1`, `x/1` to `x`; `x*0`, `x%1`, `x-x` to `0`; `x*2` to `x+x`; `x==x` to `true` and `x<x` to `false` for locals. *Why:* cheap algebra after inlining and propagation. *Pitfalls:* `x * 0` is `0` only because multiplication by zero cannot overflow; `x / -1` is *not* simplified (traps on `MIN`); `x * 2` and `x + x` overflow on exactly the same inputs, so the trap is preserved.

## dce (`opt/dce.rs`)

*What:* backward liveness; an assignment to a dead local is removed when its rvalue is pure (no trap, no effect); `SetField` on a dead aggregate is removed; then unused locals are renumbered away. Iterated to a fixed point (removing a use can kill another definition). *Why:* cleans up after propagation, removes dead stores. *Example:* in `dead_store.mir` the unused `[a, a]` array and copy vanish while `let trap = a + 1` stays.

## inline (`opt/inline.rs`)

*What:* replace a direct call by a copy of the callee's blocks (locals and blocks renumbered, parameters assigned from the arguments, `Return` becomes `dest = value; goto continuation`). *Cost model:* inline if the callee is not in a call cycle (so recursion is never unrolled), is not a `const` initialiser, and either has at most 24 statements or has exactly one call site, is not used as a function value and has at most 400; callers stop growing past 3000. Up to three rounds; functions that became unreferenced are deleted. *Why:* removes call overhead and enables constant propagation across the call (`double(21)` folds to `42`). *Trace fidelity:* the callee's scope tree is attached below the call site's scope, so a panic inside inlined code still reports `leaf`, `mid`, `top`, `main` (`trace_names_inlined_frames`). *Pitfall:* only the *physical* depth limit changes: inlining non-recursive helpers can only lower the number of real frames.

## tco (`opt/loops.rs`): self tail calls to loops

*What:* a block ending in `call self(args)` whose continuation merely copies the result to the return value becomes: assign the parameters (through temporaries when an argument reads another parameter, the classic swap problem), `goto` the old entry (moved to a fresh header block). *Why:* constant stack space for accumulator-style recursion. *Pitfall (documented deviation):* stack depth. At -O0 `count(50000, 0)` hits the 10 000-frame limit like the interpreter; at -O2 it runs in a loop (`deep_tail_recursion_is_the_documented_deviation`). Traps inside a converted loop show one frame instead of one per iteration.

## bce (`opt/loops.rs`): bounds-check elimination

*What:* a forward must-analysis tracks: `nonneg(i)` (assigned constants `>= 0`, `i + c` with `c >= 0`, lengths; overflow traps so no wrap-around), `n = len(a)`, `t = (i < n)`, and facts `0 <= i < len(a)` established on the *true edge* of `branch t`. A fact is killed when `i` or `a` is reassigned, and all array facts are killed by `push`, `pop` and any call (callees can mutate aliased arrays). `a[i]` with a fact in scope becomes `a[i] !nocheck` (`indexu` in bytecode); likewise `SetIndex`. *Why:* the canonical counted loop `while i < len(xs) { ... xs[i] ... i += 1 }`. *Examples:* `bce_counted_loop.mir` (check removed), `bce_length_hoisted.mir` (`let n = len(xs)` before the loop), `bce_not_proved.mir` (`pop` in the loop keeps the check; that program then traps). *Pitfall:* optimism: unvisited loop back-edges start as "everything holds" and the meet converges; starting pessimistic would never prove anything in a loop.

## ssa and out-of-ssa (`opt/ssa.rs`, `opt/dom.rs`)

*What:* `ssa` computes dominators (Cooper-Harvey-Kennedy) and dominance frontiers, places pruned phis (only where the local is live) and renames along the dominator tree. `SetField` becomes the functional `with_field`. The SSA verifier checks single definition, dominance of every use and one phi argument per predecessor. `sccp`, `gvn` and `licm` run only on this form (`opt::SSA_ONLY`; the pass manager refuses the wrong form). `out-of-ssa` splits critical edges and call edges, turns the phis of each edge into a parallel copy, orders it (a temporary only for a cycle), restores `SetField` and **coalesces** copies whose two locals do not interfere. *Pitfalls:* the **lost-copy problem** (a copy at the end of a block with two successors destroys a value the other path needs) and the **swap problem** (`a, b = b, a` run one copy at a time). Without coalescing the round trip left enough extra moves that `sieve_1m` at -O3 executed more VM instructions than at -O0. Byte-code generation needs the register form (`driver::compile_passes` leaves SSA on a copy when a pass list ends in SSA). Tests: `tests/ssa.rs` (random CFGs, including irreducible ones, against a brute-force dominator oracle and a small MIR evaluator; parallel copies; verifier negatives), goldens `ssa_phi_placement`, `out_of_ssa_swap`.

## sccp (`opt/sccp.rs`): sparse conditional constant propagation

*What:* Wegman-Zadeck: one worklist of executable CFG edges and one of changed SSA values. A phi meets only the arguments of executable edges, so a constant that depends on a branch which depends on the constant is still found. *Example:* `sccp_loop_constant.mir` (`x` stays 1 around a loop; `const-prop` cannot see it). *Pitfall:* folding never evaluates a trapping operation.

## gvn (`opt/gvn.rs`): global value numbering

*What:* a dominator-tree walk with a scoped table: an expression computed in a block is reused in every block it dominates. Array reads are keyed by a memory epoch that advances at each store, `push`, `pop`, call and at joins and loop headers. *Example:* `gvn_across_blocks.mir`: four multiplications become one.

## licm (`opt/licm.rs`): loop-invariant code motion

*What:* natural loops from the dominator tree, innermost first; invariant assignments move to a preheader (created if needed). *Safety:* operations that cannot trap may move from anywhere in the loop; operations that can trap move only from the trap- and effect-free prefix of the loop header, which runs whenever the loop is entered; they are never hoisted out of the body, which may run zero times. `len(array)` and checked loads move only if the loop has no store or call. *Goldens:* `licm_hoist`, `licm_no_speculation`. No loop rotation is done, so body-only trapping invariants stay where they are.

Executed VM instructions, the -O3 pipeline with and without `licm` (`cargo test --release -p cb-compiler --test ssa licm_instruction -- --nocapture`; run it for current numbers): the gain is large on `examples/bench/loop_invariant.peb` (written to show it) and zero or tiny on most other benchmarks.

## Not implemented

Loop rotation, register-immediate comparison for `==`, escape analysis for avoiding copy-on-write clones. Each is a good next exercise; the verifier and the differential tests make them safe to try.
