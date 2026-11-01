//! The page sidebar: a Pages tab (a two-column grid of thumbnails with
//! bookmark ribbons, the current page ringed) and a Bookmarks tab, above
//! a footer with the page count and book size.

use gtk::gdk;
use gtk::prelude::*;
use nira_core::i18n::{t, t_args};

use super::label;

/// The design's sidebar width.
pub const WIDTH: i32 = 232;
/// A thumbnail cell's picture.
const THUMB_W: i32 = 84;
const THUMB_H: i32 = 122;
/// One grid row: picture, page number, padding and spacing.
pub const ROW_H: f64 = THUMB_H as f64 + 46.0;
/// Padding above the first row.
pub const TOP: f64 = 14.0;

struct Cell {
    child: gtk::FlowBoxChild,
    picture: gtk::Picture,
    ribbon: gtk::Image,
}

pub struct Sidebar {
    pub root: gtk::Box,
    pub grid: gtk::FlowBox,
    pub scroll: gtk::ScrolledWindow,
    pub bookmarks: gtk::ListBox,
    bookmarks_empty: gtk::Label,
    bookmark_count: gtk::Label,
    footer: gtk::Label,
    cells: std::cell::RefCell<Vec<Cell>>,
    /// The Bookmarks tab's thumbnails, by page.
    bookmark_images: std::cell::RefCell<Vec<(usize, gtk::Image)>>,
}

/// A row of the Bookmarks tab.
pub struct BookmarkRow {
    /// 0-based.
    pub page: usize,
    pub thumb: Option<gdk::Texture>,
}

impl Sidebar {
    pub fn new() -> Sidebar {
        let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tabs.add_css_class("sidebar-tabs");
        let tab_pages = gtk::ToggleButton::with_label(&t("sidebar-pages"));
        let tab_bookmarks = gtk::ToggleButton::new();
        let bookmark_count = label("0", &["mono", "dim"]);
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        b.append(&gtk::Label::new(Some(&t("sidebar-bookmarks"))));
        b.append(&bookmark_count);
        tab_bookmarks.set_child(Some(&b));
        tab_bookmarks.set_group(Some(&tab_pages));
        tab_pages.set_active(true);
        for tab in [&tab_pages, &tab_bookmarks] {
            tab.set_focus_on_click(false);
            tabs.append(tab);
        }

        let grid = gtk::FlowBox::new();
        grid.set_min_children_per_line(2);
        grid.set_max_children_per_line(2);
        grid.set_column_spacing(10);
        grid.set_row_spacing(12);
        grid.set_selection_mode(gtk::SelectionMode::Single);
        grid.set_activate_on_single_click(true);
        grid.set_homogeneous(true);
        grid.set_valign(gtk::Align::Start);
        grid.add_css_class("thumbview");
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_child(Some(&grid));
        scroll.set_vexpand(true);

        let bookmarks = gtk::ListBox::new();
        bookmarks.add_css_class("bookmark-list");
        bookmarks.set_selection_mode(gtk::SelectionMode::None);
        bookmarks.set_activate_on_single_click(true);
        let bookmarks_empty = label(&t("sidebar-no-bookmarks"), &["dim"]);
        bookmarks_empty.set_margin_top(24);
        bookmarks_empty.set_wrap(true);
        let bm_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bm_box.append(&bookmarks);
        bm_box.append(&bookmarks_empty);
        let bm_scroll = gtk::ScrolledWindow::new();
        bm_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        bm_scroll.set_child(Some(&bm_box));

        let stack = gtk::Stack::new();
        stack.add_named(&scroll, Some("pages"));
        stack.add_named(&bm_scroll, Some("bookmarks"));
        stack.set_vexpand(true);
        tab_pages.connect_toggled({
            let stack = stack.clone();
            move |b| {
                if b.is_active() {
                    stack.set_visible_child_name("pages");
                }
            }
        });
        tab_bookmarks.connect_toggled({
            let stack = stack.clone();
            move |b| {
                if b.is_active() {
                    stack.set_visible_child_name("bookmarks");
                }
            }
        });

        let footer = label("", &["sidebar-footer"]);
        footer.set_xalign(0.0);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("sidebar");
        root.set_size_request(WIDTH, -1);
        root.set_hexpand(false);
        root.append(&tabs);
        root.append(&stack);
        root.append(&footer);
        Sidebar {
            root,
            grid,
            scroll,
            bookmarks,
            bookmarks_empty,
            bookmark_count,
            footer,
            cells: Default::default(),
            bookmark_images: Default::default(),
        }
    }

