//! Three web-style effects reproduced in Iced from state the app owns:
//! an anchored popover that flips and shifts, a shared-element transition driven by
//! measured rectangles, and a scroll-linked reveal derived from the scroll offset.
//! Illustrative: Iced's API moves between versions; this compiles against iced 0.14.
use iced::widget::{
    button, checkbox, column, container, mouse_area, row, scrollable, selector, stack, text, Space,
};
use iced::{
    animation::Animation, keyboard, window, Color, Element, Length, Padding, Rectangle, Size,
    Subscription, Task,
};
use std::time::Instant;

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

const ANCHOR: &str = "anchor";
const ROW_H: f32 = 36.0;
const VIEW_H: f32 = 180.0;

#[derive(Debug, Clone)]
enum Message {
    ToggleMenu,
    AnchorBounds(Option<Rectangle>),
    CardPressed(usize),
    CardBounds(usize, Option<Rectangle>),
    CloseDetail,
    Scrolled(f32),
    Resized(Size),
    Escape,
    Tick(Instant),
    ReducedMotion(bool),
    Dismiss,
}

struct App {
    window: Size,
    menu_open: bool,
    anchor: Option<Rectangle>,
    detail: Option<(usize, Rectangle)>,
    /// 0 to 1, driven by a time-based animation (a spring would also fit).
    detail_anim: Animation<bool>,
    scroll_offset: f32,
    /// App-level switch; also wire this to the platform's reduced-motion setting.
    reduced_motion: bool,
    now: Instant,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (
            App {
                window: Size::new(560.0, 640.0),
                menu_open: false,
                anchor: None,
                detail: None,
                detail_anim: Animation::new(false).quick(),
                scroll_offset: 0.0,
                reduced_motion: false,
                now: Instant::now(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleMenu => {
                self.menu_open = !self.menu_open;
                if self.menu_open {
                    // Measure the anchor when opening, then place the popover from its rectangle.
                    return measure(ANCHOR.to_string()).map(Message::AnchorBounds);
                }
            }
            Message::AnchorBounds(rect) => self.anchor = rect,
            Message::CardPressed(i) => {
                return measure(card_id(i)).map(move |r| Message::CardBounds(i, r));
            }
            Message::CardBounds(i, Some(rect)) => {
                self.detail = Some((i, rect));
                self.detail_anim.go_mut(true, self.now);
            }
            Message::CardBounds(_, None) => {}
            Message::CloseDetail => self.detail_anim.go_mut(false, self.now),
            Message::Scrolled(y) => self.scroll_offset = y,
            Message::Resized(size) => self.window = size,
            Message::Escape => {
                self.menu_open = false;
                if self.detail.is_some() {
                    self.detail_anim.go_mut(false, self.now);
                }
            }
            Message::Dismiss => self.menu_open = false,
            Message::ReducedMotion(on) => self.reduced_motion = on,
            Message::Tick(now) => {
                self.now = now;
                if !self.detail_anim.is_animating(now) && !self.detail_anim.value() {
                    self.detail = None; // fully closed: drop the ghost
                }
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            keyboard::listen().filter_map(|event| match event {
                keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. } => Some(Message::Escape),
                _ => None,
            }),
        ];
        // Tick per frame only while something animates; drop the subscription when idle.
        if self.detail.is_some() {
            subs.push(window::frames().map(Message::Tick));
        }
        Subscription::batch(subs)
    }

    /// Progress of the shared-element animation; instant when motion is reduced.
    fn t(&self) -> f32 {
        if self.reduced_motion {
            return if self.detail_anim.value() { 1.0 } else { 0.0 };
        }
        self.detail_anim.interpolate(0.0, 1.0, self.now)
    }

    fn view(&self) -> Element<'_, Message> {
        let anchor = container(button("Open menu").on_press(Message::ToggleMenu)).id(ANCHOR);

        let cards = row((0..3).map(|i| {
            container(button(container(text(format!("Card {}", i + 1))).center(Length::Fill)).width(110).height(70).on_press(Message::CardPressed(i)))
                .id(card_id(i))
                .into()
        }))
        .spacing(8);

        // Scroll-linked reveal: the offset is the timeline, no clock involved.
        let rows = column((0..24).map(|i| {
            let rel = i as f32 * ROW_H - self.scroll_offset;
            let t = progress(rel, VIEW_H, VIEW_H * 0.7);
            let dx = if self.reduced_motion { 0.0 } else { lerp(24.0, 0.0, t) };
            let alpha = lerp(0.15, 1.0, t);
            container(text(format!("Row {}", i + 1)).color(Color { a: alpha, ..Color::WHITE }))
                .padding(Padding { top: 6.0, left: 10.0 + dx, ..Padding::ZERO })
                .width(Length::Fill)
                .height(ROW_H - 4.0)
                .style(move |_| container::Style {
                    background: Some(Color::from_rgba8(70, 110, 180, alpha).into()),
                    border: iced::border::rounded(6),
                    ..Default::default()
                })
                .into()
        }));

        let page = column![
            checkbox(self.reduced_motion).label("Reduced motion").on_toggle(Message::ReducedMotion),
            text("Anchored popover").size(18),
            anchor,
            text("Shared element").size(18),
            cards,
            text("Scroll-linked reveal").size(18),
            scrollable(rows).height(VIEW_H).on_scroll(|v| Message::Scrolled(v.absolute_offset().y)),
        ]
        .spacing(12)
        .padding(16);

        let mut layers: Vec<Element<'_, Message>> = vec![page.into()];

        if self.menu_open {
            if let Some(a) = self.anchor {
                let (w, h, gap) = (180.0, 112.0, 4.0);
                // Flip when there is no room below, shift to stay inside the window.
                let y = match side(a.y, a.y + a.height, h, self.window.height, gap) {
                    Side::Below => a.y + a.height + gap,
                    Side::Above => a.y - gap - h,
                };
                let x = clamp_x(a.x + a.width / 2.0, w, self.window.width, 8.0);
                // Light dismiss: a full-window layer behind the popover closes it on press.
                layers.push(mouse_area(Space::new().width(Length::Fill).height(Length::Fill)).on_press(Message::Dismiss).into());
                layers.push(
                    container(
                        container(column![button("Rename").on_press(Message::Dismiss), button("Duplicate").on_press(Message::Dismiss), button("Delete").on_press(Message::Dismiss)].spacing(4).padding(8))
                            .width(w)
                            .height(h)
                            .style(container::rounded_box),
                    )
                    .padding(Padding { top: y, left: x, ..Padding::ZERO })
                    .into(),
                );
            }
        }

        if let Some((index, source)) = self.detail {
            let t = self.t();
            let target = Rectangle { x: 40.0, y: 40.0, width: self.window.width - 80.0, height: self.window.height - 80.0 };
            // The ghost tweens position and size between the measured source and the detail rectangle.
            let (x, y) = (lerp(source.x, target.x, t), lerp(source.y, target.y, t));
            let (w, h) = (lerp(source.width, target.width, t), lerp(source.height, target.height, t));
            let label = if t > 0.98 { format!("Detail for card {} (click to close)", index + 1) } else { String::new() };
            layers.push(
                container(
                    mouse_area(container(text(label)).center(Length::Fill).width(w).height(h).style(container::rounded_box)).on_press(Message::CloseDetail),
                )
                .padding(Padding { top: y, left: x, ..Padding::ZERO })
                .into(),
            );
        }

        stack(layers).into()
    }
}

