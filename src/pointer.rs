/// Horizontal yaw (radians) for a pointer offset from the character centre.
///
/// The offset is clamped to the window half-width, so the character turns at
/// most `max_yaw` radians toward either side of the window.
pub fn yaw_from_pointer_offset(offset_x: f32, half_width: f32, max_yaw: f32) -> f32 {
    if half_width <= 0.0 {
        return 0.0;
    }
    (offset_x / half_width).clamp(-1.0, 1.0) * max_yaw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_at_the_centre_faces_forward() {
        assert_eq!(yaw_from_pointer_offset(0.0, 160.0, 0.8), 0.0);
    }

    #[test]
    fn pointer_to_the_right_turns_right() {
        let yaw = yaw_from_pointer_offset(80.0, 160.0, 0.8);
        assert!(yaw > 0.0);
        assert!(yaw < 0.8);
    }

    #[test]
    fn pointer_to_the_left_turns_left() {
        let yaw = yaw_from_pointer_offset(-80.0, 160.0, 0.8);
        assert!(yaw < 0.0);
        assert!(yaw > -0.8);
    }

    #[test]
    fn pointer_outside_the_window_is_clamped_to_the_maximum_yaw() {
        assert_eq!(yaw_from_pointer_offset(500.0, 160.0, 0.8), 0.8);
        assert_eq!(yaw_from_pointer_offset(-500.0, 160.0, 0.8), -0.8);
    }

    #[test]
    fn a_degenerate_window_does_not_produce_a_yaw() {
        assert_eq!(yaw_from_pointer_offset(10.0, 0.0, 0.8), 0.0);
    }
}
