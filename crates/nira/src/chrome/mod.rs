//! The main window's chrome, built from the "Comic Reader Wireframe"
//! design: title bar, toolbar and status bar. Each part is a struct of
//! widgets whose buttons are bound to `win.*` actions, so they need no
//! signal handlers; the window pushes state into them with `update`.
//! Styling lives in `style.css`.

pub mod focus;
pub mod lens;
pub mod pagebar;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;
pub mod toolbar;
pub mod viewpanel;
pub mod welcome;

use gtk::prelude::*;

pub fn icon_button(icon: &str, tooltip: &str, action: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("icon-btn");
    b.set_tooltip_text(Some(tooltip));
    b.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    b.set_focus_on_click(false);
    b.set_valign(gtk::Align::Center);
    b.set_action_name(Some(action));
    b
}

/// A toggle bound to a boolean stateful action.
pub fn icon_toggle(icon: &str, tooltip: &str, action: &str) -> gtk::ToggleButton {
    let b = gtk::ToggleButton::new();
    b.set_icon_name(icon);
    b.add_css_class("icon-btn");
    b.set_tooltip_text(Some(tooltip));
    b.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    b.set_focus_on_click(false);
    b.set_valign(gtk::Align::Center);
    b.set_action_name(Some(action));
    b
}

/// A keycap label.
pub fn kbd(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("kbd");
    l.add_css_class("mono");
    l.set_valign(gtk::Align::Center);
    l
}

pub fn vsep() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("vsep");
    b.set_valign(gtk::Align::Center);
    b
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// A segmented control over a string-state action: one button per
/// `(label, target)`. The action's state checks the matching button.
pub fn segmented(action: &str, items: &[(&str, &str)]) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    group.add_css_class("segmented");
    group.set_valign(gtk::Align::Center);
    for (text, target) in items {
        let b = gtk::ToggleButton::with_label(text);
        b.set_focus_on_click(false);
        b.set_action_name(Some(action));
        b.set_action_target_value(Some(&target.to_variant()));
        group.append(&b);
    }
    group
}

/// Short keycap text for an accelerator: arrows as glyphs, otherwise GTK's
/// label (`Ctrl+O`, `Page Down`).
pub fn accel_label(accel: &str) -> Option<String> {
    use gtk::gdk::Key;
    let (key, mods) = gtk::accelerator_parse(accel)?;
    let plain = mods.is_empty();
    Some(match key {
        Key::Left if plain => "←".into(),
        Key::Right if plain => "→".into(),
        Key::Up if plain => "↑".into(),
        Key::Down if plain => "↓".into(),
        Key::Escape if plain => "Esc".into(),
        _ => gtk::accelerator_get_label(key, mods).to_string(),
    })
}
