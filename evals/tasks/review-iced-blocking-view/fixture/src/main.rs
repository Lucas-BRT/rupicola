use iced::widget::text;
use iced::Element;

#[derive(Default)]
struct App;

#[derive(Debug, Clone)]
enum Message {}

impl App {
    fn update(&mut self, _message: Message) {}

    fn view(&self) -> Element<'_, Message> {
        let notes = std::fs::read_to_string("notes.txt").unwrap_or_default();
        text(notes).into()
    }
}

fn main() -> iced::Result {
    iced::run("notes", App::update, App::view)
}
