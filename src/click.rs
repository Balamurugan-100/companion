use crate::hit_test::Point;
use std::time::{Duration, Instant};

/// Movement beyond this many logical pixels turns a press into a window drag.
pub const DRAG_THRESHOLD: f32 = 6.0;

/// A press held longer than this is treated as intentional dragging, not a click.
pub const MAX_CLICK_DURATION: Duration = Duration::from_millis(700);

/// Distinguishes a click on the character from a drag of its window.
///
/// Dragging starts only after the pointer moves past `DRAG_THRESHOLD`, so a
/// plain press/release stays available for opening the setup panel.
#[derive(Debug, Default)]
pub struct ClickTracker {
    pressed: Option<(Point, Instant)>,
    dragged: bool,
}

impl ClickTracker {
    /// Records the beginning of a gesture. A second press while one is active
    /// is ignored so a duplicate event cannot reset the drag state.
    pub fn press(&mut self, point: Point, now: Instant) {
        if self.pressed.is_none() {
            self.pressed = Some((point, now));
            self.dragged = false;
        }
    }

    /// Returns true once, on the first movement that qualifies as a drag.
    pub fn move_to(&mut self, point: Point) -> bool {
        let Some((start, _)) = self.pressed else {
            return false;
        };
        if self.dragged {
            return false;
        }
        if within_threshold(start, point) {
            false
        } else {
            self.dragged = true;
            true
        }
    }

    /// True once the press has turned into a drag.
    pub fn is_dragged(&self) -> bool {
        self.dragged
    }

    /// Returns true only when the whole gesture was a click.
    pub fn release(&mut self, point: Point, now: Instant) -> bool {
        let Some((start, at)) = self.pressed.take() else {
            return false;
        };
        let dragged = self.dragged;
        self.dragged = false;
        !dragged && now.duration_since(at) <= MAX_CLICK_DURATION && within_threshold(start, point)
    }

    /// Forgets an unfinished gesture, for example when the window is hidden.
    pub fn cancel(&mut self) {
        self.pressed = None;
        self.dragged = false;
    }
}

fn within_threshold(start: Point, point: Point) -> bool {
    let dx = point.x - start.x;
    let dy = point.y - start.y;
    dx * dx + dy * dy <= DRAG_THRESHOLD * DRAG_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    #[test]
    fn a_press_and_release_in_place_is_a_click() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        assert!(tracker.release(at(120.0, 130.0), now + Duration::from_millis(80)));
    }

    #[test]
    fn moving_past_the_threshold_makes_it_a_drag() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        assert!(!tracker.move_to(at(124.0, 130.0)));
        assert!(tracker.move_to(at(128.0, 130.0)));
        assert!(!tracker.move_to(at(140.0, 130.0)));
        assert!(!tracker.release(at(140.0, 130.0), now + Duration::from_millis(80)));
    }

    #[test]
    fn a_long_press_is_not_a_click() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        assert!(!tracker.release(
            at(120.0, 130.0),
            now + MAX_CLICK_DURATION + Duration::from_millis(1)
        ));
    }

    #[test]
    fn releasing_away_from_the_press_is_not_a_click() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        assert!(!tracker.release(at(180.0, 130.0), now + Duration::from_millis(80)));
    }

    #[test]
    fn release_without_a_press_is_not_a_click() {
        let mut tracker = ClickTracker::default();
        assert!(!tracker.release(at(120.0, 130.0), Instant::now()));
    }

    #[test]
    fn a_second_press_does_not_replace_the_first() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        tracker.press(at(180.0, 130.0), now);
        assert!(!tracker.release(at(180.0, 130.0), now + Duration::from_millis(80)));
    }

    #[test]
    fn cancel_forgets_an_unfinished_gesture() {
        let now = Instant::now();
        let mut tracker = ClickTracker::default();
        tracker.press(at(120.0, 130.0), now);
        tracker.cancel();
        assert!(!tracker.release(at(120.0, 130.0), now + Duration::from_millis(80)));
    }
}
