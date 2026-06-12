use criterion::{black_box, criterion_group, criterion_main, Criterion};
use medix::media::import;

/// Benchmark magic-byte format detection — called on every file during import.
fn bench_detect_format(c: &mut Criterion) {
    let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01];
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D];
    let unknown = b"this is not an image file at all";

    c.bench_function("import::detect_format_from_bytes (JPEG)", |b| {
        b.iter(|| import::detect_format_from_bytes(black_box(&jpeg_header)))
    });

    c.bench_function("import::detect_format_from_bytes (PNG)", |b| {
        b.iter(|| import::detect_format_from_bytes(black_box(&png_header)))
    });

    c.bench_function("import::detect_format_from_bytes (unknown)", |b| {
        b.iter(|| import::detect_format_from_bytes(black_box(unknown)))
    });
}

/// Benchmark extension-based file type check — called during directory scanning.
fn bench_is_supported(c: &mut Criterion) {
    c.bench_function("import::is_supported (jpg)", |b| {
        b.iter(|| import::is_supported(black_box(std::path::Path::new("photo.jpg"))))
    });

    c.bench_function("import::is_supported (mp4)", |b| {
        b.iter(|| import::is_supported(black_box(std::path::Path::new("video.mp4"))))
    });

    c.bench_function("import::is_supported (unsupported)", |b| {
        b.iter(|| import::is_supported(black_box(std::path::Path::new("doc.pdf"))))
    });
}

criterion_group!(import_benches, bench_detect_format, bench_is_supported);
criterion_main!(import_benches);
