//! The main window: a welcome screen, and the reader (the pages in a
//! scrolled view). Every command is a `win.<action>` GAction; keys reach
//! them through the application's accelerators (see `actions.rs`).
//!
//! `impl NiraWindow` is split by concern: `actions.rs`, `nav.rs` (page
//! turns), `view.rs` (drawing, fit and zoom) and `scroll.rs`.

mod about;
mod actions;
mod books;
mod controls;
mod focus;
mod keys;
mod lens;
mod nav;
mod prefs;
mod scroll;
mod sidebar;
mod view;
mod viewopts;

use std::cell::{Cell, OnceCell, Ref, RefCell};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use nira_core::bookmarks::Bookmarks;
use nira_core::history::History;
use nira_core::i18n::{t, t_args};
use nira_core::keymap::Action;
use nira_core::keymap::Keymap;
use nira_core::reader::{Book, Reader};
use nira_core::scroll::{EdgeGuard, Place, SpreadLayout};
use nira_core::settings::Settings;
use nira_core::workers::{OpenError, PersistJob, ThumbCmd, WorkerCmd, WorkerResult};
use nira_core::zoom::ZoomModel;

use crate::chrome::focus::FocusOverlay;
use crate::chrome::lens::LensView;
use crate::chrome::pagebar::PageBar;
use crate::chrome::sidebar::Sidebar;
use crate::chrome::statusbar::{PageStatus, StatusBar};
use crate::chrome::titlebar::TitleBar;
use crate::chrome::toolbar::{Toolbar, ToolbarState};
use crate::chrome::viewpanel::ViewPanel;
use crate::chrome::welcome::Welcome;
use crate::workers;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct NiraWindow {
        pub stack: gtk::Stack,
        pub scrolled: gtk::ScrolledWindow,
        pub content: gtk::Box,
        /// One picture per slot, in screen order.
        pub pics: [gtk::Picture; 2],
        pub titlebar: OnceCell<TitleBar>,
        pub toolbar: OnceCell<Toolbar>,
        pub statusbar: OnceCell<StatusBar>,
        pub pagebar: OnceCell<PageBar>,
        pub lens: OnceCell<LensView>,
        pub lens_on: Cell<bool>,
        /// The keys in use (edited in the preferences).
        pub keymap: RefCell<Keymap>,
        /// The keybinding editor's list, while the preferences are open.
        pub keys_list: RefCell<Option<gtk::ListBox>>,
        pub key_rows: RefCell<Vec<keys::KeyRow>>,
        /// The action waiting for a key press in the editor.
        pub capturing: Cell<Option<nira_core::keymap::Action>>,
        /// The page each picture shows (screen order).
        pub pic_pages: RefCell<[Option<usize>; 2]>,
        pub welcome: OnceCell<Welcome>,
        pub history: RefCell<History>,
        /// The position last written to the history.
        pub recorded: RefCell<Option<(String, u32)>>,
        /// A book is being opened.
        pub opening: Cell<bool>,
        pub focus_overlay: OnceCell<FocusOverlay>,
        /// The window's root: the content column with overlays on top.
        pub root: RefCell<Option<gtk::Overlay>>,
        /// In focus mode (fullscreen, chrome hidden).
        pub focus: Cell<bool>,
        /// Pending fade of the focus-mode hint.
        pub hint_source: RefCell<Option<glib::SourceId>>,
        pub sidebar: OnceCell<Sidebar>,
        pub thumb_tx: RefCell<Option<mpsc::Sender<ThumbCmd>>>,
        /// Bumped per book; older thumbnails are dropped.
        pub thumb_gen: Cell<u64>,
        /// The wanted pages last sent to the thumbnailer.
        pub thumbs_wanted: RefCell<Vec<usize>>,
        pub thumbs_paused: Cell<bool>,
        /// The page the sidebar last followed.
        pub sidebar_page: Cell<Option<usize>>,
        pub bookmarks: RefCell<Bookmarks>,
        /// The open book's size on disk and archive type.
        pub book_bytes: Cell<u64>,
        pub book_kind: RefCell<Option<String>>,
        pub viewpanel: OnceCell<ViewPanel>,
        /// Pending re-decode after a brightness or contrast change.
        pub enhance_source: RefCell<Option<glib::SourceId>>,
        /// Set while the window pushes values into the View panel.
        pub syncing_panel: Cell<bool>,
        /// Pending fade of the page bar and click zones.
        pub fade_source: RefCell<Option<glib::SourceId>>,
        /// True while the pointer is on the page bar.
        pub over_bar: Cell<bool>,
        /// Last pointer position over the reading area.
        pub pointer: Cell<(f64, f64)>,
        /// The running slideshow's timer.
        pub slideshow: RefCell<Option<glib::SourceId>>,
        /// The shown zoom, "73%" (empty before the page size is known).
        pub zoom_text: RefCell<String>,
        /// Toolbar collapse level for the current window width.
        pub collapse: Cell<usize>,
        pub settings: RefCell<Settings>,
        pub reader: RefCell<Option<Reader>>,
        /// The texture each slot shows (reading order), with its page.
        pub textures: RefCell<Vec<Option<(usize, gdk::Texture)>>>,
        pub page_tx: RefCell<Option<mpsc::Sender<WorkerCmd>>>,
        /// Disk writes run in order on the persistence thread.
        pub persist_tx: RefCell<Option<mpsc::Sender<PersistJob>>>,
        pub persister: RefCell<Option<JoinHandle<()>>>,
        /// Bumped per open; a finished open applies only if it is the newest.
        pub open_token: Cell<u64>,
        pub viewport: Cell<(i32, i32)>,
        /// A viewport check is queued.
        pub viewport_check: Cell<bool>,
        /// Where to scroll once the spread is laid out.
        pub scroll_to: Cell<Option<Place>>,
        /// Geometry of the shown spread, for smart scroll.
        pub layout: RefCell<Option<SpreadLayout>>,
        /// Presses against the page edge since the last scroll.
        pub edge: Cell<EdgeGuard>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NiraWindow {
        const NAME: &'static str = "NiraWindow";
        type Type = super::NiraWindow;
        type ParentType = gtk::ApplicationWindow;
    }

    impl ObjectImpl for NiraWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }

    impl WidgetImpl for NiraWindow {}
    impl WindowImpl for NiraWindow {
        fn close_request(&self) -> glib::Propagation {
            let win = self.obj();
            win.save_window_size();
            win.flush_persister();
            self.parent_close_request()
        }
    }
    impl ApplicationWindowImpl for NiraWindow {}
}

