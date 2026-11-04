//! Which page thumbnails to make, in what order: the pages around the
//! current one and those visible in the sidebar, nearest first.

use std::collections::BTreeSet;
use std::ops::Range;

/// Pages on each side of the current page that are always wanted.
pub const AROUND_CURRENT: usize = 32;

/// Pages in the visible part of a `columns`-wide grid of rows `row_h`
/// pixels tall (after `top` pixels of padding), scrolled to `value` with
/// `page_size` pixels visible, plus a few rows of margin.
pub fn visible_cells(
    value: f64,
    page_size: f64,
    row_h: f64,
    top: f64,
    columns: usize,
    n: usize,
) -> Range<usize> {
    const MARGIN_ROWS: f64 = 4.0;
    let first_row = ((value - top) / row_h).floor() - MARGIN_ROWS;
    let last_row = ((value + page_size) / row_h).ceil() + MARGIN_ROWS;
    let first = (first_row.max(0.0) as usize * columns).min(n);
    let last = ((last_row.max(0.0) as usize + 1) * columns).min(n);
    first..last
}

/// The scroll value that shows a cell `top` pixels down the grid and
/// `height` tall, keeping `margin` pixels clear at both edges of the view,
/// or `None` when it already shows. A cell that can't fit with its margins
/// shows its top.
pub fn reveal_value(value: f64, page_size: f64, top: f64, height: f64, margin: f64) -> Option<f64> {
    let (start, end) = (top - margin, top + height + margin);
    if start < value || end - start > page_size {
        Some(start)
    } else if end > value + page_size {
        Some(end - page_size)
    } else {
        None
    }
}

/// The wanted pages in priority order: the current page, then outward from
/// it, then the rest of the visible ones.
pub fn wanted_order(page: usize, n: usize, visible: Range<usize>) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let page = page.min(n - 1);
    let mut wanted: BTreeSet<usize> =
        (page.saturating_sub(AROUND_CURRENT)..=(page + AROUND_CURRENT).min(n - 1)).collect();
    wanted.extend(visible.filter(|&i| i < n));

    let mut order = Vec::with_capacity(wanted.len());
    let mut take = |i: usize, order: &mut Vec<usize>| {
        if wanted.remove(&i) {
            order.push(i);
        }
    };
    take(page, &mut order);
    for step in 1..=AROUND_CURRENT {
        take(page + step, &mut order);
        if let Some(prev) = page.checked_sub(step) {
            take(prev, &mut order);
        }
    }
    order.extend(wanted);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_cells_cover_whole_rows() {
        // Top of a 2-column grid, 3 rows tall: rows 0..=7 with the margin.
        assert_eq!(visible_cells(0.0, 300.0, 100.0, 0.0, 2, 100), 0..16);
        // Scrolled to row 20: 4 rows of margin above.
        let r = visible_cells(2000.0, 200.0, 100.0, 0.0, 2, 100);
        assert_eq!(r.start, 32);
        assert!(r.contains(&40) && r.contains(&45));
        assert_eq!(visible_cells(0.0, 1000.0, 100.0, 0.0, 2, 5), 0..5);
    }

    #[test]
    fn reveal_moves_only_as_far_as_needed() {
        // Fully in view, margins included.
        assert_eq!(reveal_value(100.0, 500.0, 200.0, 150.0, 14.0), None);
        // Above the view: its top lands one margin below the edge.
        assert_eq!(reveal_value(300.0, 500.0, 200.0, 150.0, 14.0), Some(186.0));
        // Below the view: its bottom lands one margin above the edge.
        assert_eq!(reveal_value(0.0, 500.0, 400.0, 150.0, 14.0), Some(64.0));
        // Its margin alone is enough to move the view.
        assert_eq!(reveal_value(0.0, 500.0, 340.0, 150.0, 14.0), Some(4.0));
    }

    #[test]
    fn reveal_shows_the_top_when_the_view_is_too_small() {
        assert_eq!(reveal_value(0.0, 100.0, 400.0, 150.0, 14.0), Some(386.0));
    }

    #[test]
    fn current_page_first_then_outward_then_visible() {
        let order = wanted_order(50, 200, 150..154);
        assert_eq!(&order[..5], &[50, 51, 49, 52, 48]);
        assert_eq!(&order[order.len() - 4..], &[150, 151, 152, 153]);
        let unique: BTreeSet<_> = order.iter().collect();
        assert_eq!(unique.len(), order.len(), "no page twice");
        assert_eq!(wanted_order(0, 0, 0..0), Vec::<usize>::new());
        assert_eq!(wanted_order(1, 3, 0..3), vec![1, 2, 0]);
    }
}
