//! Which book is open: the reading history (resume and "Continue
//! reading"), the start screen, Open folder, drag and drop, and moving to
//! the previous or next book.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use nira_core::i18n::t;
use nira_core::reader::SlotImage;

use super::NiraWindow;
use crate::chrome::welcome::{Welcome, SHOWN};
use crate::texture;
use crate::workers;

impl NiraWindow {
    pub(super) fn build_welcome(&self) -> gtk::Box {
        let welcome = Welcome::new();
        welcome.list.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| win.open(PathBuf::from(row.widget_name().as_str()), None)
        ));
        let root = welcome.root.clone();
        let _ = self.imp().welcome.set(welcome);
        root
    }

    pub(super) fn add_book_actions(&self) {
        let entry = |name: &str, f: fn(&NiraWindow)| {
            gio::ActionEntry::builder(name)
                .activate(move |win: &NiraWindow, _, _| f(win))
                .build()
        };
        self.add_action_entries([
            entry("open-folder", |w| w.choose_folder()),
            entry("clear-history", |w| w.clear_history()),
            entry("previous-book", |w| w.open_sibling(false, None)),
            entry("next-book", |w| w.open_sibling(true, None)),
        ]);
    }

    /// Open whatever file or folder is dropped on the window.
    pub(super) fn accept_drops(&self) {
        let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        target.connect_drop(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                let Ok(files) = value.get::<gdk::FileList>() else {
                    return false;
                };
                match files.files().first().and_then(|f| f.path()) {
                    Some(path) => {
                        win.open(path, None);
                        true
                    }
                    None => false,
                }
            }
        ));
        self.add_controller(target);
    }

    fn choose_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(t("open-folder-dialog-title"))
            .modal(true)
            .build();
        dialog.select_folder(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |res| {
                    if let Some(path) = res.ok().and_then(|f| f.path()) {
                        win.open(path, None);
                    }
                }
            ),
        );
    }

    /// Where to open `path` when no page is asked for: where it was left,
    /// or the start once finished.
    pub(super) fn resume_page(&self, path: &std::path::Path) -> usize {
        self.imp()
            .history
            .borrow()
            .get(&path.to_string_lossy())
            .map_or(0, |e| e.resume_page())
    }

    /// Remember where the open book is (when it changed), and save it in
    /// the background.
    pub(super) fn record_position(&self) {
        let Some((path, page, total)) = self.with_reader(|r| {
            let book = r.book()?;
            // The last page shown, so reading a final spread finishes the
            // book.
            let last = r.spread().iter().max().copied()?;
            Some((
                book.path.to_string_lossy().into_owned(),
                last as u32 + 1,
                r.page_count() as u32,
            ))
        }) else {
            return;
        };
        let imp = self.imp();
        let key = (path.clone(), page);
        if imp.recorded.borrow().as_ref() == Some(&key) {
            return;
        }
        imp.recorded.replace(Some(key));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let snapshot = {
            let mut history = imp.history.borrow_mut();
            history.record(&path, page, total, now);
            history.clone()
        };
        self.persist(Box::new(move || snapshot.save()));
    }

    /// Open the most recently read book that still exists.
    pub fn reopen_last(&self) {
        let last = self
            .imp()
            .history
            .borrow()
            .items
            .iter()
            .find(|e| std::path::Path::new(&e.path).exists())
            .map(|e| PathBuf::from(&e.path));
        if let Some(path) = last {
            self.open(path, None);
        }
    }

    fn clear_history(&self) {
        let snapshot = {
            let mut history = self.imp().history.borrow_mut();
            history.clear();
            history.clone()
        };
        self.persist(Box::new(move || snapshot.save()));
        self.refresh_welcome();
    }

    /// Fill "Continue reading" from the history, covers from the thumbnail
    /// cache in the background.
    pub(super) fn refresh_welcome(&self) {
        let imp = self.imp();
        let Some(welcome) = imp.welcome.get() else {
            return;
        };
        let entries: Vec<_> = imp
            .history
            .borrow()
            .items
            .iter()
            .filter(|e| std::path::Path::new(&e.path).exists())
            .take(SHOWN)
            .cloned()
            .collect();
        welcome.set_history(&entries);
        let paths = entries.into_iter().map(|e| e.path).collect();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                for (path, w, h, rgba) in workers::covers(paths).await {
                    let tex = texture::from_slot_image(SlotImage::Scaled { w, h, rgba });
                    if let Some(welcome) = win.imp().welcome.get() {
                        welcome.set_cover(&path, &tex);
                    }
                }
            }
        ));
    }

    /// Open the previous or next book. `start` is the page to open it at
    /// (`None`: where it was left; `usize::MAX`: its last page).
    pub(super) fn open_sibling(&self, forward: bool, start: Option<usize>) {
        let Some(current) = self.with_reader(|r| r.book().map(|b| b.path.clone())) else {
            return;
        };
        let folders = self.imp_settings().files.open_next_folder;
        // Opening starts with the search, so a running slideshow waits.
        self.imp().opening.set(true);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                match workers::sibling(current, forward, folders).await {
                    Some(path) => win.open(path, start),
                    None => {
                        win.imp().opening.set(false);
                        log::info!("no {} book", if forward { "next" } else { "previous" });
                    }
                }
            }
        ));
    }

    /// Close the open book and go back to the start screen; with no book
    /// open, close the window (the key used to do only that).
    pub(super) fn close_book(&self) {
        if self.with_reader(|r| r.book().is_none()) {
            self.close();
            return;
        }
        let imp = self.imp();
        // A book still opening would replace the start screen.
        imp.open_token.set(imp.open_token.get() + 1);
        imp.opening.set(false);
        // The position is already in the history: every move records it.
        self.set_slideshow(false);
        self.set_lens(false);
        if imp.focus.get() {
            self.toggle_focus();
        }
        self.with_reader(|r| r.close());
        imp.textures.borrow_mut().clear();
        *imp.pic_pages.borrow_mut() = [None, None];
        for pic in &imp.pics {
            pic.set_paintable(None::<&gdk::Paintable>);
        }
        imp.edge.take();
        imp.book_bytes.set(0);
        imp.book_kind.replace(None);
        self.reset_sidebar();
        self.refresh_welcome();
        self.update_chrome();
    }

    /// A page turn ran past the book's start or end: continue in the
    /// neighbouring book (the caller checked the setting that applies).
    /// True if one is opening.
    pub(super) fn turn_past_book(&self, forward: bool) -> bool {
        if self.imp().opening.get() {
            return false;
        }
        // Going back lands on the previous book's last page.
        self.open_sibling(forward, Some(if forward { 0 } else { usize::MAX }));
        true
    }
}
