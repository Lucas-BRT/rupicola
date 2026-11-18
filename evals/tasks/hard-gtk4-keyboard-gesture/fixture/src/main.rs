use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, GestureClick, Image, Orientation};

fn build(app: &Application) {
    let delete_icon = Image::from_icon_name("user-trash-symbolic");
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.append(&delete_icon);
    let click = GestureClick::new();
    click.connect_pressed(|_, _, _, _| println!("deleted"));
    row.add_controller(click);
    ApplicationWindow::builder().application(app).child(&row).build().present();
}
