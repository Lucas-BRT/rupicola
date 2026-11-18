use libadwaita as adw;
use adw::prelude::*;

fn report_save_error(overlay: &adw::ToastOverlay, reason: &str) {
    let toast = adw::Toast::new(&format!("Failed to save: {reason}"));
    toast.set_timeout(2);
    overlay.add_toast(toast);
}
