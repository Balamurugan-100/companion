/// Ghost mode changes AeroSpace visibility and hit-testing, not colour. The
/// saved opacity is the only transparency control, so the companion stays fully
/// coloured by default.
pub fn window_alpha(settings_opacity: f32) -> f32 {
    settings_opacity.clamp(0.1, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_mode_keeps_the_saved_full_colour_opacity() {
        assert_eq!(window_alpha(1.0), 1.0);
        assert_eq!(window_alpha(0.42), 0.42);
    }
}
