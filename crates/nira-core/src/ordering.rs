//! Double-page and manga page ordering, and the edge-press rule:
//! pure functions, unit-tested.

/// One press against a page edge in direction `dir` (+1 bottom, -1 top),
/// with `needed` presses required to turn the page. `presses` is the signed
/// run so far (positive at the bottom edge, negative at the top; a press at
/// the other edge starts a new run). Returns the new count and whether to
/// turn the page now.
pub fn edge_press(presses: i32, dir: i32, needed: i32) -> (i32, bool) {
    let dir = dir.signum();
    let run = presses * dir;
    if run >= needed - 1 {
        (0, true)
    } else {
        (dir * (run.max(0) + 1), false)
    }
}

/// The options that decide which pages share a spread.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpreadRules {
    pub double_page: bool,
    /// Show the first page alone (the cover offset).
    pub cover_alone: bool,
    /// Show wide pages alone.
    pub wide_alone: bool,
}

/// True for a page wider than tall. `dims` are as displayed (rotated).
pub fn is_wide(dims: (u32, u32)) -> bool {
    dims.0 > dims.1
}

/// True if a pair of pages with these sizes is not shown side by side: either
/// page is wide (the worker's form of the rule in `is_virtual_single`).
pub fn pair_shown_alone(a: (u32, u32), b: (u32, u32)) -> bool {
    is_wide(a) || is_wide(b)
}

/// True if `page` is displayed alone even in double-page mode: the cover with the cover offset, or,
/// with `wide_alone`, when `page` or the page after it is wide. `dims` gives
/// the known (decoded) page sizes; a page of unknown size counts as narrow.
pub fn is_virtual_single(
    n: usize,
    page: usize,
    rules: SpreadRules,
    dims: impl Fn(usize) -> Option<(u32, u32)>,
) -> bool {
    if rules.cover_alone && page == 0 {
        return true;
    }
    if !rules.wide_alone || page + 1 >= n {
        return false;
    }
    [page, page + 1]
        .into_iter()
        .any(|p| dims(p).is_some_and(is_wide))
}

/// The pages of the spread shown for `page`: `[page, page + 1]` in
/// double-page mode, or `[page]` alone (single-page mode, the last page, or
/// a virtual single page). A spread starts at the current
/// page, so with the cover offset flipping visits `[0]`, `(1,2)`, `(3,4)`, ...
pub fn spread_of(
    n: usize,
    page: usize,
    rules: SpreadRules,
    dims: impl Fn(usize) -> Option<(u32, u32)>,
) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let page = page.min(n - 1);
    if !rules.double_page || page + 1 >= n || is_virtual_single(n, page, rules, dims) {
        return vec![page];
    }
    vec![page, page + 1]
}

/// Slot indices in visual (left to right) order: manga mode shows the first
/// page of the spread on the right, Western on the left.
pub fn visual_order(spread_len: usize, manga: bool) -> Vec<usize> {
    if manga && spread_len > 1 {
        (0..spread_len).rev().collect()
    } else {
        (0..spread_len).collect()
    }
}

/// Magnitude of one page flip (1 or 2): in double-page mode with the double
/// step an extra page is skipped, except when the relevant page is displayed
/// alone (see `is_virtual_single`).
///
/// Backward, the flip returns to the spread forward flipping came from, so
/// wide pages don't make a page show twice: the spread starts are walked
/// from the first page. When `page` is not on that path (after a jump), it
/// goes one page back if the page before the target is alone, else two.
pub fn step_magnitude(
    n: usize,
    page: usize,
    forward: bool,
    double_step: bool,
    rules: SpreadRules,
    dims: impl Fn(usize) -> Option<(u32, u32)>,
) -> i32 {
    if !double_step || n == 0 {
        return 1;
    }
    let single = |p| is_virtual_single(n, p, rules, &dims);
    let step = |p| if single(p) { 1usize } else { 2 };
    if forward {
        return step(page) as i32;
    }
    let (mut prev, mut start) = (0, 0);
    while start < page {
        prev = start;
        start += step(start);
    }
    if start == page && page > 0 {
        (page - prev) as i32
    } else if page >= 2 && single(page - 2) {
        1
    } else {
        2
    }
}

/// Focus-mode page label, pages in on-screen order (left to right) plus the
/// reading direction: `37 / 41 · L→R`, or `38–37 / 41 · R→L` for a manga
/// spread. `spread` holds 0-based page indices in reading order.
pub fn focus_label(spread: &[usize], total: usize, manga: bool) -> String {
    let pages: Vec<String> = visual_order(spread.len(), manga)
        .into_iter()
        .map(|slot| (spread[slot] + 1).to_string())
        .collect();
    let dir = if manga { "R→L" } else { "L→R" };
    format!("{} / {total} · {dir}", pages.join("–"))
}

