//! Acquire and release, in code that both std and the loom model checker can run.
//!
//! Nothing here has undefined behaviour, whichever test or binary runs it. The two
//! types that use `unsafe` have fixed, correct orderings. The experiments that let
//! you choose a weak ordering use only atomics, so a weak choice can give a wrong
//! value but never a data race.

pub mod countdown;
pub mod litmus;
pub mod publish;
pub mod sync;
