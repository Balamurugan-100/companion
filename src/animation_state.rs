use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Walk,
    Happy,
    Blink,
    Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    WanderTick,
    TargetReached,
    Cheer,
    HappyFinished,
    BlinkTick,
    BlinkFinished,
    RestTick,
    RestFinished,
}

pub fn transition(state: State, event: Event) -> State {
    match (state, event) {
        (State::Idle, Event::WanderTick) => State::Walk,
        (State::Walk, Event::TargetReached) => State::Idle,
        (State::Idle, Event::BlinkTick) => State::Blink,
        (State::Blink, Event::BlinkFinished) => State::Idle,
        (_, Event::Cheer) => State::Happy,
        (State::Happy, Event::HappyFinished) => State::Idle,
        (_, Event::RestTick) => State::Rest,
        (State::Rest, Event::RestFinished) => State::Idle,
        _ => state,
    }
}

/// Calm mode runs character animation at three fifths (0.6x) of normal speed.
/// Integer arithmetic keeps sub-second timers exact and free of float drift.
const CALM_NUMERATOR: u32 = 3;
const CALM_DENOMINATOR: u32 = 5;

pub fn animation_delta(delta: Duration, calm: bool) -> Duration {
    if calm {
        delta * CALM_NUMERATOR / CALM_DENOMINATOR
    } else {
        delta
    }
}

/// Gentle, held squash used while the companion is resting.
pub fn rest_vertical_scale() -> f32 {
    0.9
}

/// Vertical scale of the blink squash/stretch pulse, from 1.0 (open) down to
/// 0.82 (closed) and back to 1.0 as `progress` goes from 0.0 to 1.0.
pub fn blink_vertical_scale(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    if p < 0.5 {
        1.0 - 0.18 * (2.0 * p)
    } else {
        0.82 + 0.18 * (2.0 * p - 1.0)
    }
}

/// Vertical scale is the product of three independent effects: the blink
/// squash pulse, the resting squash, and the breathing pacer (1.0 when idle).
/// Multiplication preserves each effect's shape instead of one replacing
/// another; the pacer stays visible even while the character blinks or rests.
pub fn composed_vertical_scale(blink_progress: f32, resting: bool, breath_scale: f32) -> f32 {
    let rest = if resting { rest_vertical_scale() } else { 1.0 };
    blink_vertical_scale(blink_progress) * rest * breath_scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_walks_when_the_wander_timer_fires() {
        assert_eq!(transition(State::Idle, Event::WanderTick), State::Walk);
    }

    #[test]
    fn walk_returns_to_idle_when_the_target_is_reached() {
        assert_eq!(transition(State::Walk, Event::TargetReached), State::Idle);
    }

    #[test]
    fn blink_interrupts_idle() {
        assert_eq!(transition(State::Idle, Event::BlinkTick), State::Blink);
    }

    #[test]
    fn blink_returns_to_idle_when_finished() {
        assert_eq!(transition(State::Blink, Event::BlinkFinished), State::Idle);
    }

    #[test]
    fn blink_is_not_interrupted_by_a_wander_tick() {
        assert_eq!(transition(State::Blink, Event::WanderTick), State::Blink);
    }

    #[test]
    fn blinking_does_not_interrupt_walking() {
        assert_eq!(transition(State::Walk, Event::BlinkTick), State::Walk);
    }

    #[test]
    fn a_cheer_interrupts_any_state() {
        assert_eq!(transition(State::Idle, Event::Cheer), State::Happy);
        assert_eq!(transition(State::Walk, Event::Cheer), State::Happy);
        assert_eq!(transition(State::Blink, Event::Cheer), State::Happy);
    }

    #[test]
    fn happy_returns_to_idle_when_finished() {
        assert_eq!(transition(State::Happy, Event::HappyFinished), State::Idle);
    }

    #[test]
    fn a_rest_tick_settles_the_character_from_any_state() {
        for state in [State::Idle, State::Walk, State::Happy, State::Blink] {
            assert_eq!(transition(state, Event::RestTick), State::Rest);
        }
    }

    #[test]
    fn rest_returns_to_idle_when_finished() {
        assert_eq!(transition(State::Rest, Event::RestFinished), State::Idle);
    }

    #[test]
    fn rest_ignores_wander_and_blink_ticks() {
        assert_eq!(transition(State::Rest, Event::WanderTick), State::Rest);
        assert_eq!(transition(State::Rest, Event::BlinkTick), State::Rest);
    }

    #[test]
    fn a_cheer_wakes_a_resting_character() {
        assert_eq!(transition(State::Rest, Event::Cheer), State::Happy);
    }

    #[test]
    fn the_rest_pose_is_a_visible_squash() {
        let scale = rest_vertical_scale();
        assert!(scale < 1.0);
        assert!(scale > 0.8);
    }

    #[test]
    fn blink_pulse_is_flat_at_both_ends_and_squashed_in_the_middle() {
        assert_eq!(blink_vertical_scale(0.0), 1.0);
        assert_eq!(blink_vertical_scale(1.0), 1.0);
        assert!(blink_vertical_scale(0.5) < 0.9);
    }

    #[test]
    fn composing_scales_multiplies_blink_rest_and_breath_effects() {
        assert_eq!(composed_vertical_scale(0.0, false, 1.0), 1.0);
        assert_eq!(
            composed_vertical_scale(0.0, true, 1.0),
            rest_vertical_scale()
        );
        assert_eq!(
            composed_vertical_scale(0.5, false, 1.0),
            blink_vertical_scale(0.5)
        );
        assert_eq!(composed_vertical_scale(0.0, false, 1.08), 1.08);
        assert!((composed_vertical_scale(0.0, true, 1.08) - 0.972).abs() < 1e-6);
    }

    #[test]
    fn normal_animation_delta_is_untouched() {
        let delta = Duration::from_millis(750);

        assert_eq!(animation_delta(delta, false), delta);
        assert_eq!(animation_delta(Duration::ZERO, false), Duration::ZERO);
    }

    #[test]
    fn calm_animation_delta_runs_at_three_fifths_speed() {
        assert_eq!(
            animation_delta(Duration::from_secs(10), true),
            Duration::from_secs(6)
        );
        assert_eq!(
            animation_delta(Duration::from_secs(1), true),
            Duration::from_millis(600)
        );
        assert_eq!(animation_delta(Duration::ZERO, true), Duration::ZERO);
    }

    #[test]
    fn breathing_makes_the_character_taller_on_inhale_than_on_exhale() {
        use crate::breathing::{Phase, vertical_scale};

        let inhale = composed_vertical_scale(0.0, false, vertical_scale(Phase::Inhale, 1.0));
        let exhale = composed_vertical_scale(0.0, false, vertical_scale(Phase::Exhale, 1.0));

        assert!(inhale > exhale);
        assert!((inhale - 1.08).abs() < 1e-6);
        assert!((exhale - 1.0).abs() < 1e-6);
    }
}
