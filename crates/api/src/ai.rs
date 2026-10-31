//! AI help, only when configured: "ask about this page" and an interviewer that reads a system design attempt against its
//! rubric and suggests marks. The owner always confirms; nothing is graded by the model alone.
//!
//! The provider is chosen by the environment, so no key is ever in the repository:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `ANNEAL_AI_PROVIDER` | `gemini` (Google's Gemini API) or `openai` (any OpenAI-compatible server, such as Ollama on a Mac) |
//! | `ANNEAL_AI_KEY` | the API key (not needed for a local server) |
//! | `ANNEAL_AI_MODEL` | the model, default `gemini-3.5-flash` (or e.g. `qwen3:8b` for Ollama) |
//! | `ANNEAL_AI_URL` | the base URL for `openai`, default `http://localhost:11434/v1` |
//!
//! Without a provider every AI endpoint answers 409 with a plain message and the pages hide their AI buttons.

use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::error::{ApiError, ApiResult};

const DEFAULT_MODEL: &str = "gemini-3.5-flash";
const DEFAULT_LOCAL_URL: &str = "http://localhost:11434/v1";
/// A request to the model may take this long; thinking models are slow.
const TIMEOUT: Duration = Duration::from_secs(120);
/// The most turns and characters one question may carry.
const MAX_TURNS: usize = 20;
const MAX_CHARS: usize = 8_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Provider {
    Gemini { key: String },
    OpenAi { url: String, key: Option<String> },
}

/// The configured model, or `None` when AI is off.
#[derive(Clone, Debug)]
pub struct AiConfig {
    pub provider: Provider,
    pub model: String,
}

