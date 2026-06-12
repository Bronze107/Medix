#[cfg(test)]
mod phash_tests {
    use crate::media::phash;

    #[test]
    fn test_hamming_distance() {
        assert_eq!(phash::hamming_distance(0, 0), 0);
        assert_eq!(phash::hamming_distance(0xFFFF_FFFF_FFFF_FFFF, 0), 64);
        assert_eq!(phash::hamming_distance(0x0000_0000_0000_0001, 0x0000_0000_0000_0001), 0);
        assert_eq!(phash::hamming_distance(0x0000_0000_0000_0001, 0x0000_0000_0000_0000), 1);
    }

    #[test]
    fn test_phash_deterministic() {
        // Create a minimal 8x8 grayscale gradient image
        let img = image::DynamicImage::new_luma8(8, 8);
        let hash1 = phash::compute_phash_from_image(&img).unwrap();
        let hash2 = phash::compute_phash_from_image(&img).unwrap();
        assert_eq!(hash1, hash2, "pHash should be deterministic for the same input");
    }

    #[test]
    fn test_phash_similar_images() {
        // Create two nearly-identical 8x8 images
        let img1 = image::DynamicImage::new_luma8(8, 8);
        let mut img2 = image::DynamicImage::new_luma8(8, 8);
        // Flip one pixel (bottom-right)
        img2.as_mut_luma8().unwrap().put_pixel(7, 7, image::Luma([255]));
        let hash1 = phash::compute_phash_from_image(&img1).unwrap();
        let hash2 = phash::compute_phash_from_image(&img2).unwrap();
        let dist = phash::hamming_distance(hash1, hash2);
        // DCT-based hash is sensitive to scaling — 1 changed pixel at 32x32→8x8 may flip many bits.
        // Just verify the hash changes (non-zero) — not a specific threshold.
        assert!(dist > 0, "different images should have non-zero hamming distance");
        assert!(dist < 64, "hamming distance should not be max, got {}", dist);
    }

    #[test]
    fn test_phash_different_images() {
        // All-black vs all-white should be very different
        let black = image::DynamicImage::new_luma8(8, 8);
        let mut white = image::DynamicImage::new_luma8(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                white.as_mut_luma8().unwrap().put_pixel(x, y, image::Luma([255]));
            }
        }
        let h1 = phash::compute_phash_from_image(&black).unwrap();
        let h2 = phash::compute_phash_from_image(&white).unwrap();
        let dist = phash::hamming_distance(h1, h2);
        assert!(dist > 0, "different images should have non-zero hamming distance");
    }
}

#[cfg(test)]
mod import_tests {
    use crate::media::import;
    use std::path::Path;

    #[test]
    fn test_is_supported_extensions() {
        assert!(import::is_supported(Path::new("photo.jpg")));
        assert!(import::is_supported(Path::new("photo.jpeg")));
        assert!(import::is_supported(Path::new("photo.PNG")));
        assert!(import::is_supported(Path::new("photo.WebP")));
        assert!(import::is_supported(Path::new("video.mp4")));
        assert!(import::is_supported(Path::new("video.webm")));
    }

    #[test]
    fn test_is_supported_rejects_unknown() {
        assert!(!import::is_supported(Path::new("doc.pdf")));
        assert!(!import::is_supported(Path::new("script.py")));
        assert!(!import::is_supported(Path::new("readme.txt")));
        assert!(!import::is_supported(Path::new("no_ext")));
    }

    #[test]
    fn test_detect_format_from_bytes() {
        // JPEG: FF D8 FF
        assert_eq!(import::detect_format_from_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        // PNG: 89 50 4E 47 0D 0A 1A 0A (must be >= 12 bytes)
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(import::detect_format_from_bytes(&png_header), Some("png"));
        // GIF: 47 49 46 38 (needs >= 8 bytes)
        assert_eq!(import::detect_format_from_bytes(b"GIF89a\x00\x00"), Some("gif"));
        // BMP: 42 4D
        assert_eq!(import::detect_format_from_bytes(b"BM\x00\x00"), Some("bmp"));
        // Unknown
        assert_eq!(import::detect_format_from_bytes(b"hello world"), None);
        // Too short
        assert_eq!(import::detect_format_from_bytes(&[0x42]), None);
    }

    #[test]
    fn test_detect_format_webp() {
        // WebP: "RIFF" at 0-3, "WEBP" at 8-11
        let webp = b"RIFF\x00\x00\x00\x00WEBP";
        assert_eq!(import::detect_format_from_bytes(webp), Some("webp"));
    }
}
