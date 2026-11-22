use rupicola_core::visual::{self, Image};
use std::path::Path;
use tempfile::TempDir;

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Image {
    Image {
        width: w,
        height: h,
        rgba: (0..w * h)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect(),
    }
}
/// An image with a filled rectangle: x0..x1, y0..y1 in `fg` on a `bg` background.
fn rect(w: u32, h: u32, bg: [u8; 3], fg: [u8; 3], x0: u32, x1: u32) -> Image {
    let mut im = solid(w, h, bg);
    for y in 0..h {
        for x in x0..x1 {
            let i = ((y * w + x) * 4) as usize;
            im.rgba[i..i + 3].copy_from_slice(&fg);
        }
    }
    im
}
fn put(dir: &Path, name: &str, im: &Image) {
    visual::save(&dir.join(name), im).unwrap();
}

#[test]
fn png_round_trip_and_identical_diff() {
    let dir = TempDir::new().unwrap();
    let im = rect(8, 4, [10, 20, 30], [200, 100, 50], 2, 5);
    put(dir.path(), "a.png", &im);
    let back = visual::load(&dir.path().join("a.png")).unwrap();
    assert_eq!((back.width, back.height, &back.rgba), (8, 4, &im.rgba));
    let (d, _) = visual::diff_images(&im, &back, 0);
    assert_eq!(d.changed_pixels, 0);
    assert!(d.bbox.is_none());
}

#[test]
fn diff_reports_region_ratio_and_dimension_mismatch() {
    let a = solid(10, 10, [0, 0, 0]);
    let b = rect(10, 10, [0, 0, 0], [255, 255, 255], 2, 5);
    let (d, image) = visual::diff_images(&a, &b, 8);
    assert_eq!(d.changed_pixels, 30);
    assert!((d.changed_ratio - 0.3).abs() < 1e-9);
    assert_eq!(d.bbox, Some([2, 0, 4, 9]));
    assert_eq!(
        image.unwrap().rgba[(2 * 4) as usize..(2 * 4 + 3) as usize],
        [255, 0, 0]
    );
    // A change below the threshold is ignored.
    let c = solid(10, 10, [5, 5, 5]);
    assert_eq!(visual::diff_images(&a, &c, 8).0.changed_pixels, 0);
    let wide = solid(12, 10, [0, 0, 0]);
    assert!(visual::diff_images(&a, &wide, 8).0.dimension_mismatch);
}

#[test]
fn frames_detect_a_tween_a_stall_and_a_jump() {
    let tween = TempDir::new().unwrap();
    for (i, x1) in [0, 20, 40, 60, 80, 100, 100].iter().enumerate() {
        put(
            tween.path(),
            &format!("f{i}.png"),
            &rect(100, 10, [0, 0, 0], [255, 255, 255], 0, *x1),
        );
    }
    let ok = visual::analyze_frames(tween.path(), 8).unwrap();
    assert_eq!((ok.first_change, ok.settled_at), (Some(1), Some(5)));
    assert!(ok.stalls.is_empty() && ok.jumps.is_empty(), "{ok:?}");

    let stall = TempDir::new().unwrap();
    for (i, x1) in [0, 20, 40, 40, 80, 100].iter().enumerate() {
        put(
            stall.path(),
            &format!("f{i}.png"),
            &rect(100, 10, [0, 0, 0], [255, 255, 255], 0, *x1),
        );
    }
    assert_eq!(
        visual::analyze_frames(stall.path(), 8).unwrap().stalls,
        vec![3]
    );

    let jump = TempDir::new().unwrap();
    for (i, x1) in [0, 2, 4, 6, 100, 100].iter().enumerate() {
        put(
            jump.path(),
            &format!("f{i}.png"),
            &rect(100, 10, [0, 0, 0], [255, 255, 255], 0, *x1),
        );
    }
    assert_eq!(
        visual::analyze_frames(jump.path(), 8).unwrap().jumps,
        vec![4]
    );

    // Numeric frame order: f2 comes before f10.
    let order = TempDir::new().unwrap();
    for i in [1_u8, 2, 10, 11] {
        put(order.path(), &format!("f{i}.png"), &solid(2, 2, [i, 0, 0]));
    }
    let names: Vec<_> = visual::list_pngs(order.path())
        .unwrap()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["f1.png", "f2.png", "f10.png", "f11.png"]);
    assert!(visual::analyze_frames(order.path(), 8).is_ok());
}

