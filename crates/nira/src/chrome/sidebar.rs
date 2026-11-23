//! The page sidebar: a Pages tab (a two-column grid of thumbnails with
//! bookmark ribbons, the current page ringed) and a Bookmarks tab, above
//! a footer with the page count and book size. Its right edge drags it
//! narrower than the design's width: the thumbnails shrink, then the grid
//! drops to one column and the tabs to icons.

use std::cell::{self, RefCell};
use std::rc::Rc;

use gtk::gdk;
use gtk::glib::{self, ControlFlow};
use gtk::prelude::*;
use nira_core::i18n::{t, t_args};
use nira_core::scroll::reveal_value;

use super::label;

/// The design's sidebar width, and the widest it gets.
pub const WIDTH: i32 = 232;
/// A thumbnail cell's picture at the design's width.
const THUMB_W: i32 = 84;
const THUMB_H: i32 = 122;
/// The smallest picture two columns shrink to before dropping to one.
const MIN_THUMB_W: i32 = 64;
/// The drag handle beside the content on the right edge, with the
/// sidebar's border: `min-width` 5 plus `border-right` 1 in `style.css`.
/// Beside the content rather than over it, so it doesn't cover the Pages
/// scrollbar.
const HANDLE_W: i32 = 6;
/// Space around the pictures, from `style.css`: the grid's side padding
/// (12 + 12), each cell's padding and border (4 + 2 on each side), the
/// column spacing (10) and the handle.
const ONE_COLUMN_EXTRA: i32 = 24 + 12 + HANDLE_W;
const TWO_COLUMN_EXTRA: i32 = 24 + 2 * 12 + 10 + HANDLE_W;
/// The narrowest sidebar: one full-size column.
pub const MIN_WIDTH: i32 = THUMB_W + ONE_COLUMN_EXTRA;
/// One grid row with page numbers, beyond the picture: page number,
/// padding and spacing. Only until the grid is laid out; then
/// `row_metrics` measures.
const ROW_EXTRA: f64 = 46.0;
/// Padding above the first row: the top padding of `flowbox.thumbview` in
/// `style.css`.
pub const TOP: f64 = 14.0;
/// Clear space kept around the followed cell.
const REVEAL_MARGIN: f64 = 14.0;

/// The cell to scroll to once the grid has a layout, and whether a tick
/// callback is waiting for that. Only the latest target counts.
#[derive(Clone, Default)]
struct Follow {
    target: Rc<RefCell<Option<gtk::FlowBoxChild>>>,
    ticking: Rc<cell::Cell<bool>>,
}

impl Follow {
    /// Wait for the next layout, then scroll to the target. An unmapped grid
    /// has no layout coming, so this starts again when it maps.
    fn start(&self, scroll: &gtk::ScrolledWindow, grid: &gtk::FlowBox) {
        if !grid.is_mapped() || self.target.borrow().is_none() || self.ticking.replace(true) {
            return;
        }
        let (follow, scroll) = (self.clone(), scroll.clone());
        // Ticks run before layout, so only a second tick has seen one.
        let waited = cell::Cell::new(false);
        grid.add_tick_callback(move |grid, _| {
            let target = follow.target.borrow().clone();
            let Some(child) = target.filter(|_| grid.is_mapped()) else {
                follow.ticking.set(false);
                return ControlFlow::Break;
            };
            if !waited.replace(true) || child.width() == 0 {
                return ControlFlow::Continue;
            }
            follow.target.replace(None);
            follow.ticking.set(false);
            scroll_to_child(&scroll, grid, &child);
            ControlFlow::Break
        });
    }
}

struct Cell {
    child: gtk::FlowBoxChild,
    picture: gtk::Picture,
    ribbon: gtk::Image,
}

/// The grid for a sidebar `width`: (columns, picture width, picture
/// height). Two columns shrink their pictures down to `MIN_THUMB_W`; below
/// that it is one full-size column.
pub fn grid_for(width: i32) -> (usize, i32, i32) {
    let w = width.clamp(MIN_WIDTH, WIDTH);
    let (columns, thumb_w) = if w >= 2 * MIN_THUMB_W + TWO_COLUMN_EXTRA {
        (2, ((w - TWO_COLUMN_EXTRA) / 2).min(THUMB_W))
    } else {
        (1, (w - ONE_COLUMN_EXTRA).min(THUMB_W))
    };
    (columns, thumb_w, thumb_w * THUMB_H / THUMB_W)
}

