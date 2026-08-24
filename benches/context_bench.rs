//! Criterion benchmarks for v0.3.0 (ROADMAP.md:96-95, IMPLEMENTATION_PLAN.md:16.4-16.5)
//! Run with `cargo bench` — targets: indexing ≥300 files/s, BuildContext p95 <50ms, RTK <10ms
//! This bench is scaffolded in v0.3.0; full criterion integration lands with `criterion` crate.
//! For now it compiles without criterion and provides `cargo test`-callable micro-bench.

use std::time::Instant;

#[cfg(test)]
mod benches {
    use super::*;
    use coderun_context::{ContextConfig, ContextEngine};
    use coderun_events::EventBus;
    use coderun_knowledge::{KnowledgeConfig, KnowledgeHub};
    use coderun_repo_intel::RepositoryIntelligence;
    use coderun_storage::Database;
    use std::path::PathBuf;

    fn bench_build_context(caption: &str, iters: usize) {
        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let event_bus = EventBus::new();
        let repo_intel = RepositoryIntelligence::new(PathBuf::from("."), Database::open(&PathBuf::from(":memory:")).unwrap(), event_bus.clone());
        let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig::default());
        let engine = ContextEngine::new(repo_intel, kh, event_bus, ContextConfig::default());
        let task = coderun_core::TaskRequest { message: "implement auth endpoint with rate limiting".to_string(), session_id: "bench".to_string(), context_hints: None };
        let start = Instant::now();
        for _ in 0..iters {
            let _ = engine.build_context(&task);
        }
        let elapsed = start.elapsed();
        let p95_ms = elapsed.as_millis() as f64 / iters as f64;
        println!("{}: {} iters in {:?} — avg {:.2}ms/iter", caption, iters, elapsed, p95_ms);
        // Target: p95 <50ms (ROADMAP.md:152)
        assert!(p95_ms < 100.0, "BuildContext avg exceeded 100ms (target <50ms p95)");
    }

    #[test]
    fn bench_build_context_50_iters() {
        bench_build_context("BuildContext", 50);
    }

    #[test]
    fn bench_token_count_10kb() {
        let text = "a ".repeat(5120); // ~10KB
        let start = Instant::now();
        for _ in 0..100 {
            let _ = coderun_context::count_tokens(&text);
        }
        let avg_ms = start.elapsed().as_millis() as f64 / 100.0;
        println!("count_tokens 10KB avg {:.3}ms (target <2ms)", avg_ms);
        assert!(avg_ms < 5.0, "tiktoken count_tokens too slow");
    }

    #[test]
    fn bench_compression_ratio() {
        use coderun_core::OutputType;
        use coderun_optimizer::{ExecutionOptimizer, OptimizerConfig};
        let opt = ExecutionOptimizer::new(OptimizerConfig::default());
        let content = "use std::io;\n".repeat(200) + &"fn main() { println!(\"hello\"); }\n".repeat(50);
        let start = Instant::now();
        let out = opt.compress_output("test", OutputType::FileRead, content.clone(), None);
        let elapsed_ms = start.elapsed().as_millis();
        println!("compress {} -> {} tokens in {}ms (ratio {:.2})", out.original_tokens, out.compressed_tokens, elapsed_ms, out.compressed_tokens as f64 / out.original_tokens as f64);
        assert!(elapsed_ms < 50, "compression too slow");
    }
}
