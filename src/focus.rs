use std::time::Duration;

pub const MAX_TICK: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Working,
    Paused,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start {
        work: Duration,
        break_duration: Duration,
    },
    Pause,
    Resume,
    Reset,
    Tick(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    WorkCompleted,
    BreakCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    phase: Phase,
    remaining: Duration,
    work: Duration,
    break_duration: Duration,
}

impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            remaining: Duration::ZERO,
            work: Duration::ZERO,
            break_duration: Duration::ZERO,
        }
    }
}

impl State {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn remaining(&self) -> Duration {
        self.remaining
    }

    pub fn work(&self) -> Duration {
        self.work
    }

    /// Total length of the currently visible phase, for the progress HUD.
    pub fn duration(&self) -> Duration {
        match self.phase {
            Phase::Working | Phase::Paused => self.work,
            Phase::Break => self.break_duration,
            Phase::Idle => Duration::ZERO,
        }
    }

    /// Elapsed fraction of the current phase, clamped to `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        let total = self.duration();
        if total.is_zero() {
            return 0.0;
        }
        (1.0 - self.remaining.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    }
}

pub fn transition(state: State, command: Command) -> (State, Vec<Event>) {
    match command {
        Command::Start {
            work,
            break_duration,
        } if !work.is_zero() && !break_duration.is_zero() => (
            State {
                phase: Phase::Working,
                remaining: work,
                work,
                break_duration,
            },
            Vec::new(),
        ),
        Command::Pause if state.phase == Phase::Working => (
            State {
                phase: Phase::Paused,
                ..state
            },
            Vec::new(),
        ),
        Command::Resume if state.phase == Phase::Paused => (
            State {
                phase: Phase::Working,
                ..state
            },
            Vec::new(),
        ),
        Command::Reset => (State::default(), Vec::new()),
        Command::Tick(delta) if state.phase == Phase::Working => {
            let remaining = state.remaining.saturating_sub(delta.min(MAX_TICK));
            if remaining.is_zero() {
                (
                    State {
                        phase: Phase::Break,
                        remaining: state.break_duration,
                        ..state
                    },
                    vec![Event::WorkCompleted],
                )
            } else {
                (State { remaining, ..state }, Vec::new())
            }
        }
        Command::Tick(delta) if state.phase == Phase::Break => {
            let remaining = state.remaining.saturating_sub(delta.min(MAX_TICK));
            if remaining.is_zero() {
                (State::default(), vec![Event::BreakCompleted])
            } else {
                (State { remaining, ..state }, Vec::new())
            }
        }
        _ => (state, Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_begins_work_with_the_configured_durations() {
        let (state, events) = transition(
            State::default(),
            Command::Start {
                work: Duration::from_secs(90),
                break_duration: Duration::from_secs(30),
            },
        );

        assert_eq!(state.phase(), Phase::Working);
        assert_eq!(state.remaining(), Duration::from_secs(90));
        assert!(events.is_empty());
    }

    fn working() -> State {
        transition(
            State::default(),
            Command::Start {
                work: Duration::from_secs(90),
                break_duration: Duration::from_secs(30),
            },
        )
        .0
    }

    #[test]
    fn each_tick_reduces_the_remaining_time_without_losing_milliseconds() {
        let mut state = working();

        for _ in 0..3 {
            state = transition(state, Command::Tick(Duration::from_millis(250))).0;
        }

        assert_eq!(state.phase(), Phase::Working);
        assert_eq!(state.remaining(), Duration::from_millis(89_250));
    }

    #[test]
    fn the_visible_phase_drives_the_hud_progress() {
        let mut state = working();
        assert_eq!(state.duration(), Duration::from_secs(90));
        assert_eq!(state.progress(), 0.0);

        // Ticks are capped at one second to survive sleep/wake jumps.
        state = transition(state, Command::Tick(MAX_TICK)).0;
        assert!((state.progress() - 1.0 / 90.0).abs() < 1e-6);

        let break_state = transition(
            State::default(),
            Command::Start {
                work: MAX_TICK,
                break_duration: Duration::from_secs(20),
            },
        )
        .0;
        let break_state = transition(break_state, Command::Tick(MAX_TICK)).0;
        assert_eq!(break_state.duration(), Duration::from_secs(20));
        assert_eq!(break_state.progress(), 0.0);
        assert_eq!(State::default().progress(), 0.0);
    }

    #[test]
    fn pausing_freezes_the_countdown_and_resuming_continues_it() {
        let state = working();
        let (paused, events) = transition(state, Command::Pause);

        assert_eq!(paused.phase(), Phase::Paused);
        assert_eq!(paused.remaining(), Duration::from_secs(90));
        assert!(events.is_empty());

        let (still_paused, _) = transition(paused, Command::Tick(Duration::from_secs(3600)));
        assert_eq!(still_paused.phase(), Phase::Paused);
        assert_eq!(still_paused.remaining(), Duration::from_secs(90));

        let (resumed, _) = transition(still_paused, Command::Resume);
        assert_eq!(resumed.phase(), Phase::Working);
        assert_eq!(
            transition(resumed, Command::Tick(Duration::from_secs(1)))
                .0
                .remaining(),
            Duration::from_secs(89)
        );
    }

    #[test]
    fn reset_clears_the_session_and_returns_to_idle() {
        let state = transition(working(), Command::Tick(Duration::from_secs(10))).0;

        let (reset, events) = transition(state, Command::Reset);

        assert_eq!(reset, State::default());
        assert!(events.is_empty());
    }

    fn short_session() -> State {
        transition(
            State::default(),
            Command::Start {
                work: MAX_TICK,
                break_duration: Duration::from_secs(2),
            },
        )
        .0
    }

    #[test]
    fn finishing_work_emits_work_completed_and_starts_the_break() {
        let (state, events) = transition(short_session(), Command::Tick(MAX_TICK));

        assert_eq!(state.phase(), Phase::Break);
        assert_eq!(state.remaining(), Duration::from_secs(2));
        assert_eq!(events, vec![Event::WorkCompleted]);
    }

    #[test]
    fn finishing_the_break_emits_break_completed_and_returns_to_idle() {
        let mut state = short_session();
        state = transition(state, Command::Tick(MAX_TICK)).0;

        let (idle, events) = transition(
            transition(state, Command::Tick(MAX_TICK)).0,
            Command::Tick(MAX_TICK),
        );

        assert_eq!(idle.phase(), Phase::Idle);
        assert_eq!(idle.remaining(), Duration::ZERO);
        assert_eq!(events, vec![Event::BreakCompleted]);
    }

    #[test]
    fn a_long_tick_fires_at_most_one_completion() {
        let (after_wake, events) =
            transition(short_session(), Command::Tick(Duration::from_secs(3600)));

        assert_eq!(after_wake.phase(), Phase::Break);
        assert_eq!(after_wake.remaining(), Duration::from_secs(2));
        assert_eq!(events, vec![Event::WorkCompleted]);

        let (after_break, events) = transition(after_wake, Command::Tick(MAX_TICK));
        assert_eq!(after_break.phase(), Phase::Break);
        assert_eq!(after_break.remaining(), Duration::from_secs(1));
        assert!(events.is_empty());
    }
}
