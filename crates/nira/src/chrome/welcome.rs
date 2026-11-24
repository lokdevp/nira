//! The start screen, shown while no book is open: a drop zone with the
//! open buttons and supported formats, and the "Continue reading" list.

use std::path::Path;

use gtk::prelude::*;
use gtk::{gdk, glib};
use nira_core::history::Entry;
use nira_core::i18n::{t, t_args};

use super::{hbox, kbd, label, vbox};

/// Formats the archive layer opens.
const FORMATS: [&str; 6] = ["CBZ", "CBR", "CB7", "CBT", "PDF", "LHA"];

/// History entries shown.
pub const SHOWN: usize = 8;

pub struct Welcome {
    pub root: gtk::Box,
    pub list: gtk::ListBox,
    empty: gtk::Label,
    covers: std::cell::RefCell<Vec<(String, gtk::Picture)>>,
}

impl Welcome {
    pub fn new() -> Welcome {
        // Left: heading and drop zone.
        let left = vbox("welcome-left");
        left.set_hexpand(true);
        let heading = vbox("welcome-heading");
        let title = label(&t("welcome-title"), &["welcome-title"]);
        title.set_xalign(0.0);
        let sub = label(&t("welcome-sub"), &["welcome-sub"]);
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        heading.append(&title);
        heading.append(&sub);
        left.append(&heading);

        let zone = vbox("dropzone");
        zone.set_vexpand(true);
        let inner = vbox("drop-inner");
        inner.set_valign(gtk::Align::Center);
        inner.set_vexpand(true);
        let icon = gtk::Image::from_icon_name("folder-download-symbolic");
        icon.add_css_class("drop-icon");
        icon.set_halign(gtk::Align::Center);
        inner.append(&icon);
        let texts = vbox("drop-texts");
        texts.append(&label(&t("welcome-drop"), &["drop-title"]));
        texts.append(&label(&t("welcome-or"), &["dim"]));
        inner.append(&texts);

        let buttons = hbox("drop-buttons");
        buttons.set_halign(gtk::Align::Center);
        let open = gtk::Button::new();
        let b = hbox("drop-open");
        b.append(&gtk::Label::new(Some(&t("welcome-open-file"))));
        b.append(&kbd("Ctrl O"));
        open.set_child(Some(&b));
        open.add_css_class("accent");
        open.set_action_name(Some("win.open"));
        let folder = gtk::Button::with_label(&t("welcome-open-folder"));
        folder.add_css_class("raised");
        folder.set_action_name(Some("win.open-folder"));
        buttons.append(&open);
        buttons.append(&folder);
        inner.append(&buttons);

        let chips = hbox("drop-chips");
        chips.set_halign(gtk::Align::Center);
        for f in FORMATS {
            chips.append(&label(f, &["chip", "mono"]));
        }
        chips.append(&label(&t("welcome-image-folder"), &["chip", "mono"]));
        inner.append(&chips);
        zone.append(&inner);
        left.append(&zone);

        // Right: recently read books.
        let right = vbox("welcome-right");
        right.set_hexpand(false);
        let head = hbox("recent-head");
        let h = label(&t("welcome-continue"), &["section-heading"]);
        h.set_hexpand(true);
        h.set_xalign(0.0);
        let clear = gtk::Button::with_label(&t("welcome-clear-history"));
        clear.add_css_class("link");
        clear.set_action_name(Some("win.clear-history"));
        head.append(&h);
        head.append(&clear);
        right.append(&head);

        let list = gtk::ListBox::new();
        list.add_css_class("recent-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_activate_on_single_click(true);
        let empty = label(&t("welcome-history-empty"), &["dim"]);
        empty.set_xalign(0.0);
        right.append(&empty);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_child(Some(&list));
        scroll.set_vexpand(true);
        right.append(&scroll);
        let note = label(&t("welcome-remembered"), &["dim"]);
        note.set_xalign(0.0);
        note.set_wrap(true);
        right.append(&note);

        let root = hbox("welcome");
        root.append(&left);
        root.append(&right);
        Welcome {
            root,
            list,
            empty,
            covers: Default::default(),
        }
    }

    /// Fill the "Continue reading" list. Rows are named after their book's
    /// path; covers arrive later through `set_cover`.
    pub fn set_history(&self, entries: &[Entry]) {
        self.list.remove_all();
        let mut covers = self.covers.borrow_mut();
        covers.clear();
        let now = glib::DateTime::now_local().ok();
        for e in entries {
            let (row, cover) = history_row(e, now.as_ref());
            let r = gtk::ListBoxRow::new();
            r.set_child(Some(&row));
            r.set_widget_name(&e.path);
            self.list.append(&r);
            covers.push((e.path.clone(), cover));
        }
        self.empty.set_visible(entries.is_empty());
    }

    pub fn set_cover(&self, path: &str, texture: &gdk::Texture) {
        for (p, picture) in self.covers.borrow().iter() {
            if p == path {
                picture.set_paintable(Some(&crate::texture::SizedPaintable::new(texture, 46, 67)));
            }
        }
    }
}

/// `~`-abbreviated parent folder of `path`, ending in `/`.
fn short_dir(path: &Path) -> String {
    let parent = path.parent().unwrap_or(path);
    let home = nira_core::paths::home_dir();
    let shown = match parent.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => parent.display().to_string(),
    };
    format!("{}/", shown.trim_end_matches('/'))
}

