# V2 Tiered Benchmarks

Use this directory to store locked cutover threshold files per hardware tier.

## Workflow

1. Capture baseline and lock thresholds on each target tier host:

```bash
cargo run -- bench \
  --tier <tier-name> \
  --output-dir target/bench/<tier-name> \
  --lock-thresholds docs/v2/benchmarks/<tier-name>.thresholds.ini
```

2. Validate future runs against the locked thresholds:

```bash
cargo run -- bench \
  --tier <tier-name> \
  --output-dir target/bench/<tier-name> \
  --thresholds docs/v2/benchmarks/<tier-name>.thresholds.ini
```

## Artifact Notes

- Runtime report: `target/bench/<tier-name>/benchmark_report.md`
- Machine-readable metrics: `target/bench/<tier-name>/benchmark_metrics.ini`
- Locked thresholds: `docs/v2/benchmarks/<tier-name>.thresholds.ini`

Threshold files are generated directly from measured baselines and include p50/p95 latency, frame time, and throughput bounds.
