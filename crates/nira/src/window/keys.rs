//! The keybinding editor (the preferences' Keys tab): each action with its
//! keys; add a key by pressing it, clear an action's keys, or restore its
//! defaults. Changes are saved to `keys.toml` and take effect at once.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use nira_core::i18n::{t, t_args};
use nira_core::keymap::{Action, Keymap};

use super::actions::FIXED_ACCELS;
use super::NiraWindow;

/// A row of the editor.
pub struct KeyRow {
    action: Action,
    keys: gtk::Box,
    clear: gtk::Button,
    reset: gtk::Button,
}
use crate::chrome::{accel_label, hbox, kbd};

impl NiraWindow {
    pub(super) fn fill_keys_page(&self, root: &gtk::Box, dialog: &gtk::Window) {
        let hint = gtk::Label::new(Some(&t("keys-hint")));
        hint.add_css_class("dim-label");
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        root.append(&hint);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        root.append(&list);
        let reset_all = gtk::Button::with_label(&t("keys-reset-all"));
        reset_all.set_halign(gtk::Align::Start);
        reset_all.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.change_keys(|k| *k = Keymap::default())
        ));
        root.append(&reset_all);
        self.imp().keys_list.replace(Some(list));
        self.imp().capturing.set(None);
        self.fill_key_rows();

        // While capturing, the next key press is the new key.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, mods| win.capture_key(key, mods)
        ));
        dialog.add_controller(keys);
        dialog.connect_close_request(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_| {
                win.imp().keys_list.replace(None);
                win.imp().key_rows.borrow_mut().clear();
                win.imp().capturing.set(None);
                win.imp().refused_key.replace(None);
                glib::Propagation::Proceed
            }
        ));
    }

    /// One row per action the window has; the keys are filled in by
    /// `refresh_key_rows`.
    fn fill_key_rows(&self) {
        let imp = self.imp();
        let Some(list) = imp.keys_list.borrow().clone() else {
            return;
        };
        let mut rows = Vec::new();
        for &action in Action::ALL {
            if self.lookup_action(action.name()).is_none() {
                continue;
            }
            let row = hbox("key-row");
            let name = gtk::Label::new(Some(&t(&format!("action-{}", action.name()))));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            row.append(&name);
            let keys = hbox("key-keys");
            row.append(&keys);
            let button = |icon: &str, tip: String| {
                let b = gtk::Button::from_icon_name(icon);
                b.add_css_class("flat");
                b.set_tooltip_text(Some(&tip));
                b
            };
            let add = button("list-add-symbolic", t("keys-add"));
            add.connect_clicked(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| {
                    win.imp().capturing.set(Some(action));
                    win.imp().refused_key.replace(None);
                    win.refresh_key_rows();
                }
            ));
            let clear = button("edit-clear-symbolic", t("keys-clear"));
            clear.connect_clicked(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| win.change_keys(|k| k.set_accels(action, Vec::new()))
            ));
            let reset = button("edit-undo-symbolic", t("keys-reset"));
            reset.connect_clicked(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| {
                    let defaults = action
                        .default_accels()
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    win.change_keys(|k| k.set_accels(action, defaults))
                }
            ));
            row.append(&add);
            row.append(&clear);
            row.append(&reset);
            list.append(&row);
            rows.push(KeyRow {
                action,
                keys,
                clear,
                reset,
            });
        }
        imp.key_rows.replace(rows);
        self.refresh_key_rows();
    }

    /// Show each row's keys (or the capture prompt) without rebuilding the
    /// rows.
    fn refresh_key_rows(&self) {
        let imp = self.imp();
        let keymap = imp.keymap.borrow().clone();
        let capturing = imp.capturing.get();
        for row in imp.key_rows.borrow().iter() {
            while let Some(child) = row.keys.first_child() {
                row.keys.remove(&child);
            }
            let accels = keymap.accels(row.action);
            if capturing == Some(row.action) {
                let prompt = match imp.refused_key.borrow().as_deref() {
                    Some(key) => t_args("keys-fixed", &[("key", key.to_string().into())]),
                    None => t("keys-press"),
                };
                row.keys.append(&gtk::Label::new(Some(&prompt)));
            } else {
                for accel in accels {
                    if let Some(text) = accel_label(accel) {
                        row.keys.append(&kbd(&text));
                    }
                }
            }
            row.clear.set_sensitive(!accels.is_empty());
            let defaults = row.action.default_accels();
            row.reset.set_sensitive(
                accels
                    .iter()
                    .map(String::as_str)
                    .ne(defaults.iter().copied()),
            );
        }
    }

    /// The key pressed while capturing becomes a key of the action (taken
    /// from any action that had it). Escape cancels.
    fn capture_key(&self, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        let imp = self.imp();
        let Some(action) = imp.capturing.get() else {
            return glib::Propagation::Proceed;
        };
        // Wait for a real key, not a lone modifier.
        if is_modifier(key) {
            return glib::Propagation::Stop;
        }
        let mods = mods & gtk::accelerator_get_default_mod_mask();
        if key == gdk::Key::Escape && mods.is_empty() {
            imp.capturing.set(None);
            imp.refused_key.replace(None);
            self.refresh_key_rows();
            return glib::Propagation::Stop;
        }
        // Stored like the defaults: Shift+R is "<Shift>r".
        let accel = gtk::accelerator_name(key.to_lower(), mods).to_string();
        if is_fixed(&accel) {
            // Keep capturing, so another key can be pressed.
            imp.refused_key
                .replace(Some(crate::chrome::accel_label(&accel).unwrap_or(accel)));
            self.refresh_key_rows();
            return glib::Propagation::Stop;
        }
        imp.capturing.set(None);
        imp.refused_key.replace(None);
        self.change_keys(|k| {
            for &other in Action::ALL {
                let kept: Vec<String> = k
                    .accels(other)
                    .iter()
                    .filter(|a| !same_accel(a, &accel))
                    .cloned()
                    .collect();
                k.set_accels(other, kept);
            }
            let mut mine = k.accels(action).to_vec();
            mine.push(accel.clone());
            k.set_accels(action, mine);
        });
        glib::Propagation::Stop
    }

    /// Change the keymap, save it, and bind the keys anew.
    fn change_keys(&self, f: impl FnOnce(&mut Keymap)) {
        let snapshot = {
            let mut keymap = self.imp().keymap.borrow_mut();
            f(&mut keymap);
            keymap.clone()
        };
        self.persist(Box::new({
            let k = snapshot.clone();
            move || k.save()
        }));
        if let Some(app) = self.application() {
            self.install_accels(&app, &snapshot);
        }
        self.set_status_keys(&snapshot);
        self.set_focus_keys(&snapshot);
        self.refresh_key_rows();
    }
}

