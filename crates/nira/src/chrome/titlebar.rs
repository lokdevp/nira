//! The title bar: the app mark (which opens the About dialog), the app's
//! name and the window buttons. The book's title is in the toolbar, so it
//! isn't repeated here.

use gtk::prelude::*;

use nira_core::i18n::t;

use super::label;

pub struct TitleBar {
    pub root: gtk::HeaderBar,
}

impl TitleBar {
    pub fn new() -> TitleBar {
        let root = gtk::HeaderBar::new();
        root.add_css_class("titlebar");
        // The app mark opens the About dialog.
        let mark = gtk::Button::with_label("N");
        mark.add_css_class("app-mark");
        mark.set_action_name(Some("win.about"));
        mark.set_tooltip_text(Some(&t("about-title")));
        mark.set_valign(gtk::Align::Center);
        root.pack_start(&mark);

        let title = label("Nira", &["app-name"]);
        root.set_title_widget(Some(&title));
        TitleBar { root }
    }
}