/// Called with the new width and whether the drag ended.
type ResizeFn = Box<dyn Fn(i32, bool)>;

/// What a width change touches, shared with the drag handle's gestures.
#[derive(Clone)]
struct Sizer {
    root: gtk::Box,
    grid: gtk::FlowBox,
    scroll: gtk::ScrolledWindow,
    cells: Rc<RefCell<Vec<Cell>>>,
    /// The tab labels, hidden for icons in one-column mode.
    tab_labels: Rc<[gtk::Label; 2]>,
    tab_icons: Rc<[gtk::Image; 2]>,
    width: Rc<cell::Cell<i32>>,
    columns: Rc<cell::Cell<usize>>,
    thumb: Rc<cell::Cell<(i32, i32)>>,
    /// The cell at the top of the view when a resize started, and how far
    /// into its row the view started (a fraction of the row height). Kept
    /// for the whole drag, so small measuring errors don't add up.
    anchor: Rc<RefCell<Option<(gtk::FlowBoxChild, f64)>>>,
    /// Whether the edge is being dragged; the anchor goes once it ends.
    dragging: Rc<cell::Cell<bool>>,
    /// Width changes, and the last one the view was put back after.
    changes: Rc<cell::Cell<u64>>,
    restored: Rc<cell::Cell<u64>>,
    /// The handler that puts the view back after each layout, while there
    /// is an anchor.
    on_layout: Rc<RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>>,
    on_resize: Rc<RefCell<Option<ResizeFn>>>,
}

impl Sizer {
    /// Set the width (clamped) and lay the grid out for it. Returns whether
    /// it changed.
    fn set_width(&self, width: i32) -> bool {
        let width = width.clamp(MIN_WIDTH, WIDTH);
        if self.width.get() == width {
            return false;
        }
        self.hold_top();
        self.width.set(width);
        self.root.set_size_request(width, -1);
        let (columns, w, h) = grid_for(width);
        if self.columns.replace(columns) != columns {
            self.grid.set_min_children_per_line(columns as u32);
            self.grid.set_max_children_per_line(columns as u32);
            for (label, icon) in self.tab_labels.iter().zip(self.tab_icons.iter()) {
                label.set_visible(columns > 1);
                icon.set_visible(columns == 1);
            }
        }
        if self.thumb.replace((w, h)) != (w, h) {
            for cell in self.cells.borrow().iter() {
                cell.picture.set_size_request(w, h);
            }
        }
        true
    }

    /// Keep the page at the top of the view there through the new layouts
    /// a resize brings: smaller rows, or twice as many, would otherwise
    /// slide other pages under an unchanged scroll value. The view is put
    /// back each frame once the grid is laid out for the current width.
    fn hold_top(&self) {
        self.changes.set(self.changes.get() + 1);
        if self.on_layout.borrow().is_some() || !self.grid.is_mapped() {
            return;
        }
        let value = self.scroll.vadjustment().value();
        if value <= 0.0 {
            return;
        }
        let (top, row_h) = self.row_metrics();
        let row = ((value - top) / row_h).floor().max(0.0);
        let Some(child) = self
            .cells
            .borrow()
            .get(row as usize * self.columns.get())
            .map(|c| c.child.clone())
        else {
            return;
        };
        let fraction = (value - top - row * row_h) / row_h;
        let Some(clock) = self.grid.frame_clock() else {
            return;
        };
        self.anchor.replace(Some((child, fraction)));
        // After GTK's own layout handler (connected first), so the cells
        // have their new positions. A value set here is laid out again
        // before the frame is drawn, so no frame shows the grid elsewhere.
        let sizer = self.clone();
        // Layouts since the drag ended, so a layout that never matches (a
        // theme sizing the pictures differently) can't keep this connected.
        let settling = cell::Cell::new(0);
        let id = clock.connect_layout(move |_| {
            sizer.restore();
            if sizer.dragging.get() {
                settling.set(0);
                return;
            }
            settling.set(settling.get() + 1);
            if sizer.restored.get() == sizer.changes.get() || settling.get() >= 10 {
                sizer.release();
            }
        });
        self.on_layout.replace(Some((clock, id)));
    }

