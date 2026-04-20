use criterion::Criterion;
use gruxi::file::normalized_path::NormalizedPath;

pub fn normalized_path_benchmark(c: &mut Criterion) {
    // Bench with a trusted path that does not require decoding, to test the fast path
    c.bench_function("normalized_path_basic", |b| {
        b.iter(|| NormalizedPath::new("/var/www", "/html/index.html", true));
    });

    // Bench with an untrusted path that requires decoding, to test the full processing
    c.bench_function("normalized_path_decoding_and_cleaning", |b| {
        b.iter(|| NormalizedPath::new("/var/www", "/var/www/html/%2e%2e/%2e%2e/etc/../passwd", false));
    });

    // Bench with an untrusted path that requires decoding, to test the full processing
    c.bench_function("normalized_path_decoding_and_cleaning_with_unicode", |b| {
        b.iter(|| NormalizedPath::new("/var/www", "/var/www/html/%2e%2e/%2e%2e/etc/passwd/%E2%9C%93", false));
    });
}
