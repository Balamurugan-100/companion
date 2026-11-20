use bevy::prelude::Vec2;
use std::cmp::Ordering;

/// Small inset kept between a clamped window and the monitor edge.
pub const CLAMP_MARGIN: f32 = 16.0;

/// Minimum visible overlap, per axis, that counts as "still on a monitor".
/// A smaller sliver is treated as off-screen and clamped back.
pub const MIN_VISIBLE_EXTENT: f32 = 64.0;

/// Monitor geometry in the same physical-pixel coordinate space as the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorBounds {
    pub origin: Vec2,
    pub size: Vec2,
}

impl MonitorBounds {
    fn overlap(&self, position: Vec2, window_size: Vec2) -> Vec2 {
        let left = position.x.max(self.origin.x);
        let right = (position.x + window_size.x).min(self.origin.x + self.size.x);
        let top = position.y.max(self.origin.y);
        let bottom = (position.y + window_size.y).min(self.origin.y + self.size.y);
        Vec2::new((right - left).max(0.0), (bottom - top).max(0.0))
    }

    fn clamped_origin(&self, position: Vec2, window_size: Vec2) -> Vec2 {
        Vec2::new(
            clamp_axis(position.x, self.origin.x, self.size.x, window_size.x),
            clamp_axis(position.y, self.origin.y, self.size.y, window_size.y),
        )
    }
}

fn clamp_axis(value: f32, origin: f32, extent: f32, window: f32) -> f32 {
    let min = origin + CLAMP_MARGIN;
    let max = origin + extent - window - CLAMP_MARGIN;
    if min > max {
        // A window larger than the monitor is centred instead of pinned to a corner.
        origin + (extent - window) / 2.0
    } else {
        value.clamp(min, max)
    }
}

