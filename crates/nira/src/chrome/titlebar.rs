//! The title bar: the app mark (which opens the About dialog), the open
//! file's name and the window buttons.

use gtk::prelude::*;

use nira_core::i18n::t;

use super::label;

pub struct TitleBar {
    pub root: gtk::HeaderBar,
    file: gtk::Label,
    separator: gtk::Label,
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

        let title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        title.add_css_class("window-title");
        let file = gtk::Label::new(None);
        file.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let separator = label(" — ", &["app-name"]);
        let app = label("Nira", &["app-name"]);
        title.append(&file);
        title.append(&separator);
        title.append(&app);
        root.set_title_widget(Some(&title));
        TitleBar {
            root,
            file,
            separator,
        }
    }

    /// `file`: the open book's file name, if any.
    pub fn update(&self, file: Option<&str>) {
        self.file.set_text(file.unwrap_or(""));
        self.file.set_visible(file.is_some());
        self.separator.set_visible(file.is_some());
    }
}
