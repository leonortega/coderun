use criterion::{black_box, criterion_group, criterion_main, Criterion};
use coderun_knowledge::{KnowledgeHub, KnowledgeConfig};
use coderun_core::KnowledgeEntry;
use coderun_events::EventBus;
use coderun_storage::Database;
use std::path::PathBuf;

/// Benchmark: BM25 knowledge retrieval (FlashRank removed from v1 per benchmark evaluation).
///
/// FlashRank rationale for removal:
/// ```text
/// Baseline BM25:   Recall@5=16.97%  MRR=0.5003  Latency=507ms
/// + FlashRank:     Recall@5=18.94%  MRR=0.4325  Latency=8532ms
/// ```
/// +1.97pp Recall@5 but -6.78pp MRR and 17x slower — not worth it.
fn bench_retrieval_bm25(c: &mut Criterion) {
    let _ = std::fs::create_dir_all("eval/results");
    let mut group = c.benchmark_group("retrieval");

    let db = Database::open(&PathBuf::from(":memory:")).unwrap();
    let cfg = KnowledgeConfig {
        rerank_enabled: false,
        memory_enabled: false,
        memory_endpoint: "http://localhost:9090".to_string(),
        max_knowledge_entries: 10000,
    };
    let hub = KnowledgeHub::new(db, EventBus::new(), cfg);
    for i in 0..20 {
        hub.store_knowledge(&KnowledgeEntry {
            id: None,
            category: "docs".to_string(),
            key: format!("k{i}"),
            value: format!("rust async tokio benchmark retrieval doc {i} contains framework"),
            confidence: 0.7,
            source: "test".to_string(),
            relevance_score: None,
        })
        .unwrap();
    }

    // Measure actual recall
    let results = hub.retrieve_knowledge("rust async", None, 20, None).unwrap();
    let recall = results.len() as f64 / 20.0;

    let bench_results = serde_json::json!({
        "bench": "retrieval",
        "BM25_only": { "recall": recall, "total_docs": 20, "returned": results.len() },
    });
    let _ = std::fs::write(
        "eval/results/retrieval_bench.json",
        serde_json::to_string_pretty(&bench_results).unwrap(),
    );

    group.bench_function("BM25 only", |b| {
        b.iter(|| {
            let r = hub
                .retrieve_knowledge(black_box("rust async"), None, 5, None)
                .unwrap();
            assert!(!r.is_empty());
        })
    });
    group.finish();
}

criterion_group!(benches, bench_retrieval_bm25);
criterion_main!(benches);
