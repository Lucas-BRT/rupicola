//! Web-style effects with libcosmic: a popover that uses the toolkit's `popover` widget (it
//! provides the light dismiss), a crossfade-and-slide detail panel driven by libcosmic's
//! `anim::State` and per-frame ticks, and a scroll-linked reveal derived from the scroll
//! offset. A shared-element ghost needs widget measurement (iced's `selector` feature), which
//! libcosmic does not expose at this revision, so it is not included here; see the iced and
//! egui examples for it. Illustrative: libcosmic's API is not stable; this compiles against
//! the git revision recorded in Cargo.lock.
use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{keyboard, window, Color, Length, Padding, Size, Subscription};
use cosmic::widget::{self, popover};
use cosmic::{anim, executor, Application, Element};
use std::time::Duration;

// --- helpers, same as reference/motion/native-effects.md ---
pub fn progress(offset: f32, start: f32, end: f32) -> f32 {
    if (end - start).abs() < f32::EPSILON {
        return if offset >= end { 1.0 } else { 0.0 };
    }
    ((offset - start) / (end - start)).clamp(0.0, 1.0)
}

const ROW_H: f32 = 36.0;
const VIEW_H: f32 = 180.0;
const DETAIL: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
enum Message {
    ToggleMenu,
    CloseMenu,
    ToggleDetail,
    Escape,
    Scrolled(f32),
    Tick,
    ReducedMotion(bool),
}

struct App {
    core: Core,
    menu_open: bool,
    detail_open: bool,
    /// Drives the detail panel; libcosmic's helper stores the time of the last change.
    detail: anim::State,
    scroll_offset: f32,
    /// App-level switch; also wire this to the platform's reduced-motion setting.
    reduced_motion: bool,
}

impl App {
    fn dur(&self) -> Duration {
        if self.reduced_motion {
            Duration::from_millis(1)
        } else {
            DETAIL
        }
    }
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "dev.example.WebEffects";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        (
            App { core, menu_open: false, detail_open: false, detail: anim::State::default(), scroll_offset: 0.0, reduced_motion: false },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleMenu => self.menu_open = !self.menu_open,
            Message::CloseMenu => self.menu_open = false,
            // The popover widget closes on an outside press only; Escape needs its own handler.
            Message::Escape => self.menu_open = false,
            Message::ToggleDetail => {
                self.detail_open = !self.detail_open;
                let dur = self.dur();
                self.detail.changed(dur);
            }
            Message::Scrolled(y) => self.scroll_offset = y,
            Message::ReducedMotion(on) => self.reduced_motion = on,
            Message::Tick => {
                let dur = self.dur();
                self.detail.anim_done(dur);
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        // `listen_with` sees events whether or not a widget captured them (an open popover may).
        let escape = cosmic::iced::event::listen_with(|event, _status, _window| match event {
            cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. }) => Some(Message::Escape),
            _ => None,
        });
        // Tick per frame only while the detail panel animates; the subscription ends when idle.
        if self.detail.last_change.is_some() {
            Subscription::batch([escape, window::frames().map(|_| Message::Tick)])
        } else {
            escape
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // 1. Popover: the widget owns placement and light dismiss (`on_close`).
        let menu = widget::Column::new()
            .spacing(4)
            .padding(8)
            .push(widget::button::text("Rename").on_press(Message::CloseMenu))
            .push(widget::button::text("Duplicate").on_press(Message::CloseMenu))
            .push(widget::button::text("Delete").on_press(Message::CloseMenu));
        let anchor = widget::button::standard("Open menu").on_press(Message::ToggleMenu);
        let mut popover_row = popover(anchor).position(popover::Position::Bottom).on_close(Message::CloseMenu);
        if self.menu_open {
            popover_row = popover_row.popup(widget::container(menu).class(cosmic::theme::Container::Card));
        }

        // 2. Crossfade and slide: `t` runs 0 to 1 when opening and 1 to 0 when closing.
        let t = self.detail.t(self.dur(), self.detail_open).clamp(0.0, 1.0);
        let t = if self.detail.last_change.is_none() { if self.detail_open { 1.0 } else { 0.0 } } else { t };
        let slide = if self.reduced_motion { 0.0 } else { anim::slerp(24.0, 0.0, t) };
        let detail = widget::container(widget::text::body("Detail panel"))
            .padding(Padding { top: 8.0, left: 8.0 + slide, ..Padding::ZERO })
            .class(cosmic::theme::Container::Card);

        // 3. Scroll-linked reveal: the offset is the timeline.
        let rows = widget::Column::new().spacing(4).extend((0..24).map(|i| {
            let rel = i as f32 * ROW_H - self.scroll_offset;
            let t = progress(rel, VIEW_H, VIEW_H * 0.7);
            let dx = if self.reduced_motion { 0.0 } else { anim::lerp(24.0, 0.0, t) };
            let alpha = anim::lerp(0.15, 1.0, t);
            widget::container(widget::text::body(format!("Row {}", i + 1)).class(cosmic::theme::Text::Color(Color { a: alpha, ..Color::WHITE })))
                .padding(Padding { top: 6.0, left: 10.0 + dx, ..Padding::ZERO })
                .width(Length::Fill)
                .height(ROW_H - 4.0)
                .into()
        }));
        let scroller = widget::scrollable(rows).height(VIEW_H).on_scroll(|v| Message::Scrolled(v.absolute_offset().y));

        widget::Column::new()
            .spacing(12)
            .padding(16)
            .push(widget::toggler(self.reduced_motion).label("Reduced motion".to_string()).on_toggle(Message::ReducedMotion))
            .push(widget::text::heading("Anchored popover"))
            .push(popover_row)
            .push(widget::text::heading("Crossfade and slide"))
            .push(widget::button::standard("Toggle detail").on_press(Message::ToggleDetail))
            // A fixed slot: the panel fades and slides inside it, so nothing below it moves.
            .push(widget::container(if t > 0.0 { Element::from(detail) } else { Element::from(widget::Space::new()) }).height(48.0))
            .push(widget::text::heading("Scroll-linked reveal"))
            .push(scroller)
            .into()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cosmic::app::run::<App>(Settings::default().size(Size::new(560.0, 640.0)), ())?;
    Ok(())
}
