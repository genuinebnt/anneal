# Chapter `patterns`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`state_machine`](../../examples/patterns/state_machine.rs) | intermediate | an enum state machine where illegal transitions are impossible to write silently. |
| 2 | [`typestate_builder`](../../examples/patterns/typestate_builder.rs) | intermediate | typestate builder: invalid states do not compile. |
| 3 | [`any_event_bus`](../../examples/patterns/any_event_bus.rs) | advanced | a type-safe event bus built on `TypeId` and `dyn Any`: runtime dispatch with compile-time-typed handlers. |
| 4 | [`design_patterns_high`](../../examples/patterns/design_patterns_high.rs) | advanced | high-level design patterns in idiomatic Rust: builder, strategy, observer, visitor, command, DI and more. |
| 5 | [`design_patterns_low`](../../examples/patterns/design_patterns_low.rs) | advanced | low-level, data-oriented patterns: arenas, interning, SoA, zero-copy, pools, bitfields, inline storage. |
| 6 | [`generic_state_machine`](../../examples/patterns/generic_state_machine.rs) | advanced | a generic state-machine trait, replaying event logs, and table-driven transitions. |
| 7 | [`sans_io_protocol`](../../examples/patterns/sans_io_protocol.rs) | advanced | sans-IO protocol design: a pure state machine driven by a simulator, a blocking socket and tokio. |
| 8 | [`tower_service_layer`](../../examples/patterns/tower_service_layer.rs) | advanced | a mini re-implementation of tower's Service/Layer model with timeout, retry, limits and a circuit breaker. |
| 9 | [`type_erasure_vtable`](../../examples/patterns/type_erasure_vtable.rs) | advanced | type erasure side by side: `dyn Trait`, `dyn Any`, `Box<dyn Fn>`, `clone_box`, and a hand-written vtable. |
| 10 | [`typestate_protocol`](../../examples/patterns/typestate_protocol.rs) | advanced | typestate protocol: a session whose legal operations depend on its state, enforced by the compiler. |
| 11 | [`undo_redo_editors`](../../examples/patterns/undo_redo_editors.rs) | advanced | undo/redo for a text editor: command, memento, persistent rope, piece table and undo tree, tested against a model |
