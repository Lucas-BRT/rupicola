use iced::widget::{column, text};
use iced::{Color, Element};

fn view<'a, Message: 'a>() -> Element<'a, Message> {
    let body = Color::from_rgb8(0x59, 0x59, 0x59);
    let hint = Color::from_rgb8(0x77, 0x77, 0x77);
    // The window background is plain white.
    column![
        text("Quarterly report").size(18).color(body),
        text("Last edited three days ago").size(14).color(hint),
    ]
    .into()
}
