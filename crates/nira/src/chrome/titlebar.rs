//! The title bar: the app's name and the window buttons. The book's title
//! is in the toolbar, so it isn't repeated here; About is in the View panel.

use gtk::prelude::*;

use super::label;

pub struct TitleBar {
    pub root: gtk::HeaderBar,
}

impl TitleBar {
    pub fn new() -> TitleBar {
        let root = gtk::HeaderBar::new();
        root.add_css_class("titlebar");
        let title = label("Nira", &["app-name"]);
        root.set_title_widget(Some(&title));
        TitleBar { root }
    }
}
