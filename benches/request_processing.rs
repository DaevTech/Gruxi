use std::{sync::Arc, time::Duration};

use criterion::Criterion;
use gruxi::{
    config::cached_configuration::get_cached_configuration,
    core::running_state_manager::get_running_state_manager,
    http::{handle_request::handle_request, request_response::gruxi_request::GruxiRequest},
    runtime::binding_manager::{BindingRuntime, BindingRuntimeConfiguration},
};
use hyper::body::Bytes;
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder as HttpAutoBuilder;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

// Benchmark the full request processing flow, including path normalization and security checks, to see how it performs under realistic conditions.
// This will help identify any bottlenecks in the request handling pipeline and ensure that optimizations in path processing translate to overall performance improvements.
// We hit the handle_request() function
pub fn request_processing_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let running_state_manager = rt.block_on(get_running_state_manager());
    let _running_state = running_state_manager.get_running_state();

    let cached_configuration = get_cached_configuration();
    let config = cached_configuration.get_configuration();

    let binding = config.bindings.first().cloned().unwrap();

    let binding_runtime_configuration = BindingRuntimeConfiguration {
        hard_connection_timeout: Duration::from_secs(30),
    };

    let binding_runtime = BindingRuntime {
        binding: binding,
        config: binding_runtime_configuration,
        tcp_listener: None,
        udp_listener: None,
        stop_binding_token: CancellationToken::new(),
        http_builder: HttpAutoBuilder::new(TokioExecutor::new()),
    };

    let arc_binding_runtime = Arc::new(binding_runtime);

    // Bench with a trusted path that does not require decoding, to test the fast path
    c.bench_function("request_basic_processing_200", |b| {
        let request = hyper::Request::builder().method("GET").uri("http://127.0.0.1/").header("HOST", "127.0.0.1").body(Bytes::new()).unwrap();

        b.iter(|| {
            let gruxi_request = GruxiRequest::new(request.clone());
            let response_result = rt.block_on(handle_request(gruxi_request, arc_binding_runtime.clone()));
            match response_result {
                Ok(response) => {
                    // We can do some basic checks on the response if needed, for now we just ensure it was processed
                    assert!(response.get_status() == 200);
                }
                Err(e) => panic!("Error processing request: {:?}", e),
            }
        });
    });

    c.bench_function("request_basic_processing_404", |b| {
        let request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1/nonexistent")
            .header("HOST", "127.0.0.1")
            .body(Bytes::new())
            .unwrap();
        b.iter(|| {
            let gruxi_request = GruxiRequest::new(request.clone());
            let response_result = rt.block_on(handle_request(gruxi_request, arc_binding_runtime.clone()));
            match response_result {
                Ok(response) => {
                    // We can do some basic checks on the response if needed, for now we just ensure it was processed
                    assert!(response.get_status() == 404);
                }
                Err(e) => panic!("Error processing request: {:?}", e),
            }
        });
    });
}