impl AiConfig {
    /// Reads the configuration from the environment (see the module docs).
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let model = var("ANNEAL_AI_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let provider = match var("ANNEAL_AI_PROVIDER").as_deref() {
            Some("gemini") | None => Provider::Gemini { key: var("ANNEAL_AI_KEY")? },
            Some("openai") => Provider::OpenAi { url: var("ANNEAL_AI_URL").unwrap_or_else(|| DEFAULT_LOCAL_URL.to_owned()), key: var("ANNEAL_AI_KEY") },
            Some(other) => {
                tracing::warn!("ANNEAL_AI_PROVIDER {other:?} is not gemini or openai; AI is off");
                return None;
            }
        };
        Some(Self { provider, model })
    }

    fn name(&self) -> &'static str {
        match self.provider {
            Provider::Gemini { .. } => "gemini",
            Provider::OpenAi { .. } => "openai",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Turn {
    /// `user` or `model`.
    pub role: String,
    pub text: String,
}

/// One answer from the model: `system` sets the role, `turns` is the conversation, `json` asks for a JSON object back.
async fn complete(cfg: &AiConfig, system: &str, turns: &[Turn], json: bool) -> Result<String, String> {
    let http = reqwest::Client::builder().timeout(TIMEOUT).build().map_err(|e| e.to_string())?;
    match &cfg.provider {
        Provider::Gemini { key } => {
            let contents: Vec<Value> = turns.iter().map(|t| json!({ "role": if t.role == "model" { "model" } else { "user" }, "parts": [{ "text": t.text }] })).collect();
            let mut body = json!({ "systemInstruction": { "parts": [{ "text": system }] }, "contents": contents, "generationConfig": { "temperature": 0.3 } });
            if json {
                body["generationConfig"]["responseMimeType"] = json!("application/json");
            }
            let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", cfg.model);
            let res = http.post(url).header("x-goog-api-key", key).json(&body).send().await.map_err(|e| format!("the model did not answer: {e}"))?;
            let status = res.status();
            let v: Value = res.json().await.map_err(|e| format!("an unreadable answer: {e}"))?;
            if !status.is_success() {
                return Err(v["error"]["message"].as_str().unwrap_or("the model refused the request").to_owned());
            }
            let text: String = v["candidates"][0]["content"]["parts"].as_array().into_iter().flatten().filter_map(|p| p["text"].as_str()).collect();
            if text.is_empty() { Err("the model gave an empty answer".into()) } else { Ok(text) }
        }
        Provider::OpenAi { url, key } => {
            let mut messages = vec![json!({ "role": "system", "content": system })];
            messages.extend(turns.iter().map(|t| json!({ "role": if t.role == "model" { "assistant" } else { "user" }, "content": t.text })));
            let mut body = json!({ "model": cfg.model, "messages": messages, "temperature": 0.3 });
            if json {
                body["response_format"] = json!({ "type": "json_object" });
            }
            let mut req = http.post(format!("{}/chat/completions", url.trim_end_matches('/'))).json(&body);
            if let Some(k) = key {
                req = req.bearer_auth(k);
            }
            let res = req.send().await.map_err(|e| format!("the model did not answer: {e}"))?;
            let status = res.status();
            let v: Value = res.json().await.map_err(|e| format!("an unreadable answer: {e}"))?;
            if !status.is_success() {
                return Err(v["error"]["message"].as_str().unwrap_or("the model refused the request").to_owned());
            }
            v["choices"][0]["message"]["content"].as_str().map(str::to_owned).filter(|t| !t.is_empty()).ok_or_else(|| "the model gave an empty answer".into())
        }
    }
}

fn config(s: &AppState) -> ApiResult<&AiConfig> {
    s.ai.as_ref().ok_or_else(|| ApiError::NotReady("AI help (set ANNEAL_AI_PROVIDER and ANNEAL_AI_KEY on the server)".into()))
}

/// `GET /api/ai`: whether AI help is on, and which model answers.
pub async fn status(State(s): State<AppState>) -> Json<Value> {
    Json(match &s.ai {
        Some(c) => json!({ "enabled": true, "provider": c.name(), "model": c.model }),
        None => json!({ "enabled": false }),
    })
}

#[derive(Deserialize)]
pub struct Ask {
    /// What the page is about: `sd-concept`, `sd-problem` or `dsa-problem`, and its id.
    kind: String,
    id: String,
    turns: Vec<Turn>,
}

/// The page's own text, so the model answers about what the owner is reading.
fn page_context(s: &AppState, kind: &str, id: &str) -> ApiResult<String> {
    match kind {
        "sd-concept" => {
            let c = s.catalog.sd.concepts.get(id).ok_or_else(|| ApiError::NotFound(format!("concept {id}")))?;
            let body: String = c.sections.iter().map(|x| format!("## {}\n{}\n", x.heading, x.body)).collect();
            Ok(format!("# {}\n{}\n{}", c.title, c.lede, body))
        }
        "sd-problem" => {
            let p = s.catalog.sd.problems.get(id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
            let steps: String = p.steps.iter().map(|st| format!("## {}\n{}\nReference: {}\n", st.name, st.prompt, st.reference)).collect();
            Ok(format!("# System design: {}\n{}\n{}", p.title, p.prompt, steps))
        }
        "dsa-problem" => {
            let (_, p) = s.catalog.problem(id).ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
            let d = p.dsa.as_ref().ok_or_else(|| ApiError::NotFound(format!("problem {id}")))?;
            Ok(format!("# LeetCode {}: {}\nDifficulty: {:?}. Topics: {}. Key insight: {}", d.number, p.meta.title, p.meta.level, p.meta.tags.join(", "), d.insight))
        }
        other => Err(ApiError::BadRequest(format!("{other:?} is not a page AI can read"))),
    }
}

/// `POST /api/ai/ask`: a question about the page being read, with the conversation so far.
pub async fn ask(State(s): State<AppState>, Json(b): Json<Ask>) -> ApiResult<Json<Value>> {
    let cfg = config(&s)?;
    if b.turns.is_empty() || b.turns.len() > MAX_TURNS || b.turns.iter().map(|t| t.text.len()).sum::<usize>() > MAX_CHARS {
        return Err(ApiError::BadRequest(format!("a question is 1 to {MAX_TURNS} turns and at most {MAX_CHARS} characters")));
    }
    let page = page_context(&s, &b.kind, &b.id)?;
    let system = format!(
        "You are a demanding but kind mentor helping one engineer prepare for interviews (system design, data structures and algorithms, Rust). \
         Answer about the page below, concisely, in plain prose with short lists where they help. Ask a probing follow-up when it would teach more than an answer. \
         For a problem the engineer has not solved, give hints before solutions.\n\nTHE PAGE:\n{page}"
    );
    let answer = complete(cfg, &system, &b.turns, false).await.map_err(ApiError::Upstream)?;
    Ok(Json(json!({ "answer": answer, "model": cfg.model })))
}

/// What the model suggests for one rubric point.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Suggestion {
    pub mark: String,
    pub why: String,
}

/// Reads the model's review, keeping only the known points and the three marks.
pub fn parse_review(text: &str, points: &[&str]) -> Option<(std::collections::BTreeMap<String, Suggestion>, Vec<String>, String)> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: Value = serde_json::from_str(&text[start..=end]).ok()?;
    let mut out = std::collections::BTreeMap::new();
    for (id, s) in v["points"].as_object()? {
        let mark = s["mark"].as_str().unwrap_or_default();
        if points.contains(&id.as_str()) && ["got", "part", "miss"].contains(&mark) {
            out.insert(id.clone(), Suggestion { mark: mark.to_owned(), why: s["why"].as_str().unwrap_or_default().chars().take(400).collect() });
        }
    }
    let followups = v["followups"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).take(3).collect()).unwrap_or_default();
    let summary = v["summary"].as_str().unwrap_or_default().chars().take(800).collect();
    Some((out, followups, summary))
}

