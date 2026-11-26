use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub struct Server {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub name: String,
    pub workers: usize,
}

pub fn server(cfg: &HashMap<String, String>) -> Result<Server, String> {
    // An empty value counts as unset, so filter each key before falling back.
    let set = |key: &str| cfg.get(key).filter(|v| !v.is_empty());
    let host = set("host")
        .or_else(|| set("bind"))
        .map_or("127.0.0.1", String::as_str)
        .to_string();
    let port = cfg
        .get("port")
        .ok_or_else(|| "missing port".to_string())
        .and_then(|v| {
            v.parse::<u16>()
                .ok()
                .filter(|&p| p != 0)
                .ok_or_else(|| format!("invalid port: {v}"))
        })?;
    let workers = match cfg.get("workers") {
        None => 1,
        Some(v) => v
            .parse::<usize>()
            .ok()
            .filter(|&w| w > 0)
            .ok_or_else(|| format!("invalid workers: {v}"))?,
    };
    Ok(Server {
        host,
        port,
        tls: cfg.get("tls").is_some_and(|v| v == "on"),
        name: cfg.get("name").cloned().unwrap_or_default(),
        workers,
    })
}
