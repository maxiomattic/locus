//! The on-screen window of a list with mixed row heights, which the shells
//! use to draw only the visible part of Todo and the calendar agenda.

use locus_core::view_model::{RowWindow, row_window};

const OVERSCAN: usize = 6;

/// The window worked out the slow way: every row that touches the viewport,
/// plus `OVERSCAN` rows either side.
fn reference(heights: &[f32], offset: f32, view_h: f32) -> (usize, usize) {
    let mut top = 0.0;
    let mut seen: Option<(usize, usize)> = None;
    for (i, &h) in heights.iter().enumerate() {
        if top + h > offset && top < offset + view_h {
            seen = Some(match seen {
                None => (i, i + 1),
                Some((a, _)) => (a, i + 1),
            });
        }
        top += h;
    }
    let (a, b) = seen.expect("some row is on screen");
    (a.saturating_sub(OVERSCAN), (b + OVERSCAN).min(heights.len()))
}

fn tops(heights: &[f32]) -> Vec<f32> {
    let mut y = 0.0;
    heights
        .iter()
        .map(|h| {
            let t = y;
            y += h;
            t
        })
        .collect()
}

fn check(heights: &[f32], offset: f32, view_h: f32) -> RowWindow {
    let w = row_window(heights, offset, view_h, OVERSCAN);
    assert_eq!((w.first, w.last), reference(heights, offset, view_h), "offset {offset}");
    let drawn: f32 = heights[w.first..w.last].iter().sum();
    let total: f32 = heights.iter().sum();
    assert_eq!(w.above, heights[..w.first].iter().sum::<f32>());
    assert_eq!(w.above + drawn + w.below, total, "spacers keep the list's height");
    w
}

/// Todo as the shells lay it out: a heading row per group, then its tasks.
fn long_todo(header: f32, task: f32) -> Vec<f32> {
    let mut h = Vec::new();
    for n in [3usize, 400, 2500, 1200] {
        h.push(header);
        h.extend(std::iter::repeat_n(task, n));
    }
    h
}

/// The agenda: a heading row per day with events, then that day's events.
fn long_agenda(header: f32, event: f32) -> Vec<f32> {
    let mut h = Vec::new();
    for day in 0..30usize {
        h.push(header);
        h.extend(std::iter::repeat_n(event, 20 + day * 7 % 40));
    }
    h
}

#[test]
fn one_height_matches_the_fixed_height_lists() {
    for row_h in [28.0f32, 40.0, 52.0] {
        let count = 1000usize;
        let heights = vec![row_h; count];
        let view_h = 600.0;
        let mut offset = 0.0;
        while offset + view_h <= count as f32 * row_h {
            // The formula `virtual_list` and `virtual_rows` use.
            let first = ((offset / row_h).floor() as usize).saturating_sub(OVERSCAN).min(count);
            let last = (((offset + view_h) / row_h).ceil() as usize + OVERSCAN).min(count);
            let w = row_window(&heights, offset, view_h, OVERSCAN);
            assert_eq!((w.first, w.last), (first, last), "row_h {row_h} offset {offset}");
            assert_eq!(w.above, first as f32 * row_h);
            assert_eq!(w.below, (count - last) as f32 * row_h);
            offset += 37.0;
        }
    }
}

#[test]
fn long_todo_draws_only_the_screenful() {
    let heights = long_todo(24.0, 52.0);
    let total: f32 = heights.iter().sum();
    let view_h = 700.0;
    let mut offset = 0.0;
    while offset + view_h <= total {
        let w = check(&heights, offset, view_h);
        let screen = (view_h / 24.0).ceil() as usize + 2;
        assert!(w.last - w.first <= screen + 2 * OVERSCAN, "{} rows drawn", w.last - w.first);
        offset += 911.0;
    }
    let w = check(&heights, 0.0, view_h);
    assert_eq!(w.first, 0);
    assert_eq!(w.above, 0.0);
}

#[test]
fn long_agenda_draws_only_the_screenful() {
    let heights = long_agenda(28.0, 28.0 + 4.0);
    let total: f32 = heights.iter().sum();
    let view_h = 500.0;
    let mut offset = 0.0;
    while offset + view_h <= total {
        let w = check(&heights, offset, view_h);
        assert!(w.last - w.first <= (view_h / 28.0).ceil() as usize + 2 + 2 * OVERSCAN);
        offset += 333.0;
    }
    // Scrolled to the end: the last row is drawn and nothing follows it.
    let w = check(&heights, total - view_h, view_h);
    assert_eq!(w.last, heights.len());
    assert_eq!(w.below, 0.0);
}

#[test]
fn a_row_scrolled_to_is_drawn() {
    // Scrolling to the selection puts its top at the viewport's top or its
    // bottom at the viewport's bottom; either way the row must be built.
    let heights = long_todo(24.0, 52.0);
    let tops = tops(&heights);
    let view_h = 640.0;
    for i in [1usize, 4, 405, 1800, 2905, heights.len() - 1] {
        let at_top = row_window(&heights, tops[i], view_h, OVERSCAN);
        assert!((at_top.first..at_top.last).contains(&i), "row {i} at the top");
        let at_bottom = row_window(&heights, (tops[i] + heights[i] - view_h).max(0.0), view_h, OVERSCAN);
        assert!((at_bottom.first..at_bottom.last).contains(&i), "row {i} at the bottom");
    }
}

#[test]
fn edges() {
    let none = row_window(&[], 120.0, 600.0, OVERSCAN);
    assert_eq!(none, RowWindow { first: 0, last: 0, above: 0.0, below: 0.0 });

    let heights = long_agenda(28.0, 32.0);
    let total: f32 = heights.iter().sum();
    // A negative offset (an overscrolled scrollable) is the top.
    assert_eq!(
        row_window(&heights, -50.0, 600.0, OVERSCAN),
        row_window(&heights, 0.0, 600.0, OVERSCAN)
    );
    // An offset past the end (the list shrank since the last scroll) keeps
    // a valid window that reaches the end.
    let past = row_window(&heights, total + 5000.0, 600.0, OVERSCAN);
    assert!(past.first <= past.last);
    assert_eq!(past.last, heights.len());
    assert_eq!(past.below, 0.0);
    // A viewport not yet measured draws only the overscan, without panicking.
    let flat = row_window(&heights, 0.0, 0.0, OVERSCAN);
    assert_eq!((flat.first, flat.last), (0, OVERSCAN));
    // Fewer rows than the screen holds: all are drawn.
    let short = row_window(&[24.0, 52.0, 52.0], 0.0, 600.0, OVERSCAN);
    assert_eq!(short, RowWindow { first: 0, last: 3, above: 0.0, below: 0.0 });
}
