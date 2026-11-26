use super::*;

fn solid_img(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage {
    RgbaImage {
        width,
        height,
        data: std::iter::repeat_n(rgba, (width * height) as usize)
            .flatten()
            .collect(),
    }
}

#[test]
fn encode_and_decode_rgba_round_trip() {
    let img = solid_img(12, 8, [10, 120, 240, 255]);
    let bytes = encode_rgba(&img).unwrap();
    let decoded = decode_rgba(&bytes).unwrap();
    assert_eq!((decoded.width, decoded.height), (12, 8));
    assert_eq!(decoded.data, img.data);
}

#[test]
fn identical_images_have_zero_diff_and_no_regions() {
    let a = solid_img(64, 64, [30, 40, 50, 255]);
    let b = solid_img(64, 64, [30, 40, 50, 255]);
    let (_, ratio, regions) = compute_diff(&a, &b, VisualDiffMode::Highlight);
    assert!(ratio.abs() < f64::EPSILON);
    assert_eq!(regions, Vec::new());
}

#[test]
fn localized_change_produces_bounded_region_and_side_by_side_doubles_width() {
    let a = solid_img(64, 64, [20, 20, 20, 255]);
    let mut b = solid_img(64, 64, [20, 20, 20, 255]);
    // Paint one pixel inside tile (1, 1) => x=35, y=35
    let idx = (35 * 64 + 35) * 4;
    b.data[idx..idx + 4].copy_from_slice(&[250, 10, 10, 255]);

    let (hl, ratio, regions) = compute_diff(&a, &b, VisualDiffMode::Highlight);
    assert_eq!((hl.width, hl.height), (64, 64));
    assert!(ratio > 0.0 && ratio < 0.01);
    assert_eq!(
        regions,
        vec![CropRect {
            x: 32,
            y: 32,
            width: 32,
            height: 32,
        }]
    );

    let (sbs, _, _) = compute_diff(&a, &b, VisualDiffMode::SideBySide);
    assert_eq!((sbs.width, sbs.height), (128, 64));
    let (hm, _, _) = compute_diff(&a, &b, VisualDiffMode::Heatmap);
    assert_eq!((hm.width, hm.height), (64, 64));
}

#[test]
fn decode_rgba_rejects_corrupted_truncated_and_decompression_bomb_pngs() {
    assert!(decode_rgba(b"not a png at all").is_err());

    let valid = encode_rgba(&solid_img(8, 8, [1, 2, 3, 255])).unwrap();
    assert!(decode_rgba(&valid[..valid.len() / 2]).is_err());

    // Craft a minimal PNG signature + IHDR claiming 20000x20000 (400M pixels > MAX_PNG_PIXELS)
    let mut bomb = Vec::new();
    bomb.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    bomb.extend_from_slice(&13u32.to_be_bytes());
    bomb.extend_from_slice(b"IHDR");
    bomb.extend_from_slice(&ihdr);
    let mut crc_input = b"IHDR".to_vec();
    crc_input.extend_from_slice(&ihdr);
    let crc = crc32_ieee(&crc_input);
    bomb.extend_from_slice(&crc.to_be_bytes());
    // Append an empty IDAT chunk so `png::Decoder::read_info` completes and hits our pixel guard.
    bomb.extend_from_slice(&0u32.to_be_bytes());
    bomb.extend_from_slice(b"IDAT");
    bomb.extend_from_slice(&crc32_ieee(b"IDAT").to_be_bytes());
    let err = decode_rgba(&bomb)
        .err()
        .expect("oversized PNG must be rejected");
    assert!(err.to_string().contains("exceed safety limit"), "{err}");
}

fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
