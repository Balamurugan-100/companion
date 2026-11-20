/// Clickable diameter around the character, in logical window pixels.
///
/// The Fox is scaled to roughly 200px wide in the 320px window; the extra
/// margin keeps the whole character clickable while it walks a short distance.
pub const CHARACTER_DIAMETER: f32 = 280.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClickThrough {
    Capture,
    PassThrough,
}

pub fn decision(area: Rect, cursor: Option<Point>) -> ClickThrough {
    if area.contains(cursor) {
        ClickThrough::Capture
    } else {
        ClickThrough::PassThrough
    }
}

pub fn centered_area(width: f32, height: f32, diameter: f32) -> Rect {
    Rect {
        left: (width - diameter) / 2.0,
        top: (height - diameter) / 2.0,
        right: (width + diameter) / 2.0,
        bottom: (height + diameter) / 2.0,
    }
}

impl Rect {
    pub fn contains(&self, cursor: Option<Point>) -> bool {
        match cursor {
            Some(point) => {
                point.x >= self.left
                    && point.x < self.right
                    && point.y >= self.top
                    && point.y < self.bottom
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        left: 20.0,
        top: 20.0,
        right: 140.0,
        bottom: 140.0,
    };

    fn at(x: f32, y: f32) -> Option<Point> {
        Some(Point { x, y })
    }

    #[test]
    fn captures_clicks_over_the_companion() {
        assert_eq!(decision(AREA, at(80.0, 80.0)), ClickThrough::Capture);
    }

    #[test]
    fn passes_clicks_through_outside_the_companion() {
        assert_eq!(decision(AREA, at(10.0, 80.0)), ClickThrough::PassThrough);
        assert_eq!(decision(AREA, at(150.0, 80.0)), ClickThrough::PassThrough);
        assert_eq!(decision(AREA, at(80.0, 10.0)), ClickThrough::PassThrough);
        assert_eq!(decision(AREA, at(80.0, 150.0)), ClickThrough::PassThrough);
    }

    #[test]
    fn passes_clicks_through_when_cursor_is_not_over_the_window() {
        assert_eq!(decision(AREA, None), ClickThrough::PassThrough);
    }

    #[test]
    fn top_and_left_edges_are_inside() {
        assert_eq!(decision(AREA, at(20.0, 20.0)), ClickThrough::Capture);
    }

    #[test]
    fn right_and_bottom_edges_are_outside() {
        assert_eq!(decision(AREA, at(140.0, 140.0)), ClickThrough::PassThrough);
    }

    #[test]
    fn centered_area_covers_the_placeholder() {
        assert_eq!(
            centered_area(320.0, 320.0, 160.0),
            Rect {
                left: 80.0,
                top: 80.0,
                right: 240.0,
                bottom: 240.0,
            }
        );
    }

    #[test]
    fn centered_area_accepts_the_placeholder_edge_but_rejects_the_window_edge() {
        let area = centered_area(320.0, 320.0, 160.0);
        assert_eq!(decision(area, at(80.0, 80.0)), ClickThrough::Capture);
        assert_eq!(decision(area, at(10.0, 160.0)), ClickThrough::PassThrough);
    }

    #[test]
    fn character_hit_area_uses_the_full_character_diameter() {
        let area = centered_area(320.0, 320.0, CHARACTER_DIAMETER);
        assert_eq!(area.right - area.left, CHARACTER_DIAMETER);
        assert_eq!(area.bottom - area.top, CHARACTER_DIAMETER);
        assert_eq!(
            area,
            Rect {
                left: 20.0,
                top: 20.0,
                right: 300.0,
                bottom: 300.0,
            }
        );
        assert_eq!(decision(area, at(160.0, 160.0)), ClickThrough::Capture);
    }

    #[test]
    fn character_hit_area_still_passes_edges_through() {
        let area = centered_area(320.0, 320.0, CHARACTER_DIAMETER);
        assert_eq!(decision(area, at(10.0, 160.0)), ClickThrough::PassThrough);
        assert_eq!(decision(area, at(300.0, 160.0)), ClickThrough::PassThrough);
        assert_eq!(decision(area, at(160.0, 10.0)), ClickThrough::PassThrough);
        assert_eq!(decision(area, at(160.0, 300.0)), ClickThrough::PassThrough);
    }
}