#[test]
fn palette_finds_background_text_and_accent() {
    // 70% white, 20% near-black text, 10% saturated blue.
    let mut im = solid(100, 10, [255, 255, 255]);
    for y in 0_usize..10 {
        for x in 0..100 {
            let i = (y * 100 + x) * 4;
            if x >= 90 {
                im.rgba[i..i + 3].copy_from_slice(&[20, 90, 220]);
            } else if x >= 70 {
                im.rgba[i..i + 3].copy_from_slice(&[17, 17, 17]);
            }
        }
    }
    let p = visual::palette(&im, 6);
    assert_eq!(p.suggested.background.as_deref(), Some("#ffffff"));
    assert_eq!(p.suggested.text.as_deref(), Some("#111111"));
    assert!(p.suggested.text_contrast.unwrap() > 15.0);
    assert_eq!(p.suggested.accent.as_deref(), Some("#145adc"));
    assert!((p.colors[0].share - 0.7).abs() < 0.01);
}

#[test]
fn sheet_lays_out_thumbnails_in_a_grid() {
    let dir = TempDir::new().unwrap();
    for (i, c) in [[255, 0, 0], [0, 255, 0], [0, 0, 255]].iter().enumerate() {
        put(dir.path(), &format!("s{i}.png"), &solid(200, 100, *c));
    }
    let files = visual::list_pngs(dir.path()).unwrap();
    let sheet = visual::sheet(&files, 2, 100).unwrap();
    // Two columns of 100 px thumbnails and two rows of 50 px, with 8 px padding.
    assert_eq!((sheet.width, sheet.height), (2 * 108 + 8, 2 * 58 + 8));
    let first = ((8 * sheet.width + 8) * 4) as usize;
    assert_eq!(&sheet.rgba[first..first + 3], &[255, 0, 0]);
    assert!(visual::sheet(&[], 2, 100).is_err());
}

#[test]
fn matrix_plans_runs_and_records_failures() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src.png");
    visual::save(&src, &solid(4, 4, [1, 2, 3])).unwrap();
    let out = dir.path().join("shots");
    std::fs::create_dir_all(&out).unwrap();
    let spec = visual::MatrixSpec {
        states: vec!["empty".into(), "error".into()],
        themes: vec!["light".into(), "dark".into()],
        scales: vec!["1".into()],
        text_scales: vec!["1".into(), "2".into()],
        sizes: vec!["400x300".into()],
        locales: vec!["en".into()],
    };
    let mut plan = visual::matrix_plan(&spec, &out);
    assert_eq!(plan.len(), 8);
    assert!(
        plan[0]
            .env
            .iter()
            .any(|(k, v)| k == "RUPICOLA_WIDTH" && v == "400")
    );
    // Fails for the dark theme only.
    let cmd = format!(
        "[ \"$RUPICOLA_THEME\" = dark ] && exit 3; cp '{}' \"$RUPICOLA_OUT\"",
        src.display()
    );
    visual::matrix_run(&mut plan, &cmd, std::time::Duration::from_secs(10));
    assert_eq!(plan.iter().filter(|e| e.ok).count(), 4);
    assert!(
        plan.iter()
            .filter(|e| !e.ok)
            .all(|e| e.exit == Some(3) && e.name.contains("dark"))
    );
    // Exit status 0 without writing the file is not a success.
    let mut one = visual::matrix_plan(
        &visual::MatrixSpec {
            states: vec!["x".into()],
            themes: vec!["light".into()],
            scales: vec!["1".into()],
            text_scales: vec!["1".into()],
            sizes: vec!["10x10".into()],
            locales: vec!["en".into()],
        },
        &out.join("none"),
    );
    visual::matrix_run(&mut one, "true", std::time::Duration::from_secs(10));
    assert!(!one[0].ok);
}

fn tree(json: &str) -> visual::Tree {
    serde_json::from_str(json).unwrap()
}

