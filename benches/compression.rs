use criterion::Criterion;
use gruxi::compression::response_compression::compress_content;

pub fn compression_basic(c: &mut Criterion) {
    let content = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(1024).into_bytes();

    c.bench_function("compression_basic", |b| {
        b.iter(|| {
            compress_content(&content).unwrap();
        });
    });
}
