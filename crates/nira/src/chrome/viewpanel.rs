//! The View settings panel beside the page: image enhancement, the viewer
//! background, and reading options. Switches and choices are bound to
//! `win.*` actions; the window listens to the two sliders.

use gtk::gio;
use gtk::prelude::*;
use nira_core::i18n::t;
use nira_core::settings::Settings;

use super::{kbd, label};

/// Background presets: (action target, swatch CSS class, colour). `auto`
/// follows the shown page's border colour.
pub const BACKGROUNDS: [(&str, &str, Option<[u8; 3]>); 4] = [
    ("black", "swatch-black", Some([0x0E, 0x0F, 0x11])),
    ("grey", "swatch-grey", Some([0x3A, 0x3D, 0x42])),
    ("paper", "swatch-paper", Some([0xED, 0xEB, 0xE4])),
    ("auto", "swatch-auto", None),
];

/// The preset matching the settings (`None` for a custom colour).
pub fn background_preset(settings: &Settings) -> Option<&'static str> {
    if settings.appearance.match_page_color {
        return Some("auto");
    }
    BACKGROUNDS
        .iter()
        .find(|(_, _, c)| *c == Some(settings.appearance.background))
        .map(|(name, _, _)| *name)
}

/// Slideshow intervals offered, in seconds.
pub const SLIDESHOW_SECONDS: [u64; 6] = [3, 5, 8, 10, 15, 30];

/// Slider value (0–100, 50 = unchanged) for a brightness or contrast
/// factor (0.5–1.5), and back.
pub fn slider_value(factor: f64) -> f64 {
    ((factor - 0.5) * 100.0).round().clamp(0.0, 100.0)
}

pub fn slider_factor(value: f64) -> f64 {
    0.5 + value / 100.0
}

pub struct ViewPanel {
    pub root: gtk::ScrolledWindow,
    pub brightness: gtk::Scale,
    pub contrast: gtk::Scale,
    brightness_value: gtk::Label,
    lens_key: gtk::Label,
    contrast_value: gtk::Label,
    slideshow: gtk::MenuButton,
}

fn section(title: &str) -> (gtk::Box, gtk::Box) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 14);
    let heading = label(&title.to_uppercase(), &["section-title"]);
    heading.set_xalign(0.0);
    b.append(&heading);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    b.append(&body);
    (b, body)
}

/// A row with a label (and optional hint below it) and a control at the
/// end.
fn row(title: &str, hint: Option<&str>, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let t = gtk::Label::new(Some(title));
    t.set_xalign(0.0);
    t.set_wrap(true);
    text.append(&t);
    if let Some(hint) = hint {
        let h = label(hint, &["dim"]);
        h.set_xalign(0.0);
        h.set_wrap(true);
        text.append(&h);
    }
    r.append(&text);
    control.set_valign(gtk::Align::Center);
    r.append(control);
    r
}

fn switch(action: &str) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_action_name(Some(action));
    s.set_focus_on_click(false);
    s
}

/// A labelled 0–100 slider with its value shown at the right.
fn slider(title: &str) -> (gtk::Box, gtk::Scale, gtk::Label) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let t = gtk::Label::new(Some(title));
    t.set_hexpand(true);
    t.set_xalign(0.0);
    let value = label("", &["mono", "dim"]);
    top.append(&t);
    top.append(&value);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_draw_value(false);
    scale.set_focusable(false);
    b.append(&top);
    b.append(&scale);
    (b, scale, value)
}

