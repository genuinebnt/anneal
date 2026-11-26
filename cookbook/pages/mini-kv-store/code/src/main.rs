//! `cargo run`: write, shadow, delete, "crash", recover, compact, and show the files after each step.

use mini_kv::{Result, Store};
use std::fs;
use std::path::Path;

fn show(step: &str, store: &mut Store, dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        files.push(format!("{} ({} B)", entry.file_name().to_string_lossy(), entry.metadata()?.len()));
    }
    files.sort();
    println!("{step}\n   files: {}", files.join(", "));
    for key in ["apple", "banana", "cherry"] {
        let value = store.get(key.as_bytes())?.map(|v| String::from_utf8_lossy(&v).into_owned());
        println!("   {key:<6} -> {value:?}");
    }
    Ok(())
}

fn main() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("mini_kv_demo_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    let mut store = Store::open(&dir, 1 << 20)?;
    store.put(b"apple", b"red")?;
    store.put(b"banana", b"yellow")?;
    store.flush()?;
    show("1. two puts, then a flush", &mut store, &dir)?;

    store.put(b"apple", b"green")?;
    store.delete(b"banana")?;
    store.flush()?;
    show("2. overwrite and delete, then flush: newer table shadows older", &mut store, &dir)?;

    store.put(b"cherry", b"dark")?;
    drop(store);
    let mut store = Store::open(&dir, 1 << 20)?;
    show("3. one more put, drop without flushing, reopen: the log is replayed", &mut store, &dir)?;

    store.compact()?;
    show("4. compact: one table, no old versions, no tombstone", &mut store, &dir)?;
    println!("   records in the table: {}", store.tables()[0].read_all()?.len());

    fs::remove_dir_all(&dir)?;
    Ok(())
}
