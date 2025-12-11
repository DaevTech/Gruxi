#[cfg(target_os = "linux")]
mod bench {

    use criterion::{Criterion, criterion_group, criterion_main};
    use grux::logging::syslog::*;
    use hyper::body::Body;
    use hyper::client::Client;
    use hyper::client::connect::HttpConnector;
    use hyper::http::{Request, Response};
    use hyper::rt::TokioExecutor;
    use std::fs::File;
    use tokio::runtime::Runtime;

    async fn request_benchmark_concurrency() {
        let connector = HttpConnector::new();
        let client: Client<_, _> = Client::builder(TokioExecutor::new()).build(connector);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                tokio::spawn(async {
                    let request = Request::builder().method("GET").uri("http://127.0.0.1/").body(()).unwrap();
                    let _response: Response<Body> = client.request(request).await.unwrap();
                    println!("Status: {}", _response.status());
                })
            })
            .collect();
        futures::future::join_all(handles).await;
    }

    fn request_benchmark_with_high_concurrency(c: &mut Criterion) {
        /*    let guard = pprof::ProfilerGuardBuilder::default()
                .frequency(1000)
                .blocklist(&["libc", "libgcc", "pthread", "vdso"])
                .build()
                .unwrap();
        */
        let rt = Runtime::new().unwrap();

        // Create a configuration site and binding for testing
        let default_site = Site {
            id: 1,
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-default".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            enabled_handlers: vec![], // No specific handlers enabled by default
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };

        let binding = grux::configuration::binding::Binding {
            id: 1,
            ip: "0.0.0.0".to_string(),
            port: 8080,
            is_tls: false,
            is_admin: false,
            sites: vec![default_site],
        };

        // Start the http server
        rt.block_on(async {
            grux::http::http_server::start_server_binding(binding).await;
        });

        c.bench_function("syslog_error", |b| {
            b.iter(|| rt.block_on({ request_benchmark_concurrency() }));
        });
        /*
        if let Ok(report) = guard.report().build() {
            let file = File::create("flamegraph.svg").unwrap();
            report.flamegraph(file).unwrap();
        };*/
    }

    criterion_group!(benches, request_benchmark_with_high_concurrency);
    criterion_main!(benches);
}