#[cfg(test)]
mod spread_tests {

    #[test]
    fn edge_press_turns_after_n_presses_in_a_row() {
        // One press needed: turns at once, either way.
        assert_eq!(edge_press(0, 1, 1), (0, true));
        assert_eq!(edge_press(0, -1, 1), (0, true));
        // Three needed: two presses build up, the third turns.
        assert_eq!(edge_press(0, 1, 3), (1, false));
        assert_eq!(edge_press(1, 1, 3), (2, false));
        assert_eq!(edge_press(2, 1, 3), (0, true));
        assert_eq!(edge_press(-2, -1, 3), (0, true));
        // A press at the other edge restarts the run.
        assert_eq!(edge_press(2, -1, 3), (-1, false));
        assert_eq!(edge_press(-2, 1, 3), (1, false));
    }
    use super::*;

    const TALL: (u32, u32) = (800, 1200);
    const WIDE: (u32, u32) = (1600, 1200);

    fn rules(cover_alone: bool, wide_alone: bool) -> SpreadRules {
        SpreadRules {
            double_page: true,
            cover_alone,
            wide_alone,
        }
    }

    /// Page sizes where the pages in `wide` are wide and the rest tall.
    fn dims(wide: &[usize]) -> impl Fn(usize) -> Option<(u32, u32)> + '_ {
        move |p| Some(if wide.contains(&p) { WIDE } else { TALL })
    }

    fn unknown(_: usize) -> Option<(u32, u32)> {
        None
    }

    /// The spreads visited flipping from the first page to the end, then
    /// back from the last spread to the start, with the double step.
    fn walk(n: usize, r: SpreadRules, wide: &[usize]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let step = |p, fwd| step_magnitude(n, p, fwd, true, r, dims(wide)) as usize;
        let mut page = 0usize;
        let mut fwd = vec![spread_of(n, page, r, dims(wide))];
        loop {
            let next = page + step(page, true);
            if next >= n {
                break;
            }
            page = next;
            fwd.push(spread_of(n, page, r, dims(wide)));
            assert!(fwd.len() <= n);
        }
        let mut back = vec![spread_of(n, page, r, dims(wide))];
        while page > 0 {
            page = page.saturating_sub(step(page, false));
            back.push(spread_of(n, page, r, dims(wide)));
            assert!(back.len() <= n);
        }
        (fwd, back)
    }

    #[test]
    fn spread_without_cover_offset() {
        let r = rules(false, false);
        // A spread starts at the current page.
        assert_eq!(spread_of(10, 0, r, unknown), vec![0, 1]);
        assert_eq!(spread_of(10, 1, r, unknown), vec![1, 2]);
        assert_eq!(spread_of(10, 2, r, unknown), vec![2, 3]);
        // Last page alone when it has no pair.
        assert_eq!(spread_of(10, 9, r, unknown), vec![9]);
        // Single page mode.
        let single = SpreadRules::default();
        assert_eq!(spread_of(10, 4, single, unknown), vec![4]);
        // No pages / single-page document.
        assert!(spread_of(0, 0, r, unknown).is_empty());
        assert_eq!(spread_of(1, 0, r, unknown), vec![0]);
    }

    #[test]
    fn spread_with_cover_offset() {
        let r = rules(true, false);
        // Cover alone, then pairs from the current page.
        assert_eq!(spread_of(10, 0, r, unknown), vec![0]);
        assert_eq!(spread_of(10, 1, r, unknown), vec![1, 2]);
        assert_eq!(spread_of(10, 3, r, unknown), vec![3, 4]);
        // Last page has no pair.
        assert_eq!(spread_of(10, 9, r, unknown), vec![9]);
        // The offset applies only in double-page mode.
        let single = SpreadRules {
            cover_alone: true,
            ..SpreadRules::default()
        };
        assert_eq!(spread_of(10, 0, single, unknown), vec![0]);
        assert_eq!(spread_of(10, 3, single, unknown), vec![3]);
    }

    #[test]
    fn wide_pages_are_shown_alone() {
        let r = rules(false, true);
        let d = dims(&[4]);
        // The wide page itself, and the page before it (it cannot pair).
        assert_eq!(spread_of(10, 4, r, &d), vec![4]);
        assert_eq!(spread_of(10, 3, r, &d), vec![3]);
        // Pages around it pair as usual.
        assert_eq!(spread_of(10, 1, r, &d), vec![1, 2]);
        assert_eq!(spread_of(10, 5, r, &d), vec![5, 6]);
        // Unknown sizes count as narrow.
        assert_eq!(spread_of(10, 4, r, unknown), vec![4, 5]);
        // Option off: wide pages pair.
        assert_eq!(spread_of(10, 4, rules(false, false), &d), vec![4, 5]);
        // A wide last page has nothing to pair with anyway.
        assert_eq!(spread_of(10, 9, r, dims(&[9])), vec![9]);
    }

    #[test]
    fn wide_rule_uses_rotated_size() {
        // A tall page rotated by 90 degrees reaches the rule as wide.
        assert!(is_wide((1200, 800)));
        assert!(!is_wide((800, 1200)));
        assert!(!is_wide((1000, 1000)));
        assert!(pair_shown_alone(TALL, WIDE));
        assert!(pair_shown_alone(WIDE, TALL));
        assert!(!pair_shown_alone(TALL, TALL));
    }

    #[test]
    fn stepping_without_cover_offset() {
        let r = rules(false, false);
        // Plain double stepping visits the pair starts.
        assert_eq!(step_magnitude(10, 0, true, true, r, unknown), 2);
        assert_eq!(step_magnitude(10, 2, true, true, r, unknown), 2);
        assert_eq!(step_magnitude(10, 2, false, true, r, unknown), 2);
        // Single stepping when the preference is off.
        assert_eq!(step_magnitude(10, 0, true, false, r, unknown), 1);
        assert_eq!(step_magnitude(10, 5, false, false, r, unknown), 1);
    }

    #[test]
    fn stepping_with_cover_offset() {
        let r = rules(true, false);
        // Forward from the cover advances a single page (to the first pair).
        assert_eq!(step_magnitude(10, 0, true, true, r, unknown), 1);
        assert_eq!(step_magnitude(10, 1, true, true, r, unknown), 2);
        assert_eq!(step_magnitude(10, 3, true, true, r, unknown), 2);
        // Backward from the first pair lands on the cover.
        assert_eq!(step_magnitude(10, 1, false, true, r, unknown), 1);
        // Backward from the second pair lands on the first pair.
        assert_eq!(step_magnitude(10, 3, false, true, r, unknown), 2);
        assert_eq!(step_magnitude(10, 5, false, true, r, unknown), 2);
        // Off the flipping path (a jump to page 2): the fallback rule
        // goes back one page because the cover cannot pair.
        assert_eq!(step_magnitude(10, 2, false, true, r, unknown), 1);
        assert_eq!(step_magnitude(10, 4, false, true, r, unknown), 2);
    }

    #[test]
    fn cover_offset_visits_every_pair() {
        let (fwd, back) = walk(10, rules(true, false), &[]);
        let expect = vec![
            vec![0],
            vec![1, 2],
            vec![3, 4],
            vec![5, 6],
            vec![7, 8],
            vec![9],
        ];
        assert_eq!(fwd, expect);
        let mut rev = expect.clone();
        rev.reverse();
        assert_eq!(back, rev);
    }

    #[test]
    fn wide_page_walk_visits_every_page_once() {
        // 8 pages, page 5 (index 4) wide, cover offset on.
        let (fwd, back) = walk(8, rules(true, true), &[4]);
        let expect = vec![vec![0], vec![1, 2], vec![3], vec![4], vec![5, 6], vec![7]];
        assert_eq!(fwd, expect);
        let mut rev = expect.clone();
        rev.reverse();
        assert_eq!(back, rev);
    }

    #[test]
    fn consecutive_wide_pages_walk() {
        // No cover offset; indices 2 and 3 wide.
        let (fwd, back) = walk(7, rules(false, true), &[2, 3]);
        let expect = vec![vec![0, 1], vec![2], vec![3], vec![4, 5], vec![6]];
        assert_eq!(fwd, expect);
        let mut rev = expect.clone();
        rev.reverse();
        assert_eq!(back, rev);
    }

    #[test]
    fn visual_order_manga_puts_first_page_right() {
        // Western: slot 0 (first page) left, slot 1 right.
        assert_eq!(visual_order(2, false), vec![0, 1]);
        // Manga: slot 1 appended first (left), slot 0 (first page) last = right.
        assert_eq!(visual_order(2, true), vec![1, 0]);
        // Single page: order is irrelevant.
        assert_eq!(visual_order(1, true), vec![0]);
        assert_eq!(visual_order(1, false), vec![0]);
    }

    #[test]
    fn focus_labels() {
        assert_eq!(focus_label(&[36], 41, false), "37 / 41 · L→R");
        assert_eq!(focus_label(&[36, 37], 41, false), "37–38 / 41 · L→R");
        assert_eq!(focus_label(&[36, 37], 41, true), "38–37 / 41 · R→L");
        // Cover alone in a manga spread.
        let cover = spread_of(41, 0, rules(true, false), unknown);
        assert_eq!(focus_label(&cover, 41, true), "1 / 41 · R→L");
    }
}
