use iced::widget::text_input;
use iced::{border, Background, Color, Theme};

pub fn field(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::WHITE),
        border: border::color(Color::from_rgb8(0xBB, 0xBB, 0xBB)).width(1),
        icon: Color::BLACK,
        placeholder: Color::from_rgb8(0x59, 0x59, 0x59),
        value: Color::BLACK,
        selection: Color::from_rgb8(0xA0, 0xC4, 0xFF),
    }
}