    /// One empty cell per page. `names` are the pages' names (tooltips).
    pub fn fill(&self, names: &[String], numbers: bool) {
        self.grid.remove_all();
        let mut cells = self.cells.borrow_mut();
        cells.clear();
        for (idx, name) in names.iter().enumerate() {
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 6);
            let overlay = gtk::Overlay::new();
            let picture = gtk::Picture::new();
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_size_request(THUMB_W, THUMB_H);
            picture.add_css_class("thumb-paper");
            overlay.set_child(Some(&picture));
            let ribbon = gtk::Image::from_icon_name("user-bookmarks-symbolic");
            ribbon.add_css_class("ribbon");
            ribbon.set_halign(gtk::Align::End);
            ribbon.set_valign(gtk::Align::Start);
            ribbon.set_margin_end(4);
            ribbon.set_visible(false);
            overlay.add_overlay(&ribbon);
            cell.append(&overlay);
            if numbers {
                cell.append(&label(&(idx + 1).to_string(), &["page-num", "mono"]));
            }
            cell.set_tooltip_text(Some(name));
            let child = gtk::FlowBoxChild::new();
            child.set_child(Some(&cell));
            self.grid.insert(&child, -1);
            cells.push(Cell {
                child,
                picture,
                ribbon,
            });
        }
    }

    pub fn set_thumbnail(&self, page: usize, texture: &gdk::Texture) {
        if let Some(cell) = self.cells.borrow().get(page) {
            cell.picture.set_paintable(Some(texture));
            cell.picture.remove_css_class("thumb-paper");
        }
    }

    pub fn thumbnail(&self, page: usize) -> Option<gdk::Texture> {
        self.cells
            .borrow()
            .get(page)
            .and_then(|c| c.picture.paintable())
            .and_downcast::<gdk::Texture>()
    }

    /// The page of a grid cell.
    pub fn page_of(&self, child: &gtk::FlowBoxChild) -> usize {
        child.index().max(0) as usize
    }

    /// Ring page `page` and scroll it into view.
    pub fn select(&self, page: usize) {
        let cells = self.cells.borrow();
        let Some(cell) = cells.get(page) else {
            return;
        };
        self.grid.select_child(&cell.child);
        if cell.child.width() == 0 {
            // A freshly filled grid has no layout yet: scroll once it has.
            let (scroll, grid, child) =
                (self.scroll.clone(), self.grid.clone(), cell.child.clone());
            gtk::glib::idle_add_local_once(move || scroll_to_child(&scroll, &grid, &child));
            return;
        }
        scroll_to_child(&self.scroll, &self.grid, &cell.child);
    }

    /// Show ribbons on the bookmarked pages (0-based) and list them.
    pub fn set_bookmarks(&self, rows: &[BookmarkRow]) {
        for (idx, cell) in self.cells.borrow().iter().enumerate() {
            cell.ribbon.set_visible(rows.iter().any(|r| r.page == idx));
        }
        self.bookmark_count.set_text(&rows.len().to_string());
        self.bookmarks.remove_all();
        let mut images = self.bookmark_images.borrow_mut();
        images.clear();
        for row in rows {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let image = gtk::Image::new();
            image.set_pixel_size(44);
            if let Some(tex) = &row.thumb {
                image.set_paintable(Some(tex));
            }
            images.push((row.page, image.clone()));
            b.append(&image);
            b.append(&gtk::Label::new(Some(&t_args(
                "sidebar-bookmark-page",
                &[("page", (row.page + 1).into())],
            ))));
            let r = gtk::ListBoxRow::new();
            r.set_child(Some(&b));
            r.set_widget_name(&row.page.to_string());
            self.bookmarks.append(&r);
        }
        self.bookmarks_empty.set_visible(rows.is_empty());
    }

    /// Show a newly made thumbnail in the Bookmarks tab too.
    pub fn set_bookmark_thumbnail(&self, page: usize, texture: &gdk::Texture) {
        for (p, image) in self.bookmark_images.borrow().iter() {
            if *p == page {
                image.set_paintable(Some(texture));
            }
        }
    }

    pub fn set_footer(&self, text: &str) {
        self.footer.set_text(text);
    }
}

/// Scroll the grid just enough to show `child`.
fn scroll_to_child(scroll: &gtk::ScrolledWindow, grid: &gtk::FlowBox, child: &gtk::FlowBoxChild) {
    let Some(bounds) = child.compute_bounds(grid) else {
        return;
    };
    let adj = scroll.vadjustment();
    let (top, h) = (bounds.y() as f64, bounds.height() as f64);
    let view = adj.page_size();
    if top < adj.value() {
        adj.set_value(top);
    } else if top + h > adj.value() + view {
        adj.set_value(top + h - view);
    }
}
