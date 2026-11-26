//! A language server for PEBBLE: JSON-RPC 2.0 with `Content-Length` framing, written
//! on std + `serde_json`. See `docs/LSP.md` for the feature table and design notes.
//!
//! * [`transport`]: framing (`read_message`/`write_message`).
//! * [`Server`]: the protocol state machine. It is a pure function from incoming
//!   messages to outgoing messages ([`Server::process`]), so tests drive it in-process;
//!   [`serve`] wraps it with a reader thread for real stdio use.
//! * [`snapshot`]: one analysed version of a document plus UTF-16 conversions.
//! * `features`, `completion`, `semtok`: the language features.

mod completion;
mod features;
pub mod fuzzy;
mod nav;
mod semtok;
mod server;
pub mod snapshot;
pub mod transport;
pub mod workspace;

pub use server::{Server, SyncKind, diagnostic_json, serve};
pub use snapshot::Snapshot;
