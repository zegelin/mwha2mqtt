mod application;
mod main_window;
mod zone_control;

use self::application::MwhaMixerApplication;
use self::main_window::MainWindow;

// use config::{GETTEXT_PACKAGE, LOCALEDIR, PKGDATADIR};
// use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain};
use gtk::gio;
use gtk::prelude::*;

fn main() {
    // // Set up gettext translations
    // bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    // bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8")
    //     .expect("Unable to set the text domain encoding");
    // textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    // Load resources
    // let resources = gio::Resource::load(PKGDATADIR.to_owned() + "/gnome-builder-test2.gresource")
    //     .expect("Could not load resources");
    // gio::resources_register(&resources);
    gio::resources_register_include!("compiled.gresource")
        .expect("Failed to register resources.");

    // Create a new GtkApplication. The application manages our main loop,
    // application windows, integration with the window manager/compositor, and
    // desktop features such as file opening and single-instance applications.
    let app = MwhaMixerApplication::new("com.zegelin.mwhamixergtk", &gio::ApplicationFlags::empty());

    // Run the application. This function will block until the application
    // exits. Upon return, we have our exit code to return to the shell. (This
    // is the code you see when you do `echo $?` after running a command in a
    // terminal.
    std::process::exit(app.run().into());
}