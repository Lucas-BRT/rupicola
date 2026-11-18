//! Three web-style effects reproduced in GPUI from state the app owns: a popover placed by
//! `anchored()` (it snaps into the window, which is the flip-and-shift of the web feature),
//! a shared-element transition driven by measured bounds and `with_animation`, and a
//! scroll-linked reveal derived from the scroll handle's offset.
//! Illustrative: GPUI's API moves between revisions; this compiles against gpui 0.2.2.
use gpui::{
    anchored, canvas, deferred, div, point, prelude::*, px, rgb, size, Animation, AnimationExt, App,
    Application, Bounds, Context, Corner, FocusHandle, FontWeight, KeyDownEvent, Pixels, Render, ScrollHandle,
    Window, WindowBounds, WindowOptions,
};
use std::time::Duration;

// --- helpers, same as reference/motion/native-effects.md ---
pub fn progress(offset: f32, start: f32, end: f32) -> f32 {
    if (end - start).abs() < f32::EPSILON {
        return if offset >= end { 1.0 } else { 0.0 };
    }
    ((offset - start) / (end - start)).clamp(0.0, 1.0)
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

const ROW_H: f32 = 36.0;
const VIEW_H: f32 = 180.0;
const WINDOW: (f32, f32) = (560.0, 640.0);

struct Demo {
    focus: FocusHandle,
    scroll: ScrollHandle,
    menu_open: bool,
    /// Measured bounds of each card, written during prepaint and read on click.
    card_bounds: [Bounds<Pixels>; 3],
    /// Card index and its source bounds while the detail view is open or closing.
    detail: Option<(usize, Bounds<Pixels>)>,
    detail_open: bool,
    /// Changes on every open or close so `with_animation` restarts.
    generation: usize,
    /// App-level switch; also wire this to the platform's reduced-motion setting.
    reduced_motion: bool,
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        Demo {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            menu_open: false,
            card_bounds: [Bounds::default(); 3],
            detail: None,
            detail_open: false,
            generation: 0,
            reduced_motion: false,
        }
    }

    /// Closing animates back to the card; the ghost must be dropped afterwards, or it stays on
    /// top of the card and swallows its clicks.
    fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.detail_open = false;
        self.generation += 1;
        let generation = self.generation;
        let wait = self.millis(250) + Duration::from_millis(60);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, cx| {
                if !this.detail_open && this.generation == generation {
                    this.detail = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// A duration of about zero applies the change at once when motion is reduced.
    fn millis(&self, ms: u64) -> Duration {
        Duration::from_millis(if self.reduced_motion { 1 } else { ms })
    }
}

impl Render for Demo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let entity = cx.entity();

        // 1. Popover. `anchored()` positions the menu next to its parent and snaps it into
        // the window; `on_mouse_down_out` is the light dismiss.
        let menu = div()
            .id("menu")
            .w(px(180.0))
            .p_2()
            .gap_1()
            .flex()
            .flex_col()
            .bg(rgb(0x2b2b2b))
            .rounded_md()
            .text_color(rgb(0xffffff))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu_open = false;
                cx.notify();
            }))
            .children(["Rename", "Duplicate", "Delete"].map(|label| {
                div().id(label).px_2().py_1().rounded_sm().hover(|s| s.bg(rgb(0x444444))).child(label).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.menu_open = false;
                        cx.notify();
                    },
                ))
            }));
        let popover_row = div().relative().child(
            div()
                .id("anchor")
                .px_3()
                .py_1()
                .w(px(120.0))
                .bg(rgb(0x4d6fb4))
                .text_color(rgb(0xffffff))
                .child("Open menu")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.menu_open = !this.menu_open;
                    cx.notify();
                })),
        )
        .when(self.menu_open, |d| {
            d.child(deferred(
                anchored().anchor(Corner::TopLeft).offset(point(px(0.0), px(34.0))).snap_to_window_with_margin(px(8.0)).child(menu),
            ).with_priority(1))
        });

        // 2. Shared element: measure each card's bounds during prepaint, animate a ghost.
        let cards = div().flex().gap_2().children((0..3).map(|i| {
            let entity = entity.clone();
            div()
                .id(("card", i))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .w(px(110.0))
                .h(px(70.0))
                .bg(rgb(0x464646))
                .rounded_md()
                .text_color(rgb(0xffffff))
                .child(format!("Card {}", i + 1))
                .child(canvas(
                    move |bounds, _, cx| entity.update(cx, |this, _| this.card_bounds[i] = bounds),
                    |_, _, _, _| {},
                ).absolute().size_full())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.detail.is_none() {
                        this.detail = Some((i, this.card_bounds[i]));
                        this.detail_open = true;
                        this.generation += 1;
                        cx.notify();
                    }
                }))
        }));

        // 3. Scroll-linked reveal: the scroll offset is the timeline.
        let offset_y: f32 = self.scroll.offset().y.into();
        // The handle knows the real scroll range; do not recompute it from row constants.
        let max_offset: f32 = self.scroll.max_offset().height.into();
        let reduced = self.reduced_motion;
        let rows = (0..24).map(move |i| {
            let rel = i as f32 * ROW_H + offset_y;
            // At the end of the list the last rows never leave the entry band; show them fully.
            let at_end = offset_y <= -max_offset + 1.0;
            let t = if at_end { 1.0 } else { progress(rel, VIEW_H, VIEW_H * 0.7) };
            // Reduced motion keeps the fade and drops the translation.
            let dx = if reduced { 0.0 } else { lerp(24.0, 0.0, t) };
            div()
                .h(px(ROW_H - 4.0))
                .ml(px(dx))
                .px_2()
                .rounded_md()
                .bg(rgb(0x466eb4))
                .opacity(lerp(0.15, 1.0, t))
                .text_color(rgb(0xffffff))
                .child(format!("Row {}", i + 1))
        });
        let scroller = div()
            .id("scroller")
            .h(px(VIEW_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .children(rows);

        let mut root = div()
            .id("root")
            .relative()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xdddddd))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.menu_open = false;
                    if this.detail.is_some() && this.detail_open {
                        this.close_detail(cx);
                    }
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("reduced")
                    .child(if self.reduced_motion { "Reduced motion: on" } else { "Reduced motion: off" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reduced_motion = !this.reduced_motion;
                        cx.notify();
                    })),
            )
            .child(div().font_weight(FontWeight::BOLD).child("Anchored popover"))
            .child(popover_row)
            .child(div().font_weight(FontWeight::BOLD).child("Shared element"))
            .child(cards)
            .child(div().font_weight(FontWeight::BOLD).child("Scroll-linked reveal"))
            .child(scroller);

        if let Some((index, source)) = self.detail {
            let open = self.detail_open;
            let (win_w, win_h) = WINDOW;
            let from = (f32::from(source.origin.x), f32::from(source.origin.y), f32::from(source.size.width), f32::from(source.size.height));
            let to = (40.0, 40.0, win_w - 80.0, win_h - 80.0);
            let ghost = div()
                .id("ghost")
                .absolute()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0x464646))
                .rounded_lg()
                .text_color(rgb(0xffffff))
                .child(if open { format!("Detail for card {} (Esc to close)", index + 1) } else { String::new() })
                .on_click(cx.listener(|this, _, _, cx| this.close_detail(cx)))
                .with_animation(
                    ("ghost", self.generation),
                    Animation::new(self.millis(250)).with_easing(gpui::ease_in_out),
                    move |el, delta| {
                        // Tween between the measured source and the detail rectangle.
                        let t = if open { delta } else { 1.0 - delta };
                        el.left(px(lerp(from.0, to.0, t)))
                            .top(px(lerp(from.1, to.1, t)))
                            .w(px(lerp(from.2, to.2, t)))
                            .h(px(lerp(from.3, to.3, t)))
                    },
                );
            root = root.child(ghost);
        }
        root
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW.0), px(WINDOW.1)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |_, cx| cx.new(Demo::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
