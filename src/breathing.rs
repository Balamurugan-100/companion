use std::time::Duration;

/// A single tick is capped so a display sleep/wake jump (or a stalled frame)
/// cannot fast-forward the pacer through the whole session. Time missed during
/// the jump is skipped, not caught up: the session resumes from where it
/// stopped instead of firing a burst of phase changes on resume.
pub const MAX_TICK: Duration = Duration::from_secs(1);

pub const MIN_SCALE: f32 = 1.0;
pub const MAX_SCALE: f32 = 1.08;

/// Pure mapping from pacer phase and its progress to a character-friendly
/// vertical scale: 1.0 → 1.08 on inhale, held during hold, 1.08 → 1.0 on exhale.
pub fn vertical_scale(phase: Phase, progress: f32) -> f32 {
    let p = if progress.is_nan() {
        0.0
    } else {
        progress.clamp(0.0, 1.0)
    };
    let span = MAX_SCALE - MIN_SCALE;
    match phase {
        Phase::Inhale => MIN_SCALE + span * p,
        Phase::Hold => MAX_SCALE,
        Phase::Exhale => MAX_SCALE - span * p,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Inhale,
    Hold,
    Exhale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub inhale: Duration,
    pub hold: Duration,
    pub exhale: Duration,
    pub session_length: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    config: Config,
    elapsed: Duration,
    finished: bool,
}

impl Session {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            elapsed: Duration::ZERO,
            finished: false,
        }
    }

    /// Advances the session by an explicit delta capped at [`MAX_TICK`]. The
    /// caller owns the clock; the pure module never reads wall time.
    pub fn tick(&mut self, delta: Duration) {
        if self.finished {
            return;
        }
        self.elapsed = self
            .elapsed
            .saturating_add(delta.min(MAX_TICK))
            .min(self.config.session_length);
        self.finished = self.elapsed >= self.config.session_length;
    }

    pub fn phase(&self) -> Phase {
        phase_at(&self.config, self.elapsed).0
    }

    pub fn progress(&self) -> f32 {
        phase_at(&self.config, self.elapsed).1
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn scale(&self) -> f32 {
        let (phase, progress) = phase_at(&self.config, self.elapsed);
        vertical_scale(phase, progress)
    }

    pub fn remaining(&self) -> Duration {
        self.config.session_length.saturating_sub(self.elapsed)
    }
}

