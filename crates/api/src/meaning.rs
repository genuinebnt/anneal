//! Meaning vectors: every DSA problem's title, key insight and technique embedded by a small local model
//! (bge-small-en-v1.5, 384 dimensions), and exact nearest-neighbour search over them. Nothing leaves the machine.
//!
//! The vectors are built ahead of time (`cargo run --release -p anneal-api --bin anneal-embed`) into
//! `content/dsa/meaning.bin`, so the API answers "what means the same as this problem" without loading the model. The
//! model is only needed to embed free text (`~ two numbers that add up`), and to rebuild the vectors at start-up when
//! the file is missing or no longer matches the catalog.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anneal_content::Catalog;
use anyhow::{Context, bail, ensure};
use axum::Json;
use axum::extract::{Query, State};
use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use crate::AppState;
use crate::dsa::dsa_tracks;
use crate::error::{ApiError, ApiResult};

/// The model, pinned to one revision: vectors from another revision would not be comparable.
pub const MODEL: &str = "bge-small-en-v1.5";
pub const REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
pub const DIMS: usize = 384;
/// bge's instruction for a short query searched against longer passages; passages themselves take none.
const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";
const MAGIC: &[u8] = b"anneal-meaning 1\n";
const BATCH: usize = 32;
/// The most neighbours one request can ask for.
const MAX_K: usize = 50;

/// The text each problem is embedded from: what it is called, the move that solves it, and the technique it drills.
/// Topic tags are left out on purpose: they are the other similarity, compared exactly.
pub fn texts(catalog: &Catalog) -> Vec<(String, String)> {
    let names: HashMap<&str, &str> = catalog.dsa.techniques.iter().map(|t| (t.id.as_str(), t.name.as_str())).collect();
    dsa_tracks(catalog)
        .flat_map(|t| &t.problems)
        .chain(catalog.practice_tracks.iter().flat_map(|t| &t.problems))
        .filter_map(|p| {
            let d = p.dsa.as_ref()?;
            let technique = names.get(d.technique.as_str()).copied().unwrap_or_default();
            Some((p.id.clone(), format!("{}. {} Technique: {}.", p.meta.title, d.insight, technique)))
        })
        .collect()
}

/// A digest of the model revision and every text, in order: the vectors file is current only while it matches.
pub fn fingerprint(texts: &[(String, String)]) -> String {
    let mut h = Sha256::new();
    h.update(REVISION.as_bytes());
    for (id, text) in texts {
        h.update([0]);
        h.update(id.as_bytes());
        h.update([0]);
        h.update(text.as_bytes());
    }
    format!("{:x}", h.finalize())
}

/// The model and its tokenizer, on the CPU. Embedding is CPU work: call it from a blocking thread, not a runtime one.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Embedder {
    /// Loads `config.json`, `tokenizer.json` and `model.safetensors` from `dir`.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).context("config.json")?).context("config.json")?;
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| anyhow::anyhow!("tokenizer.json: {e}"))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        tokenizer.with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() })).map_err(|e| anyhow::anyhow!("tokenizer truncation: {e}"))?;
        // Read into memory rather than memory-mapped: a mapped file changed underneath would be undefined behaviour.
        let weights = std::fs::read(dir.join("model.safetensors")).context("model.safetensors")?;
        let vb = VarBuilder::from_buffered_safetensors(weights, DType::F32, &device)?;
        let model = BertModel::load(vb, &config)?;
        Ok(Self { model, tokenizer, device })
    }

    /// Unit-length vectors for passages (problem texts).
    pub fn passages(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(BATCH) {
            out.extend(self.batch(chunk)?);
        }
        Ok(out)
    }

    /// The unit-length vector for a short search query.
    pub fn query(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut v = self.batch(&[&format!("{QUERY_PREFIX}{text}")])?;
        v.pop().context("the model returned no vector")
    }

    fn batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let enc = self.tokenizer.encode_batch(texts.to_vec(), true).map_err(|e| anyhow::anyhow!("tokenizing: {e}"))?;
        let len = enc.first().map_or(0, |e| e.len());
        let n = enc.len();
        let flat = |f: fn(&tokenizers::Encoding) -> &[u32]| enc.iter().flat_map(|e| f(e).iter().copied()).collect::<Vec<u32>>();
        let ids = Tensor::from_vec(flat(|e| e.get_ids()), (n, len), &self.device)?;
        let types = Tensor::from_vec(flat(|e| e.get_type_ids()), (n, len), &self.device)?;
        let mask = Tensor::from_vec(flat(|e| e.get_attention_mask()), (n, len), &self.device)?;
        let hidden = self.model.forward(&ids, &types, Some(&mask))?;
        // bge pools by the [CLS] token, then normalises
        let cls = hidden.i((.., 0))?;
        let norm = cls.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?;
        Ok(cls.broadcast_div(&norm)?.to_vec2::<f32>()?)
    }
}