impl ViewPanel {
    pub fn new() -> ViewPanel {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 22);
        body.add_css_class("view-panel-body");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let title = label(&t("panel-title"), &["panel-title"]);
        title.set_hexpand(true);
        title.set_xalign(0.0);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some(&t("panel-close")));
        close.set_action_name(Some("win.view-panel"));
        header.append(&title);
        header.append(&close);
        body.append(&header);

        // Image.
        let (image, rows) = section(&t("panel-image"));
        rows.append(&row(
            &t("panel-auto-enhance"),
            Some(&t("panel-auto-enhance-hint")),
            &switch("win.auto-enhance"),
        ));
        let (b, brightness, brightness_value) = slider(&t("panel-brightness"));
        rows.append(&b);
        let (c, contrast, contrast_value) = slider(&t("panel-contrast"));
        rows.append(&c);
        body.append(&image);

        // Background.
        let (background, rows) = section(&t("panel-background"));
        let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        swatches.set_homogeneous(true);
        for (name, class, _) in BACKGROUNDS {
            let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let fill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            fill.add_css_class("swatch-fill");
            fill.add_css_class(class);
            fill.set_vexpand(true);
            content.append(&fill);
            content.append(&gtk::Label::new(Some(&t(&format!("background-{name}")))));
            let b = gtk::ToggleButton::new();
            b.set_child(Some(&content));
            b.add_css_class("bg-swatch");
            b.set_focus_on_click(false);
            b.set_action_name(Some("win.background"));
            b.set_action_target_value(Some(&name.to_variant()));
            swatches.append(&b);
        }
        rows.append(&swatches);
        body.append(&background);

        // Reading.
        let (reading, rows) = section(&t("panel-reading"));
        let lens_title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        lens_title.append(&gtk::Label::new(Some(&t("panel-lens"))));
        let lens_key = kbd("L");
        lens_title.append(&lens_key);
        let lens_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        lens_title.set_hexpand(true);
        lens_row.append(&lens_title);
        let lens_switch = switch("win.lens");
        lens_switch.set_valign(gtk::Align::Center);
        lens_row.append(&lens_switch);
        rows.append(&lens_row);
        rows.append(&row(
            &t("panel-cover-alone"),
            None,
            &switch("win.cover-alone"),
        ));
        rows.append(&row(
            &t("panel-auto-hide"),
            None,
            &switch("win.auto-hide-controls"),
        ));
        let menu = gio::Menu::new();
        for secs in SLIDESHOW_SECONDS {
            let item = gio::MenuItem::new(Some(&format!("{secs} s")), None);
            item.set_action_and_target_value(
                Some("win.slideshow-delay"),
                Some(&secs.to_string().to_variant()),
            );
            menu.append_item(&item);
        }
        let slideshow = gtk::MenuButton::new();
        slideshow.set_menu_model(Some(&menu));
        slideshow.add_css_class("raised");
        slideshow.set_always_show_arrow(true);
        slideshow.add_css_class("small");
        rows.append(&row(&t("panel-slideshow"), None, &slideshow));
        body.append(&reading);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        body.append(&spacer);
        // Footer: "Reset to defaults" above "All settings…" and "About".
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        footer.add_css_class("panel-footer");
        let link = |text: &str, action: &str| {
            let b = gtk::Button::with_label(text);
            b.add_css_class("link");
            b.set_action_name(Some(action));
            b.set_halign(gtk::Align::Start);
            b
        };
        footer.append(&link(&t("panel-reset"), "win.reset-view"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let all = link(&t("panel-all-settings"), "win.preferences");
        all.set_hexpand(true);
        row.append(&all);
        row.append(&link(&t("panel-about"), "win.about"));
        footer.append(&row);
        body.append(&footer);

        let root = gtk::ScrolledWindow::new();
        root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        root.set_child(Some(&body));
        root.add_css_class("view-panel");
        root.set_size_request(288, -1);
        // The rows' labels expand; without this the panel would take a
        // share of the window's spare width from the page.
        root.set_hexpand(false);
        ViewPanel {
            root,
            brightness,
            contrast,
            brightness_value,
            lens_key,
            contrast_value,
            slideshow,
        }
    }

    /// The lens keycap, from the keymap.
    pub fn set_lens_key(&self, key: &str) {
        self.lens_key.set_text(key);
    }

    /// Show the settings' values on the sliders and the interval button.
    /// The window ignores the sliders' change signals meanwhile.
    pub fn update(&self, settings: &Settings) {
        let b = slider_value(settings.enhance.brightness);
        let c = slider_value(settings.enhance.contrast);
        if self.brightness.value() != b {
            self.brightness.set_value(b);
        }
        if self.contrast.value() != c {
            self.contrast.set_value(c);
        }
        self.brightness_value.set_text(&format!("{b}"));
        self.contrast_value.set_text(&format!("{c}"));
        self.slideshow
            .set_label(&format!("{} s", settings.slideshow.delay_ms / 1000));
    }
}
