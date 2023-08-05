use gtk::glib::Object;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/zegelin/mwhamixergtk/zone_control.ui.xml")]
    pub struct ZoneControl {
        // #[template_child]
        // pub header_bar: TemplateChild<gtk::HeaderBar>,

        // #[template_child]
        // pub scroll: TemplateChild<gtk::ScrolledWindow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ZoneControl {
        const NAME: &'static str = "ZoneControl";
        type Type = super::ZoneControl;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ZoneControl {}
    impl WidgetImpl for ZoneControl {}
    impl BoxImpl for ZoneControl {}
    // impl WindowImpl for ZoneControl {}
    // impl ApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct ZoneControl(ObjectSubclass<imp::ZoneControl>)
        @extends gtk::Widget, gtk::Box,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ZoneControl {
    pub fn new() -> Self {
        Object::builder().build()
    }
}