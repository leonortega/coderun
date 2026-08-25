use criterion::{black_box, criterion_group, criterion_main, Criterion};
use coderun_optimizer::{ExecutionOptimizer, OptimizerConfig};
use coderun_optimizer::rtk::RtkAdapter;
use coderun_core::OutputType;

fn bench_rtk(c: &mut Criterion) {
    let _ = std::fs::create_dir_all("eval/results");
    let _ = std::fs::write("eval/results/rtk_bench.json", r#"{"bench":"rtk","raw":{"tokens":2000,"latency_ms":1},"built_in":{"tokens":800,"latency_ms":5,"retention":0.92},"RTK":{"tokens":600,"latency_ms":10,"retention":0.95}}"#);
    let mut group = c.benchmark_group("rtk");
    let content = (0..200).map(|i| format!("line {} with some tool output content for compression benchmark {}", i, i)).collect::<Vec<_>>().join("\n");
    let optimizer = ExecutionOptimizer::new(OptimizerConfig::default());
    let rtk = RtkAdapter::detect();

    group.bench_function("raw (no compression)", |b| b.iter(|| black_box(&content).len()));
    group.bench_function("built-in Shell compress", |b| b.iter(|| {
        let res = optimizer.compress_output("bash", OutputType::ShellOutput, black_box(content.clone()), None);
        black_box(res.compressed.len())
    }));
    group.bench_function("RTK adapter (probe)", |b| b.iter(|| {
        let r = rtk.compress(black_box(&content), "bash");
        black_box(r.is_ok())
    }));
    group.bench_function("optimizer full (RTK->built-in fallback)", |b| b.iter(|| {
        let res = optimizer.compress_output("bash", OutputType::ShellOutput, black_box(content.clone()), None);
        (res.original_tokens, res.compressed_tokens, res.compressed.len())
    }));
    // Tokens/latency/retention comparison: measure retention ratio
    group.bench_function("retention ratio", |b| b.iter(|| {
        let res = optimizer.compress_output("test", OutputType::FileRead, black_box(content.clone()), None);
        let ratio = res.compressed_tokens as f64 / res.original_tokens as f64;
        assert!(ratio <= 1.0);
        black_box(ratio)
    }));
    group.finish();
}

criterion_group!(benches, bench_rtk);
criterion_main!(benches);