    /// Put the anchored cell back at the top, once the grid is laid out for
    /// the current width.
    fn restore(&self) {
        let Some((child, fraction)) = self.anchor.borrow().clone() else {
            return;
        };
        if !self.laid_out() {
            return;
        }
        if let Some(b) = child.compute_bounds(&self.grid) {
            let (_, row_h) = self.row_metrics();
            let adj = self.scroll.vadjustment();
            let max = (adj.upper() - adj.page_size()).max(adj.lower());
            let value = TOP + b.y() as f64 + fraction * row_h;
            adj.set_value(value.clamp(adj.lower(), max));
        }
        self.restored.set(self.changes.get());
    }

    /// Forget the anchor and stop following layouts.
    fn release(&self) {
        self.anchor.replace(None);
        if let Some((clock, id)) = self.on_layout.take() {
            clock.disconnect(id);
        }
    }

    /// The drag is over: keep the width on screen if the sidebar is held
    /// wider than asked (so that is what gets saved), and let go of the
    /// anchor now if the view is already back, otherwise after the layouts
    /// still to come.
    fn drag_ended(&self) {
        self.dragging.set(false);
        let shown = self.root.width().min(WIDTH);
        if shown > self.width.get() {
            self.set_width(shown);
        }
        if self.restored.get() == self.changes.get() {
            self.release();
        }
    }

    /// Whether the grid's layout is the one for the current width: the
    /// column count and the thumbnail height. Until then cell positions
    /// are the old ones.
    fn laid_out(&self) -> bool {
        let cells = self.cells.borrow();
        let y = |i: usize| {
            cells
                .get(i)?
                .child
                .compute_bounds(&self.grid)
                .map(|b| b.y())
        };
        let columns_ok = match (y(0), y(1)) {
            (Some(a), Some(b)) => (self.columns.get() == 1) == (b > a),
            _ => true,
        };
        let height_ok = cells
            .first()
            .is_none_or(|c| c.picture.height() == self.thumb.get().1);
        columns_ok && height_ok
    }

    /// Where the first row starts and the distance from one row to the
    /// next, in scroll units, measured from the laid-out cells (a row's
    /// height depends on the page numbers, the font and the theme). Before
    /// a layout, or with a single row, the design's sizes.
    fn row_metrics(&self) -> (f64, f64) {
        let cells = self.cells.borrow();
        let y = |i: usize| {
            let b = cells.get(i)?.child.compute_bounds(&self.grid)?;
            (b.height() > 0.0).then_some(b.y() as f64)
        };
        match (y(0), y(self.columns.get())) {
            (Some(first), Some(second)) if second > first => (TOP + first, second - first),
            _ => (TOP, self.thumb.get().1 as f64 + ROW_EXTRA),
        }
    }

    /// Set the width from the handle and tell the window.
    fn drag_to(&self, width: i32, done: bool) {
        if self.set_width(width) || done {
            self.notify(done);
        }
    }

    fn notify(&self, done: bool) {
        if let Some(f) = self.on_resize.borrow().as_ref() {
            f(self.width.get(), done);
        }
    }
}