/// Every problem's vector, in one block, with its id.
pub struct Index {
    ids: Vec<String>,
    /// `ids.len() * DIMS` floats, row by row; every row has unit length.
    vecs: Vec<f32>,
}

impl Index {
    pub fn new(ids: Vec<String>, rows: Vec<Vec<f32>>) -> anyhow::Result<Self> {
        ensure!(ids.len() == rows.len(), "{} ids for {} vectors", ids.len(), rows.len());
        ensure!(rows.iter().all(|r| r.len() == DIMS), "a vector is not {DIMS} long");
        Ok(Self { ids, vecs: rows.concat() })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    fn row(&self, i: usize) -> &[f32] {
        &self.vecs[i * DIMS..(i + 1) * DIMS]
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.ids.iter().position(|x| x == id)
    }

    /// The `k` rows closest to `v` by cosine (a dot product: every row has unit length), best first, leaving out `skip`.
    /// Exact: every row is compared.
    fn nearest(&self, v: &[f32], k: usize, skip: Option<usize>) -> Vec<(usize, f32)> {
        let mut all: Vec<(usize, f32)> = (0..self.len()).filter(|&i| Some(i) != skip).map(|i| (i, dot(v, self.row(i)))).collect();
        all.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        all.truncate(k);
        all
    }

    /// Writes the vectors file: the magic line, a JSON header line, then the floats, little-endian.
    pub fn write(&self, path: &Path, fingerprint: &str) -> anyhow::Result<()> {
        let header = serde_json::to_string(&Header { model: MODEL.into(), revision: REVISION.into(), dims: DIMS, fingerprint: fingerprint.into(), ids: self.ids.clone() })?;
        let mut out = Vec::with_capacity(MAGIC.len() + header.len() + 1 + self.vecs.len() * 4);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(header.as_bytes());
        out.push(b'\n');
        for x in &self.vecs {
            out.extend_from_slice(&x.to_le_bytes());
        }
        std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))
    }

    /// Reads a vectors file and its fingerprint.
    pub fn read(path: &Path) -> anyhow::Result<(Self, String)> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let rest = bytes.strip_prefix(MAGIC).context("not a meaning vectors file")?;
        let end = rest.iter().position(|&b| b == b'\n').context("no header line")?;
        let header: Header = serde_json::from_slice(&rest[..end]).context("header")?;
        if header.revision != REVISION || header.dims != DIMS {
            bail!("built by {} at {} with {} dims", header.model, header.revision, header.dims);
        }
        let floats = &rest[end + 1..];
        ensure!(floats.len() == header.ids.len() * DIMS * 4, "{} bytes of vectors for {} ids", floats.len(), header.ids.len());
        let vecs = floats.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
        Ok((Self { ids: header.ids, vecs }, header.fingerprint))
    }
}

#[derive(Serialize, Deserialize)]
struct Header {
    model: String,
    revision: String,
    dims: usize,
    fingerprint: String,
    ids: Vec<String>,
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Where the API stands with meaning vectors.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// No vectors file that matches and no model to build one.
    Off,
    /// The vectors are being built from the model.
    Building,
    Ready,
    Failed,
}

#[derive(Default)]
struct Inner {
    index: Option<Arc<Index>>,
    embedder: Option<Arc<Embedder>>,
    building: bool,
    error: Option<String>,
    /// How long the last build took, when the vectors were built here rather than read from the file.
    built_ms: Option<u128>,
}

/// The vectors and the model, shared by the handlers. Locks are held only to clone an `Arc` in or out, never across
/// an await or while embedding.
#[derive(Default)]
pub struct Meaning {
    inner: RwLock<Inner>,
}

