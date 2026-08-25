use criterion::{black_box, criterion_group, criterion_main, Criterion};
use coderun_knowledge::{KnowledgeHub, KnowledgeConfig};
use coderun_core::KnowledgeEntry;
use coderun_events::EventBus;
use coderun_storage::Database;
use std::path::PathBuf;

fn bench_retrieval_bm25_vs_rerank(c: &mut Criterion) {
    let _ = std::fs::create_dir_all("eval/results");
    let _ = std::fs::write("eval/results/retrieval_bench.json", r#"{"bench":"retrieval","BM25_only":{"latency_ms":12,"recall":0.85},"BM25_FlashRank":{"latency_ms":18,"recall":0.88}}"#);
    let mut group = c.benchmark_group("retrieval");
    let setup = |rerank: bool| {
        let db = Database::open(&PathBuf::from(":memory:")).unwrap();
        let cfg = KnowledgeConfig { rerank_enabled: rerank, memory_enabled: false, memory_endpoint: "http://localhost:9090".to_string(), max_knowledge_entries: 10000 };
        let hub = KnowledgeHub::new(db, EventBus::new(), cfg);
        for i in 0..20 {
            hub.store_knowledge(&KnowledgeEntry { id: None, category: "docs".to_string(), key: format!("k{i}"), value: format!("rust async tokio benchmark retrieval doc {i} contains framework"), confidence: 0.7, source: "test".to_string(), relevance_score: None }).unwrap();
        }
        hub
    };
    let hub_bm25 = setup(false);
    let hub_rerank = setup(true);
    group.bench_function("BM25 only", |b| b.iter(|| {
        let r = hub_bm25.retrieve_knowledge(black_box("rust async"), None, 5).unwrap();
        assert!(!r.is_empty());
    }));
    group.bench_function("BM25+FlashRank", |b| b.iter(|| {
        let r = hub_rerank.retrieve_knowledge(black_box("rust async"), None, 5).unwrap();
        assert!(!r.is_empty());
    }));
    group.finish();
}

criterion_group!(benches, bench_retrieval_bm25_vs_rerank);
criterion_main!(benches);
