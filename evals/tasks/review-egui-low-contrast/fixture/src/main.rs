use eframe::egui::{self, Color32, RichText};

struct App {
    status: String,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.visuals_mut().panel_fill = Color32::WHITE;
        ui.heading("Backup");
        ui.label(RichText::new("Last run: yesterday").color(Color32::from_gray(200)));
        ui.label(RichText::new(&self.status).color(Color32::from_gray(210)));
    }
}

fn main() -> eframe::Result {
    let app = App { status: "All files are up to date".into() };
    eframe::run_native("backup", eframe::NativeOptions::default(), Box::new(|_| Ok(Box::new(app))))
}
