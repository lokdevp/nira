//! Scrolling decisions, from plain adjustment numbers: line scrolling, the
//! "turn the page after N presses at the edge" rule, and smart scroll over
//! the spread's layout.

use crate::layout::{FiniteLayout, SCROLL_TO_END, SCROLL_TO_START};
use crate::ordering::edge_press;

/// One scroll axis: what a `gtk::Adjustment` reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
    pub page_size: f64,
}

impl Axis {
    fn max(&self) -> f64 {
        (self.upper - self.page_size).max(self.lower)
    }

    /// The page is no larger than the viewport along this axis.
    pub fn fits(&self) -> bool {
        self.upper - self.lower <= self.page_size + 1.0
    }
}

/// Result of a scroll step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Scroll to this value.
    To(f64),
    /// Already at the edge in that direction.
    Edge,
    /// The page fits: there is nothing to scroll.
    Fits,
}

/// Scroll `axis` by `delta` pixels (negative: up or left).
pub fn line_step(axis: Axis, delta: f64) -> Step {
    if axis.fits() {
        return Step::Fits;
    }
    let at_edge = if delta > 0.0 {
        axis.value >= axis.max() - 1.0
    } else {
        axis.value <= axis.lower + 1.0
    };
    if at_edge {
        Step::Edge
    } else {
        Step::To((axis.value + delta).clamp(axis.lower, axis.max()))
    }
}

/// The scroll value that shows a cell `top` pixels down the content and
/// `height` tall, keeping up to `margin` pixels clear at both edges of the
/// view, or `None` when it already shows. The margin shrinks to what fits;
/// a cell taller than the view shows its top.
pub fn reveal_value(value: f64, page_size: f64, top: f64, height: f64, margin: f64) -> Option<f64> {
    let margin = margin.min((page_size - height) / 2.0).max(0.0);
    let (start, end) = (top - margin, top + height + margin);
    let target = if start < value || height > page_size {
        start
    } else if end > value + page_size {
        end - page_size
    } else {
        return None;
    };
    (target != value).then_some(target)
}

/// Counts presses against a page edge; the page turns after `needed` in a
/// row in the same direction.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeGuard {
    presses: i32,
}

impl EdgeGuard {
    /// One press at the edge. True if the page should turn now.
    pub fn press(&mut self, forward: bool, needed: u32) -> bool {
        let dir = if forward { 1 } else { -1 };
        let (presses, turn) = edge_press(self.presses, dir, needed.max(1) as i32);
        self.presses = presses;
        turn
    }

    pub fn reset(&mut self) {
        self.presses = 0;
    }
}

/// Where a page turn leaves the view: at the start of the new spread
/// (going forward) or its end (going back).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Start,
    End,
}

/// The shown spread's geometry for smart scroll and page-turn positions.
/// Positions are scroll values (top-left of the viewport in content
/// coordinates).
pub struct SpreadLayout {
    layout: FiniteLayout,
}

/// Gap between the pages of a spread, in pixels (the content box spacing).
pub const SPREAD_SPACING: i64 = 2;

impl SpreadLayout {
    /// `sizes` are the display sizes in reading order; `viewport` is the
    /// scrolled view's size. Manga spreads read right to left.
    pub fn new(sizes: &[(u32, u32)], viewport: (f64, f64), manga: bool) -> SpreadLayout {
        let content: Vec<[i64; 2]> = sizes.iter().map(|&(w, h)| [w as i64, h as i64]).collect();
        let orientation = if manga { [-1, 1] } else { [1, 1] };
        let layout = FiniteLayout::new(
            &content,
            [viewport.0 as i64, viewport.1 as i64],
            orientation,
            SPREAD_SPACING,
            false,
            0,
            1,
        );
        SpreadLayout { layout }
    }

    /// The scroll position for `place`.
    pub fn position(&mut self, place: Place) -> (f64, f64) {
        let dest = match place {
            Place::Start => SCROLL_TO_START,
            Place::End => SCROLL_TO_END,
        };
        self.layout.scroll_to_predefined([dest, dest], Some(0));
        let [x, y] = self.layout.viewport_box.pos;
        (x as f64, y as f64)
    }

