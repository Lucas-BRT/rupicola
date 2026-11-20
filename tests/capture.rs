use rupicola::capture::{
    CaptureConfig, CaptureReport, PixelFormat, RamCapture, VideoOptions, VideoReport,
    encode_png_dir, encode_raw_stream,
};
use rupicola::visual::{self, Image};
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Image {
    Image {
        width: w,
        height: h,
        rgba: (0..w * h)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
    }
}

#[test]
fn pixel_format_parsing_and_conversions() {
    assert_eq!(PixelFormat::parse("rgba").unwrap(), PixelFormat::Rgba8);
    assert_eq!(
        PixelFormat::parse("premultiplied-rgba").unwrap(),
        PixelFormat::PremultipliedRgba8
    );
    assert_eq!(PixelFormat::parse("BGRA8").unwrap(), PixelFormat::Bgra8);
    assert_eq!(
        PixelFormat::parse("pm-bgra").unwrap(),
        PixelFormat::PremultipliedBgra8
    );
    assert!(PixelFormat::parse("rgb24").is_err());

    // Straight BGRA -> RGBA
    let bgra = [10u8, 20, 30, 255];
    assert_eq!(
        PixelFormat::Bgra8.to_rgba8(&bgra).unwrap(),
        [30, 20, 10, 255]
    );

    // Premultiplied RGBA -> straight RGBA (transparent, half alpha, opaque)
    let pm = [
        0u8, 0, 0, 0, // fully transparent
        100, 50, 0, 128, // ~50% alpha: ~199, ~100, 0, 128
        12, 34, 56, 255, // opaque
    ];
    let rgba = PixelFormat::PremultipliedRgba8.to_rgba8(&pm).unwrap();
    assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
    assert!((i16::from(rgba[4]) - 199).abs() <= 1);
    assert!((i16::from(rgba[5]) - 100).abs() <= 1);
    assert_eq!(rgba[6], 0);
    assert_eq!(rgba[7], 128);
    assert_eq!(&rgba[8..12], &[12, 34, 56, 255]);

    // Premultiplied BGRA -> straight RGBA
    let pm_bgra = [0u8, 50, 100, 128];
    let rgba_from_bgra = PixelFormat::PremultipliedBgra8.to_rgba8(&pm_bgra).unwrap();
    assert!((i16::from(rgba_from_bgra[0]) - 199).abs() <= 1);
    assert!((i16::from(rgba_from_bgra[1]) - 100).abs() <= 1);
    assert_eq!(rgba_from_bgra[2], 0);
    assert_eq!(rgba_from_bgra[3], 128);

    assert!(PixelFormat::Rgba8.to_rgba8(&[1, 2, 3]).is_err());
}

