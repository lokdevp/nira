//! Focus mode's overlays: the hint pill at the top (fades after a moment),
//! the page label at the bottom right and the progress line along the
//! bottom edge.

use gtk::prelude::*;
use nira_core::i18n::t;

use super::{kbd, label};
use crate::theme::px;

pub struct FocusOverlay {
    pub hint: gtk::Box,
    exit_key: gtk::Label,
    pub label: gtk::Label,
    pub progress: gtk::ProgressBar,
}

impl FocusOverlay {
    pub fn new() -> FocusOverlay {
        let hint = gtk::Box::new(gtk::Orientation::Horizontal, px(14));
        hint.add_css_class("focus-hint");
        hint.add_css_class("autohide");
        hint.set_halign(gtk::Align::Center);
        hint.set_valign(gtk::Align::Start);
        hint.set_margin_top(px(18));
        hint.append(&gtk::Label::new(Some(&t("focus-hint"))));
        let exit = gtk::Button::new();
        let exit_key = kbd("Esc");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, px(8));
        content.append(&exit_key);
        content.append(&gtk::Label::new(Some(&t("focus-exit"))));
        exit.set_child(Some(&content));
        exit.set_focus_on_click(false);
        exit.set_action_name(Some("win.fullscreen"));
        hint.append(&exit);

        let label = label("", &["mono", "focus-label"]);
        label.set_halign(gtk::Align::End);
        label.set_valign(gtk::Align::End);
        label.set_margin_end(px(20));
        label.set_margin_bottom(px(14));
        label.set_can_target(false);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("thin");
        progress.set_valign(gtk::Align::End);
        progress.set_can_target(false);

        let overlay = FocusOverlay {
            hint,
            exit_key,
            label,
            progress,
        };
        overlay.set_visible(false);
        overlay.progress.set_visible(false);
        overlay
    }

    /// The hint and label; the progress line has its own setting.
    pub fn set_visible(&self, on: bool) {
        self.hint.set_visible(on);
        self.label.set_visible(on);
    }

    /// `text` like "38–37 / 41 · R→L"; `fraction` read so far. Manga
    /// fills the line from the right.
    pub fn update(&self, text: &str, fraction: f64, manga: bool) {
        self.label.set_text(text);
        self.progress.set_fraction(fraction);
        self.progress.set_inverted(manga);
    }

    pub fn set_exit_key(&self, key: &str) {
        self.exit_key.set_text(key);
    }
}
