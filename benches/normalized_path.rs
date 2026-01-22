use criterion::Criterion;
use gruxi::file::normalized_path::NormalizedPath;
use rand::Rng;

const RANDOM_WEB_ROOT_LIST: [&str; 5] = ["/var/www", "/usr/share/nginx/html", "/srv/http", "/home/user/public_html", "/opt/web/content"];

const RANDOM_PATH_LIST: [&str; 20] = [
    "/index.html",
    "/about/contact.html",
    "/products/item1.html",
    "/blog/2024/06/post.html",
    "/docs/api/reference.html",
    "/images/logo.png",
    "/css/styles.css",
    "/js/app.js",
    "/downloads/file.zip",
    "/videos/intro.mp4",
    "/scripts/setup.sh",
    "/data/data.json",
    "/fonts/font.woff2",
    "/archive/2023/report.pdf",
    "/media/audio.mp3",
    "/themes/theme.css",
    "/configs/config.yaml",
    "/logs/log.txt",
    "/temp/tempfile.tmp",
    "/extras/extra.html",
];

pub fn normalized_path_benchmark(c: &mut Criterion) {
    c.bench_function("normalized_path_same_web_root_and_path", |b| {
        b.iter(|| NormalizedPath::new("/var/www", "/html/index.html"));
    });

    c.bench_function("normalized_path_random_web_root_and_path", |b| {
        let mut rng = rand::rng();

        b.iter(|| {
            let random_web_root = RANDOM_WEB_ROOT_LIST[rng.random_range(0..RANDOM_WEB_ROOT_LIST.len())];
            let random_path = RANDOM_PATH_LIST[rng.random_range(0..RANDOM_PATH_LIST.len())];
            let _ = NormalizedPath::new(random_web_root, random_path);
        });
    });
}