glib::wrapper! {
    pub struct NiraWindow(ObjectSubclass<imp::NiraWindow>)
        @extends gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl NiraWindow {
    pub fn new(app: &gtk::Application, settings: Settings) -> Self {
        let win: Self = glib::Object::builder()
            .property("application", app)
            .property("title", "Nira")
            .build();
        win.set_default_size(settings.window.width, settings.window.height);
        let zoom = ZoomModel {
            fit_mode: settings.view.fit,
            scale_up: settings.view.scale_up,
            fit_size: settings.view.fit_size,
            ..ZoomModel::default()
        };
        let reader = Reader::new(
            settings.view.clone(),
            zoom,
            settings.enhance.clone(),
            settings.files.cached_pages,
        );
        let imp = win.imp();
        imp.reader.replace(Some(reader));
        imp.settings.replace(settings);
        imp.bookmarks.replace(Bookmarks::load());
        imp.history.replace(History::load());
        win.update_background();
        // Workers first: setting up the actions may save settings.
        win.start_worker();
        win.add_actions();
        win.add_view_actions();
        win.add_book_actions();
        win.accept_drops();
        win.refresh_welcome();
        let keymap = Keymap::load();
        win.install_accels(app, &keymap);
        win.set_status_keys(&keymap);
        win.set_focus_keys(&keymap);
        imp.keymap.replace(keymap);
        win.update_chrome();
        win
    }

    fn build(&self) {
        let imp = self.imp();

        self.add_css_class("nira-main");
        let titlebar = TitleBar::new();
        self.set_titlebar(Some(&titlebar.root));
        let toolbar = Toolbar::new();
        let statusbar = StatusBar::new();

        let welcome = self.build_welcome();

        // Reader: pages side by side, centred in the scrolled view.
        imp.content.set_orientation(gtk::Orientation::Horizontal);
        imp.content
            .set_spacing(nira_core::scroll::SPREAD_SPACING as i32);
        imp.content.set_halign(gtk::Align::Center);
        imp.content.set_valign(gtk::Align::Center);
        // Pages are laid out left to right whatever the UI direction; manga
        // order is handled by `Frame::visual`.
        imp.content.set_direction(gtk::TextDirection::Ltr);
        for pic in &imp.pics {
            pic.set_can_shrink(false);
            pic.set_content_fit(gtk::ContentFit::Fill);
        }
        imp.scrolled.set_child(Some(&imp.content));
        imp.scrolled.add_css_class("viewer");
        imp.scrolled.set_hexpand(true);
        imp.scrolled.set_vexpand(true);
        // Keys go to the window's accelerators, not the scrolled view's own
        // bindings (Page_Down, Home, …).
        imp.scrolled.set_focusable(false);
        let vadj = imp.scrolled.vadjustment();
        vadj.connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.apply_scroll_to()
        ));

        imp.content.add_css_class("spread");
        imp.stack.add_named(&welcome, Some("welcome"));
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&self.build_sidebar());
        let reading_area = self.build_reading_area();
        reading_area.set_hexpand(true);
        body.append(&reading_area);
        body.append(&self.build_view_panel());
        imp.stack.add_named(&body, Some("reader"));
        imp.stack.set_visible_child_name("welcome");
        imp.stack.set_vexpand(true);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&toolbar.root);
        column.append(&imp.stack);
        column.append(&statusbar.root);
        let root = self.build_focus(&column);
        self.set_child(Some(&root));
        let _ = imp.titlebar.set(titlebar);
        let _ = imp.toolbar.set(toolbar);
        let _ = imp.statusbar.set(statusbar);

        self.watch_viewport();
        self.connect_wheel();
    }

    // ---- reader access -------------------------------------------------

    /// Run `f` on the reader. Never call back into widgets from `f`.
    fn with_reader<R>(&self, f: impl FnOnce(&mut Reader) -> R) -> R {
        let mut reader = self.imp().reader.borrow_mut();
        f(reader.as_mut().expect("reader is set in new()"))
    }

    /// The settings, read-only. Don't hold the borrow across calls that may
    /// change them.
    fn imp_settings(&self) -> Ref<'_, Settings> {
        self.imp().settings.borrow()
    }

    /// Change the settings and save them in the background.
    fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let snapshot = {
            let mut settings = self.imp().settings.borrow_mut();
            f(&mut settings);
            settings.clone()
        };
        self.persist(Box::new(move || snapshot.save()));
    }

    fn persist(&self, job: PersistJob) {
        if let Some(tx) = self.imp().persist_tx.borrow().as_ref() {
            if tx.send(job).is_ok() {
                return;
            }
        }
        log::error!("the persistence thread stopped; a write was lost");
    }

    /// Finish every pending write (on close).
    fn flush_persister(&self) {
        let imp = self.imp();
        imp.persist_tx.replace(None);
        if let Some(handle) = imp.persister.take() {
            let _ = handle.join();
        }
    }

    fn start_worker(&self) {
        let imp = self.imp();
        let (tx, handle) = workers::start_persister();
        imp.persist_tx.replace(Some(tx));
        imp.persister.replace(Some(handle));
        self.start_thumbnailer();
        let (tx, rx) = workers::start_page_worker();
        imp.page_tx.replace(Some(tx));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                while let Ok(res) = rx.recv().await {
                    win.on_worker_result(res);
                }
            }
        ));
    }

    /// Send the reader's pending commands to the page worker.
    fn pump(&self) {
        let (cmds, loading) = self.with_reader(|r| (r.take_commands(), r.is_loading()));
        self.pause_thumbnails(loading);
        let tx = self.imp().page_tx.borrow();
        let Some(tx) = tx.as_ref() else {
            return;
        };
        for cmd in cmds {
            if tx.send(cmd).is_err() {
                log::error!("the page worker stopped");
                return;
            }
        }
    }

    fn on_worker_result(&self, res: WorkerResult) {
        if let Some(frame) = self.with_reader(|r| r.on_result(res)) {
            self.draw(frame);
        }
        self.pump();
    }

    // ---- opening ---------------------------------------------------------

    fn choose_file(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(t("open-dialog-title"))
            .modal(true)
            .build();
        dialog.open(
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

    /// Open the book at `path` at page `start` (0-based; `None`: where it
    /// was left).
    pub fn open(&self, path: PathBuf, start: Option<usize>) {
        let imp = self.imp();
        let token = imp.open_token.get() + 1;
        imp.open_token.set(token);
        imp.opening.set(true);
        log::info!("opening {}", path.display());
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let res = workers::open(path.clone()).await;
                if win.imp().open_token.get() != token {
                    return; // a newer open replaced this one
                }
                win.imp().opening.set(false);
                win.finish_open(path, start, res);
            }
        ));
    }

    fn finish_open(
        &self,
        path: PathBuf,
        start: Option<usize>,
        res: Result<nira_core::workers::OpenedArchive, OpenError>,
    ) {
        let opened = match res {
            Ok(opened) => opened,
            Err(e) => {
                self.show_error(&open_error_text(&path, &e));
                return;
            }
        };
        log::info!(
            "{}: {} pages ({})",
            path.display(),
            opened.pages.len(),
            opened.kind.as_deref().unwrap_or("folder")
        );
        let start = start.unwrap_or_else(|| self.resume_page(&path));
        self.imp().book_bytes.set(opened.size);
        self.imp().book_kind.replace(opened.kind.clone());
        let book = Book {
            path,
            pages: Arc::new(opened.pages),
            is_archive: opened.kind.is_some(),
        };
        self.imp().textures.borrow_mut().clear();
        self.imp().scroll_to.set(Some(Place::Start));
        let viewport = self.viewport();
        self.with_reader(|r| {
            r.set_viewport(viewport);
            r.open(book, start);
        });
        self.reset_sidebar();
        self.update_chrome();
        self.pump();
    }

    fn show_error(&self, text: &str) {
        log::warn!("{text}");
        let dialog = gtk::AlertDialog::builder()
            .message(t("open-failed"))
            .detail(text)
            .modal(true)
            .build();
        dialog.show(Some(self));
    }

    /// Refresh everything that shows the book and position: title bar,
    /// toolbar, status bar, and which screen is shown.
    pub(super) fn update_chrome(&self) {
        let (name, file, page, total, spread, fit, status, manga) = self.with_reader(|r| {
            let book = r.book();
            let name = book.map(|b| book_title(&b.path)).unwrap_or_default();
            let file =
                book.and_then(|b| b.path.file_name().map(|f| f.to_string_lossy().into_owned()));
            let spread = r.spread();
            let first = spread.first().copied();
            let status = PageStatus {
                dims: first.and_then(|p| r.page_size(p)),
                file: first
                    .and_then(|p| book.and_then(|b| b.pages.get(p)))
                    .map(|n| page_file_name(n))
                    .unwrap_or_default(),
                bytes: first.and_then(|p| r.page_file_bytes(p)),
            };
            (
                name,
                file,
                r.page(),
                r.page_count(),
                spread,
                r.zoom.fit_mode,
                status,
                r.view.manga,
            )
        });
        let imp = self.imp();
        let reading = total > 0;
        let screen = if reading { "reader" } else { "welcome" };
        if imp.stack.visible_child_name().as_deref() != Some(screen) {
            imp.stack.set_visible_child_name(screen);
        }
        if reading {
            self.record_position();
        }

        let progress = if reading {
            let last = spread.iter().max().map_or(page, |&p| p) + 1;
            let pages = match spread.as_slice() {
                [a, b] => format!("{}–{}", a + 1, b + 1),
                _ => (page + 1).to_string(),
            };
            t_args(
                "toolbar-progress",
                &[
                    ("pages", pages.into()),
                    ("total", total.into()),
                    ("percent", ((last * 100) / total).into()),
                ],
            )
        } else {
            String::new()
        };
        let focus = imp.focus.get();
        if let Some(toolbar) = imp.toolbar.get() {
            if !focus {
                toolbar.root.set_visible(reading);
            }
            toolbar.update(&ToolbarState {
                title: &name,
                progress: &progress,
                zoom: &imp.zoom_text.borrow(),
                fit,
            });
        }
        if let Some(bar) = imp.pagebar.get() {
            bar.update(page + 1, total, manga);
        }
        if reading {
            self.sync_sidebar(page);
        }
        self.update_focus();
        if let Some(statusbar) = imp.statusbar.get() {
            statusbar.root.set_visible(reading && !focus);
            statusbar.update(&status);
        }
        if let Some(titlebar) = imp.titlebar.get() {
            titlebar.update(file.as_deref());
        }
        self.set_title(Some(if name.is_empty() { "Nira" } else { &name }));
    }

    /// Key hints in the status bar, from the keymap.
    pub(super) fn set_status_keys(&self, keymap: &Keymap) {
        let first = |a: Action| keymap.accels(a).first().map(String::as_str);
        if let Some(statusbar) = self.imp().statusbar.get() {
            statusbar.set_keys(
                first(Action::PrevPage),
                first(Action::NextPage),
                first(Action::Fullscreen),
            );
        }
    }

    fn save_window_size(&self) {
        let (w, h) = (self.width(), self.height());
        if w <= 0 || h <= 0 || self.is_fullscreen() || self.is_maximized() {
            return;
        }
        let window = &self.imp_settings().window;
        if (window.width, window.height) != (w, h) {
            self.update_settings(|s| {
                s.window.width = w;
                s.window.height = h;
            });
        }
    }
}

/// The book's name: a folder's name, or an archive's name without its
/// extension.
fn book_title(path: &Path) -> String {
    let stem = if path.is_dir() {
        path.file_name()
    } else {
        path.file_stem()
    };
    stem.map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A page's file name without the folders inside the archive.
fn page_file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

fn open_error_text(path: &Path, e: &OpenError) -> String {
    let file = path.display().to_string();
    match e {
        OpenError::Open(reason) => t_args(
            "open-error",
            &[("file", file.into()), ("reason", reason.clone().into())],
        ),
        OpenError::List(reason) => t_args(
            "open-error-list",
            &[("file", file.into()), ("reason", reason.clone().into())],
        ),
        OpenError::Empty => t_args("open-error-empty", &[("file", file.into())]),
    }
}