/// `POST /api/sd/attempts/{id}/ai-review`: the interviewer reads the attempt against the rubric and suggests a mark and a reason for
/// each point, two or three follow-up questions, and a short summary. The suggestion is kept with the attempt; the owner's marks
/// stay theirs.
pub async fn review_attempt(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let cfg = config(&s)?;
    let (problem_id, steps): (String, Jsonb<Value>) = sqlx::query_as("SELECT problem_id, steps FROM sd_attempts WHERE id = $1").bind(id).fetch_optional(&s.db).await?.ok_or_else(|| ApiError::NotFound(format!("attempt {id}")))?;
    let p = s.catalog.sd.problems.get(&problem_id).ok_or_else(|| ApiError::NotFound(format!("problem {problem_id}")))?;
    let mut brief = format!("PROBLEM: {}\n{}\n\n", p.title, p.prompt);
    for st in &p.steps {
        let answer = steps.0[&st.name]["answer"].as_str().unwrap_or("").trim();
        brief.push_str(&format!("STEP: {}\nCANDIDATE'S ANSWER:\n{}\nREFERENCE:\n{}\nRUBRIC POINTS:\n", st.name, if answer.is_empty() { "(nothing written)" } else { answer }, st.reference));
        for pt in &st.points {
            brief.push_str(&format!("- {} [{}]: {}\n", pt.id, pt.level, pt.text));
        }
        brief.push('\n');
    }
    let system = "You are a system design interviewer marking a written practice attempt against a rubric. For every rubric point decide \
                  \"got\" (clearly covered), \"part\" (touched but incomplete or vague) or \"miss\" (absent or wrong), judging only what the \
                  candidate wrote, never the reference. Give a one-sentence reason quoting or naming what was or was not said. Then write two or \
                  three follow-up questions a real interviewer would ask about the weakest parts, and a two-sentence summary. Answer with JSON \
                  only: {\"points\": {\"<point id>\": {\"mark\": \"got|part|miss\", \"why\": \"...\"}}, \"followups\": [\"...\"], \"summary\": \"...\"}";
    let text = complete(cfg, system, &[Turn { role: "user".into(), text: brief }], true).await.map_err(ApiError::Upstream)?;
    let ids: Vec<&str> = p.steps.iter().flat_map(|st| st.points.iter().map(|pt| pt.id.as_str())).collect();
    let (points, followups, summary) = parse_review(&text, &ids).ok_or_else(|| ApiError::Upstream("the model's review could not be read".into()))?;
    let review = json!({ "points": points, "followups": followups, "summary": summary, "model": cfg.model });
    sqlx::query("UPDATE sd_attempts SET ai_review = $2 WHERE id = $1").bind(id).bind(Jsonb(&review)).execute(&s.db).await?;
    Ok(Json(review))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_keeps_only_known_points_and_real_marks() {
        let text = r#"Here you go: {"points": {"a": {"mark": "got", "why": "named it"}, "b": {"mark": "maybe", "why": "?"}, "zz": {"mark": "miss", "why": "x"}}, "followups": ["one", "two", "three", "four"], "summary": "ok"}"#;
        let (points, followups, summary) = parse_review(text, &["a", "b"]).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points["a"], Suggestion { mark: "got".into(), why: "named it".into() });
        assert_eq!(followups.len(), 3);
        assert_eq!(summary, "ok");
        assert!(parse_review("no json here", &["a"]).is_none());
    }
}