/// Measure a widget's visible rectangle by id (the selector feature of iced 0.14).
fn measure(id: String) -> Task<Option<Rectangle>> {
    selector::find(selector::id(id)).map(|target| target.and_then(|t| t.visible_bounds()))
}

fn card_id(i: usize) -> String {
    format!("card-{i}")
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .window_size((560.0, 640.0))
        .run()
}

/// Headless captures with iced_test. State is set directly, because tasks (widget measurement)
/// and subscriptions do not run here. Set RUPICOLA_SHOTS to a directory to write PNGs.
#[cfg(test)]
mod capture {
    use super::*;
    use iced::Theme;
    use iced_test::Simulator;
    use std::path::PathBuf;

    const SIZE: Size = Size::new(560.0, 640.0);

    fn shots() -> Option<PathBuf> {
        let dir = PathBuf::from(std::env::var_os("RUPICOLA_SHOTS")?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn snapshot(app: &App, name: &str) -> Rectangle {
        let mut sim = Simulator::with_size(iced::Settings::default(), SIZE, app.view());
        let anchor = sim.find(selector::id(ANCHOR)).ok().and_then(|t| t.visible_bounds()).unwrap_or(Rectangle::new(iced::Point::ORIGIN, Size::ZERO));
        if let Some(dir) = shots() {
            let snap = sim.snapshot(&Theme::Dark).expect("snapshot");
            snap.matches_image(dir.join(name)).expect("write png");
        }
        anchor
    }

    #[test]
    fn states_render() {
        let (mut app, _) = App::new();
        let anchor = snapshot(&app, "01-initial");
        assert!(anchor.width > 10.0, "the anchor must be laid out: {anchor:?}");

        // Popover, placed from the anchor's measured rectangle.
        app.menu_open = true;
        app.anchor = Some(anchor);
        snapshot(&app, "02-popover-open");

        // Detail: a source rectangle and the animation halfway and finished.
        app.menu_open = false;
        let start = app.now;
        app.detail = Some((1, Rectangle { x: 126.0, y: 130.0, width: 110.0, height: 70.0 }));
        app.detail_anim.go_mut(true, start);
        app.now = start + std::time::Duration::from_millis(60);
        snapshot(&app, "03-detail-early");
        app.now = start + std::time::Duration::from_millis(2000);
        snapshot(&app, "04-detail-open");

        // Scroll-linked reveal at an offset.
        app.detail = None;
        app.detail_anim = Animation::new(false).quick();
        app.scroll_offset = 300.0;
        snapshot(&app, "05-scrolled");

        // Reduced motion: no translation of the rows.
        app.reduced_motion = true;
        app.scroll_offset = 0.0;
        snapshot(&app, "06-reduced-motion");
    }
}
