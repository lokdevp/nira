//! The toolbar: Open, the page sidebar and Close book on the left; the book's title and
//! progress between the previous/next book buttons in the middle; layout,
//! direction, fit, zoom, the lens, bookmark, View panel and focus mode on
//! the right.

use gtk::gio;
use gtk::prelude::*;
use nira_core::i18n::{t, t_args};
use nira_core::zoom::FitMode;

use super::{icon_button, icon_toggle, label, segmented, vsep};
use crate::theme::px;

pub struct Toolbar {
    /// Holds the bar without imposing its width on the window: narrow
    /// windows hide groups (`set_collapse`) instead.
    pub root: gtk::ScrolledWindow,
    title: gtk::Label,
    subtitle: gtk::Label,
    book_nav: [gtk::Button; 2],
    layout: gtk::Box,
    direction: gtk::Box,
    fit: gtk::MenuButton,
    zoom_pill: gtk::Box,
    zoom: gtk::Label,
    lens: gtk::ToggleButton,
}

/// What the toolbar shows.
pub struct ToolbarState<'a> {
    pub title: &'a str,
    /// "Page X of N · P% read", or empty without a book.
    pub progress: &'a str,
    /// "73%", or empty before the page size is known.
    pub zoom: &'a str,
    pub fit: FitMode,
}

impl Toolbar {
    pub fn new() -> Toolbar {
        // Left: Open, the page sidebar and Close book.
        let start = gtk::Box::new(gtk::Orientation::Horizontal, px(4));
        let open = gtk::Button::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, px(8));
        content.append(&gtk::Image::from_icon_name("folder-open-symbolic"));
        content.append(&gtk::Label::new(Some(&t("toolbar-open"))));
        open.set_child(Some(&content));
        open.add_css_class("raised");
        open.set_valign(gtk::Align::Center);
        open.set_focus_on_click(false);
        open.set_tooltip_text(Some(&t("open-tooltip")));
        open.set_action_name(Some("win.open"));
        start.append(&open);
        start.append(&icon_toggle(
            "sidebar-show-symbolic",
            &t("action-thumbnails"),
            "win.thumbnails",
        ));
        start.append(&icon_button(
            "window-close-symbolic",
            &t("action-close-book"),
            "win.close-book",
        ));

        // Middle: previous book, title and progress, next book.
        let center = gtk::Box::new(gtk::Orientation::Horizontal, px(6));
        let prev = icon_button(
            "media-skip-backward-symbolic",
            &t("toolbar-previous-book"),
            "win.previous-book",
        );
        let next = icon_button(
            "media-skip-forward-symbolic",
            &t("toolbar-next-book"),
            "win.next-book",
        );
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
        titles.set_valign(gtk::Align::Center);
        let title = label("", &["book-title"]);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(36);
        let subtitle = label("", &["book-sub"]);
        titles.append(&title);
        titles.append(&subtitle);
        center.append(&prev);
        center.append(&titles);
        center.append(&next);

        // Right: layout, direction, fit, zoom, lens | bookmark, panel, focus.
        let end = gtk::Box::new(gtk::Orientation::Horizontal, px(8));
        let layout = segmented(
            "win.page-layout",
            &[
                (&t("layout-single"), "single"),
                (&t("layout-spread"), "spread"),
            ],
        );
        let direction = segmented(
            "win.direction",
            &[(&t("direction-ltr"), "ltr"), (&t("direction-rtl"), "rtl")],
        );

        let menu = gio::Menu::new();
        for mode in FitMode::ALL {
            let item = gio::MenuItem::new(Some(&t(&format!("fit-{}", mode.name()))), None);
            item.set_action_and_target_value(Some("win.fit"), Some(&mode.name().to_variant()));
            menu.append_item(&item);
        }
        let fit = gtk::MenuButton::new();
        fit.set_menu_model(Some(&menu));
        fit.add_css_class("raised");
        fit.set_always_show_arrow(true);
        fit.set_valign(gtk::Align::Center);
        fit.set_focus_on_click(false);
        fit.set_tooltip_text(Some(&t("toolbar-fit")));

        let zoom_pill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        zoom_pill.add_css_class("zoom-pill");
        zoom_pill.set_valign(gtk::Align::Center);
        let zoom_out = gtk::Button::from_icon_name("list-remove-symbolic");
        zoom_out.set_tooltip_text(Some(&t("action-zoom-out")));
        zoom_out.set_action_name(Some("win.zoom-out"));
        let zoom = label("", &["mono"]);
        let zoom_in = gtk::Button::from_icon_name("list-add-symbolic");
        zoom_in.set_tooltip_text(Some(&t("action-zoom-in")));
        zoom_in.set_action_name(Some("win.zoom-in"));
        for b in [&zoom_out, &zoom_in] {
            b.set_focus_on_click(false);
        }
        zoom_pill.append(&zoom_out);
        zoom_pill.append(&zoom);
        zoom_pill.append(&zoom_in);

        end.append(&layout);
        end.append(&direction);
        end.append(&fit);
        let lens = icon_toggle("edit-find-symbolic", &t("action-lens"), "win.lens");

        end.append(&zoom_pill);
        end.append(&lens);
        end.append(&vsep());
        end.append(&icon_toggle(
            "bookmark-new-symbolic",
            &t("toolbar-bookmark"),
            "win.bookmark",
        ));
        end.append(&icon_toggle(
            "emblem-system-symbolic",
            &t("toolbar-view-settings"),
            "win.view-panel",
        ));
        end.append(&icon_button(
            "view-fullscreen-symbolic",
            &t("toolbar-focus"),
            "win.fullscreen",
        ));

        let bar = gtk::CenterBox::new();
        bar.add_css_class("toolbar");
        bar.set_start_widget(Some(&start));
        bar.set_center_widget(Some(&center));
        bar.set_end_widget(Some(&end));
        let root = gtk::ScrolledWindow::new();
        root.set_policy(gtk::PolicyType::External, gtk::PolicyType::Never);
        root.set_child(Some(&bar));
        Toolbar {
            root,
            title,
            subtitle,
            book_nav: [prev, next],
            layout,
            direction,
            fit,
            zoom_pill,
            zoom,
            lens,
        }
    }

    pub fn update(&self, state: &ToolbarState) {
        self.title.set_text(state.title);
        self.subtitle.set_text(state.progress);
        self.subtitle.set_visible(!state.progress.is_empty());
        self.zoom.set_text(state.zoom);
        self.fit.set_label(&t(&format!("fit-{}", state.fit.name())));
    }

    /// The lens button's tooltip, with its key from the keymap if bound.
    pub fn set_lens_key(&self, key: Option<&str>) {
        let tip = match key {
            Some(key) => t_args("toolbar-lens", &[("key", key.into())]),
            None => t("action-lens"),
        };
        self.lens.set_tooltip_text(Some(&tip));
        self.lens
            .update_property(&[gtk::accessible::Property::Label(&tip)]);
    }

    /// Hide groups as the window narrows (0 = everything shown). Order:
    /// zoom pill, direction, layout, then the book buttons.
    pub fn set_collapse(&self, level: usize) {
        self.zoom_pill.set_visible(level < 1);
        self.direction.set_visible(level < 2);
        self.layout.set_visible(level < 3);
        for b in &self.book_nav {
            b.set_visible(level < 4);
        }
    }

    /// The collapse level for a window `width` pixels wide.
    pub fn collapse_for(width: i32) -> usize {
        [1120, 1000, 880, 760]
            .iter()
            .filter(|&&limit| width < limit)
            .count()
    }
}