pub struct Sidebar {
    pub root: gtk::Box,
    pub grid: gtk::FlowBox,
    pub scroll: gtk::ScrolledWindow,
    pub bookmarks: gtk::ListBox,
    bookmarks_empty: gtk::Label,
    bookmark_count: gtk::Label,
    footer: gtk::Label,
    cells: Rc<RefCell<Vec<Cell>>>,
    follow: Follow,
    sizer: Sizer,
    /// The Bookmarks tab's thumbnails, by page.
    bookmark_images: RefCell<Vec<(usize, gtk::Image)>>,
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
        // Each tab is a label, or in one-column mode an icon with the label
        // as tooltip; the labels ellipsize rather than hold the sidebar wide.
        let tab = |text: &str, icon: &str| {
            let label = gtk::Label::new(Some(text));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let icon = gtk::Image::from_icon_name(icon);
            icon.set_visible(false);
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            b.append(&icon);
            b.append(&label);
            let button = gtk::ToggleButton::new();
            button.set_child(Some(&b));
            button.set_tooltip_text(Some(text));
            (button, b, label, icon)
        };
        let (tab_pages, _, pages_label, pages_icon) =
            tab(&t("sidebar-pages"), "view-grid-symbolic");
        let (tab_bookmarks, b, bookmarks_label, bookmarks_icon) =
            tab(&t("sidebar-bookmarks"), "user-bookmarks-symbolic");
        let bookmark_count = label("0", &["mono", "dim"]);
        b.append(&bookmark_count);
        tab_bookmarks.set_group(Some(&tab_pages));
        tab_pages.set_active(true);
        for tab in [&tab_pages, &tab_bookmarks] {
            tab.set_focus_on_click(false);
            tabs.append(tab);
        }

        let grid = gtk::FlowBox::new();
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
        footer.set_ellipsize(gtk::pango::EllipsizeMode::End);
        // Its preferred width would be the whole text, and the body hands
        // out preferred widths before the reading area's spare space, so a
        // long footer ("353 pages · 542.6 MB") held a narrow sidebar wider
        // than asked. It still gets the full width, and ellipsizes only
        // when the text doesn't fit.
        footer.set_max_width_chars(1);

        let follow = Follow::default();
        grid.connect_map({
            let (follow, scroll) = (follow.clone(), scroll.clone());
            move |grid| follow.start(&scroll, grid)
        });

        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&tabs);
        column.append(&stack);
        column.append(&footer);
        column.set_hexpand(true);
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("sidebar");
        root.set_hexpand(false);
        root.append(&column);

        let cells: Rc<RefCell<Vec<Cell>>> = Default::default();
        let sizer = Sizer {
            root: root.clone(),
            grid: grid.clone(),
            scroll: scroll.clone(),
            cells: cells.clone(),
            tab_labels: Rc::new([pages_label, bookmarks_label]),
            tab_icons: Rc::new([pages_icon, bookmarks_icon]),
            width: Rc::new(cell::Cell::new(0)),
            columns: Rc::new(cell::Cell::new(0)),
            thumb: Rc::new(cell::Cell::new((THUMB_W, THUMB_H))),
            anchor: Default::default(),
            dragging: Default::default(),
            changes: Default::default(),
            restored: Default::default(),
            on_layout: Default::default(),
            on_resize: Default::default(),
        };
        sizer.set_width(WIDTH);
        root.append(&resize_handle(&sizer));

        Sidebar {
            root,
            grid,
            scroll,
            bookmarks,
            bookmarks_empty,
            bookmark_count,
            footer,
            cells,
            follow,
            sizer,
            bookmark_images: Default::default(),
        }
    }

    /// Set the width, clamped to what the sidebar allows.
    pub fn set_width(&self, width: i32) {
        self.sizer.set_width(width);
    }

    /// Thumbnails per grid row at the current width.
    pub fn columns(&self) -> usize {
        self.sizer.columns.get()
    }

    /// Call `f` with the new width while the edge is dragged, and with
    /// `done` once the drag ends (or a double-click resets it).
    pub fn connect_resize(&self, f: impl Fn(i32, bool) + 'static) {
        self.sizer.on_resize.replace(Some(Box::new(f)));
    }

    /// One empty cell per page. `names` are the pages' names (tooltips).
    pub fn fill(&self, names: &[String], numbers: bool) {
        self.follow.target.replace(None);
        self.grid.remove_all();
        let mut cells = self.cells.borrow_mut();
        cells.clear();
        for (idx, name) in names.iter().enumerate() {
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 6);
            let overlay = gtk::Overlay::new();
            let picture = gtk::Picture::new();
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Contain);
            let (w, h) = self.sizer.thumb.get();
            picture.set_size_request(w, h);
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
        if self.grid.is_mapped() && cell.child.width() > 0 {
            self.follow.target.replace(None);
            scroll_to_child(&self.scroll, &self.grid, &cell.child);
        } else {
            // No layout yet, or a hidden tab: scroll once it has one.
            self.follow.target.replace(Some(cell.child.clone()));
            self.follow.start(&self.scroll, &self.grid);
        }
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

    /// Where the first row starts and the distance from one row to the
    /// next, in scroll units.
    pub fn row_metrics(&self) -> (f64, f64) {
        self.sizer.row_metrics()
    }
}

