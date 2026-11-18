//! Three web-style effects reproduced in egui from state the app owns:
//! an anchored popover that flips and shifts, a shared-element transition driven by
//! measured rectangles, and scroll-linked reveal derived from the scroll offset.
//! Illustrative: egui's API moves between versions; this compiles against eframe 0.37.
use eframe::egui::{self, Align2, Color32, Id, LayerId, Order, Pos2, Rect, Sense, Vec2};

const VIEW_H: f32 = 180.0;

// --- helpers, same as reference/motion/native-effects.md and tactics/native-components.md ---
pub fn progress(offset: f32, start: f32, end: f32) -> f32 {
    if (end - start).abs() < f32::EPSILON {
        return if offset >= end { 1.0 } else { 0.0 };
    }
    ((offset - start) / (end - start)).clamp(0.0, 1.0)
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
#[derive(Debug, PartialEq)]
pub enum Side {
    Below,
    Above,
}
pub fn side(anchor_top: f32, anchor_bottom: f32, popup_h: f32, window_h: f32, gap: f32) -> Side {
    let below = window_h - anchor_bottom - gap;
    let above = anchor_top - gap;
    if below >= popup_h || below >= above {
        Side::Below
    } else {
        Side::Above
    }
}
pub fn clamp_x(anchor_center: f32, popup_w: f32, window_w: f32, margin: f32) -> f32 {
    (anchor_center - popup_w / 2.0).clamp(margin, (window_w - popup_w - margin).max(margin))
}

/// Keep the popup inside the window vertically, after choosing a side.
pub fn clamp_y(y: f32, popup_h: f32, window_h: f32, margin: f32) -> f32 {
    y.clamp(margin, (window_h - popup_h - margin).max(margin))
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(lerp(a.min.x, b.min.x, t), lerp(a.min.y, b.min.y, t)),
        Pos2::new(lerp(a.max.x, b.max.x, t), lerp(a.max.y, b.max.y, t)),
    )
}

struct App {
    popover_open: bool,
    /// Index and source rectangle of the card that opened the detail view.
    detail: Option<(usize, Rect)>,
    detail_open: bool,
    /// Rectangles of the cards from the last frame (for tests and measurement tools).
    card_rects: [Rect; 3],
    /// App-level switch; also wire this to the platform's reduced-motion setting.
    reduced_motion: bool,
}

impl Default for App {
    fn default() -> Self {
        App { popover_open: false, detail: None, detail_open: false, card_rects: [Rect::ZERO; 3], reduced_motion: false }
    }
}

impl App {
    /// Animation time that respects the reduced-motion switch (zero means an instant change).
    fn time(&self, seconds: f32) -> f32 {
        if self.reduced_motion {
            0.0
        } else {
            seconds
        }
    }

