use eframe::egui;

struct App {
    entries: Vec<String>,
    load_failed: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            load_failed: true,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("Entries");
        for entry in &self.entries {
            ui.label(entry);
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "entries",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}