fn phase_at(config: &Config, elapsed: Duration) -> (Phase, f32) {
    let inhale = config.inhale.as_secs_f32();
    let hold = config.hold.as_secs_f32();
    let exhale = config.exhale.as_secs_f32();
    let cycle = inhale + hold + exhale;
    if cycle <= 0.0 {
        return (Phase::Inhale, 0.0);
    }
    let into_cycle = elapsed.as_secs_f32() % cycle;
    if into_cycle < inhale {
        (Phase::Inhale, into_cycle / inhale)
    } else if into_cycle < inhale + hold {
        (Phase::Hold, (into_cycle - inhale) / hold)
    } else {
        (Phase::Exhale, (into_cycle - inhale - hold) / exhale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            inhale: Duration::from_secs(4),
            hold: Duration::from_secs(2),
            exhale: Duration::from_secs(6),
            session_length: Duration::from_secs(20),
        }
    }

    fn advance(session: &mut Session, seconds: u64) {
        for _ in 0..seconds {
            session.tick(Duration::from_secs(1));
        }
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-6
    }

    #[test]
    fn a_new_session_starts_inhaling_at_zero_progress() {
        let session = Session::new(config());

        assert_eq!(session.phase(), Phase::Inhale);
        assert_eq!(session.progress(), 0.0);
        assert_eq!(session.elapsed(), Duration::ZERO);
        assert!(!session.finished());
    }

    #[test]
    fn ticking_advances_elapsed_and_phase_progress() {
        let mut session = Session::new(config());

        advance(&mut session, 2);

        assert_eq!(session.elapsed(), Duration::from_secs(2));
        assert_eq!(session.phase(), Phase::Inhale);
        assert!((session.progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn exact_boundaries_switch_phases_without_losing_time() {
        let mut session = Session::new(config());

        advance(&mut session, 4);
        assert_eq!(session.elapsed(), Duration::from_secs(4));
        assert_eq!(session.phase(), Phase::Hold);
        assert_eq!(session.progress(), 0.0);

        advance(&mut session, 2);
        assert_eq!(session.phase(), Phase::Exhale);
        assert_eq!(session.progress(), 0.0);

        advance(&mut session, 6);
        assert_eq!(session.elapsed(), Duration::from_secs(12));
        assert_eq!(session.phase(), Phase::Inhale);
        assert_eq!(session.progress(), 0.0);
    }

    #[test]
    fn a_sleep_wake_jump_cannot_skip_the_whole_session_in_one_tick() {
        let mut session = Session::new(config());

        session.tick(Duration::from_secs(3600));

        assert_eq!(session.elapsed(), MAX_TICK);
        assert!(!session.finished());
        assert_eq!(session.phase(), Phase::Inhale);
    }

    #[test]
    fn a_session_completes_at_its_configured_length_and_then_stays_put() {
        let mut session = Session::new(Config {
            session_length: Duration::from_secs(5),
            ..config()
        });

        advance(&mut session, 4);
        assert!(!session.finished());

        advance(&mut session, 1);
        assert!(session.finished());
        assert_eq!(session.elapsed(), Duration::from_secs(5));

        session.tick(Duration::from_secs(60));
        assert_eq!(session.elapsed(), Duration::from_secs(5));
    }

    #[test]
    fn a_zero_hold_is_skipped_instead_of_stalling_the_cycle() {
        let mut session = Session::new(Config {
            inhale: Duration::from_secs(2),
            hold: Duration::ZERO,
            exhale: Duration::from_secs(2),
            session_length: Duration::from_secs(10),
        });

        advance(&mut session, 2);
        assert_eq!(session.phase(), Phase::Exhale);
        assert_eq!(session.progress(), 0.0);

        advance(&mut session, 2);
        assert_eq!(session.phase(), Phase::Inhale);
        assert_eq!(session.progress(), 0.0);
    }

    #[test]
    fn the_scale_curve_expands_on_inhale_holds_and_contracts_on_exhale() {
        assert_eq!(vertical_scale(Phase::Inhale, 0.0), 1.0);
        assert!(close(vertical_scale(Phase::Inhale, 0.5), 1.04));
        assert!(close(vertical_scale(Phase::Inhale, 1.0), 1.08));
        assert_eq!(vertical_scale(Phase::Hold, 0.0), 1.08);
        assert_eq!(vertical_scale(Phase::Hold, 0.7), 1.08);
        assert!(close(vertical_scale(Phase::Exhale, 0.0), 1.08));
        assert!(close(vertical_scale(Phase::Exhale, 0.5), 1.04));
        assert!(close(vertical_scale(Phase::Exhale, 1.0), 1.0));
    }

    #[test]
    fn scale_progress_is_clamped_to_the_phase() {
        assert_eq!(vertical_scale(Phase::Inhale, -1.0), 1.0);
        assert!(close(vertical_scale(Phase::Inhale, 2.0), 1.08));
        assert!(close(vertical_scale(Phase::Exhale, -1.0), 1.08));
        assert_eq!(vertical_scale(Phase::Exhale, 2.0), 1.0);
    }

    #[test]
    fn a_session_reports_the_scale_for_its_current_phase() {
        let mut session = Session::new(config());

        assert_eq!(session.scale(), 1.0);
        advance(&mut session, 2);
        assert!(close(session.scale(), 1.04));
        advance(&mut session, 2);
        assert!(close(session.scale(), 1.08));
        advance(&mut session, 5);
        assert!(close(session.scale(), 1.04));
    }
}
