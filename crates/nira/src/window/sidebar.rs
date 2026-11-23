//! The page sidebar: lazy thumbnails, bookmarks, and following the
//! current page.

use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use nira_core::bookmarks::Bookmark;
use nira_core::reader::SlotImage;
use nira_core::thumbs::{visible_cells, wanted_order};
use nira_core::workers::{ThumbCmd, ThumbResult};

use super::NiraWindow;
use crate::chrome::sidebar::{BookmarkRow, Sidebar};
use crate::texture;
use crate::workers;

impl NiraWindow {
    pub(super) fn build_sidebar(&self) -> gtk::Overlay {
        let side = Sidebar::new();
        side.connect_resize(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |width, done| win.on_sidebar_resized(width, done)
        ));
        side.grid.connect_child_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, child| {
                let page = win.imp().sidebar.get().map(|s| s.page_of(child));
                if let Some(page) = page {
                    win.go_to(page);
                }
            }
        ));
        side.bookmarks.connect_row_activated(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, row| {
                if let Ok(page) = row.widget_name().parse::<usize>() {
                    win.go_to(page);
                }
            }
        ));
        let adj = side.scroll.vadjustment();
        adj.connect_value_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.refresh_thumbnails()
        ));
        // A layout (the first one after opening a book, a resize, page
        // numbers turned on or off) changes the rows' measured size.
        adj.connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.refresh_thumbnails()
        ));
        let root = side.root.clone();
        let _ = self.imp().sidebar.set(side);
        root
    }

    pub(super) fn start_thumbnailer(&self) {
        let (tx, rx) = workers::start_thumbnailer();
        self.imp().thumb_tx.replace(Some(tx));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                while let Ok(res) = rx.recv().await {
                    win.on_thumbnail(res);
                }
            }
        ));
    }

    fn on_thumbnail(&self, (gen, page, w, h, rgba): ThumbResult) {
        let imp = self.imp();
        if gen != imp.thumb_gen.get() {
            return;
        }
        let tex = texture::from_slot_image(SlotImage::Scaled { w, h, rgba });
        if let Some(side) = imp.sidebar.get() {
            side.set_thumbnail(page, &tex);
            side.set_bookmark_thumbnail(page, &tex);
        }
    }

    /// A new book: one empty cell per page, thumbnails from scratch.
    pub(super) fn reset_sidebar(&self) {
        let imp = self.imp();
        imp.thumb_gen.set(imp.thumb_gen.get() + 1);
        imp.thumbs_wanted.borrow_mut().clear();
        imp.sidebar_page.set(None);
        let names = self.with_reader(|r| {
            r.book()
                .map(|b| b.pages.as_ref().clone())
                .unwrap_or_default()
        });
        let numbers = self.imp_settings().appearance.thumbnail_page_numbers;
        if let Some(side) = imp.sidebar.get() {
            side.fill(&names, numbers);
            let size = glib::format_size(imp.book_bytes.get());
            side.set_footer(&nira_core::i18n::t_args(
                "sidebar-footer",
                &[
                    ("pages", names.len().into()),
                    ("size", size.to_string().into()),
                ],
            ));
        }
        self.update_bookmarks();
    }

    /// Ask for the thumbnails around the current page and in view.
    pub(super) fn refresh_thumbnails(&self) {
        let imp = self.imp();
        let Some(side) = imp.sidebar.get() else {
            return;
        };
        if !side.root.is_visible() {
            return;
        }
        let Some((path, pages, page)) = self.with_reader(|r| {
            r.book()
                .map(|b| (b.path.clone(), b.pages.clone(), r.page()))
        }) else {
            return;
        };
        let adj = side.scroll.vadjustment();
        let (top, row_h) = side.row_metrics();
        let visible = visible_cells(
            adj.value(),
            adj.page_size(),
            row_h,
            top,
            side.columns(),
            pages.len(),
        );
        let order = wanted_order(page, pages.len(), visible);
        if *imp.thumbs_wanted.borrow() == order {
            return;
        }
        imp.thumbs_wanted.replace(order.clone());
        if let Some(tx) = imp.thumb_tx.borrow().as_ref() {
            let _ = tx.send(ThumbCmd::SetWanted {
                gen: imp.thumb_gen.get(),
                path,
                pages,
                indices: order,
            });
        }
    }

    /// The saved sidebar width. The window is built before the settings
    /// are loaded.
    pub(super) fn apply_sidebar_width(&self) {
        let width = self.imp_settings().appearance.sidebar_width;
        if let Some(side) = self.imp().sidebar.get() {
            side.set_width(width);
        }
    }

    /// The sidebar's edge was dragged: save the width once the drag ends.
    /// The grid keeps its top page itself, and the thumbnails follow its
    /// new layout through the adjustment.
    fn on_sidebar_resized(&self, width: i32, done: bool) {
        if done && self.imp_settings().appearance.sidebar_width != width {
            self.update_settings(|s| s.appearance.sidebar_width = width);
        }
    }

    /// Thumbnails wait while a page decodes, so page turns get the CPU.
    pub(super) fn pause_thumbnails(&self, pause: bool) {
        let imp = self.imp();
        if imp.thumbs_paused.replace(pause) == pause {
            return;
        }
        if let Some(tx) = imp.thumb_tx.borrow().as_ref() {
            let _ = tx.send(ThumbCmd::Pause(pause));
        }
    }

    /// Follow the current page in the sidebar.
    /// Only a page change moves the grid, so scrolling it by hand sticks.
    pub(super) fn sync_sidebar(&self, page: usize) {
        let imp = self.imp();
        if let Some(side) = imp.sidebar.get() {
            if side.root.is_visible() && imp.sidebar_page.replace(Some(page)) != Some(page) {
                side.select(page);
            }
        }
        self.refresh_thumbnails();
        self.set_action_state("bookmark", self.page_bookmarked().to_variant());
    }

    /// The Pages toggle. Normal and focus mode each remember their own
    /// choice.
    pub(super) fn show_sidebar(&self, on: bool) {
        self.set_sidebar_visible(on);
        if self.imp().focus.get() {
            self.update_settings(|s| s.appearance.focus_show_thumbnails = on);
        } else {
            self.update_settings(|s| s.appearance.show_thumbnails = on);
        }
    }

    pub(super) fn set_sidebar_visible(&self, on: bool) {
        if let Some(side) = self.imp().sidebar.get() {
            side.root.set_visible(on);
        }
        if on {
            self.imp().sidebar_page.set(None);
            let page = self.with_reader(|r| r.page());
            self.sync_sidebar(page);
        }
    }

    fn book_path(&self) -> Option<String> {
        self.with_reader(|r| r.book().map(|b| b.path.to_string_lossy().into_owned()))
    }

    fn page_bookmarked(&self) -> bool {
        let page = self.with_reader(|r| r.page()) as u32 + 1;
        self.book_path()
            .is_some_and(|p| self.imp().bookmarks.borrow().contains(&p, page))
    }

    /// Add or remove the bookmark on the current page.
    pub(super) fn toggle_bookmark(&self) {
        let Some(path) = self.book_path() else {
            return;
        };
        let (page, total, name) = self.with_reader(|r| {
            let name = r
                .book()
                .and_then(|b| b.path.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (r.page() as u32 + 1, r.page_count() as u32, name)
        });
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let snapshot = {
            let mut marks = self.imp().bookmarks.borrow_mut();
            marks.toggle(Bookmark {
                name,
                path,
                page,
                numpages: total,
                archive_type: self.imp().book_kind.borrow().clone(),
                date_added: now,
            });
            marks.clone()
        };
        self.persist(Box::new(move || snapshot.save()));
        self.update_bookmarks();
        self.set_action_state("bookmark", self.page_bookmarked().to_variant());
    }

    /// Ribbons and the Bookmarks tab for the open book.
    fn update_bookmarks(&self) {
        let imp = self.imp();
        let Some(side) = imp.sidebar.get() else {
            return;
        };
        let pages: Vec<usize> = self
            .book_path()
            .map(|p| {
                imp.bookmarks
                    .borrow()
                    .same_path(&p)
                    .iter()
                    .map(|b| b.page.saturating_sub(1) as usize)
                    .collect()
            })
            .unwrap_or_default();
        let rows: Vec<BookmarkRow> = pages
            .into_iter()
            .map(|page| BookmarkRow {
                page,
                thumb: side.thumbnail(page),
            })
            .collect();
        side.set_bookmarks(&rows);
    }
}
