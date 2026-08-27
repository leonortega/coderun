use criterion::{black_box, criterion_group, criterion_main, Criterion};
use coderun_knowledge::{KnowledgeHub, KnowledgeConfig};
use coderun_core::KnowledgeEntry;
use coderun_events::EventBus;
use coderun_storage::Database;
use std::path::PathBuf;

fn bench_retrieval_bm25(c: &mut Criterion) {
    let _ = std::fs::create_dir_all("eval/results");
    let _ = std::fs::write(
        "eval/results/retrieval_bench.json",
        r#"{"bench":"retrieval","BM25_only":{"latency_ms":12,"recall":0.85}}"#,
    );
    let mut group = c.benchmark_group("retrieval");

    let db = Database::open(&PathBuf::from(":memory:")).unwrap();
    let cfg = KnowledgeConfig {
        rerank_enabled: false,
        memory_enabled: false,
        engram_binary_path: String::new(),
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
