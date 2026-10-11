//! Packs the learner-facing part of every course (course.toml, lectures.toml, modules/, template/) into one blob that is compiled into the
//! binary, so `cargo install --git ...` gives an `anneal` that can `course init` with no checkout of the repository. The reference solution
//! is never included (it is not part of those directories). A missing `courses/` directory (a build context without it) gives an empty blob.
//!
//! The format and the packing are `anneal_content::bundle`, which the app also uses to serve a course's bundle (docs/CLI_INSTALL.md).

use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "../content/src/bundle.rs"]
mod bundle;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let courses = manifest.join("../../courses");
    if let Ok(ids) = fs::read_dir(&courses) {
        let mut ids: Vec<_> = ids.flatten().map(|e| e.path()).filter(|p| p.join("course.toml").is_file()).collect();
        ids.sort();
        for id in ids {
            for sub in ["modules", "template"] {
                println!("cargo:rerun-if-changed={}", id.join(sub).display());
            }
            println!("cargo:rerun-if-changed={}", id.join("course.toml").display());
            println!("cargo:rerun-if-changed={}", id.join("lectures.toml").display());
        }
    }
    println!("cargo:rerun-if-changed={}", courses.display());
    let blob = bundle::pack(&bundle::collect(&courses, None));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("courses.bin");
    fs::write(out, blob).unwrap();
}