/// "Today", "Yesterday", a weekday within the last week, otherwise "Oct 19"
/// (with the year when it differs).
pub fn when_label(now: &glib::DateTime, then: &glib::DateTime) -> String {
    let midnight = |d: &glib::DateTime| {
        glib::DateTime::new(
            &d.timezone(),
            d.year(),
            d.month(),
            d.day_of_month(),
            0,
            0,
            0.0,
        )
        .map(|m| m.to_unix())
        .unwrap_or(0)
    };
    // Calendar days, rounded so DST shifts don't matter.
    let days = ((midnight(now) - midnight(then)) as f64 / 86400.0).round() as i64;
    let fmt = |f: &str| then.format(f).map(|s| s.to_string()).unwrap_or_default();
    match days {
        0 => t("when-today"),
        1 => t("when-yesterday"),
        2..=6 => fmt("%a"),
        _ if now.year() == then.year() => fmt("%b %-d"),
        _ => fmt("%b %-d, %Y"),
    }
}

fn history_row(e: &Entry, now: Option<&glib::DateTime>) -> (gtk::Box, gtk::Picture) {
    let path = Path::new(&e.path);
    let row = hbox("recent-row");
    let cover = gtk::Picture::new();
    cover.set_can_shrink(true);
    cover.set_content_fit(gtk::ContentFit::Cover);
    cover.add_css_class("cover");
    cover.set_valign(gtk::Align::Center);
    row.append(&cover);

    let col = vbox("recent-text");
    col.set_hexpand(true);
    col.set_valign(gtk::Align::Center);
    let top = hbox("recent-top");
    let mut name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| e.path.clone());
    if path.is_dir() {
        name.push('/');
    }
    let name = label(&name, &["recent-name"]);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_hexpand(true);
    name.set_xalign(0.0);
    let when = now
        .zip(glib::DateTime::from_unix_local(e.time).ok())
        .map(|(now, then)| when_label(now, &then))
        .unwrap_or_default();
    top.append(&name);
    top.append(&label(&when, &["dim"]));
    col.append(&top);

    let dir = label(&short_dir(path), &["dim", "mono"]);
    dir.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    dir.set_xalign(0.0);
    col.append(&dir);

    let bottom = hbox("recent-bottom");
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("bar");
    bar.set_hexpand(true);
    bar.set_valign(gtk::Align::Center);
    let total = e.total.max(1);
    bar.set_fraction(e.page.min(total) as f64 / total as f64);
    let progress = if e.finished() {
        t("welcome-finished")
    } else {
        t_args(
            "welcome-progress",
            &[("page", e.page.into()), ("total", e.total.into())],
        )
    };
    bottom.append(&bar);
    bottom.append(&label(&progress, &["recent-progress"]));
    col.append(&bottom);
    row.append(&col);
    (row, cover)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_dir_abbreviates_home() {
        let home = nira_core::paths::home_dir();
        assert_eq!(short_dir(&home.join("Comics/A/ch1.cbz")), "~/Comics/A/");
        assert_eq!(short_dir(&home.join("x.cbz")), "~/");
        assert_eq!(short_dir(Path::new("/srv/books/x.cbz")), "/srv/books/");
    }

    #[test]
    fn when_labels() {
        let tz = glib::TimeZone::utc();
        let at = |y, m, d, h| glib::DateTime::new(&tz, y, m, d, h, 0, 0.0).unwrap();
        let now = at(2026, 10, 29, 10);
        assert_eq!(when_label(&now, &at(2026, 10, 29, 1)), "Today");
        assert_eq!(when_label(&now, &at(2026, 10, 28, 23)), "Yesterday");
        // 2026-10-26 is a Monday.
        assert_eq!(when_label(&now, &at(2026, 10, 26, 12)), "Mon");
        assert_eq!(when_label(&now, &at(2026, 10, 19, 12)), "Oct 19");
        assert_eq!(when_label(&now, &at(2025, 12, 31, 12)), "Dec 31, 2025");
    }
}
