//! Three web-style effects with GTK4 and libadwaita: a popover (GTK already flips and
//! shifts it, so no placement code is needed), a shared-element transition driven by
//! measured rectangles and `adw::TimedAnimation`, and a scroll-linked reveal derived
//! from the scroll adjustment. Illustrative: the API moves between versions; this
//! compiles against gtk4 0.11 and libadwaita 0.9.
use adw::prelude::*;
use gtk::glib;
use std::cell::Cell;
use std::rc::Rc;

const ROW_H: i32 = 36;
const VIEW_H: i32 = 180;
const ROWS: i32 = 24;

// --- helpers, same as reference/motion/native-effects.md ---
pub fn progress(offset: f64, start: f64, end: f64) -> f64 {
    if (end - start).abs() < f64::EPSILON {
        return if offset >= end { 1.0 } else { 0.0 };
    }
    ((offset - start) / (end - start)).clamp(0.0, 1.0)
}
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Reduced motion: the app switch, or the system setting `gtk-enable-animations`.
fn reduced(switch: &gtk::Switch) -> bool {
    switch.is_active()
        || gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations())
}

fn build(app: &adw::Application) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(16);
    page.set_margin_bottom(16);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let reduced_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let reduced_switch = gtk::Switch::new();
    reduced_row.append(&gtk::Label::new(Some("Reduced motion")));
    reduced_row.append(&reduced_switch);
    page.append(&reduced_row);

    // 1. Popover. `autohide` is the light dismiss (outside click, Escape); GTK flips the
    // popover when there is no room and keeps it inside the window.
    page.append(&heading("Anchored popover"));
    let menu_button = gtk::Button::with_label("Open menu");
    let popover = gtk::Popover::new();
    popover.set_parent(&menu_button);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_autohide(true);
    let items = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for label in ["Rename", "Duplicate", "Delete"] {
        let item = gtk::Button::with_label(label);
        item.add_css_class("flat");
        let popover = popover.clone();
        item.connect_clicked(move |_| popover.popdown());
        items.append(&item);
    }
    popover.set_child(Some(&items));
    {
        let popover = popover.clone();
        menu_button.connect_clicked(move |_| popover.popup());
    }
    page.append(&menu_button);

    // 2. Shared element: a ghost in an overlay animates between measured rectangles.
    page.append(&heading("Shared element"));
    let cards = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    // A bare `gtk::Frame` draws only a thin border, so the ghost would look hollow; the
    // libadwaita `card` style class gives it a background, border and radius.
    let ghost = gtk::Box::new(gtk::Orientation::Vertical, 0);
    ghost.add_css_class("card");
    ghost.set_halign(gtk::Align::Start);
    ghost.set_valign(gtk::Align::Start);
    ghost.set_visible(false);
    let ghost_label = gtk::Label::new(None);
    ghost_label.set_hexpand(true);
    ghost_label.set_vexpand(true);
    ghost.append(&ghost_label);
    let source = Rc::new(Cell::new((0.0_f64, 0.0_f64, 110.0_f64, 70.0_f64)));
    let opened = Rc::new(Cell::new(false));
    let overlay = gtk::Overlay::new();
    let animate_ghost = {
        let (ghost, overlay, source, opened) = (ghost.clone(), overlay.clone(), source.clone(), opened.clone());
        let reduced_switch = reduced_switch.clone();
        Rc::new(move |open: bool| {
            opened.set(open);
            let (sx, sy, sw, sh) = source.get();
            let (tx, ty) = (40.0, 40.0);
            let (tw, th) = (f64::from(overlay.width() - 80), f64::from(overlay.height() - 80));
            let (from, to) = if open { (0.0, 1.0) } else { (1.0, 0.0) };
            ghost.set_visible(true);
            let ghost_for_close = ghost.clone();
            let target = adw::CallbackAnimationTarget::new({
                let ghost = ghost.clone();
                move |t| {
                    ghost.set_margin_start(lerp(sx, tx, t) as i32);
                    ghost.set_margin_top(lerp(sy, ty, t) as i32);
                    ghost.set_size_request(lerp(sw, tw, t) as i32, lerp(sh, th, t) as i32);
                }
            });
            // A duration of 0 applies the change at once when motion is reduced.
            let duration = if reduced(&reduced_switch) { 0 } else { 250 };
            let animation = adw::TimedAnimation::new(&ghost, from, to, duration, target);
            animation.set_easing(adw::Easing::EaseInOutCubic);
            animation.connect_done(move |_| {
                if !open {
                    ghost_for_close.set_visible(false);
                }
            });
            animation.play();
        })
    };
    for i in 0..3 {
        let card = gtk::Button::with_label(&format!("Card {}", i + 1));
        card.set_size_request(110, 70);
        let (overlay, source, animate_ghost, ghost_label) = (overlay.clone(), source.clone(), animate_ghost.clone(), ghost_label.clone());
        card.connect_clicked(move |card| {
            // Measure the source rectangle in the overlay's coordinates, then open.
            if let Some(b) = card.compute_bounds(&overlay) {
                source.set((f64::from(b.x()), f64::from(b.y()), f64::from(b.width()), f64::from(b.height())));
                ghost_label.set_text(&format!("Detail for card {} (click to close)", i + 1));
                animate_ghost(true);
            }
        });
        cards.append(&card);
    }
    page.append(&cards);
    {
        let animate_ghost = animate_ghost.clone();
        let click = gtk::GestureClick::new();
        click.connect_pressed(move |_, _, _, _| animate_ghost(false));
        ghost.add_controller(click);
    }

    // 3. Scroll-linked reveal: the adjustment value is the timeline.
    page.append(&heading("Scroll-linked reveal"));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let rows: Vec<gtk::Label> = (0..ROWS)
        .map(|i| {
            let row = gtk::Label::new(Some(&format!("Row {}", i + 1)));
            row.set_xalign(0.0);
            row.set_height_request(ROW_H - 4);
            row.set_margin_bottom(4);
            row.add_css_class("card");
            list.append(&row);
            row
        })
        .collect();
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_min_content_height(VIEW_H);
    scroller.set_child(Some(&list));
    let update_rows = {
        let (rows, scroller, reduced_switch) = (rows, scroller.clone(), reduced_switch.clone());
        move || {
            let offset = scroller.vadjustment().value();
            let no_move = reduced(&reduced_switch);
            for (i, row) in rows.iter().enumerate() {
                let rel = i as f64 * f64::from(ROW_H) - offset;
                let t = progress(rel, f64::from(VIEW_H), f64::from(VIEW_H) * 0.7);
                row.set_opacity(lerp(0.15, 1.0, t));
                // Reduced motion keeps the fade and drops the translation.
                row.set_margin_start(if no_move { 0 } else { lerp(24.0, 0.0, t) as i32 });
            }
        }
    };
    update_rows();
    scroller.vadjustment().connect_value_changed(move |_| update_rows());
    page.append(&scroller);

    overlay.set_child(Some(&page));
    overlay.add_overlay(&ghost);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(560)
        .default_height(640)
        .content(&overlay)
        .build();
    window.present();
}

fn heading(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id("dev.example.WebEffects").build();
    app.connect_activate(build);
    app.run()
}
