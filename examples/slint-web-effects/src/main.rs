//! Three web-style effects reproduced in Slint from properties the UI owns:
//! an anchored popover that flips and shifts, a shared-element transition driven by
//! measured positions, and a scroll-linked reveal derived from the Flickable offset.
//! Illustrative: Slint's API moves between versions; this compiles against slint 1.19.
slint::slint! {
    #[edition = "1.19"]
    import { CheckBox } from "std-widgets.slint";
    // 0..1 position of `offset` between `start` and `end` (the scroll offset is the timeline).
    export global Motion {
        pure public function progress(offset: length, start: length, end: length) -> float {
            if (end == start) { return offset >= end ? 1.0 : 0.0; }
            return Math.min(Math.max((offset - start) / (end - start), 0.0), 1.0);
        }
    }

    export component App inherits Window {
        width: 560px;
        height: 640px;
        // App-level switch; also wire this to the platform's reduced-motion setting.
        in-out property <bool> reduced-motion: false;
        property <duration> move-time: reduced-motion ? 0ms : 250ms;

        VerticalLayout {
            padding: 16px;
            spacing: 12px;
            // Without this, extra window height is spread over the headings and leaves large gaps.
            alignment: start;

            CheckBox { text: "Reduced motion"; checked <=> root.reduced-motion; }

            Text { text: "Anchored popover"; font-weight: 700; }
            anchor := Rectangle {
                height: 32px;
                width: 120px;
                background: rgb(77, 111, 180);
                Text { text: "Open menu"; color: white; }
                TouchArea { clicked => { menu.show(); } }

                // Flip when there is no room below the anchor, shift to stay inside the window.
                menu := PopupWindow {
                    close-policy: close-on-click-outside;
                    width: 180px;
                    height: 112px;
                    property <length> gap: 4px;
                    property <bool> below: root.height - anchor.absolute-position.y - anchor.height - gap >= self.height
                        || root.height - anchor.absolute-position.y - anchor.height >= anchor.absolute-position.y;
                    y: below ? anchor.height + gap : -self.height - gap;
                    x: Math.min(Math.max(anchor.absolute-position.x + anchor.width / 2 - self.width / 2, 8px),
                                max(root.width - self.width - 8px, 8px)) - anchor.absolute-position.x;
                    Rectangle {
                        background: rgb(43, 43, 43);
                        border-radius: 8px;
                        VerticalLayout {
                            padding: 8px;
                            for label in ["Rename", "Duplicate", "Delete"]: Rectangle {
                                height: 30px;
                                Text { text: label; color: white; }
                                TouchArea { clicked => { menu.close(); } }
                            }
                        }
                    }
                }
            }

            Text { text: "Shared element"; font-weight: 700; }
            HorizontalLayout {
                spacing: 8px;
                alignment: start;
                for i in 3: card := Rectangle {
                    width: 110px;
                    height: 70px;
                    background: rgb(70, 70, 70);
                    border-radius: 8px;
                    Text { text: "Card " + (i + 1); color: white; }
                    TouchArea {
                        clicked => {
                            // Record the source rectangle in window coordinates, then open.
                            ghost.src-x = card.absolute-position.x;
                            ghost.src-y = card.absolute-position.y;
                            ghost.open = true;
                        }
                    }
                }
            }

            Text { text: "Scroll-linked reveal"; font-weight: 700; }
            flick := Flickable {
                height: 180px;
                content-height: 24 * 36px;
                for i in 24: Rectangle {
                    // Position of the row inside the viewport; the offset is the timeline.
                    property <length> rel: i * 36px + flick.content-y;
                    property <float> t: Motion.progress(rel, flick.height, flick.height * 0.7);
                    y: i * 36px;
                    height: 32px;
                    // Reduced motion keeps the fade and drops the translation.
                    x: reduced-motion ? 0px : (1.0 - t) * 24px;
                    opacity: 0.15 + 0.85 * t;
                    background: rgb(70, 110, 180);
                    border-radius: 6px;
                    Text { text: "Row " + (i + 1); color: white; x: 10px; }
                }
            }
        }

        // The ghost animates between the recorded source rectangle and the detail rectangle.
        ghost := Rectangle {
            in-out property <bool> open: false;
            in-out property <length> src-x;
            in-out property <length> src-y;
            visible: self.open || self.width > 111px;
            x: open ? 40px : src-x;
            y: open ? 40px : src-y;
            width: open ? root.width - 80px : 110px;
            height: open ? root.height - 80px : 70px;
            background: rgb(70, 70, 70);
            border-radius: open ? 16px : 8px;
            animate x { duration: root.move-time; easing: ease-in-out; }
            animate y { duration: root.move-time; easing: ease-in-out; }
            animate width { duration: root.move-time; easing: ease-in-out; }
            animate height { duration: root.move-time; easing: ease-in-out; }
            animate border-radius { duration: root.move-time; easing: ease-in-out; }
            Text { text: "Detail (click to close)"; color: white; opacity: open ? 1 : 0; animate opacity { duration: root.move-time; } }
            TouchArea { clicked => { ghost.open = false; } }
        }
    }
}

fn main() {
    App::new().unwrap().run().unwrap();
}
