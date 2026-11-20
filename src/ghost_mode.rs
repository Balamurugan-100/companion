use bevy::prelude::Resource;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostMode {
    Ghost,
    Touchable,
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct GhostState {
    pub mode: GhostMode,
    touchable_until: Option<Instant>,
}

impl Default for GhostState {
    fn default() -> Self {
        Self {
            mode: GhostMode::Ghost,
            touchable_until: None,
        }
    }
}

impl GhostState {
    pub fn toggle(&mut self, touch_hold: Duration) {
        self.mode = match self.mode {
            GhostMode::Ghost => {
                self.touchable_until = Some(Instant::now() + touch_hold);
                GhostMode::Touchable
            }
            GhostMode::Touchable => {
                self.touchable_until = None;
                GhostMode::Ghost
            }
        };
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if self.mode == GhostMode::Touchable
            && self.touchable_until.is_some_and(|deadline| now >= deadline)
        {
            self.mode = GhostMode::Ghost;
            self.touchable_until = None;
            return true;
        }
        false
    }

    pub fn force_ghost(&mut self) {
        self.mode = GhostMode::Ghost;
        self.touchable_until = None;
    }

    pub fn is_ghost(self) -> bool {
        self.mode == GhostMode::Ghost
    }
}

pub const TOUCH_HOLD: Duration = Duration::from_secs(10);

/// Ghost mode changes hit-testing, not colour. The saved opacity is the only
/// transparency control, so the companion stays fully coloured by default.
pub fn window_alpha(settings_opacity: f32) -> f32 {
    settings_opacity.clamp(0.1, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_as_ghost() {
        assert!(GhostState::default().is_ghost());
    }

    #[test]
    fn toggle_enters_touchable_and_expires_back_to_ghost() {
        let mut state = GhostState::default();
        let start = Instant::now();
        state.toggle(Duration::from_secs(1));

        assert_eq!(state.mode, GhostMode::Touchable);
        assert!(!state.tick(start));
        assert!(state.tick(start + Duration::from_secs(2)));
        assert!(state.is_ghost());
    }

    #[test]
    fn toggling_touchable_off_clears_expiry() {
        let mut state = GhostState::default();
        state.toggle(Duration::from_secs(1));
        state.toggle(Duration::from_secs(1));

        assert!(state.is_ghost());
        assert!(!state.tick(Instant::now() + Duration::from_secs(10)));
    }

    #[test]
    fn ghost_mode_keeps_the_saved_full_colour_opacity() {
        assert_eq!(window_alpha(1.0), 1.0);
        assert_eq!(window_alpha(0.42), 0.42);
    }
}
