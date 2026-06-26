use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};

fn main() {
    let app = Application::builder()
        .application_id("io.github.codemonkeyninja.lenzu.gtk4-test")
        .build();

    app.connect_activate(|app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("GTK4 Lens Test")
            .default_width(400)
            .default_height(400)
            .decorated(false)
            .build();

        // TODO: add x11rb cursor tracking, window positioning, click-through
        // once the prototypes side confirms the GTK3→GTK4 migration patterns.
        // See https://github.com/CodeMonkeyNinja/lenzu/wiki/prototypes-desktop-issues §1 for workarounds.

        window.present();
    });

    app.run();
}
