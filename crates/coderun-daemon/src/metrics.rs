use std::sync::OnceLock;
use std::time::Instant;

/// Lightweight metrics — no `prometheus` crate dependency for v0.4.0
/// Exposes `GET /metrics` as Prometheus exposition format.
/// Keeps p95 histogram in-memory without external crate; swap to `prometheus` crate later.
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[derive(Default)]
struct Histogram {
    buckets: Vec<(f64, usize)>, // (upper_bound, count)
    sum: f64,
    count: usize,
}

impl Histogram {
    fn new(bounds: Vec<f64>) -> Self {
        Self { buckets: bounds.into_iter().map(|b| (b, 0)).collect(), sum: 0.0, count: 0 }
    }
    fn observe(&mut self, v: f64) {
        self.count += 1;
        self.sum += v;
        for (bound, cnt) in &mut self.buckets {
            if v <= *bound { *cnt += 1; }
        }
    }
    fn exposition(&self, name: &str, help: &str) -> String {
        let mut out = format!("# HELP {} {}\n# TYPE {} histogram\n", name, help, name);
        for (bound, cnt) in &self.buckets {
            out.push_str(&format!("{}{{le=\"{}\"}} {}\n", name, bound, cnt));
        }
        out.push_str(&format!("{}{{le=\"+Inf\"}} {}\n", name, self.count));
        out.push_str(&format!("{}_sum {}\n", name, self.sum));
        out.push_str(&format!("{}_count {}\n", name, self.count));
        out
    }
}

#[derive(Default)]
pub struct Metrics {
    requests_total: Mutex<HashMap<String, usize>>,
    build_duration: Mutex<Histogram>,
    fail_open_total: Mutex<usize>,
    index_files: Mutex<usize>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            requests_total: Mutex::new(HashMap::new()),
            build_duration: Mutex::new(Histogram::new(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 30.0])),
            fail_open_total: Mutex::new(0),
            index_files: Mutex::new(0),
        }
    }

    pub fn inc_requests(&self, hook_type: &str, tier: &str) {
        let key = format!("{}_{}", hook_type, tier);
        if let Ok(mut m) = self.requests_total.lock() { *m.entry(key).or_insert(0) += 1; }
    }

    pub fn observe_build_duration(&self, secs: f64) {
        if let Ok(mut h) = self.build_duration.lock() { h.observe(secs); }
    }

    pub fn inc_fail_open(&self) {
        if let Ok(mut c) = self.fail_open_total.lock() { *c += 1; }
    }

    pub fn set_index_files(&self, n: usize) {
        if let Ok(mut g) = self.index_files.lock() { *g = n; }
    }

    pub fn exposition(&self) -> String {
        let mut out = String::new();
        out.push_str("# HELP coderun_requests_total Total requests by hook+tier\n# TYPE coderun_requests_total counter\n");
        if let Ok(m) = self.requests_total.lock() {
            for (k, v) in m.iter() {
                out.push_str(&format!("coderun_requests_total{{key=\"{}\"}} {}\n", k, v));
            }
        }
        if let Ok(h) = self.build_duration.lock() {
            out.push_str(&h.exposition("coderun_build_context_duration_seconds", "BuildContext duration"));
        }
        if let Ok(c) = self.fail_open_total.lock() {
            out.push_str(&format!("# HELP coderun_fail_open_total Fail-open count\n# TYPE coderun_fail_open_total counter\ncoderun_fail_open_total {}\n", *c));
        }
        if let Ok(g) = self.index_files.lock() {
            out.push_str(&format!("# HELP coderun_index_files Indexed files\n# TYPE coderun_index_files gauge\ncoderun_index_files {}\n", *g));
        }
        // Tier selection histogram is derived from requests_total; keep simple for v0.4.0
        out
    }
}

static GLOBAL: OnceLock<Arc<Metrics>> = OnceLock::new();

pub fn global() -> Arc<Metrics> {
    GLOBAL.get_or_init(|| Arc::new(Metrics::new())).clone()
}

/// RAII timer for BuildContext
pub struct Timer { start: Instant, metrics: Arc<Metrics> }
impl Timer {
    pub fn start() -> Self { Self { start: Instant::now(), metrics: global() } }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let secs = self.start.elapsed().as_secs_f64();
        self.metrics.observe_build_duration(secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_metrics_exposition() {
        let m = Metrics::new();
        m.inc_requests("PreGeneration", "balanced");
        m.observe_build_duration(0.03);
        m.inc_fail_open();
        m.set_index_files(42);
        let exp = m.exposition();
        assert!(exp.contains("coderun_requests_total"));
        assert!(exp.contains("coderun_build_context_duration_seconds"));
        assert!(exp.contains("coderun_fail_open_total"));
        assert!(exp.contains("coderun_index_files 42"));
    }

    #[test]
    fn test_global_singleton() {
        let a = global();
        let b = global();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
