//! The top of an extendible hash table. It maps the **upper bits** of a hash to one of up to 2^max_depth directory pages, so a table can have many
//! directories, each growing independently.
//!
//! ```text
//! | directory_page_ids [i32; 512] | max_depth u32 |
//! ```
//!
//! The view reads and writes these fields by offset in the page's bytes.

// TODO(2b-02): your page type goes here: the table module names it, and what it stores and how is up to you.
