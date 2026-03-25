use crate::core::monitoring::get_monitoring_state;
use opentelemetry::metrics::{AsyncInstrument, MeterProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Encoder, TextEncoder};
use tokio::sync::OnceCell;

struct MetricsExporter {
    registry: prometheus::Registry,
    // Hold the provider to keep instruments alive
    _provider: SdkMeterProvider,
}

impl MetricsExporter {
    fn new() -> Self {
        let registry = prometheus::Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()
            .expect("Failed to build Prometheus exporter");

        let provider = SdkMeterProvider::builder()
            .with_reader(exporter)
            .build();

        let meter = provider.meter("gruxi");

        // Register observable instruments that read from MonitoringState on each scrape

        let _requests_served = meter
            .u64_observable_gauge("gruxi_requests_served_total")
            .with_description("Total number of HTTP requests served")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_requests_served() as u64, &[]);
                }
            })
            .build();

        let _requests_per_sec = meter
            .f64_observable_gauge("gruxi_requests_per_second")
            .with_description("Current requests per second")
            .with_callback(|observer: &dyn AsyncInstrument<f64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_requests_per_sec(), &[]);
                }
            })
            .build();

        let _active_connections = meter
            .u64_observable_gauge("gruxi_active_connections")
            .with_description("Number of currently active connections")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_active_connections() as u64, &[]);
                }
            })
            .build();

        let _uptime_seconds = meter
            .u64_observable_gauge("gruxi_uptime_seconds")
            .with_description("Server uptime in seconds")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_uptime_seconds(), &[]);
                }
            })
            .build();

        let _file_cache_enabled = meter
            .u64_observable_gauge("gruxi_file_cache_enabled")
            .with_description("Whether the file cache is enabled (1) or disabled (0)")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(if state.get_file_cache_enabled() { 1 } else { 0 }, &[]);
                }
            })
            .build();

        let _file_cache_current_items = meter
            .u64_observable_gauge("gruxi_file_cache_current_items")
            .with_description("Current number of items in the file cache")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_file_cache_current_items() as u64, &[]);
                }
            })
            .build();

        let _file_cache_max_items = meter
            .u64_observable_gauge("gruxi_file_cache_max_items")
            .with_description("Maximum number of items the file cache can hold")
            .with_callback(|observer: &dyn AsyncInstrument<u64>| {
                let monitoring = get_monitoring_state_sync();
                if let Some(state) = monitoring {
                    observer.observe(state.get_file_cache_max_items() as u64, &[]);
                }
            })
            .build();

        MetricsExporter {
            registry,
            _provider: provider,
        }
    }
}

/// Cached reference to the monitoring state for synchronous access in OTel callbacks.
static CACHED_MONITORING_STATE: std::sync::OnceLock<&'static crate::core::monitoring::MonitoringState> = std::sync::OnceLock::new();

/// Synchronous helper to get the monitoring state without awaiting.
/// Returns None if monitoring hasn't been initialized yet.
fn get_monitoring_state_sync() -> Option<&'static crate::core::monitoring::MonitoringState> {
    CACHED_MONITORING_STATE.get().copied()
}

/// Must be called after monitoring state is initialized, to cache the reference
/// for synchronous access in OpenTelemetry callbacks.
pub async fn cache_monitoring_state_ref() {
    let state = get_monitoring_state().await;
    let _ = CACHED_MONITORING_STATE.set(state);
}

static METRICS_EXPORTER: OnceCell<MetricsExporter> = OnceCell::const_new();

pub async fn initialize_metrics_exporter() {
    // Cache the monitoring state reference for sync callbacks
    cache_monitoring_state_ref().await;
    // Initialize the exporter singleton
    METRICS_EXPORTER.get_or_init(|| async { MetricsExporter::new() }).await;
}

pub async fn render_metrics() -> String {
    let exporter = METRICS_EXPORTER.get_or_init(|| async { MetricsExporter::new() }).await;

    let encoder = TextEncoder::new();
    let metric_families = exporter.registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
    String::from_utf8(buffer).unwrap_or_default()
}