    fn popover(&mut self, ui: &mut egui::Ui) {
        let anchor = ui.button("Open menu");
        if anchor.clicked() {
            self.popover_open = !self.popover_open;
        }
        if !self.popover_open {
            return;
        }
        let ctx = ui.ctx().clone();
        let window = ctx.content_rect();
        let size = Vec2::new(180.0, 84.0);
        let gap = 4.0;
        // Flip when there is no room below, shift to stay inside the window.
        let y = match side(anchor.rect.top(), anchor.rect.bottom(), size.y, window.height(), gap) {
            Side::Below => anchor.rect.bottom() + gap,
            Side::Above => anchor.rect.top() - gap - size.y,
        };
        let y = clamp_y(y, size.y, window.height(), 8.0);
        let x = clamp_x(anchor.rect.center().x, size.x, window.width(), 8.0);
        let area = egui::Area::new(Id::unique("popover"))
            .order(Order::Foreground)
            .fixed_pos(Pos2::new(x, y))
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(size.x);
                    for label in ["Rename", "Duplicate", "Delete"] {
                        if ui.button(label).clicked() {
                            self.popover_open = false;
                        }
                    }
                });
            });
        // Light dismiss: Escape, or a press outside both the popover and its anchor.
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p) && !anchor.rect.contains(p))
        });
        if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.popover_open = false;
        }
    }

    fn shared_element(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for i in 0..3 {
                let (rect, response) = ui.allocate_exact_size(Vec2::new(110.0, 70.0), Sense::click());
                ui.painter().rect_filled(rect, 8.0, Color32::from_gray(70));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("Card {}", i + 1), egui::FontId::proportional(14.0), Color32::WHITE);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Card {}", i + 1)));
                self.card_rects[i] = rect;
                if response.clicked() && self.detail.is_none() {
                    // Record the source rectangle in window coordinates, then open.
                    self.detail = Some((i, rect));
                    self.detail_open = true;
                }
            }
        });
        let ctx = ui.ctx().clone();
        // Call this every frame, even while the detail is closed: egui starts a never-seen id at
        // its target value, so the first call must happen while the target is still `false`.
        let t = ctx.animate_bool_with_time(Id::unique("detail"), self.detail_open, self.time(0.25));
        let Some((index, source)) = self.detail else {
            return;
        };
        if !self.detail_open && t == 0.0 {
            self.detail = None; // fully closed: drop the ghost
            return;
        }
        let target = ctx.content_rect().shrink(40.0);
        let ghost = lerp_rect(source, target, t);
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("ghost")));
        painter.rect_filled(ghost, lerp(8.0, 16.0, t), Color32::from_gray(70));
        if t > 0.98 {
            painter.text(ghost.center(), Align2::CENTER_CENTER, format!("Detail for card {} (Esc to close)", index + 1), egui::FontId::proportional(18.0), Color32::WHITE);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.detail_open = false;
        }
    }

    fn scroll_reveal(&self, ui: &mut egui::Ui) {
        let row_h = 36.0;
        egui::ScrollArea::vertical().max_height(VIEW_H).show_viewport(ui, |ui, viewport| {
            // Rows are real allocations (not painter-only), so the area has content to scroll.
            let origin = ui.cursor().min;
            for i in 0..24 {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h - 4.0), Sense::hover());
                ui.add_space(4.0);
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                // Position of the row inside the viewport; the scroll offset is the timeline.
                let rel = (rect.min.y - origin.y) - viewport.min.y;
                let t = progress(rel, viewport.height(), viewport.height() * 0.7);
                // Reduced motion keeps the fade and drops the translation.
                let dx = if self.reduced_motion { 0.0 } else { lerp(24.0, 0.0, t) };
                // Slide in from the left edge while the right edge stays on the area's edge.
                let shifted = Rect::from_min_max(rect.min + Vec2::new(dx, 0.0), rect.max);
                let alpha = (lerp(0.15, 1.0, t) * 255.0) as u8;
                ui.painter().rect_filled(shifted, 6.0, Color32::from_rgba_unmultiplied(70, 110, 180, alpha));
                ui.painter().text(shifted.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, format!("Row {}", i + 1), egui::FontId::proportional(14.0), Color32::from_rgba_unmultiplied(255, 255, 255, alpha));
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl App {
    fn show(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.reduced_motion, "Reduced motion");
        ui.separator();
        ui.heading("Anchored popover");
        self.popover(ui);
        ui.separator();
        ui.heading("Shared element");
        self.shared_element(ui);
        ui.separator();
        ui.heading("Scroll-linked reveal");
        self.scroll_reveal(ui);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "web effects in egui",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

/// Headless captures with egui_kittest (software Vulkan is enough). Set FERRIS_SHOTS to a
/// directory to write PNGs and a UI tree; without it the test only checks the flow.
#[cfg(test)]
mod capture {
    use super::*;
    use egui_kittest::kittest::{NodeT, Queryable};
    use egui_kittest::Harness;
    use std::path::PathBuf;

    fn shots() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("FERRIS_SHOTS")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn harness(size: Vec2) -> Harness<'static, App> {
        Harness::builder()
            .wgpu()
            .with_size(size)
            .with_step_dt(1.0 / 60.0)
            .build_ui_state(|ui, app: &mut App| app.show(ui), App::default())
    }

    fn shot(h: &mut Harness<'_, App>, name: &str) {
        if let Some(dir) = shots() {
            h.render().expect("render").save(dir.join(format!("{name}.png"))).expect("save png");
        }
    }

    fn click_at(h: &mut Harness<'_, App>, pos: Pos2) {
        h.hover_at(pos);
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        h.step();
    }

    /// Writes the accessibility tree in the JSON shape `ferris-atelier audit` reads.
    fn dump_tree(h: &Harness<'_, App>, size: Vec2, name: &str) {
        let Some(dir) = shots() else { return };
        let mut nodes = Vec::new();
        for (i, n) in h.root().children_recursive().enumerate() {
            let Some(r) = n.accesskit_node().bounding_box() else { continue };
            let label = n.accesskit_node().label().unwrap_or_default();
            let role = n.accesskit_node().role();
            if matches!(role, egui::accesskit::Role::ScrollBar) {
                continue; // the track sits partly outside its area by design
            }
            let interactive = n.accesskit_node().data().supports_action(egui::accesskit::Action::Click)
                && !matches!(role, egui::accesskit::Role::Label | egui::accesskit::Role::TextRun);
            nodes.push(format!(
                "{{\"id\":\"n{i}\",\"role\":\"{:?}\",\"name\":{:?},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"interactive\":{interactive}}}",
                n.accesskit_node().role(), label, r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0
            ));
        }
        let json = format!("{{\"window\":{{\"w\":{},\"h\":{}}},\"nodes\":[{}]}}", size.x, size.y, nodes.join(","));
        std::fs::write(dir.join(format!("{name}.tree.json")), json).unwrap();
    }

    #[test]
    fn states_are_rendered_and_behave() {
        let size = Vec2::new(560.0, 640.0);
        let mut h = harness(size);
        h.run();
        shot(&mut h, "01-initial");
        dump_tree(&h, size, "01-initial");

        // Popover opens below the anchor and closes on Escape.
        h.get_by_label("Open menu").click();
        h.run();
        assert!(h.state().popover_open);
        shot(&mut h, "02-popover-open");
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().popover_open, "Escape must dismiss the popover");

        // Shared element: a click on card 2 opens the detail; frames are captured on the way.
        let card = h.state().card_rects[1].center();
        click_at(&mut h, card);
        assert!(h.state().detail.is_some(), "clicking a card opens the detail");
        if let Some(dir) = shots() {
            let frames = dir.join("frames");
            std::fs::create_dir_all(&frames).unwrap();
            let mut distinct = 0;
            let mut previous: Option<Vec<u8>> = None;
            for i in 0..24 {
                h.step();
                let image = h.render().unwrap();
                image.save(frames.join(format!("f{i:02}.png"))).unwrap();
                if previous.as_deref() != Some(image.as_raw().as_slice()) {
                    distinct += 1;
                }
                previous = Some(image.into_raw());
            }
            assert!(distinct >= 8, "opening should tween over many frames, got {distinct} distinct");
        } else {
            h.run_steps(40);
        }
        shot(&mut h, "03-detail-open");
        h.key_press(egui::Key::Escape);
        h.run_steps(40);
        assert!(h.state().detail.is_none(), "Escape closes the detail after the animation");

        // Scroll-linked reveal: scrolling changes what is visible.
        h.hover_at(Pos2::new(200.0, 320.0));
        h.run_steps(2);
        let before = h.render().unwrap();
        h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: Vec2::new(0.0, -6.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Default::default() });
        h.run_steps(40);
        let after = h.render().unwrap();
        assert_ne!(before.as_raw(), after.as_raw(), "scrolling should change the rendering");
        shot(&mut h, "04-scrolled");

        // Reduced motion: the detail reaches its final size without intermediate frames.
        h.get_by_label("Reduced motion").click();
        h.run();
        assert!(h.state().reduced_motion);
        let card = h.state().card_rects[0].center();
        click_at(&mut h, card);
        h.step();
        h.step();
        shot(&mut h, "05-reduced-motion-detail");
    }

    #[test]
    fn popover_flips_and_stays_in_a_short_window() {
        // Too short for the menu below the anchor, and no taller above it.
        let size = Vec2::new(560.0, 150.0);
        let mut h = harness(size);
        h.run();
        h.get_by_label("Open menu").click();
        h.run();
        shot(&mut h, "06-short-window-popover");
        dump_tree(&h, size, "06-short-window-popover");
        for label in ["Rename", "Duplicate", "Delete"] {
            let r = h.get_by_label(label).rect();
            assert!(r.min.y >= 0.0 && r.max.y <= size.y, "{label} must stay inside the window: {r:?}");
        }
    }
}