/// The strip on the sidebar's right edge that drags its width;
/// double-click restores the design's width. The gestures sit on the
/// sidebar, whose left edge stays put, rather than on the strip, which
/// moves under the pointer; they take only presses on the strip.
fn resize_handle(sizer: &Sizer) -> gtk::Box {
    let handle = gtk::Box::new(gtk::Orientation::Vertical, 0);
    handle.add_css_class("sidebar-handle");
    handle.set_cursor_from_name(Some("col-resize"));

    let on_edge = {
        let root = sizer.root.clone();
        move |x: f64| x >= (root.width() - HANDLE_W) as f64
    };
    // The width the drag started from, while one is under way.
    let start: Rc<cell::Cell<Option<i32>>> = Default::default();
    let drag = gtk::GestureDrag::new();
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    drag.connect_drag_begin({
        let (sizer, start, on_edge) = (sizer.clone(), start.clone(), on_edge.clone());
        move |g, x, _| {
            if on_edge(x) {
                g.set_state(gtk::EventSequenceState::Claimed);
                // The width on screen, which can be more than the one asked
                // for if something in the sidebar needs more room.
                start.set(Some(sizer.root.width()));
                sizer.dragging.set(true);
            } else {
                g.set_state(gtk::EventSequenceState::Denied);
            }
        }
    });
    drag.connect_drag_update({
        let (sizer, start) = (sizer.clone(), start.clone());
        move |_, dx, _| {
            if let Some(w) = start.get() {
                sizer.drag_to(w + dx.round() as i32, false);
            }
        }
    });
    drag.connect_drag_end({
        let (sizer, start) = (sizer.clone(), start.clone());
        move |_, _, _| {
            sizer.drag_ended();
            if start.take().is_some() {
                sizer.notify(true);
            }
        }
    });

    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed({
        let sizer = sizer.clone();
        move |_, n, x, _| {
            if n == 2 && on_edge(x) {
                sizer.drag_to(WIDTH, true);
            }
        }
    });
    sizer.root.add_controller(drag.clone());
    sizer.root.add_controller(click.clone());
    // Grouped, so the drag claiming a press on the strip doesn't deny the
    // click.
    click.group_with(&drag);
    handle
}

/// Scroll the grid just enough to show `child` and the space around it.
/// Measured against `grid`, which doesn't move with a scroll that hasn't
/// been laid out yet; its coordinates leave out the padding the adjustment
/// counts.
fn scroll_to_child(scroll: &gtk::ScrolledWindow, grid: &gtk::FlowBox, child: &gtk::FlowBoxChild) {
    let Some(bounds) = child.compute_bounds(grid) else {
        return;
    };
    let adj = scroll.vadjustment();
    let top = TOP + bounds.y() as f64;
    let height = bounds.height() as f64;
    if let Some(value) = reveal_value(adj.value(), adj.page_size(), top, height, REVEAL_MARGIN) {
        adj.set_value(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_width_is_two_full_columns() {
        assert_eq!(grid_for(WIDTH), (2, THUMB_W, THUMB_H));
    }

    #[test]
    fn two_columns_shrink_then_one_full_column() {
        let two_min = 2 * MIN_THUMB_W + TWO_COLUMN_EXTRA;
        assert_eq!(
            grid_for(two_min),
            (2, MIN_THUMB_W, MIN_THUMB_W * THUMB_H / THUMB_W)
        );
        assert_eq!(grid_for(two_min - 1).0, 1);
        assert_eq!(grid_for(two_min - 1).1, THUMB_W);
        assert_eq!(grid_for(MIN_WIDTH), (1, THUMB_W, THUMB_H));
    }

    #[test]
    fn out_of_range_widths_clamp() {
        assert_eq!(grid_for(0), grid_for(MIN_WIDTH));
        assert_eq!(grid_for(10_000), grid_for(WIDTH));
    }
}
