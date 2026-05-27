use criterion::Criterion;
use gruxi::compression::response_compression::compress_content;

pub fn compression_basic(c: &mut Criterion) {
    let content = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(1024).into_bytes();
    let mut gzip_content = Vec::with_capacity(content.len());

    c.bench_function("compression_basic", |b| {
        b.iter(|| {
            gzip_content.clear();
            compress_content(&content, &mut gzip_content).unwrap();
        });
    });
}