impl Meaning {
    /// Meaning search turned off (tests, or a server without the model and the file).
    pub fn off() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Reads `content/dsa/meaning.bin` when it matches the catalog, and loads the model from `model_dir` when there is
    /// one, in the background. A missing or stale file is rebuilt from the model (a minute or two of CPU) and kept in
    /// memory only: the content directory is read-only in production.
    pub fn start(catalog: &Catalog, content: &Path, model_dir: Option<PathBuf>) -> Arc<Self> {
        let me = Self::off();
        let texts = texts(catalog);
        let print = fingerprint(&texts);
        match Index::read(&content.join("dsa").join("meaning.bin")) {
            Ok((index, found)) if found == print => {
                tracing::info!(problems = index.len(), "meaning vectors loaded");
                me.write().index = Some(Arc::new(index));
            }
            Ok(_) => tracing::warn!("content/dsa/meaning.bin is stale; run `cargo run --release -p anneal-api --bin anneal-embed`"),
            Err(e) => tracing::warn!("no meaning vectors: {e:#}"),
        }
        let Some(dir) = model_dir else {
            return me;
        };
        let rebuild = me.read().index.is_none();
        me.write().building = rebuild;
        let shared = me.clone();
        std::thread::Builder::new()
            .name("meaning".into())
            .spawn(move || {
                let started = Instant::now();
                let result = Embedder::load(&dir).with_context(|| format!("loading the model from {}", dir.display())).and_then(|embedder| {
                    let index = if rebuild {
                        let refs: Vec<&str> = texts.iter().map(|(_, t)| t.as_str()).collect();
                        Some(Index::new(texts.iter().map(|(id, _)| id.clone()).collect(), embedder.passages(&refs)?)?)
                    } else {
                        None
                    };
                    Ok((embedder, index))
                });
                let mut w = shared.write();
                w.building = false;
                match result {
                    Ok((embedder, index)) => {
                        w.embedder = Some(Arc::new(embedder));
                        if let Some(index) = index {
                            tracing::info!(problems = index.len(), ms = started.elapsed().as_millis(), "meaning vectors built");
                            w.built_ms = Some(started.elapsed().as_millis());
                            w.index = Some(Arc::new(index));
                        }
                    }
                    Err(e) => {
                        tracing::error!("meaning model: {e:#}");
                        w.error = Some(format!("{e:#}"));
                    }
                }
            })
            .expect("spawning the meaning thread");
        me
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind, still consistent: keep serving it.
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }

    fn phase(i: &Inner) -> Phase {
        match (&i.index, i.building, &i.error) {
            (Some(_), _, _) => Phase::Ready,
            (None, true, _) => Phase::Building,
            (None, false, Some(_)) => Phase::Failed,
            (None, false, None) => Phase::Off,
        }
    }
}

#[derive(Deserialize)]
pub struct Ask {
    /// A problem id: the problems that mean the most nearly the same.
    like: Option<String>,
    /// Free text: the problems closest to it in meaning (needs the model).
    q: Option<String>,
    k: Option<usize>,
}

#[derive(Serialize)]
pub struct Hit {
    id: String,
    score: f32,
}

#[derive(Serialize)]
pub struct Answer {
    phase: Phase,
    model: &'static str,
    dims: usize,
    /// Problems compared: every one, since the search is exact.
    compared: usize,
    /// Free-text search is possible (the model is loaded).
    can_search: bool,
    /// The vectors were built at start-up, in this many milliseconds, instead of read from the file.
    built_ms: Option<u128>,
    error: Option<String>,
    hits: Vec<Hit>,
}

