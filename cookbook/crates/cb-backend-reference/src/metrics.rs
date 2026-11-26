//! BLOCK 12 (part): a minimal Prometheus text-format registry (one counter and one histogram family).
//!
//! The one rule that matters: labels must have BOUNDED cardinality. We label by route TEMPLATE
//! (`/v1/tasks/{id}`), never by raw path, and fold unknown HTTP methods into `OTHER`; otherwise a client
//! can create unlimited time series by requesting random URLs.
//! Production alternatives: `metrics` + `metrics-exporter-prometheus`, `prometheus`, OpenTelemetry.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;

/// Upper bounds in seconds. Cumulative, as Prometheus expects.
const BUCKETS: [f64; 8] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.5, 1.0, 5.0];

#[derive(Debug, Default, Clone)]
struct Histogram {
    buckets: [u64; BUCKETS.len()],
    sum: f64,
    count: u64,
}

#[derive(Debug, Default)]
struct Inner {
    requests: BTreeMap<(&'static str, String, u16), u64>,
    latency: BTreeMap<String, Histogram>,
}

#[derive(Debug, Default)]
pub struct Metrics {
    inner: Mutex<Inner>,
}

fn method_label(m: &str) -> &'static str {
    ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].into_iter().find(|k| *k == m).unwrap_or("OTHER")
}

impl Metrics {
    pub fn observe(&self, method: &str, route: &str, status: u16, secs: f64) {
        let mut inner = self.inner.lock().unwrap();
        *inner.requests.entry((method_label(method), route.to_string(), status)).or_insert(0) += 1;
        let h = inner.latency.entry(route.to_string()).or_default();
        for (slot, bound) in h.buckets.iter_mut().zip(BUCKETS) {
            if secs <= bound {
                *slot += 1;
            }
        }
        h.sum += secs;
        h.count += 1;
    }

    #[must_use]
    pub fn render(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let mut out = String::from("# TYPE http_requests_total counter\n");
        for ((method, route, status), n) in &inner.requests {
            let _ =
                writeln!(out, "http_requests_total{{method=\"{method}\",route=\"{route}\",status=\"{status}\"}} {n}");
        }
        out.push_str("# TYPE http_request_duration_seconds histogram\n");
        for (route, h) in &inner.latency {
            let name = "http_request_duration_seconds";
            for (bound, n) in BUCKETS.iter().zip(h.buckets) {
                let _ = writeln!(out, "{name}_bucket{{route=\"{route}\",le=\"{bound}\"}} {n}");
            }
            let _ = writeln!(out, "{name}_bucket{{route=\"{route}\",le=\"+Inf\"}} {}", h.count);
            let _ = writeln!(out, "{name}_sum{{route=\"{route}\"}} {}", h.sum);
            let _ = writeln!(out, "{name}_count{{route=\"{route}\"}} {}", h.count);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_are_cumulative() {
        let m = Metrics::default();
        for s in [0.001, 0.02, 0.3, 10.0] {
            m.observe("GET", "/a", 200, s);
        }
        let text = m.render();
        for line in [
            "http_requests_total{method=\"GET\",route=\"/a\",status=\"200\"} 4",
            "_bucket{route=\"/a\",le=\"0.005\"} 1",
            "_bucket{route=\"/a\",le=\"0.025\"} 2",
            "_bucket{route=\"/a\",le=\"0.5\"} 3",
            "_bucket{route=\"/a\",le=\"5\"} 3",
            "_bucket{route=\"/a\",le=\"+Inf\"} 4",
            "_count{route=\"/a\"} 4",
        ] {
            assert!(text.contains(line), "missing {line} in:\n{text}");
        }
    }

    #[test]
    fn unknown_methods_do_not_create_new_series() {
        let m = Metrics::default();
        for i in 0..50 {
            m.observe(&format!("X{i}"), "unmatched", 405, 0.0);
        }
        assert!(m.render().contains("method=\"OTHER\",route=\"unmatched\",status=\"405\"} 50"));
    }
}
