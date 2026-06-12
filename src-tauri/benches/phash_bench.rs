use criterion::{black_box, criterion_group, criterion_main, Criterion};
use medix::media::phash;

/// Benchmark the raw 8x8 DCT function — the hottest numeric code in import.
fn bench_dct_8x8(c: &mut Criterion) {
    // dct_8x8 is private; we benchmark compute_phash_from_image instead,
    // which includes resize + DCT + bit-packing end-to-end.
    // This captures the real-world cost per image.
    let img = image::DynamicImage::new_luma8(8, 8);
    c.bench_function("phash::compute_phash_from_image (8x8)", |b| {
        b.iter(|| phash::compute_phash_from_image(black_box(&img)))
    });

    let img_512 = image::DynamicImage::new_luma8(512, 512);
    c.bench_function("phash::compute_phash_from_image (512x512)", |b| {
        b.iter(|| phash::compute_phash_from_image(black_box(&img_512)))
    });
}

/// Hamming distance is called O(n^2) times during dedup scans.
fn bench_hamming_distance(c: &mut Criterion) {
    let x: u64 = 0xDEAD_BEEF_CAFE_BABE;
    let y: u64 = 0x1234_5678_9ABC_DEF0;
    c.bench_function("phash::hamming_distance", |b| {
        b.iter(|| phash::hamming_distance(black_box(x), black_box(y)))
    });
}

criterion_group!(phash_benches, bench_dct_8x8, bench_hamming_distance);
criterion_main!(phash_benches);