#[test]
fn ram_capture_records_deterministic_timestamps_timings_pngs_and_analysis() {
    let dir = TempDir::new().unwrap();
    let config = CaptureConfig {
        width: 100,
        height: 10,
        fps: 60.0,
        frame_count: 7,
        pixel_format: PixelFormat::PremultipliedRgba8,
    };

    // Validation errors
    let bad = CaptureConfig { width: 0, ..config };
    assert!(bad.validate().is_err());
    let bad_fps = CaptureConfig {
        fps: f64::NAN,
        ..config
    };
    assert!(bad_fps.validate().is_err());

    let mut seen_timestamps = Vec::new();
    let capture = RamCapture::record(config, |frame, ts_ms, buf| {
        seen_timestamps.push(ts_ms);
        let fill_x = (frame * 20).min(100);
        for y in 0..10 {
            for x in 0..100 {
                let i = (y * 100 + x) * 4;
                if x < fill_x {
                    buf[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                } else {
                    buf[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        Ok(())
    })
    .unwrap();

    assert_eq!(capture.frames.len(), 7);
    assert_eq!(capture.report.frame_count, 7);
    assert!((capture.report.budget_ms - (1000.0 / 60.0)).abs() < 1e-6);
    assert!((seen_timestamps[0] - 0.0).abs() < 1e-9);
    assert!((seen_timestamps[1] - 1000.0 / 60.0).abs() < 1e-6);
    assert!((seen_timestamps[6] - 100.0).abs() < 1e-6);
    assert!(capture.report.max_render_ms >= capture.report.mean_render_ms);
    assert!(capture.report.p95_render_ms <= capture.report.max_render_ms);

    let in_ram = capture.analyze(8).unwrap();
    assert_eq!((in_ram.first_change, in_ram.settled_at), (Some(1), Some(5)));
    assert!(in_ram.stalls.is_empty() && in_ram.jumps.is_empty());

    let saved = capture.save_pngs(dir.path()).unwrap();
    assert_eq!(saved.len(), 7);
    let timing_text = std::fs::read_to_string(dir.path().join("timing.json")).unwrap();
    let parsed_report: CaptureReport = serde_json::from_str(&timing_text).unwrap();
    assert_eq!(parsed_report.frame_count, 7);

    let from_disk = visual::analyze_frames(dir.path(), 8).unwrap();
    assert_eq!(from_disk.first_change, in_ram.first_change);
    assert_eq!(from_disk.settled_at, in_ram.settled_at);
}

#[test]
fn missing_ffmpeg_returns_actionable_error() {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.mp4");
    visual::save(&dir.path().join("f0.png"), &solid(8, 8, [10, 20, 30])).unwrap();

    let opts = VideoOptions {
        ffmpeg_bin: "rupicola-nonexistent-ffmpeg-bin".into(),
        ..VideoOptions::default()
    };
    let err = encode_png_dir(dir.path(), &out, &opts).unwrap_err();
    assert!(err.contains("not found in PATH"), "unexpected error: {err}");
    assert!(err.contains("install ffmpeg"), "unexpected error: {err}");
}

#[test]
fn encode_png_dir_and_raw_stream_handle_odd_dimensions_and_errors() {
    if !ffmpeg_available() {
        return;
    }

    let dir = TempDir::new().unwrap();
    let png_dir = dir.path().join("pngs");
    std::fs::create_dir_all(&png_dir).unwrap();
    // Odd dimensions (33x21) verify yuv420p even-dimension padding (34x22).
    for i in 0..6u8 {
        visual::save(
            &png_dir.join(format!("f{i}.png")),
            &solid(33, 21, [i * 40, 20, 200 - i * 20]),
        )
        .unwrap();
    }

    let mp4 = dir.path().join("from_pngs.mp4");
    let report = encode_png_dir(&png_dir, &mp4, &VideoOptions::default()).unwrap();
    assert_eq!(report.width, 33);
    assert_eq!(report.height, 21);
    assert_eq!(report.encoded_width, 34);
    assert_eq!(report.encoded_height, 22);
    assert_eq!(report.frame_count, 6);
    assert!(report.bytes > 0);
    assert!(mp4.is_file());

    // Dimension mismatch fails cleanly.
    visual::save(&png_dir.join("f99.png"), &solid(40, 21, [0, 0, 0])).unwrap();
    let mismatch = encode_png_dir(&png_dir, &mp4, &VideoOptions::default()).unwrap_err();
    assert!(mismatch.contains("do not match"), "{mismatch}");

    // Raw stream (premultiplied RGBA) -> MP4 + saved lossless PNGs.
    let mut raw_bytes = Vec::new();
    for i in 0..5u8 {
        for _ in 0..(17 * 15) {
            raw_bytes.extend_from_slice(&[i * 40, 100, 50, 255]);
        }
    }
    let raw_mp4 = dir.path().join("from_raw.mp4");
    let saved_dir = dir.path().join("saved_frames");
    let raw_report = encode_raw_stream(
        raw_bytes.as_slice(),
        17,
        15,
        PixelFormat::PremultipliedRgba8,
        &raw_mp4,
        Some(&saved_dir),
        &VideoOptions {
            fps: 30.0,
            ..VideoOptions::default()
        },
    )
    .unwrap();
    assert_eq!(raw_report.frame_count, 5);
    assert_eq!(raw_report.encoded_width, 18);
    assert_eq!(raw_report.encoded_height, 16);
    assert!(raw_report.bytes > 0);
    assert_eq!(visual::list_pngs(&saved_dir).unwrap().len(), 5);
    assert!(visual::analyze_frames(&saved_dir, 8).is_ok());

    // Truncated raw stream fails cleanly.
    let truncated = vec![0u8; 17 * 15 * 4 + 12];
    let trunc_err = encode_raw_stream(
        truncated.as_slice(),
        17,
        15,
        PixelFormat::Rgba8,
        &dir.path().join("trunc.mp4"),
        None,
        &VideoOptions::default(),
    )
    .unwrap_err();
    assert!(trunc_err.contains("truncated raw frame"), "{trunc_err}");
}

#[test]
fn cli_video_subcommand_supports_png_dir_and_raw_stdin() {
    let bin = env!("CARGO_BIN_EXE_rupicola");
    let dir = TempDir::new().unwrap();

    // Validation works even without ffmpeg.
    let missing_width = Command::new(bin)
        .args(["video", "--raw", "--out"])
        .arg(dir.path().join("x.mp4"))
        .output()
        .unwrap();
    assert!(!missing_width.status.success());
    let stderr = String::from_utf8_lossy(&missing_width.stderr);
    assert!(stderr.contains("--width is required"), "{stderr}");

    if !ffmpeg_available() {
        return;
    }

    // PNG directory mode
    let frames = dir.path().join("frames");
    std::fs::create_dir_all(&frames).unwrap();
    for i in 0..4u8 {
        visual::save(
            &frames.join(format!("frame_{i}.png")),
            &solid(16, 16, [i * 60, 0, 255]),
        )
        .unwrap();
    }
    let out_png = dir.path().join("cli_png.mp4");
    let output = Command::new(bin)
        .args(["video"])
        .arg(&frames)
        .args(["--out"])
        .arg(&out_png)
        .args(["--fps", "30", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: VideoReport = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.frame_count, 4);
    assert!(report.bytes > 0);

    // Raw stdin mode + --save-frames
    let out_raw = dir.path().join("cli_raw.mp4");
    let saved = dir.path().join("cli_saved");
    let mut child = Command::new(bin)
        .args([
            "video",
            "--raw",
            "--width",
            "15",
            "--height",
            "13",
            "--fps",
            "60",
            "--pixel-format",
            "bgra",
            "--out",
        ])
        .arg(&out_raw)
        .args(["--save-frames"])
        .arg(&saved)
        .arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for i in 0..3u8 {
            let frame: Vec<u8> = (0..(15 * 13))
                .flat_map(|_| [255, i * 80, 10, 255])
                .collect();
            stdin.write_all(&frame).unwrap();
        }
    }
    let raw_out = child.wait_with_output().unwrap();
    assert!(
        raw_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&raw_out.stderr)
    );
    let raw_report: VideoReport = serde_json::from_slice(&raw_out.stdout).unwrap();
    assert_eq!((raw_report.width, raw_report.height), (15, 13));
    assert_eq!(
        (raw_report.encoded_width, raw_report.encoded_height),
        (16, 14)
    );
    assert_eq!(raw_report.frame_count, 3);
    assert_eq!(visual::list_pngs(&saved).unwrap().len(), 3);
}
