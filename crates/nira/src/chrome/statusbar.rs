//! The status bar: the shown page's size, file name and file size, then
//! key hints.

use gtk::prelude::*;
use nira_core::i18n::t;

use super::{accel_label, hbox, kbd, label};

pub struct StatusBar {
    pub root: gtk::Box,
    dims: gtk::Label,
    file: gtk::Label,
    bytes: gtk::Label,
    turn_keys: gtk::Box,
    focus_keys: gtk::Box,
}

/// What the status bar shows about the current page.
#[derive(Default)]
pub struct PageStatus {
    pub dims: Option<(u32, u32)>,
    pub file: String,
    pub bytes: Option<u64>,
}

impl StatusBar {
    pub fn new() -> StatusBar {
        let root = hbox("statusbar");
        let dims = gtk::Label::new(None);
        let file = label("", &["mono"]);
        file.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let bytes = gtk::Label::new(None);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        root.append(&dims);
        root.append(&file);
        root.append(&bytes);
        root.append(&spacer);

        let hint = |keys: &gtk::Box, text: &str| {
            let b = hbox("status-hint");
            b.append(keys);
            b.append(&gtk::Label::new(Some(text)));
            root.append(&b);
        };
        let turn_keys = hbox("status-keys");
        hint(&turn_keys, &t("hint-turn-page"));
        let focus_keys = hbox("status-keys");
        hint(&focus_keys, &t("hint-focus-mode"));
        StatusBar {
            root,
            dims,
            file,
            bytes,
            turn_keys,
            focus_keys,
        }
    }

    pub fn update(&self, page: &PageStatus) {
        let dims = page
            .dims
            .map(|(w, h)| format!("{w} × {h} px"))
            .unwrap_or_default();
        self.dims.set_text(&dims);
        self.file.set_text(&page.file);
        let bytes = page
            .bytes
            .map(|b| gtk::glib::format_size(b).to_string())
            .unwrap_or_default();
        self.bytes.set_text(&bytes);
    }

    /// Show the first key of each action as the hints' keycaps.
    pub fn set_keys(&self, previous: Option<&str>, next: Option<&str>, focus: Option<&str>) {
        fill(&self.turn_keys, &[previous, next]);
        fill(&self.focus_keys, &[focus]);
    }
}

fn fill(keys: &gtk::Box, accels: &[Option<&str>]) {
    while let Some(child) = keys.first_child() {
        keys.remove(&child);
    }
    for label in accels.iter().flatten().filter_map(|a| accel_label(a)) {
        keys.append(&kbd(&label));
    }
    keys.parent()
        .inspect(|p| p.set_visible(keys.first_child().is_some()));
}
