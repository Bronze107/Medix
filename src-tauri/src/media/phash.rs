use std::path::Path;

/// Compute 64-bit perceptual hash (pHash) of an image file.
pub fn compute_phash(path: &Path) -> Option<u64> {
    let img = image::open(path).ok()?;
    compute_phash_from_image(&img)
}

/// Compute pHash from an already-decoded image, avoiding re-decode.
pub fn compute_phash_from_image(img: &image::DynamicImage) -> Option<u64> {
    let gray = img.grayscale();

    // Resize to 32x32 then 8x8 for DCT
    let small = image::imageops::resize(&gray, 32, 32, image::imageops::FilterType::Lanczos3);
    let tiny = image::imageops::resize(&small, 8, 8, image::imageops::FilterType::Lanczos3);

    // Get pixel values (Luma<u8> -> f64)
    let pixels: Vec<f64> = tiny
        .pixels()
        .map(|p| p.0[0] as f64)
        .collect();

    // Compute DCT for 8x8
    let dct = dct_8x8(&pixels);

    // Compute mean of top-left 7x7 (excluding [0][0] which is DC component)
    let mut sum = 0.0;
    let mut count = 0;
    for y in 0..8 {
        for x in 0..8 {
            if x == 0 && y == 0 {
                continue;
            }
            sum += dct[y * 8 + x];
            count += 1;
        }
    }
    let mean = sum / count as f64;

    // Build hash: bit = 1 if DCT > mean
    let mut hash: u64 = 0;
    for y in 0..8 {
        for x in 0..8 {
            if x == 0 && y == 0 {
                continue;
            }
            if dct[y * 8 + x] > mean {
                hash |= 1;
            }
            hash = hash.rotate_left(1);
        }
    }

    Some(hash)
}

fn dct_8x8(pixels: &[f64]) -> Vec<f64> {
    let n = 8;
    let mut result = vec![0.0; n * n];

    for u in 0..n {
        for v in 0..n {
            let mut sum = 0.0;
            for x in 0..n {
                for y in 0..n {
                    let pixel = pixels[y * n + x];
                    let cx = (std::f64::consts::PI * u as f64 * (2.0 * x as f64 + 1.0)) / (2.0 * n as f64);
                    let cy = (std::f64::consts::PI * v as f64 * (2.0 * y as f64 + 1.0)) / (2.0 * n as f64);
                    sum += pixel * cx.cos() * cy.cos();
                }
            }
            let cu = if u == 0 { 1.0 / (2.0_f64).sqrt() } else { 1.0 };
            let cv = if v == 0 { 1.0 / (2.0_f64).sqrt() } else { 1.0 };
            result[v * n + u] = (2.0 / n as f64) * cu * cv * sum;
        }
    }
    result
}

/// Hamming distance between two u64 hashes.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}