#[test]
fn audit_flags_each_measurable_problem() {
    let t = tree(
        r##"{"window":{"w":400,"h":300},"nodes":[
        {"id":"ok","role":"button","name":"Save","x":10,"y":10,"w":80,"h":32,"interactive":true,"fg":"#000000","bg":"#ffffff"},
        {"id":"small","role":"button","name":"Undo","x":10,"y":60,"w":16,"h":16,"interactive":true},
        {"id":"nameless","role":"button","x":10,"y":90,"w":40,"h":40,"interactive":true},
        {"id":"hint","role":"text","x":10,"y":140,"w":100,"h":20,"fg":"#777777","bg":"#ffffff","font_px":14},
        {"id":"big","role":"text","x":10,"y":170,"w":100,"h":30,"fg":"#777777","bg":"#ffffff","font_px":28},
        {"id":"border","role":"textbox","name":"Email","x":10,"y":210,"w":100,"h":30,"fg":"#bbbbbb","bg":"#ffffff","kind":"boundary"},
        {"id":"cut","role":"text","x":10,"y":250,"w":50,"h":20,"clipped":true},
        {"id":"out","role":"image","name":"Logo","x":380,"y":10,"w":50,"h":20},
        {"id":"o1","role":"button","name":"A","x":200,"y":10,"w":60,"h":30,"interactive":true},
        {"id":"o2","role":"button","name":"B","x":210,"y":10,"w":60,"h":30,"interactive":true},
        {"id":"p1","role":"text","x":300,"y":100,"w":20,"h":20,"parent":"row"},
        {"id":"p2","role":"text","x":302,"y":130,"w":20,"h":20,"parent":"row"}
        ]}"##,
    );
    let rules = |id: &str| -> Vec<&'static str> {
        visual::audit(&t)
            .into_iter()
            .filter(|f| f.node == id)
            .map(|f| f.rule)
            .collect()
    };
    assert_eq!(rules("ok"), [] as [&str; 0]);
    assert_eq!(rules("small"), ["target-size"]);
    assert_eq!(rules("nameless"), ["missing-name"]);
    assert_eq!(rules("hint"), ["contrast"]); // 4.48:1 against 4.5:1
    assert_eq!(rules("big"), [] as [&str; 0]); // large text needs only 3:1
    assert_eq!(rules("border"), ["contrast"]); // 1.92:1 against 3:1
    assert_eq!(rules("cut"), ["clipped"]);
    assert_eq!(rules("out"), ["offscreen"]);
    assert_eq!(rules("o1"), ["overlap"]);
    assert_eq!(rules("p2"), ["near-alignment"]);
}

#[test]
fn audit_rejects_unknown_fields() {
    assert!(serde_json::from_str::<visual::Tree>(r#"{"window":{"w":1,"h":1},"nodes":[{"id":"a","role":"x","x":0,"y":0,"w":1,"h":1,"typo":1}]}"#).is_err());
}

#[test]
fn hex_parsing_and_contrast_match_the_documented_values() {
    use visual::Rgb;
    // WCAG 2 reference values: #777 on white is the classic 4.48:1 gray, and
    // #595959 is the lightest gray that reaches 7:1 on white.
    const AA_GRAY: Rgb = Rgb::gray(0x77);
    const AAA_GRAY: Rgb = Rgb::gray(0x59);
    const WHITE: Rgb = Rgb::gray(u8::MAX);
    const AA_GRAY_CONTRAST: f64 = 4.478;
    const AAA_GRAY_CONTRAST: f64 = 7.0;
    const TOLERANCE: f64 = 0.05;

    assert_eq!("#777".parse::<Rgb>(), Ok(AA_GRAY));
    assert_eq!("#777777".parse::<Rgb>(), Ok(AA_GRAY));
    assert_eq!(AA_GRAY.to_string(), "#777777");
    assert!("#zzzzzz".parse::<Rgb>().is_err());
    assert!((AA_GRAY.contrast(WHITE) - AA_GRAY_CONTRAST).abs() < TOLERANCE);
    assert!((AAA_GRAY.contrast(WHITE) - AAA_GRAY_CONTRAST).abs() < TOLERANCE);
}
