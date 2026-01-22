mod syslog_benchmark;
mod normalized_path;

use criterion::{criterion_group, criterion_main};

criterion_group!(
    benches,
    syslog_benchmark::syslog_benchmark_internal,
    syslog_benchmark::syslog_benchmark_without_stdout_single,
    syslog_benchmark::syslog_benchmark_without_stdout_high_concurrency,
    normalized_path::normalized_path_benchmark,
);

criterion_main!(benches);