//! Builds `content/dsa/meaning.bin`: the meaning vector of every DSA problem (see `anneal_api::meaning`).
//!
//! `cargo run --release -p anneal-api --bin anneal-embed [content] [model]`, with the model from `tools/fetch-model.sh`
//! (default `$ANNEAL_MODEL`, then `~/.cache/anneal/models/bge-small-en-v1.5`). Run it after changing a problem's
//! title, key insight or technique; a test fails until the file matches the catalog again.

use std::path::PathBuf;
use std::time::Instant;

use anneal_api::meaning::{Embedder, Index, fingerprint, texts};
use anneal_content::Catalog;
use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let content = args.next().map_or_else(|| PathBuf::from("content"), PathBuf::from);
    let model = args
        .next()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ANNEAL_MODEL").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/anneal/models/bge-small-en-v1.5")))
        .context("no model directory: pass one, or set ANNEAL_MODEL")?;
    let catalog = Catalog::load(&content).with_context(|| format!("loading {}", content.display()))?.catalog;
    let texts = texts(&catalog);
    let embedder = Embedder::load(&model).with_context(|| format!("loading the model from {}", model.display()))?;
    let started = Instant::now();
    let refs: Vec<&str> = texts.iter().map(|(_, t)| t.as_str()).collect();
    let index = Index::new(texts.iter().map(|(id, _)| id.clone()).collect(), embedder.passages(&refs)?)?;
    let out = content.join("dsa").join("meaning.bin");
    index.write(&out, &fingerprint(&texts))?;
    println!("{} problems embedded in {:.1}s into {}", index.len(), started.elapsed().as_secs_f32(), out.display());
    Ok(())
}