/// The key and modifiers of `accel`, with letters lowercased: GTK reads
/// "<Shift>R" and "<Shift>r" as the same key.
fn parsed(accel: &str) -> Option<(gdk::Key, gdk::ModifierType)> {
    gtk::accelerator_parse(accel).map(|(key, mods)| (key.to_lower(), mods))
}

/// Whether two accelerators are the same key, however they are written.
fn same_accel(a: &str, b: &str) -> bool {
    match (parsed(a), parsed(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

/// Whether `accel` is one of the keys that can't be changed.
fn is_fixed(accel: &str) -> bool {
    FIXED_ACCELS
        .iter()
        .flat_map(|(_, accels)| accels.iter())
        .any(|fixed| same_accel(fixed, accel))
}

fn is_modifier(key: gdk::Key) -> bool {
    use gdk::Key;
    matches!(
        key,
        Key::Shift_L
            | Key::Shift_R
            | Key::Control_L
            | Key::Control_R
            | Key::Alt_L
            | Key::Alt_R
            | Key::Super_L
            | Key::Super_R
            | Key::Meta_L
            | Key::Meta_R
            | Key::ISO_Level3_Shift
            | Key::Caps_Lock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test: parsing accelerators needs GTK, which needs a display
    /// and stays on the thread that started it.
    #[test]
    fn accels_compare_as_keys() {
        if gtk::init().is_err() {
            eprintln!("skipped: no display for GTK");
            return;
        }
        assert!(same_accel("<Shift>R", "<Shift>r"));
        assert!(same_accel("<Ctrl>o", "<Control>o"));
        assert!(!same_accel("r", "<Shift>r"));
        assert!(!same_accel("<Control>o", "o"));
        // The fixed keys, however they are written.
        assert!(is_fixed("q"));
        assert!(is_fixed("<Control>O"));
        assert!(is_fixed("<Ctrl>comma"));
        assert!(!is_fixed("<Shift>q"));
        assert!(!is_fixed("<Control>s"));
    }
}
