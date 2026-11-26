# 05. Compilers and languages

Prerequisite track: `paths/tracks/08_compilers_languages.md`. Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet. Companion book: *Crafting Interpreters* (free online).

Test strategy: **golden files** (`tests/cases/NAME.src` + `NAME.out`), differential tests against a reference (Python or the host `eval`), and **fuzzing for no-panic** (random byte mutations of valid programs must yield `Err`, never a panic or hang).

---

## 5.1 `calc`: a calculator with a Pratt parser and error recovery  `S`

**Goal.** REPL and file mode for expressions with `+ - * / ^ %`, unary minus, parentheses, functions (`sqrt(2)`, `max(1,2,3)`), variables (`x = 3`), and error messages with a caret under the offending token.
**Learn.** Binding powers, right-associativity of `^`, token spans, error recovery to the next `;`.
**Read before.** `text_and_parsing/pratt_parser`, `text_and_parsing/expression_parsing_three_ways`. **During.** `errors/diagnostics_spans`, `text_and_parsing/zero_copy_lexer`.
**Milestones.**
1. Lexer producing tokens with `(start, end)` spans; numbers with `_`, exponents, hex.
2. Pratt parser with precedence table: `^` (right, 70) > unary (60) > `* / %` (50) > `+ -` (40).
3. Evaluator with `Result<f64, EvalError>` (division by zero, unknown name, arity).
4. Diagnostics: `1 + * 2` prints the source line and `^` under `*` with "expected expression".
5. Recovery: a file with 3 bad lines reports all 3 errors, evaluates the good lines.
**Acceptance.** 300 generated random valid expressions agree with Python `eval` (relative error under 1e-12); 1,000 mutated inputs never panic; `2^3^2 == 512`, `-2^2 == -4`.
**Data.** Expression generator with depth <= 6 emitting both the string and its expected value computed in your oracle.
**Write-up.** How does your parser decide associativity? Where do you stop on error and why?
**Stretch.** Pretty-printer that round-trips (parse -> print -> parse gives the same tree).
**Resembles.** [rust-analyzer expression parsing](https://github.com/rust-lang/rust-analyzer), [Rhai](https://github.com/rhaiscript/rhai).

## 5.2 `tomlite`: a config-language parser with spans and good diagnostics  `S`

**Goal.** Parse a TOML-like subset (tables, arrays, inline tables, strings with escapes, ints, floats, bools, dates) into a tree with source spans, and produce multi-error, compiler-quality diagnostics.
**Learn.** Parser combinators or recursive descent, span arithmetic, error recovery, UTF-8 columns, duplicate-key detection.
**Read before.** `text_and_parsing/parser_combinators`, `text_and_parsing/json_parser`, `errors/diagnostics_spans`. **During.** `text_and_parsing/edge_dsl_parser`, `encoding_compression/serde_custom`.
**Milestones.**
1. Scalars and arrays with spans; string escapes `\n \t é`.
2. `[table]` and `[[array.of.tables]]`, dotted keys.
3. Duplicate key and type-conflict errors point at both occurrences.
4. Multiple errors per file (recover at the next newline).
5. Round trip: `format(parse(x))` preserves comments (store trivia).
**Acceptance.** All valid cases in the official toml-test suite for your subset pass (clone `toml-lang/toml-test`); 5,000 random mutations: no panic, no hang over 100 ms; diagnostics snapshots for 15 broken files.
**Data.** toml-test valid/invalid directories plus your own 15 broken files.
**Write-up.** Why store trivia (comments, whitespace)? How do you compute a column for a line containing multi-byte characters?
**Stretch.** `serde::Deserializer` over your tree.
**Resembles.** [toml / toml_edit](https://github.com/toml-rs/toml), [Biome](https://github.com/biomejs/biome) parser diagnostics.

## 5.3 `loxrs`: a tree-walking interpreter  `M`

**Goal.** Implement the Lox language (or your own small dynamic language): variables, closures, classes with inheritance, control flow, native functions, and a resolver pass for static scoping.
**Learn.** AST design with `Box`/arena, environments with `Rc<RefCell<_>>`, closures capturing by reference, resolver pass, runtime errors with line numbers.
**Read before.** `capstones/toy_language_interpreter`, `language/closures_hof`, `language/smart_pointers`. **During.** `memory/arena_ids`, `language/pattern_matching`, `algorithms/index_graph`.
**Milestones.**
1. Expressions, `print`, global and local variables, blocks.
2. `if/while/for`, functions, `return`, native `clock()`.
3. Resolver: binds each variable use to a scope depth; reports "can't read local variable in its own initializer".
4. Classes: methods, `this`, `init`, single inheritance with `super`.
5. Runtime errors with stack trace (function names + lines).
**Acceptance.** The Lox test suite from `munificent/craftinginterpreters` (`test/` directory) passes at least 95% of the chapter-appropriate cases; `fib(30)` completes (record the time); a deep recursion of 10,000 returns a clean "stack overflow" error instead of crashing.
**Data.** Craftinginterpreters tests; your own 10 closure-capture puzzles (counter factories, loop capture).
**Write-up.** How do closures keep variables alive in your representation? What is the cost of `Rc<RefCell>` environments vs resolved slots?
**Stretch.** Mark-and-sweep GC instead of `Rc` (see `memory/garbage_collectors`), which also fixes cycles.
**Resembles.** [Rhai](https://github.com/rhaiscript/rhai), CPython's early AST interpreter.

## 5.4 `loxvm`: a bytecode compiler and stack VM  `M`

**Goal.** Compile your 5.3 AST (or source directly) to a compact bytecode chunk and run it on a stack VM with a disassembler, constant pool, locals as stack slots, upvalues, and a `--trace` flag.
**Learn.** Instruction design, stack discipline, jump patching, closures with upvalues, dispatch loop performance, interning strings.
**Read before.** `text_and_parsing/bytecode_vm`, `compilers_runtimes/wasm_binary_interpreter`. **During.** `perf/branches_vectorization`, `memory/bump_arena_allocator`, `perf/small_string_inline`.
**Milestones.**
1. `Chunk` with `Vec<u8>` code, constants, line table; disassembler `--dump`.
2. Expression compiler: constants, arithmetic, comparison, `print`.
3. Locals as slots (scope depth tracking), `if/while` with jump patching (`JUMP_IF_FALSE` + backpatch).
4. Functions and call frames; native functions; closures via upvalues (open upvalues closed on scope exit).
5. Differential mode: run every test through 5.3 and 5.4; outputs identical.
**Acceptance.** All 5.3 tests pass on the VM; `fib(32)` at least 5x faster than the tree-walker (report actual ratio); `--trace` output for `1+2*3` matches a hand-written expected stack trace (golden).
**Data.** The same suite; microbenchmarks: loops 10M iterations, string concat 100k, method calls 5M.
**Write-up.** Why do locals become stack slots, and how does that make closures harder? What would a register VM change?
**Stretch.** NaN-boxing values (see `unsafe_ffi/repr_layout_ffi`), computed-goto-like dispatch experiments, peephole optimizer.
**Resembles.** [Lua](https://github.com/lua/lua) (the model for upvalues and the register/stack VM design), [Wasmtime](https://github.com/bytecodealliance/wasmtime), [Rhai](https://github.com/rhaiscript/rhai).

## 5.5 `hmlite`: a type checker with Hindley-Milner inference  `M`

**Goal.** A mini-ML (lambda, let, if, ints/bools, lists, tuples) with type inference via unification, let-polymorphism, and readable type errors ("expected int, found bool at 3:14").
**Learn.** Type variables, substitution, occurs check, generalization and instantiation, unification-based error messages, union-find for efficient unification.
**Read before.** `compilers_runtimes/hindley_milner_inference`, `algorithms/union_find`. **During.** `language/generics_bounds`, `database/cascades_memo_optimizer` (memo/e-class ideas).
**Milestones.**
1. Types `Int | Bool | Fun(a,b) | Var(n)`; unify with occurs check.
2. Infer lambda/application/if.
3. `let` generalization: `let id = \x. x in (id 1, id true)` accepted; `\f. (f 1, f true)` rejected.
4. Lists and tuples with parametric `map`, `fold` typed correctly in a prelude.
5. Error messages with spans and both types printed with normalized names (`a, b, c`).
**Acceptance.** 60 golden programs: the inferred principal types match expected (e.g. `compose : (b -> c) -> (a -> b) -> a -> c`); 20 ill-typed programs yield errors, not panics; unification over a 10,000-node chain finishes in under 50 ms.
**Data.** Classic examples: `compose`, `flip`, `S`/`K` combinators, `fix`-free Church numerals, `omega` (must fail occurs check).
**Write-up.** Why does generalization only happen at `let`? Where does the value restriction matter?
**Stretch.** Type classes via dictionary passing; bidirectional checking for rank-N annotations.
**Resembles.** OCaml/Haskell type checkers; [rust-analyzer's `hir-ty`](https://github.com/rust-lang/rust-analyzer).

## 5.6 `rx`: a regex engine from NFA to lazy DFA  `M`

**Goal.** A regex engine supporting `. * + ? | () [] [^] ^ $ {m,n}` and UTF-8, via parse -> Thompson NFA -> Pike VM, then a lazily built DFA cache for speed, with linear-time guarantees (no backtracking blowup).
**Learn.** Thompson construction, epsilon closure, subset construction, state-cache eviction, leftmost-first semantics, why backtracking is exponential.
**Read before.** `capstones/regex_nfa_engine`, `algorithms/aho_corasick`. **During.** `algorithms/string_algos`, `text_and_parsing/parser_combinators`, `data_structures/bitset_rank_select_roaring`.
**Milestones.**
1. Parser to AST with escape and class handling.
2. NFA compile; Pike VM simulation tracking one state set.
3. Capture groups in the Pike VM.
4. Lazy DFA: cache up to 10,000 states, clear on overflow, fall back to the VM for captures.
5. Benchmark `(a*)*b` on `"a"*30` (backtracker takes forever; yours is linear).
**Acceptance.** Differential test vs the `regex` crate (dev-dependency) on 5,000 random (pattern, haystack) pairs from a small alphabet: same match/no-match and same leftmost-first span; `(a|aa)*b` on 10,000 `a`s under 50 ms.
**Data.** Random pattern generator over alphabet `{a,b,c}` with depth <= 4; haystacks length 0-40.
**Write-up.** Where do leftmost-first semantics (not longest) show up in your VM's thread priority? What is the memory bound of your DFA cache?
**Stretch.** Literal prefix optimization using `memchr`-like scan; anchored vs unanchored searches.
**Resembles.** [regex / regex-automata](https://github.com/rust-lang/regex), [ripgrep](https://github.com/BurntSushi/ripgrep).

## 5.7 `lspd`: a language server for your language  `L`

**Goal.** An LSP server (stdio, JSON-RPC 2.0) for your 5.3/5.4 language offering diagnostics on change, hover (types or docs), go-to-definition, document symbols and completion for in-scope names.
**Learn.** JSON-RPC framing (`Content-Length` headers), incremental document sync, error-tolerant parsing, position encodings (UTF-16!), symbol tables, incremental recomputation.
**Read before.** `web/jsonrpc_graphql_dataloader`, `errors/diagnostics_spans`, `text_and_parsing/zero_copy_lexer`. **During.** `compilers_runtimes/parser_generators_lr_peg_earley`, `algorithms/index_graph`, 🔜 `compilers_runtimes/lsp_server_skeleton` (planned), `dev_tools/tracing_logs`.
**Milestones.**
1. `initialize`/`shutdown` handshake; log every message to a file with `tracing`.
2. `textDocument/didOpen|didChange` -> publish diagnostics from your parser with correct line/character (UTF-16 code units).
3. Error-tolerant parse: one syntax error does not remove symbols from the rest of the file.
4. `definition` and `hover` using a resolver; `documentSymbol` outline.
5. Completion filtered by prefix and scope; debounce recomputation (50 ms).
**Acceptance.** A scripted client test (feed recorded JSON-RPC messages, compare responses) with 25 golden exchanges; a file with emoji (`"😀"` is 2 UTF-16 units) places the diagnostic caret correctly; responsive: diagnostics for a 5,000-line file in under 100 ms after edit.
**Data.** The VS Code "LSP sample" or Neovim `vim.lsp` as live clients; generated 5,000-line programs.
**Write-up.** Why UTF-16 positions, and how do you convert from your byte offsets? What state do you recompute vs cache per keystroke (see salsa)?
**Stretch.** Incremental queries à la [salsa](https://github.com/salsa-rs/salsa); semantic tokens.
**Resembles.** [rust-analyzer](https://github.com/rust-lang/rust-analyzer), [LSP specification](https://github.com/microsoft/language-server-protocol).

## 5.8 `miniir`: SSA IR with optimizations and register allocation  `L`

**Goal.** Lower an expression-and-loops language to a three-address IR, convert to SSA, run constant folding, copy propagation, dead-code elimination, then linear-scan register allocation onto 6 virtual registers, and interpret the output to prove it equals the unoptimized semantics.
**Learn.** Basic blocks and CFG, dominators, phi nodes, def-use chains, fixpoint iteration, liveness analysis, interval-based allocation and spilling.
**Read before.** `compilers_runtimes/ssa_register_allocation`, `algorithms/tarjan_scc`. **During.** `algorithms/index_graph`, `compilers_runtimes/wasm_binary_interpreter`, `memory/arena_ids`.
**Milestones.**
1. IR types with `BlockId`, `ValueId` newtypes; builder; printer; verifier (every use dominated by its def).
2. CFG and dominator tree (Cooper-Harvey-Kennedy).
3. SSA construction (dominance frontiers) and destruction before regalloc.
4. Passes: const-fold, copy-prop, DCE, simple CSE; iterate to fixpoint with a pass manager.
5. Liveness + linear scan allocator with spills to stack slots; interpreter for the allocated code.
**Acceptance.** 500 random programs (generator with loops of bounded trip count): outputs of {unoptimized IR, SSA, optimized, allocated} identical; verifier runs after every pass; optimizer reduces instruction count by at least 20% on a hand-written arithmetic-heavy sample.
**Data.** A seeded program generator (no division to avoid UB, bounded loops) plus 10 hand-written kernels (fib, gcd, sieve, matrix 3x3).
**Write-up.** Why does SSA make DCE and constant propagation simpler? What do you do for a phi when destroying SSA (lost-copy problem)?
**Stretch.** Emit x86-64/AArch64 text assembly for a function and link with `cc`; or target Cranelift.
**Resembles.** [Cranelift in Wasmtime](https://github.com/bytecodealliance/wasmtime), LLVM mid-end.

---

## Katas (30 minutes each)

1. Write a lexer for string literals with `\"`, `\\`, `\u{1F600}`; 25 table-driven cases including an unterminated string error at the right column.
2. Parse `a ? b : c ? d : e` with a Pratt parser and show it is right-associative by printing the S-expression.
3. Convert infix to RPN with the shunting-yard algorithm; assert the RPN evaluates equal to your Pratt result on 500 random expressions.
4. Implement `Display` for an AST that inserts only the parentheses needed; assert `parse(print(t)) == t` for 1,000 random trees.
5. Write constant folding over `Add/Mul/Neg` trees that also simplifies `x*1`, `x+0`, `x*0`; prove equivalence by random evaluation at 100 points.
6. Implement unification for `Var | Con(name, args)` with an occurs check; unify `f(X, g(Y))` with `f(g(Z), X)` and print the substitution.
7. Convert a regex `ab|c*` to an NFA by hand in code and print the transition table; run 20 strings through it.
8. Write a disassembler for a 6-opcode bytecode (`PUSH, ADD, MUL, JMP, JZ, HALT`) with offsets; golden-test 3 programs.
9. Compute FIRST and FOLLOW sets for a 6-rule grammar and use them to detect a left-recursion or LL(1) conflict.
10. Implement a symbol table with nested scopes and shadowing; test lookup, shadowing and "use before define" for 15 snippets.