    /// One smart-scroll step from `pos`, moving at most `fraction` of the
    /// viewport per axis. `invert` scrolls across before down. `None` when
    /// the spread is exhausted in that direction (turn the page).
    pub fn smart_step(
        &mut self,
        pos: (f64, f64),
        viewport: (f64, f64),
        fraction: f64,
        backwards: bool,
        invert: bool,
    ) -> Option<(f64, f64)> {
        self.layout
            .set_viewport_position([pos.0.round() as i64, pos.1.round() as i64]);
        let step = [
            (viewport.0 * fraction).max(1.0),
            (viewport.1 * fraction).max(1.0),
        ];
        let axis_map = invert.then_some([1, 0]);
        self.layout
            .scroll_smartly(step, backwards, axis_map)
            .map(|[x, y]| (x as f64, y as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(value: f64, upper: f64, page: f64) -> Axis {
        Axis {
            value,
            lower: 0.0,
            upper,
            page_size: page,
        }
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
    fn reveal_shrinks_the_margin_to_show_the_whole_cell() {
        // 156 tall in a 160 view leaves 2 on each side.
        assert_eq!(reveal_value(0.0, 160.0, 400.0, 156.0, 14.0), Some(398.0));
        assert_eq!(reveal_value(500.0, 160.0, 400.0, 156.0, 14.0), Some(398.0));
        assert_eq!(reveal_value(398.0, 160.0, 400.0, 156.0, 14.0), None);
    }

    #[test]
    fn reveal_shows_the_top_when_the_view_is_too_small() {
        assert_eq!(reveal_value(0.0, 100.0, 400.0, 150.0, 14.0), Some(400.0));
        assert_eq!(reveal_value(450.0, 100.0, 400.0, 150.0, 14.0), Some(400.0));
        // Already there.
        assert_eq!(reveal_value(400.0, 100.0, 400.0, 150.0, 14.0), None);
    }

    #[test]
    fn line_steps_stop_at_the_edges() {
        assert_eq!(line_step(axis(0.0, 500.0, 500.0), 50.0), Step::Fits);
        assert_eq!(line_step(axis(0.0, 1000.0, 500.0), 50.0), Step::To(50.0));
        assert_eq!(line_step(axis(480.0, 1000.0, 500.0), 50.0), Step::To(500.0));
        assert_eq!(line_step(axis(500.0, 1000.0, 500.0), 50.0), Step::Edge);
        assert_eq!(line_step(axis(0.0, 1000.0, 500.0), -50.0), Step::Edge);
    }

    #[test]
    fn edge_guard_needs_presses_in_a_row() {
        let mut g = EdgeGuard::default();
        assert!(!g.press(true, 3));
        assert!(!g.press(true, 3));
        assert!(g.press(true, 3));
        assert!(!g.press(true, 3));
        assert!(!g.press(false, 3), "the other edge starts over");
        g.reset();
        assert!(g.press(false, 1));
    }

    #[test]
    fn smart_scroll_reads_down_then_turns() {
        // A page twice as tall as the viewport, as wide.
        let mut l = SpreadLayout::new(&[(400, 800)], (400.0, 400.0), false);
        assert_eq!(l.position(Place::Start), (0.0, 0.0));
        assert_eq!(l.position(Place::End), (0.0, 400.0));
        let next = l.smart_step((0.0, 0.0), (400.0, 400.0), 0.5, false, false);
        assert_eq!(next, Some((0.0, 200.0)));
        assert_eq!(
            l.smart_step((0.0, 400.0), (400.0, 400.0), 0.5, false, false),
            None
        );
        assert!(l
            .smart_step((0.0, 400.0), (400.0, 400.0), 0.5, true, false)
            .is_some());
    }

    #[test]
    fn manga_starts_at_the_right() {
        // A spread wider than the viewport.
        let mut l = SpreadLayout::new(&[(400, 400), (400, 400)], (400.0, 400.0), true);
        let (x, _) = l.position(Place::Start);
        assert!(x > 0.0, "manga reading starts on the right: {x}");
        let mut w = SpreadLayout::new(&[(400, 400), (400, 400)], (400.0, 400.0), false);
        assert_eq!(w.position(Place::Start).0, 0.0);
    }
}
