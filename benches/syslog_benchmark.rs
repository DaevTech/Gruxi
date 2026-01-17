use criterion::{Criterion, criterion_group, criterion_main};
use gruxi::logging::syslog::*;
use tokio::runtime::Runtime;

fn syslog_benchmark_without_stdout_single(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Initialize the syslog
        SysLog::set_new_stdout_log_level(LogType::Warn);
        SYS_LOG.write().unwrap().calculate_enabled_levels();
    });

    c.bench_function("syslog_error", |b| {
        b.iter(|| {
            info("This is a syslog error message for benchmarking purposes.");
        })
    });
}

async fn syslog_benchmark_concurrency() {
    let handles: Vec<_> = (0..1000).map(|_| tokio::spawn(async { info("This is a syslog error message for benchmarking purposes.") })).collect();
    futures::future::join_all(handles).await;
}

fn syslog_benchmark_without_stdout_high_concurrency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Initialize the syslog
        SysLog::set_new_stdout_log_level(LogType::Warn);
        SYS_LOG.write().unwrap().calculate_enabled_levels();
    });

    c.bench_function("syslog_error", |b| {
        b.iter(|| rt.block_on(syslog_benchmark_concurrency()));
    });
}

criterion_group!(benches, syslog_benchmark_without_stdout_single, syslog_benchmark_without_stdout_high_concurrency);
criterion_main!(benches);
