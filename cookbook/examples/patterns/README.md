# Design Patterns

Typestate, state machines, builders, command/undo, middleware, sans-IO, type erasure, event buses.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

11 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`typestate_builder`](typestate_builder.rs) | typestate builder: invalid states do not compile | intermediate | HTTP clients, connection/session objects, config types where a required step must not be skippable. | std only |
| [`state_machine`](state_machine.rs) | an enum state machine where illegal transitions are impossible to write silently | intermediate | order/payment/job/connection lifecycles: make illegal transitions unrepresentable and testable. | std only |
| [`typestate_protocol`](typestate_protocol.rs) | typestate protocol: a session whose legal operations depend on its state, enforced by the compiler | advanced | protocol clients, builders with mandatory steps, hardware drivers; invalid sequences fail to compile. | std only |
| [`generic_state_machine`](generic_state_machine.rs) | a generic state-machine trait, replaying event logs, and table-driven transitions | advanced | order/account/ticket lifecycles; event sourcing (replay events to rebuild state); exhaustive model checking. | std only |
| [`any_event_bus`](any_event_bus.rs) | a type-safe event bus built on `TypeId` and `dyn Any`: runtime dispatch with compile-time-typed handlers | advanced | plugin systems, UI/event buses, ECS resources (Bevy), service locators. | std only |
| [`type_erasure_vtable`](type_erasure_vtable.rs) | type erasure side by side: `dyn Trait`, `dyn Any`, `Box<dyn Fn>`, `clone_box`, and a hand-written vtable | advanced | thread-pool job queues (`Box<dyn FnOnce() + Send>`), executors' `RawWaker`s, plugin registries, | std only |
| [`design_patterns_high`](design_patterns_high.rs) | high-level design patterns in idiomatic Rust: builder, strategy, observer, visitor, command, DI and more | advanced | HTTP middleware stacks, text editors/undo (e.g. in IDE buffers), compilers (AST walkers, rustc's `Visitor`), | std only |
| [`design_patterns_low`](design_patterns_low.rs) | low-level, data-oriented patterns: arenas, interning, SoA, zero-copy, pools, bitfields, inline storage | advanced | rustc/rust-analyzer style id-indexed IRs and symbol interners, ECS and game engines (struct-of-arrays), database | std only |
| [`sans_io_protocol`](sans_io_protocol.rs) | sans-IO protocol design: a pure state machine driven by a simulator, a blocking socket and tokio | advanced | QUIC/HTTP3 stacks, TLS libraries, WebRTC/SIP/DNS clients, embedded or `no_std` protocol cores, deterministic simulation | tokio |
| [`tower_service_layer`](tower_service_layer.rs) | a mini re-implementation of tower's Service/Layer model with timeout, retry, limits and a circuit breaker | advanced | client resilience (timeouts, retries with backoff, circuit breakers, rate limits) in hyper/tonic/reqwest-style stacks, | axum, tokio, tower |
| [`undo_redo_editors`](undo_redo_editors.rs) | undo/redo for a text editor: command, memento, persistent rope, piece table and undo tree, tested against a model | advanced | text and code editors (VS Code uses a piece-tree buffer; Vim has an undo tree), Photoshop-style history, | std only |
