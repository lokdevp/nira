//! The floating page bar over the reading area: previous, a page scrubber,
//! "page / total", next, and the slideshow button. Plus the click zones
//! along the left and right edges.

use gtk::prelude::*;
use nira_core::i18n::t;

use super::{icon_toggle, label, vsep};

pub struct PageBar {
    pub root: gtk::Box,
    pub scale: gtk::Scale,
    count: gtk::Label,
    slideshow: gtk::ToggleButton,
    left: gtk::Button,
    right: gtk::Button,
    /// Click zones along the reading area's left and right edges.
    pub zones: [gtk::Button; 2],
}

fn nav_button(icon: &str, action: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("nav");
    b.set_focus_on_click(false);
    b.set_action_name(Some(action));
    b
}

fn zone(icon: &str, action: &str, align: gtk::Align) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("click-zone");
    b.add_css_class("autohide");
    b.set_focus_on_click(false);
    b.set_action_name(Some(action));
    b.set_halign(align);
    b.set_valign(gtk::Align::Fill);
    // Leave the page bar's strip free.
    b.set_margin_bottom(88);
    b
}

impl PageBar {
    pub fn new() -> PageBar {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.add_css_class("pagebar");
        root.add_css_class("autohide");
        root.set_halign(gtk::Align::Center);
        root.set_valign(gtk::Align::End);
        root.set_margin_bottom(20);
        root.set_margin_start(16);
        root.set_margin_end(16);
        root.set_size_request(620, -1);

        let left = nav_button("go-previous-symbolic", "win.page-left");
        let right = nav_button("go-next-symbolic", "win.page-right");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 2.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_focusable(false);
        scale.set_margin_start(6);
        scale.set_margin_end(6);
        let count = label("", &["mono", "count"]);
        let slideshow = icon_toggle(
            "media-playback-start-symbolic",
            &t("action-slideshow"),
            "win.slideshow",
        );
        root.append(&left);
        root.append(&scale);
        root.append(&count);
        root.append(&right);
        root.append(&vsep());
        root.append(&slideshow);

        let zones = [
            zone("go-previous-symbolic", "win.page-left", gtk::Align::Start),
            zone("go-next-symbolic", "win.page-right", gtk::Align::End),
        ];
        PageBar {
            root,
            scale,
            count,
            slideshow,
            left,
            right,
            zones,
        }
    }

    /// `page` is 1-based. In manga mode the scrubber runs right to left and
    /// the left arrow means "next".
    pub fn update(&self, page: usize, total: usize, manga: bool) {
        self.scale.set_range(1.0, total.max(2) as f64);
        self.scale.set_value(page as f64);
        self.scale.set_sensitive(total > 1);
        self.scale.set_inverted(manga);
        self.count.set_markup(&format!(
            "{page} <span foreground=\"#8A9098\">/ {total}</span>"
        ));
        let (left, right) = if manga {
            (t("action-next-page"), t("action-previous-page"))
        } else {
            (t("action-previous-page"), t("action-next-page"))
        };
        for (button, tip) in [
            (&self.left, &left),
            (&self.right, &right),
            (&self.zones[0], &left),
            (&self.zones[1], &right),
        ] {
            button.set_tooltip_text(Some(tip));
        }
    }

    pub fn set_playing(&self, playing: bool) {
        self.slideshow.set_icon_name(if playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
    }

    /// Fade the bar and the click zones out (or back in).
    pub fn set_faded(&self, faded: bool) {
        for w in [
            self.root.upcast_ref::<gtk::Widget>(),
            self.zones[0].upcast_ref(),
            self.zones[1].upcast_ref(),
        ] {
            if faded {
                w.add_css_class("faded");
            } else {
                w.remove_css_class("faded");
            }
        }
    }
}
