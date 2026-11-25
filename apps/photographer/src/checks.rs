//! UI-tree checks run by `rupicola-photographer` on each captured checkpoint snapshot.

use rupicola_api::{Checks, ContrastLevel};
use rupicola_protocol::{Rect, UiSnapshot, WidgetKind, WidgetNode};
use std::fmt::Write as _;

const OVERFLOW_TOLERANCE_PX: f64 = 1.0;
const MAX_REPORTED_VIOLATIONS: usize = 5;
const LARGE_TEXT_PX: f64 = 24.0;
const WCAG_AA_NORMAL: f64 = 4.5;
const WCAG_AA_LARGE: f64 = 3.0;
const WCAG_AAA_NORMAL: f64 = 7.0;
const WCAG_AAA_LARGE: f64 = 4.5;

fn is_interactive(kind: WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Button
            | WidgetKind::Link
            | WidgetKind::TextBox
            | WidgetKind::Checkbox
            | WidgetKind::Radio
            | WidgetKind::Switch
            | WidgetKind::Slider
            | WidgetKind::ComboBox
            | WidgetKind::Tab
            | WidgetKind::MenuItem
    )
}

fn describe_node(node: &WidgetNode) -> String {
    match node
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        Some(label) => format!("{} `{label}` (#{})", node.role, node.id),
        None => format!("{} (#{})", node.role, node.id),
    }
}

fn summarize_list(items: &[String]) -> String {
    let shown: Vec<&str> = items
        .iter()
        .take(MAX_REPORTED_VIOLATIONS)
        .map(String::as_str)
        .collect();
    let mut out = shown.join(", ");
    if items.len() > MAX_REPORTED_VIOLATIONS {
        let _ = write!(out, " (+{} more)", items.len() - MAX_REPORTED_VIOLATIONS);
    }
    out
}

fn collect_visible<'a>(
    node: &'a WidgetNode,
    out: &mut Vec<(&'a WidgetNode, Vec<usize>)>,
    path: &[usize],
) {
    if !node.state.visible {
        return;
    }
    let idx = out.len();
    out.push((node, path.to_vec()));
    for (child_i, child) in node.children.iter().enumerate() {
        let mut child_path = Vec::with_capacity(path.len() + 1);
        child_path.extend_from_slice(path);
        child_path.push(child_i);
        let _ = idx;
        collect_visible(child, out, &child_path);
    }
}

fn matches_simple_selector(node: &WidgetNode, selector: &str) -> bool {
    let sel = selector.trim();
    if let Some(cls) = sel.strip_prefix('.') {
        return node
            .style_hint
            .as_deref()
            .is_some_and(|hint| hint.split_whitespace().any(|t| t == cls));
    }
    if let Some(id) = sel.strip_prefix('#') {
        return node.id == id;
    }
    node.role.eq_ignore_ascii_case(sel)
}

fn matches_selector_chain(
    nodes: &[(&WidgetNode, Vec<usize>)],
    index: usize,
    selector: &str,
) -> bool {
    // Support basic descendant/child chains like `.diff-side > div` or `.card button`.
    let parts: Vec<&str> = selector
        .split('>')
        .flat_map(str::split_whitespace)
        .filter(|s| !s.is_empty())
        .collect();
    let Some(last) = parts.last() else {
        return false;
    };
    let (node, path) = &nodes[index];
    if !matches_simple_selector(node, last) {
        return false;
    }
    if parts.len() == 1 {
        return true;
    }
    let mut current_path = path.as_slice();
    for part in parts[..parts.len() - 1].iter().rev() {
        let found = nodes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (cand, cand_path))| {
                cand_path.len() < current_path.len()
                    && current_path.starts_with(cand_path)
                    && matches_simple_selector(cand, part)
            });
        let Some((_, (_, parent_path))) = found else {
            return false;
        };
        current_path = parent_path.as_slice();
    }
    true
}

fn rects_overlap_strictly(a: Rect, b: Rect) -> bool {
    let ax2 = a.x + a.width;
    let ay2 = a.y + a.height;
    let bx2 = b.x + b.width;
    let by2 = b.y + b.height;
    a.x < bx2 && ax2 > b.x && a.y < by2 && ay2 > b.y
}

#[derive(Clone, Copy, Debug)]
struct Rgba {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

impl Rgba {
    fn over(self, bg: Self) -> Self {
        let a = self.a.clamp(0.0, 1.0);
        let inv = 1.0 - a;
        Self {
            r: self.r * a + bg.r * inv,
            g: self.g * a + bg.g * inv,
            b: self.b * a + bg.b * inv,
            a: 1.0,
        }
    }

