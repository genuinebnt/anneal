//! The bottom level of an extendible hash table. A bucket is a small unsorted array of `(key, value)` pairs. Keys are unique.
//!
//! ```text
//! | size u32 | max_size u32 | (key, value) | (key, value) | ... |
//! ```

// TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
