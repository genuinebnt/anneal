//! RECIPE 43 (language): files, paths, `Read`/`Write`, buffering, and crash-safe writes.
//! LEVEL: intermediate
//! Use in: persistence (saving executions/state), reading workflow files, log output, testing IO code.   Run: cargo run --example io_files_paths
//! COMMON USES: saving state safely, reading config, testing IO code with in-memory readers/writers.
//!
//! Key ideas:
//!  * `Read` and `Write` are traits. Write functions over `impl Write` / `impl Read` and you can test with `Vec<u8>`
//!    and `&[u8]` instead of real files.
//!  * Wrap files in `BufReader`/`BufWriter` for many small reads/writes (see bufwriter).
//!  * To update a file safely: write a temp file, flush+sync, then `rename` over the target (atomic on POSIX), so a
//!    crash never leaves a half-written file.
//! Docs: https://doc.rust-lang.org/std/io/index.html and https://doc.rust-lang.org/std/fs/index.html
#![allow(dead_code)]
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};

// generic over any writer: works for files, stdout, TcpStream, Vec<u8>...
fn write_report(mut w: impl Write, rows: &[(&str, u32)]) -> io::Result<()> {
    for (name, n) in rows {
        writeln!(w, "{name}\t{n}")?;
    }
    w.flush()
}
// generic over any reader
fn count_lines(r: impl Read) -> io::Result<usize> {
    Ok(BufReader::new(r).lines().count())
}

fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(contents)?;
    f.sync_all()?; // data is on disk before the rename makes it visible
    fs::rename(&tmp, path)
}

fn main() -> io::Result<()> {
    // test-friendly IO: no files needed
    let mut sink = Vec::new();
    write_report(&mut sink, &[("a", 1), ("b", 2)])?;
    println!("{:?}", String::from_utf8_lossy(&sink));
    println!("{}", count_lines(&b"one\ntwo\nthree"[..])?);
    println!("{}", count_lines(Cursor::new("x\ny\n"))?);

    // real files
    let dir: PathBuf = std::env::temp_dir().join("flowforge_cookbook_io");
    fs::create_dir_all(&dir)?;
    let file = dir.join("state.json");
    atomic_write(&file, br#"{"version":1}"#)?;
    atomic_write(&file, br#"{"version":2}"#)?;
    println!("{}", fs::read_to_string(&file)?);

    let log = dir.join("events.log");
    {
        let mut w = BufWriter::new(File::create(&log)?);
        for i in 0..3 {
            writeln!(w, "event {i}")?;
        }
    } // BufWriter flushes on drop (but errors are lost: call flush() explicitly in real code)
    for line in BufReader::new(File::open(&log)?).lines() {
        println!("read: {}", line?);
    }

    // errors you should handle
    match File::open(dir.join("missing.txt")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => println!("not found: {e}"),
        other => println!("{other:?}"),
    }

    // paths
    let p = Path::new("/data/videos/clip.final.mp4");
    println!(
        "{:?} {:?} {:?} {:?}",
        p.file_name(),
        p.extension(),
        p.file_stem(),
        p.parent()
    );
    println!(
        "{:?} {}",
        p.with_extension("webm"),
        Path::new("a").join("b").join("c.txt").display()
    );
    let mut names: Vec<_> = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort(); // read_dir order is unspecified: sort for deterministic output
    println!("{names:?}");
    fs::remove_dir_all(&dir)
}
