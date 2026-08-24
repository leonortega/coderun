use criterion::{black_box, criterion_group, criterion_main, Criterion};
use coderun_context::{ContextEngine, ContextConfig};
use coderun_knowledge::{KnowledgeHub, KnowledgeConfig};
use coderun_repo_intel::RepositoryIntelligence;
use coderun_events::EventBus;
use coderun_storage::Database;
use std::path::PathBuf;

fn bench_build_context(c: &mut Criterion) {
    let db = Database::open(&PathBuf::from(":memory:")).unwrap();
    let event_bus = EventBus::new();
    let repo_intel = RepositoryIntelligence::new(PathBuf::from("."), Database::open(&PathBuf::from(":memory:")).unwrap(), event_bus.clone());
    let kh = KnowledgeHub::new(db, event_bus.clone(), KnowledgeConfig::default());
    let engine = ContextEngine::new(repo_intel, kh, event_bus, ContextConfig::default());
    let task = coderun_core::TaskRequest { message: "implement auth middleware with rate limiting".to_string(), session_id: "bench".to_string(), context_hints: None };
    c.bench_function("BuildContext p95 <50ms target", |b| b.iter(|| engine.build_context(black_box(&task))));
}

criterion_group!(benches, bench_build_context);
criterion_main!(benches);
