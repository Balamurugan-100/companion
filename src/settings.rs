use std::ops::RangeInclusive;

/// User preferences persisted in the local store.
///
/// Kept as plain data: the store maps each field to one string key/value row.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub focus_minutes: u32,
    pub break_minutes: u32,
    pub calm_mode: bool,
    pub window_opacity: f32,
    pub breath_inhale_secs: u32,
    pub breath_hold_secs: u32,
    pub breath_exhale_secs: u32,
    pub breath_session_secs: u32,
}

pub const BREATH_INHALE_RANGE: RangeInclusive<u32> = 1..=20;
pub const BREATH_HOLD_RANGE: RangeInclusive<u32> = 0..=20;
pub const BREATH_EXHALE_RANGE: RangeInclusive<u32> = 1..=20;
pub const BREATH_SESSION_RANGE: RangeInclusive<u32> = 30..=600;

pub const MIN_OPACITY: f32 = 0.1;
pub const MAX_OPACITY: f32 = 1.0;
const DEFAULT_OPACITY: f32 = 1.0;

pub fn defaults() -> Settings {
    Settings {
        focus_minutes: 25,
        break_minutes: 5,
        calm_mode: false,
        window_opacity: DEFAULT_OPACITY,
        breath_inhale_secs: 4,
        breath_hold_secs: 4,
        breath_exhale_secs: 6,
        breath_session_secs: 120,
    }
}

/// Shared bounds for the panel sliders and the persisted values, so a hand-
/// edited database cannot feed a zero-length phase to the pacer.
pub fn clamp_breathing(settings: &mut Settings) {
    settings.breath_inhale_secs = clamp_to(settings.breath_inhale_secs, &BREATH_INHALE_RANGE);
    settings.breath_hold_secs = clamp_to(settings.breath_hold_secs, &BREATH_HOLD_RANGE);
    settings.breath_exhale_secs = clamp_to(settings.breath_exhale_secs, &BREATH_EXHALE_RANGE);
    settings.breath_session_secs = clamp_to(settings.breath_session_secs, &BREATH_SESSION_RANGE);
}

fn clamp_to(value: u32, range: &RangeInclusive<u32>) -> u32 {
    value.clamp(*range.start(), *range.end())
}

/// Opacity is a pure clamp so both the panel and the store can enforce the
/// same bounds. NaN has no meaningful position on the range, so it resets to
/// the default; infinities are handled by `f32::clamp`.
pub fn clamp_opacity(value: f32) -> f32 {
    if value.is_nan() {
        DEFAULT_OPACITY
    } else {
        value.clamp(MIN_OPACITY, MAX_OPACITY)
    }
}

impl Default for Settings {
    fn default() -> Self {
        defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_ticket_spec() {
        let settings = defaults();
        assert_eq!(settings.focus_minutes, 25);
        assert_eq!(settings.break_minutes, 5);
        assert!(!settings.calm_mode);
        assert_eq!(settings.window_opacity, 1.0);
    }

    #[test]
    fn breathing_defaults_match_the_ticket_spec() {
        let settings = defaults();
        assert_eq!(settings.breath_inhale_secs, 4);
        assert_eq!(settings.breath_hold_secs, 4);
        assert_eq!(settings.breath_exhale_secs, 6);
        assert_eq!(settings.breath_session_secs, 120);
    }

    #[test]
    fn breathing_durations_are_clamped_to_editable_ranges() {
        let mut settings = Settings {
            breath_inhale_secs: 0,
            breath_hold_secs: 999,
            breath_exhale_secs: 999,
            breath_session_secs: 1,
            ..defaults()
        };

        clamp_breathing(&mut settings);

        assert_eq!(settings.breath_inhale_secs, 1);
        assert_eq!(settings.breath_hold_secs, 20);
        assert_eq!(settings.breath_exhale_secs, 20);
        assert_eq!(settings.breath_session_secs, 30);
    }

    #[test]
    fn opacity_is_clamped_to_the_spec_range() {
        assert_eq!(clamp_opacity(0.01), 0.1);
        assert_eq!(clamp_opacity(2.5), 1.0);
    }

    #[test]
    fn opacity_inside_the_range_is_unchanged() {
        assert_eq!(clamp_opacity(0.1), 0.1);
        assert_eq!(clamp_opacity(0.5), 0.5);
        assert_eq!(clamp_opacity(1.0), 1.0);
    }

    #[test]
    fn a_non_finite_opacity_falls_back_to_the_default() {
        assert_eq!(clamp_opacity(f32::NAN), 1.0);
        assert_eq!(clamp_opacity(f32::INFINITY), 1.0);
        assert_eq!(clamp_opacity(f32::NEG_INFINITY), 0.1);
    }
}
