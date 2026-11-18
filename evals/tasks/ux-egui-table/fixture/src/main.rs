use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub fn amounts(ui: &mut egui::Ui, rows: &[(String, f64)]) {
    TableBuilder::new(ui)
        .column(Column::auto())
        .column(Column::remainder())
        .header(20.0, |mut h| {
            h.col(|ui| { ui.label("Item"); });
            h.col(|ui| { ui.label("Amount"); });
        })
        .body(|mut body| {
            for (name, value) in rows {
                body.row(18.0, |mut row| {
                    row.col(|ui| { ui.label(name); });
                    row.col(|ui| { ui.label(value.to_string()); });
                });
            }
        });
}
