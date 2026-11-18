use iced::widget::text;
use iced::Element;

fn selection_label<'a, Message: 'a>(n: usize) -> Element<'a, Message> {
    let noun = if n == 1 { "file" } else { "files" };
    text(format!("{} {} selected", n, noun)).into()
}