    fn relative_luminance(self) -> f64 {
        fn linear(channel: f64) -> f64 {
            let v = (channel / 255.0).clamp(0.0, 1.0);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    fn contrast_ratio(self, other: Self) -> f64 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
    }
}

fn parse_css_color(raw: &str) -> Option<Rgba> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        let bytes = hex.as_bytes();
        let hex_u8 = |hi: u8, lo: u8| -> Option<f64> {
            let buf = [hi, lo];
            let pair = std::str::from_utf8(&buf).ok()?;
            u8::from_str_radix(pair, 16).ok().map(f64::from)
        };
        let hex_nibble = |ch: u8| -> Option<f64> {
            let buf = [ch];
            let digit = std::str::from_utf8(&buf).ok()?;
            u8::from_str_radix(digit, 16)
                .ok()
                .map(|v| f64::from(v * 17))
        };
        return match bytes.len() {
            3 => Some(Rgba {
                r: hex_nibble(bytes[0])?,
                g: hex_nibble(bytes[1])?,
                b: hex_nibble(bytes[2])?,
                a: 1.0,
            }),
            6 => Some(Rgba {
                r: hex_u8(bytes[0], bytes[1])?,
                g: hex_u8(bytes[2], bytes[3])?,
                b: hex_u8(bytes[4], bytes[5])?,
                a: 1.0,
            }),
            8 => Some(Rgba {
                r: hex_u8(bytes[0], bytes[1])?,
                g: hex_u8(bytes[2], bytes[3])?,
                b: hex_u8(bytes[4], bytes[5])?,
                a: hex_u8(bytes[6], bytes[7])? / 255.0,
            }),
            _ => None,
        };
    }