/// `GET /api/dsa/meaning?like=<id>` or `?q=<text>`, with `k` neighbours (12 by default). Without either it reports the
/// phase only.
pub async fn get(State(s): State<AppState>, Query(ask): Query<Ask>) -> ApiResult<Json<Answer>> {
    let (index, embedder, phase, built_ms, error) = {
        let i = s.meaning.read();
        (i.index.clone(), i.embedder.clone(), Meaning::phase(&i), i.built_ms, i.error.clone())
    };
    let k = ask.k.unwrap_or(12).clamp(1, MAX_K);
    let mut hits = Vec::new();
    if let Some(index) = &index {
        let found = if let Some(id) = ask.like.as_deref() {
            let at = index.position(id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
            index.nearest(index.row(at), k, Some(at))
        } else if let Some(q) = ask.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            if q.len() > 500 {
                return Err(ApiError::BadRequest("a meaning search is at most 500 characters".into()));
            }
            let Some(embedder) = embedder.clone() else {
                return Err(ApiError::NotReady("search by meaning (the model is not loaded)".into()));
            };
            let (index, q) = (index.clone(), q.to_owned());
            tokio::task::spawn_blocking(move || embedder.query(&q).map(|v| index.nearest(&v, k, None)))
                .await
                .map_err(|e| ApiError::Internal(anyhow::anyhow!("the meaning thread stopped: {e}")))?
                .map_err(ApiError::Internal)?
        } else {
            Vec::new()
        };
        hits = found.into_iter().map(|(i, score)| Hit { id: index.ids[i].clone(), score }).collect();
    }
    Ok(Json(Answer { phase, model: MODEL, dims: DIMS, compared: index.as_ref().map_or(0, |i| i.len()), can_search: embedder.is_some(), built_ms, error, hits }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: &[f32]) -> Vec<f32> {
        let n = dot(v, v).sqrt();
        let mut out: Vec<f32> = v.iter().map(|x| x / n).collect();
        out.resize(DIMS, 0.0);
        out
    }

    #[test]
    fn nearest_is_best_first_and_leaves_out_the_problem_itself() {
        let index = Index::new(vec!["a".into(), "b".into(), "c".into()], vec![unit(&[1.0, 0.0]), unit(&[0.9, 0.1]), unit(&[0.0, 1.0])]).unwrap();
        let near = index.nearest(index.row(0), 5, Some(0));
        assert_eq!(near.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [1, 2]);
        assert!(near[0].1 > 0.99 && near[1].1.abs() < 1e-6);
    }

    #[test]
    fn the_file_round_trips_and_keeps_its_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.bin");
        let index = Index::new(vec!["a".into(), "b".into()], vec![unit(&[1.0, 2.0]), unit(&[3.0, -1.0])]).unwrap();
        index.write(&path, "abc").unwrap();
        let (back, print) = Index::read(&path).unwrap();
        assert_eq!(print, "abc");
        assert_eq!(back.ids, index.ids);
        assert_eq!(back.vecs, index.vecs);
    }

    /// The committed vectors must match the catalog, or production rebuilds them at every start.
    #[test]
    fn the_committed_vectors_match_the_catalog() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let catalog = Catalog::load(&root).expect("content").catalog;
        let texts = texts(&catalog);
        let (index, print) = Index::read(&root.join("dsa/meaning.bin")).expect("content/dsa/meaning.bin");
        assert_eq!(
            print,
            fingerprint(&texts),
            "content/dsa/meaning.bin is stale: a title, key insight or technique changed. Rebuild it with `cargo run --release -p anneal-api --bin anneal-embed` (needs the model: tools/fetch-model.sh)"
        );
        assert_eq!(index.len(), texts.len());
    }

    /// With the model present (`ANNEAL_MODEL`, set in CI by tools/fetch-model.sh), a question finds the problem it
    /// describes, and two problems about the same move are closer than two about different ones.
    #[test]
    fn the_model_finds_problems_by_what_they_mean() {
        let Some(dir) = std::env::var_os("ANNEAL_MODEL").map(PathBuf::from) else {
            eprintln!("ANNEAL_MODEL is not set: skipping the model test");
            return;
        };
        let e = Embedder::load(&dir).expect("model");
        let docs = ["Two Sum. Store each number's index in a map and look up target minus the current number. Technique: Complement lookup.", "Maximum Depth of Binary Tree. Depth is one plus the deeper of the two subtrees. Technique: Tree DFS.", "Merge Intervals. Sort by start and extend the last interval while the next one overlaps. Technique: Merge overlapping."];
        let v = e.passages(&docs).unwrap();
        assert!(v.iter().all(|r| (dot(r, r) - 1.0).abs() < 1e-4), "vectors have unit length");
        let index = Index::new(vec!["two-sum".into(), "depth".into(), "merge".into()], v).unwrap();
        let ask = |q: &str| index.ids[index.nearest(&e.query(q).unwrap(), 1, None)[0].0].clone();
        assert_eq!(ask("find two numbers in an array that add up to a target"), "two-sum");
        assert_eq!(ask("how deep is a tree"), "depth");
        assert_eq!(ask("combine overlapping ranges"), "merge");
    }
}
