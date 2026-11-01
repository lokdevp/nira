//! The About dialog: GTK's own, with the logo, version, credits and license.

use gtk::prelude::*;
use gtk::{gdk, glib};
use nira_core::i18n::t;

use super::NiraWindow;

/// The logo, rendered from `assets/nira.svg` by `assets/render-icons.sh`.
const LOGO_PNG: &[u8] = include_bytes!("../../../../assets/nira-about.png");

impl NiraWindow {
    pub(super) fn show_about(&self) {
        let dialog = gtk::AboutDialog::builder()
            .transient_for(self)
            .modal(true)
            .program_name("Nira")
            .version(env!("CARGO_PKG_VERSION"))
            .comments(t("about-comments"))
            .website(env!("CARGO_PKG_HOMEPAGE"))
            .website_label(t("about-website"))
            .copyright("© 2026 lokesh, Rippa The Hutt")
            .license_type(gtk::License::Gpl20)
            .authors(["lokesh", "Rippa The Hutt"])
            .artists(["lokesh"])
            .build();
        match gdk::Texture::from_bytes(&glib::Bytes::from_static(LOGO_PNG)) {
            Ok(logo) => dialog.set_logo(Some(&logo)),
            Err(e) => log::warn!("about: cannot load the logo: {e}"),
        }
        dialog.present();
    }
}
