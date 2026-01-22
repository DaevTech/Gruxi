use criterion::Criterion;
use gruxi::logging::syslog::*;
use tokio::runtime::Runtime;

pub fn syslog_benchmark_internal(c: &mut Criterion) {
    let mut syslog = SysLog::new(LogType::Warn, LogType::Off);
    syslog.calculate_enabled_levels();

    c.bench_function("syslog_internal_trace_msg", |b| {
        b.iter(|| {
            syslog.add_log(LogType::Trace, "This is a syslog trace message for benchmarking purposes.".to_string());
        })
    });

    c.bench_function("syslog_internal_warn_msg", |b| {
        b.iter(|| {
            syslog.add_log(LogType::Warn, "This is a syslog warn message for benchmarking purposes.".to_string());
        })
    });
}

pub fn syslog_benchmark_without_stdout_single(c: &mut Criterion) {
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

pub fn syslog_benchmark_without_stdout_high_concurrency(c: &mut Criterion) {
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