fn finite(point: Vec2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn distance_squared(a: Vec2, b: Vec2) -> f32 {
    (a - b).length_squared()
}

/// Clamps a window origin back onto the available monitors.
///
/// A position with a meaningful part still visible is left untouched, so normal
/// dragging is never fought. Only an effectively off-screen window is moved, to
/// the clamped position nearest its current one. An empty or non-finite monitor
/// set leaves the position unchanged: there is nothing useful to clamp against.
pub fn clamp_window_position(
    position: Vec2,
    window_size: Vec2,
    monitors: &[MonitorBounds],
) -> Vec2 {
    if monitors.is_empty()
        || !finite(position)
        || !finite(window_size)
        || monitors
            .iter()
            .any(|monitor| !finite(monitor.origin) || !finite(monitor.size))
    {
        return position;
    }

    let visible_x = MIN_VISIBLE_EXTENT.min(window_size.x.max(0.0));
    let visible_y = MIN_VISIBLE_EXTENT.min(window_size.y.max(0.0));
    let still_visible = monitors.iter().any(|monitor| {
        let overlap = monitor.overlap(position, window_size);
        overlap.x >= visible_x && overlap.y >= visible_y
    });
    if still_visible {
        return position;
    }

    monitors
        .iter()
        .map(|monitor| monitor.clamped_origin(position, window_size))
        .min_by(|a, b| {
            distance_squared(*a, position)
                .partial_cmp(&distance_squared(*b, position))
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: f32, y: f32, width: f32, height: f32) -> MonitorBounds {
        MonitorBounds {
            origin: Vec2::new(x, y),
            size: Vec2::new(width, height),
        }
    }

    fn at(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }

    const WINDOW: Vec2 = Vec2::new(320.0, 320.0);

    #[test]
    fn a_fully_visible_window_is_left_where_the_user_put_it() {
        let position = clamp_window_position(
            at(400.0, 300.0),
            WINDOW,
            &[monitor(0.0, 0.0, 1920.0, 1080.0)],
        );

        assert_eq!(position, at(400.0, 300.0));
    }

    #[test]
    fn a_window_still_meaningfully_on_a_monitor_is_not_moved() {
        let monitors = [
            monitor(0.0, 0.0, 1920.0, 1080.0),
            monitor(1920.0, 0.0, 1920.0, 1080.0),
        ];

        // 300px of the window is on the second monitor: normal straddling.
        assert_eq!(
            clamp_window_position(at(1900.0, 400.0), WINDOW, &monitors),
            at(1900.0, 400.0)
        );
        // 180px of the window is on the monitor at a negative origin.
        assert_eq!(
            clamp_window_position(
                at(-1740.0, 400.0),
                WINDOW,
                &[monitor(-1920.0, 0.0, 1920.0, 1080.0)]
            ),
            at(-1740.0, 400.0)
        );
    }

    #[test]
    fn a_window_with_only_a_sliver_visible_is_moved_back_with_a_margin() {
        let monitors = [monitor(0.0, 0.0, 1920.0, 1080.0)];

        assert_eq!(
            clamp_window_position(at(-312.0, 400.0), WINDOW, &monitors),
            at(CLAMP_MARGIN, 400.0)
        );
        assert_eq!(
            clamp_window_position(at(1912.0, 400.0), WINDOW, &monitors),
            at(1920.0 - 320.0 - CLAMP_MARGIN, 400.0)
        );
    }

    #[test]
    fn unplugging_a_monitor_moves_the_window_to_the_remaining_one() {
        // The window was on the second monitor when it was unplugged.
        let remaining = [monitor(0.0, 0.0, 1920.0, 1080.0)];

        assert_eq!(
            clamp_window_position(at(2500.0, 200.0), WINDOW, &remaining),
            at(1584.0, 200.0)
        );
    }

    #[test]
    fn a_window_moves_to_the_nearest_of_several_monitors() {
        let monitors = [
            monitor(0.0, 0.0, 1000.0, 800.0),
            monitor(1000.0, 0.0, 1000.0, 800.0),
        ];

        // Off the left edge: the first monitor is nearest.
        assert_eq!(
            clamp_window_position(at(-300.0, 100.0), WINDOW, &monitors),
            at(CLAMP_MARGIN, 100.0)
        );
        // Below both monitors: the second monitor is nearer horizontally.
        assert_eq!(
            clamp_window_position(at(1050.0, 900.0), WINDOW, &monitors),
            at(1050.0, 464.0)
        );
    }

    #[test]
    fn negative_monitor_origins_are_handled() {
        let monitors = [monitor(-1920.0, 0.0, 1920.0, 1080.0)];

        assert_eq!(
            clamp_window_position(at(-2300.0, 120.0), WINDOW, &monitors),
            at(-1904.0, 120.0)
        );
    }

    #[test]
    fn a_single_monitor_clamps_both_axes() {
        let monitors = [monitor(0.0, 0.0, 800.0, 600.0)];

        assert_eq!(
            clamp_window_position(at(900.0, 700.0), WINDOW, &monitors),
            at(464.0, 264.0)
        );
    }

    #[test]
    fn a_window_bigger_than_its_monitor_is_centered() {
        let monitors = [monitor(100.0, 100.0, 100.0, 100.0)];

        assert_eq!(
            clamp_window_position(at(500.0, 500.0), WINDOW, &monitors),
            at(-10.0, -10.0)
        );
    }

    #[test]
    fn a_small_window_fully_inside_a_monitor_is_left_alone() {
        let monitors = [monitor(0.0, 0.0, 1920.0, 1080.0)];

        assert_eq!(
            clamp_window_position(at(100.0, 100.0), Vec2::splat(40.0), &monitors),
            at(100.0, 100.0)
        );
    }

    #[test]
    fn no_available_monitors_leaves_the_position_unchanged() {
        assert_eq!(
            clamp_window_position(at(2500.0, -100.0), WINDOW, &[]),
            at(2500.0, -100.0)
        );
    }

    #[test]
    fn non_finite_input_leaves_the_position_unchanged() {
        let monitors = [monitor(0.0, 0.0, 1920.0, 1080.0)];

        assert!(
            clamp_window_position(at(f32::NAN, 0.0), WINDOW, &monitors)
                .x
                .is_nan()
        );
        assert_eq!(
            clamp_window_position(at(2500.0, 0.0), Vec2::splat(f32::INFINITY), &monitors),
            at(2500.0, 0.0)
        );
    }
}