    let inner = trimmed
        .strip_prefix("rgba(")
        .or_else(|| trimmed.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let red: f64 = parts[0].parse().ok()?;
    let green: f64 = parts[1].parse().ok()?;
    let blue: f64 = parts[2].parse().ok()?;
    let alpha: f64 = if parts.len() == 4 {
        parts[3].parse().ok()?
    } else {
        1.0
    };
    Some(Rgba {
        r: red,
        g: green,
        b: blue,
        a: alpha,
    })
}

fn resolve_background(nodes: &[(&WidgetNode, Vec<usize>)], index: usize) -> Option<Rgba> {
    let path = &nodes[index].1;
    let mut layers = Vec::new();
    for (cand, cand_path) in nodes[..=index].iter().rev() {
        if !path.starts_with(cand_path) {
            continue;
        }
        let Some(bg_raw) = cand
            .style
            .as_ref()
            .and_then(|s| s.background_color.as_deref())
        else {
            continue;
        };
        let Some(rgba) = parse_css_color(bg_raw) else {
            continue;
        };
        if rgba.a <= 0.0 {
            continue;
        }
        layers.push(rgba);
        if rgba.a >= 1.0 {
            break;
        }
    }
    let mut acc = *layers.last()?;
    for layer in layers.iter().rev().skip(1) {
        acc = layer.over(acc);
    }
    (acc.a > 0.0).then_some(acc)
}

fn has_visible_text(node: &WidgetNode) -> bool {
    node.label.as_deref().is_some_and(|l| !l.trim().is_empty())
        || node.value.as_deref().is_some_and(|v| !v.trim().is_empty())
}

fn min_contrast_ratio(level: ContrastLevel, font_size_px: Option<f64>) -> f64 {
    let large = font_size_px.is_some_and(|px| px >= LARGE_TEXT_PX);
    match (level, large) {
        (ContrastLevel::Aa, false) => WCAG_AA_NORMAL,
        (ContrastLevel::Aa, true) => WCAG_AA_LARGE,
        (ContrastLevel::Aaa, false) => WCAG_AAA_NORMAL,
        (ContrastLevel::Aaa, true) => WCAG_AAA_LARGE,
    }
}

/// Evaluates the tree-based checks in `checks` against `snapshot`, returning finding notes.
#[must_use]
pub fn evaluate_checks(checks: &Checks, checkpoint_id: &str, snapshot: &UiSnapshot) -> Vec<String> {
    let mut nodes = Vec::new();
    collect_visible(&snapshot.root, &mut nodes, &[]);
    let mut findings = Vec::new();

    if checks.no_horizontal_overflow {
        let limit = f64::from(snapshot.viewport[0]) + OVERFLOW_TOLERANCE_PX;
        let overflowing: Vec<String> = nodes
            .iter()
            .filter(|(n, _)| n.bounds.width > 0.0 && n.bounds.height > 0.0)
            .filter(|(n, _)| n.bounds.x + n.bounds.width > limit)
            .map(|(n, _)| {
                format!(
                    "{} extends to {:.0}px (viewport {}px)",
                    describe_node(n),
                    n.bounds.x + n.bounds.width,
                    snapshot.viewport[0]
                )
            })
            .collect();
        if !overflowing.is_empty() {
            findings.push(format!(
                "check `no_horizontal_overflow` failed at `{checkpoint_id}`: {}",
                summarize_list(&overflowing)
            ));
        }
    }

    if let Some(min_size) = checks.min_target_size {
        let min_px = f64::from(min_size);
        let too_small: Vec<String> = nodes
            .iter()
            .filter(|(n, _)| {
                is_interactive(n.kind) && n.bounds.width > 0.0 && n.bounds.height > 0.0
            })
            .filter(|(n, _)| n.bounds.width < min_px || n.bounds.height < min_px)
            .map(|(n, _)| {
                format!(
                    "{} is {:.0}x{:.0}px (< {min_size}px)",
                    describe_node(n),
                    n.bounds.width,
                    n.bounds.height
                )
            })
            .collect();
        if !too_small.is_empty() {
            findings.push(format!(
                "check `min_target_size` ({min_size}px) failed at `{checkpoint_id}`: {}",
                summarize_list(&too_small)
            ));
        }
    }

    if checks.accessible_names {
        let unnamed: Vec<String> = nodes
            .iter()
            .filter(|(n, _)| {
                is_interactive(n.kind) && n.bounds.width > 0.0 && n.bounds.height > 0.0
            })
            .filter(|(n, _)| n.label.as_deref().is_none_or(|l| l.trim().is_empty()))
            .map(|(n, _)| describe_node(n))
            .collect();
        if !unnamed.is_empty() {
            findings.push(format!(
                "check `accessible_names` failed at `{checkpoint_id}`: {}",
                summarize_list(&unnamed)
            ));
        }
    }

    for selector in &checks.no_overlap {
        let matched: Vec<usize> = (0..nodes.len())
            .filter(|&i| {
                let (n, _) = nodes[i];
                n.bounds.width > 0.0
                    && n.bounds.height > 0.0
                    && matches_selector_chain(&nodes, i, selector)
            })
            .collect();
        let mut overlaps = Vec::new();
        for (pos, &i) in matched.iter().enumerate() {
            for &j in &matched[pos + 1..] {
                let (na, pa) = &nodes[i];
                let (nb, pb) = &nodes[j];
                if pa.starts_with(pb) || pb.starts_with(pa) {
                    continue;
                }
                if rects_overlap_strictly(na.bounds, nb.bounds) {
                    overlaps.push(format!("{} vs {}", describe_node(na), describe_node(nb)));
                }
            }
        }
        if !overlaps.is_empty() {
            findings.push(format!(
                "check `no_overlap` (`{selector}`) failed at `{checkpoint_id}`: {}",
                summarize_list(&overlaps)
            ));
        }
    }

    if let Some(level) = checks.contrast
        && let Some(finding) = check_contrast(&nodes, level, checkpoint_id)
    {
        findings.push(finding);
    }

    findings
}

fn is_contrast_text_node(node: &WidgetNode) -> bool {
    if matches!(
        node.kind,
        WidgetKind::Slider
            | WidgetKind::Checkbox
            | WidgetKind::Radio
            | WidgetKind::Switch
            | WidgetKind::Image
    ) {
        return false;
    }
    if !node.children.is_empty()
        && matches!(
            node.kind,
            WidgetKind::Group | WidgetKind::List | WidgetKind::Dialog | WidgetKind::Other
        )
    {
        return false;
    }
    has_visible_text(node)
}

fn check_contrast(
    nodes: &[(&WidgetNode, Vec<usize>)],
    level: ContrastLevel,
    checkpoint_id: &str,
) -> Option<String> {
    let mut low_contrast = Vec::new();
    for (i, (node, _)) in nodes.iter().enumerate() {
        if node.bounds.width <= 0.0 || node.bounds.height <= 0.0 || !is_contrast_text_node(node) {
            continue;
        }
        let Some(style) = node.style.as_ref() else {
            continue;
        };
        let Some(fg_raw) = style.color.as_deref() else {
            continue;
        };
        let Some(fg) = parse_css_color(fg_raw).filter(|c| c.a > 0.0) else {
            continue;
        };
        let Some(bg) = resolve_background(nodes, i) else {
            continue;
        };
        let ratio = fg.over(bg).contrast_ratio(bg);
        let min_ratio = min_contrast_ratio(level, style.font_size_px);
        if ratio < min_ratio {
            low_contrast.push(format!(
                "{} contrast {ratio:.2}:1 (< {min_ratio:.1}:1)",
                describe_node(node)
            ));
        }
    }
    (!low_contrast.is_empty()).then(|| {
        format!(
            "check `contrast` ({}) failed at `{checkpoint_id}`: {}",
            level.as_str(),
            summarize_list(&low_contrast)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_protocol::{SnapshotSource, ToolkitKind, WidgetState};

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn node(
        id: &str,
        role: &str,
        kind: WidgetKind,
        label: Option<&str>,
        bounds: Rect,
        style_hint: Option<&str>,
        children: Vec<WidgetNode>,
    ) -> WidgetNode {
        WidgetNode {
            id: id.to_owned(),
            role: role.to_owned(),
            kind,
            label: label.map(str::to_owned),
            value: None,
            bounds,
            state: WidgetState {
                visible: true,
                ..WidgetState::default()
            },
            style_hint: style_hint.map(str::to_owned),
            style: None,
            children,
        }
    }

    fn snapshot(width: u32, children: Vec<WidgetNode>) -> UiSnapshot {
        UiSnapshot {
            protocol_version: 1,
            app_id: "test".to_owned(),
            toolkit: ToolkitKind::WebDom,
            title: None,
            viewport: [width, 800],
            scale_factor: 1.0,
            timestamp_ms: 0,
            source: SnapshotSource::Dom,
            root: node(
                "root",
                "body",
                WidgetKind::Group,
                None,
                rect(0.0, 0.0, f64::from(width), 800.0),
                None,
                children,
            ),
        }
    }

    #[test]
    fn passes_when_all_tree_checks_are_satisfied() {
        let snap = snapshot(
            390,
            vec![node(
                "btn",
                "button",
                WidgetKind::Button,
                Some("Menu"),
                rect(10.0, 10.0, 44.0, 44.0),
                Some("nav-btn"),
                Vec::new(),
            )],
        );
        let checks = Checks {
            no_horizontal_overflow: true,
            min_target_size: Some(24),
            no_overlap: vec![".nav-btn".to_owned()],
            accessible_names: true,
            contrast: None,
        };
        assert_eq!(evaluate_checks(&checks, "full", &snap).len(), 0);
    }

    #[test]
    fn reports_overflow_small_target_missing_name_and_overlap() {
        let snap = snapshot(
            390,
            vec![
                node(
                    "wide",
                    "div",
                    WidgetKind::Group,
                    None,
                    rect(0.0, 0.0, 433.0, 40.0),
                    Some("box"),
                    Vec::new(),
                ),
                node(
                    "tiny",
                    "button",
                    WidgetKind::Button,
                    None,
                    rect(10.0, 10.0, 16.0, 16.0),
                    Some("box"),
                    Vec::new(),
                ),
            ],
        );
        let checks = Checks {
            no_horizontal_overflow: true,
            min_target_size: Some(24),
            no_overlap: vec![".box".to_owned()],
            accessible_names: true,
            contrast: None,
        };
        let findings = evaluate_checks(&checks, "full", &snap);
        assert_eq!(findings.len(), 4, "{findings:?}");
        assert!(findings[0].contains("no_horizontal_overflow"));
        assert!(findings[1].contains("min_target_size"));
        assert!(findings[2].contains("accessible_names"));
        assert!(findings[3].contains("no_overlap"));
    }

    #[test]
    fn contrast_check_evaluates_aa_aaa_and_inherited_background() {
        use rupicola_protocol::ComputedStyle;

        let mut parent = node(
            "card",
            "div",
            WidgetKind::Group,
            None,
            rect(0.0, 0.0, 300.0, 200.0),
            None,
            vec![
                {
                    let mut ok_btn = node(
                        "ok",
                        "button",
                        WidgetKind::Button,
                        Some("Save"),
                        rect(10.0, 10.0, 80.0, 36.0),
                        None,
                        Vec::new(),
                    );
                    ok_btn.style = Some(ComputedStyle {
                        color: Some("rgb(255, 255, 255)".to_owned()),
                        background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                        font_size_px: Some(16.0),
                        ..ComputedStyle::default()
                    });
                    ok_btn
                },
                {
                    // #777777 on #09090b is ~4.4:1: fails AA normal (4.5), passes AA large (3.0)
                    let mut muted_small = node(
                        "muted-small",
                        "span",
                        WidgetKind::Text,
                        Some("Caption"),
                        rect(10.0, 60.0, 80.0, 20.0),
                        None,
                        Vec::new(),
                    );
                    muted_small.style = Some(ComputedStyle {
                        color: Some("#777777".to_owned()),
                        background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                        font_size_px: Some(14.0),
                        ..ComputedStyle::default()
                    });
                    muted_small
                },
                {
                    let mut muted_large = node(
                        "muted-large",
                        "h1",
                        WidgetKind::Heading,
                        Some("Heading"),
                        rect(10.0, 90.0, 200.0, 32.0),
                        None,
                        Vec::new(),
                    );
                    muted_large.style = Some(ComputedStyle {
                        color: Some("#777777".to_owned()),
                        background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                        font_size_px: Some(24.0),
                        ..ComputedStyle::default()
                    });
                    muted_large
                },
            ],
        );
        parent.style = Some(ComputedStyle {
            background_color: Some("rgb(9, 9, 11)".to_owned()),
            ..ComputedStyle::default()
        });

        let snap = snapshot(390, vec![parent]);
        let aa = Checks {
            contrast: Some(ContrastLevel::Aa),
            ..Checks::default()
        };
        let findings_aa = evaluate_checks(&aa, "full", &snap);
        assert_eq!(findings_aa.len(), 1, "{findings_aa:?}");
        assert!(findings_aa[0].contains("muted-small"));
        assert!(!findings_aa[0].contains("muted-large"));
        assert!(!findings_aa[0].contains("Save"));

        let aaa = Checks {
            contrast: Some(ContrastLevel::Aaa),
            ..Checks::default()
        };
        let findings_aaa = evaluate_checks(&aaa, "full", &snap);
        assert_eq!(findings_aaa.len(), 1, "{findings_aaa:?}");
        assert!(findings_aaa[0].contains("muted-small"));
        assert!(findings_aaa[0].contains("muted-large"));
    }

    #[test]
    fn contrast_check_ignores_sliders_and_non_leaf_containers() {
        use rupicola_protocol::ComputedStyle;

        let mut slider = node(
            "split-slider",
            "slider",
            WidgetKind::Slider,
            Some("Before and after split slider"),
            rect(0.0, 0.0, 200.0, 24.0),
            None,
            Vec::new(),
        );
        slider.style = Some(ComputedStyle {
            color: Some("rgb(120, 120, 120)".to_owned()),
            background_color: Some("rgb(255, 255, 255)".to_owned()),
            font_size_px: Some(14.0),
            ..ComputedStyle::default()
        });

        let mut container = node(
            "hero-diff",
            "div",
            WidgetKind::Group,
            Some("Concatenated child text"),
            rect(0.0, 0.0, 300.0, 120.0),
            None,
            vec![{
                let mut leaf = node(
                    "leaf",
                    "span",
                    WidgetKind::Text,
                    Some("Readable"),
                    rect(10.0, 10.0, 100.0, 20.0),
                    None,
                    Vec::new(),
                );
                leaf.style = Some(ComputedStyle {
                    color: Some("rgb(17, 17, 19)".to_owned()),
                    background_color: Some("rgba(0, 0, 0, 0)".to_owned()),
                    font_size_px: Some(16.0),
                    ..ComputedStyle::default()
                });
                leaf
            }],
        );
        container.style = Some(ComputedStyle {
            color: Some("rgb(255, 255, 255)".to_owned()),
            background_color: Some("rgb(255, 255, 255)".to_owned()),
            font_size_px: Some(16.0),
            ..ComputedStyle::default()
        });

        let snap = snapshot(390, vec![slider, container]);
        let aa = Checks {
            contrast: Some(ContrastLevel::Aa),
            ..Checks::default()
        };
        assert_eq!(evaluate_checks(&aa, "full", &snap), Vec::<String>::new());
    }
}
