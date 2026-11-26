use super::*;

#[test]
fn parse_rate_handles_fractions_and_invalid() {
    assert!((parse_rate("60/1").unwrap() - 60.0).abs() < 1e-6);
    assert!((parse_rate("60000/1001").unwrap() - 59.940_059).abs() < 1e-4);
    assert!(parse_rate("0/0").is_none());
    assert!(parse_rate("invalid").is_none());
}

#[test]
fn progressive_sampling_strategies_never_repeat_excluded_frames() {
    let coarse = select_indices(120, 60.0, &FrameSampleStrategy::Coarse { count: 15 });
    assert_eq!(coarse.len(), 15);
    assert_eq!(*coarse.first().unwrap(), 0);
    assert_eq!(*coarse.last().unwrap(), 119);

    let bisect = select_indices(
        120,
        60.0,
        &FrameSampleStrategy::Bisect {
            start_ms: 0,
            end_ms: 2000,
            count: 15,
            exclude_frames: coarse.clone(),
        },
    );
    assert_eq!(bisect.len(), 15);
    for idx in &bisect {
        assert!(
            !coarse.contains(idx),
            "bisect repeated already-read frame {idx}"
        );
    }

    let window = select_indices(
        120,
        60.0,
        &FrameSampleStrategy::Window {
            start_ms: 500,
            end_ms: 700,
            fps: 60,
        },
    );
    assert_eq!(window.first().copied(), Some(30));
    assert_eq!(window.last().copied(), Some(42));
}

#[test]
fn ffmpeg_probe_and_extract_work_on_synthetic_clip() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mp4_path = dir.path().join("clip.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.5:r=30",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&mp4_path)
        .status()
        .unwrap();
    assert!(status.success());

    let info = probe_video_path(&mp4_path, "art-mp4", "video/mp4").unwrap();
    assert_eq!((info.width, info.height), (64, 64));
    assert!(info.frame_count.unwrap_or(0) >= 10);

    let bytes = std::fs::read(&mp4_path).unwrap();
    let extracted = extract_frame_pngs(
        &bytes,
        "video/mp4",
        "art-mp4",
        &FrameSampleStrategy::Coarse { count: 4 },
    )
    .unwrap();
    assert_eq!(extracted.frames.len(), 4);
}

proptest::proptest! {
    #[test]
    fn proptest_coarse_and_bisect_invariants(
        total_frames in 1u64..1000,
        fps in 1.0f64..120.0,
        count in 1u32..80,
        start_ms in 0u64..10_000,
        span_ms in 0u64..10_000,
        exclude in proptest::collection::vec(0u64..1000, 0..30),
    ) {
        let coarse = select_indices(total_frames, fps, &FrameSampleStrategy::Coarse { count });
        proptest::prop_assert!(!coarse.is_empty());
        proptest::prop_assert!(coarse.len() <= (count.clamp(1, 60) as usize));
        proptest::prop_assert!(coarse.windows(2).all(|w| w[0] < w[1]));
        proptest::prop_assert!(coarse.iter().all(|&i| i < total_frames));

        let bisect = select_indices(
            total_frames,
            fps,
            &FrameSampleStrategy::Bisect {
                start_ms,
                end_ms: start_ms.saturating_add(span_ms),
                count,
                exclude_frames: exclude.clone(),
            },
        );
        proptest::prop_assert!(bisect.len() <= (count.clamp(1, 60) as usize));
        proptest::prop_assert!(bisect.windows(2).all(|w| w[0] < w[1]));
        proptest::prop_assert!(bisect.iter().all(|&i| i < total_frames && !exclude.contains(&i)));

        let window = select_indices(
            total_frames,
            fps,
            &FrameSampleStrategy::Window {
                start_ms,
                end_ms: start_ms.saturating_add(span_ms),
                fps: count.clamp(1, 60),
            },
        );
        proptest::prop_assert!(window.len() <= 120);
        proptest::prop_assert!(window.windows(2).all(|w| w[0] < w[1]));
        proptest::prop_assert!(window.iter().all(|&i| i < total_frames));
    }
}
