use crate::{focus, store};
use bevy::prelude::*;
use chrono::{DateTime, Local};
use std::time::Duration;

#[derive(Message)]
pub enum FocusCommand {
    Start,
    Pause,
    Resume,
    Reset,
}

#[derive(Message)]
pub struct FocusStarted;

#[derive(Message)]
pub struct FocusCompleted;

#[derive(Message)]
pub struct BreakDue;

#[derive(Resource)]
pub struct Runtime {
    state: focus::State,
    started_at_ms: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: focus::State::default(),
            started_at_ms: 0,
        }
    }
}

impl Runtime {
    pub fn phase(&self) -> focus::Phase {
        self.state.phase()
    }

    pub fn remaining(&self) -> Duration {
        self.state.remaining()
    }

    pub fn progress(&self) -> f32 {
        self.state.progress()
    }
}

fn minutes(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}

fn current_millis() -> u64 {
    let millis = Local::now().timestamp_millis();
    millis.max(0) as u64
}

/// Applies panel commands, advances the pure state machine with this frame's
/// delta, and turns its events into local store writes and typed signals.
pub fn advance_timer(
    time: Res<Time>,
    settings: Res<crate::SettingsResource>,
    store: Res<crate::AppStore>,
    mut runtime: ResMut<Runtime>,
    mut commands: MessageReader<FocusCommand>,
    mut started: MessageWriter<FocusStarted>,
    mut completed: MessageWriter<FocusCompleted>,
    mut break_due: MessageWriter<BreakDue>,
) {
    for command in commands.read() {
        match command {
            FocusCommand::Start => {
                let (state, _) = focus::transition(
                    runtime.state,
                    focus::Command::Start {
                        work: minutes(settings.0.focus_minutes),
                        break_duration: minutes(settings.0.break_minutes),
                    },
                );
                if state.phase() == focus::Phase::Working {
                    runtime.started_at_ms = current_millis();
                    started.write(FocusStarted);
                }
                runtime.state = state;
            }
            FocusCommand::Pause => {
                runtime.state = focus::transition(runtime.state, focus::Command::Pause).0;
            }
            FocusCommand::Resume => {
                runtime.state = focus::transition(runtime.state, focus::Command::Resume).0;
            }
            FocusCommand::Reset => {
                runtime.state = focus::transition(runtime.state, focus::Command::Reset).0;
            }
        }
    }

    let (state, events) = focus::transition(runtime.state, focus::Command::Tick(time.delta()));
    runtime.state = state;
    for event in events {
        if event == focus::Event::WorkCompleted {
            store.0.save_focus_session(store::FocusSession {
                started_at_ms: runtime.started_at_ms,
                planned_seconds: runtime.state.work().as_secs(),
                completed: true,
            });
            completed.write(FocusCompleted);
            break_due.write(BreakDue);
        }
    }
}

pub fn format_remaining(duration: Duration) -> String {
    let total = duration.as_secs();
    format!("{:02}:{:02}", total / 60, total % 60)
}

pub fn phase_label(phase: focus::Phase) -> &'static str {
    match phase {
        focus::Phase::Idle => "Idle",
        focus::Phase::Working => "Work",
        focus::Phase::Paused => "Paused",
        focus::Phase::Break => "Break",
    }
}

pub fn format_session(session: store::FocusSession) -> String {
    let time = DateTime::from_timestamp_millis(session.started_at_ms as i64)
        .map(|utc| utc.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_owned());
    format!(
        "{time} · {} min{}",
        session.planned_seconds / 60,
        if session.completed { " ✓" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_time_is_shown_as_mm_ss() {
        assert_eq!(format_remaining(Duration::ZERO), "00:00");
        assert_eq!(format_remaining(Duration::from_secs(9)), "00:09");
        assert_eq!(format_remaining(Duration::from_millis(59_999)), "00:59");
        assert_eq!(format_remaining(Duration::from_secs(65)), "01:05");
        assert_eq!(format_remaining(Duration::from_secs(3661)), "61:01");
    }

    #[derive(Resource, Default)]
    struct Signals {
        started: usize,
        completed: usize,
        break_due: usize,
    }

    fn record_signals(
        mut started: MessageReader<FocusStarted>,
        mut completed: MessageReader<FocusCompleted>,
        mut break_due: MessageReader<BreakDue>,
        mut signals: ResMut<Signals>,
    ) {
        signals.started += started.read().count();
        signals.completed += completed.read().count();
        signals.break_due += break_due.read().count();
    }

    #[test]
    fn completing_work_persists_a_session_and_emits_signals() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let mut app = App::new();
        app.add_message::<FocusCommand>();
        app.add_message::<FocusStarted>();
        app.add_message::<FocusCompleted>();
        app.add_message::<BreakDue>();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(crate::SettingsResource(crate::settings::Settings {
            focus_minutes: 1,
            ..crate::settings::defaults()
        }));
        app.insert_resource(crate::AppStore(store::Store::open(&dir.path().join("db"))));
        app.insert_resource(Runtime::default());
        app.init_resource::<Signals>();
        app.add_systems(Update, (advance_timer, record_signals).chain());

        app.world_mut().write_message(FocusCommand::Start);
        app.update();
        assert_eq!(app.world().resource::<Signals>().started, 1);

        for _ in 0..60 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(1));
            app.update();
        }

        let signals = app.world().resource::<Signals>();
        assert_eq!(signals.completed, 1);
        assert_eq!(signals.break_due, 1);
        assert_eq!(
            app.world().resource::<Runtime>().phase(),
            focus::Phase::Break
        );
        let sessions = app.world().resource::<crate::AppStore>().0.focus_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].planned_seconds, 60);
        assert!(sessions[0].completed);
    }

    #[test]
    fn a_sleep_wake_delta_does_not_burst_events_or_fast_forward_the_session() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let mut app = App::new();
        app.add_message::<FocusCommand>();
        app.add_message::<FocusStarted>();
        app.add_message::<FocusCompleted>();
        app.add_message::<BreakDue>();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(crate::SettingsResource(crate::settings::Settings {
            focus_minutes: 1,
            ..crate::settings::defaults()
        }));
        app.insert_resource(crate::AppStore(store::Store::open(&dir.path().join("db"))));
        app.insert_resource(Runtime::default());
        app.init_resource::<Signals>();
        app.add_systems(Update, (advance_timer, record_signals).chain());

        app.world_mut().write_message(FocusCommand::Start);
        app.update();
        assert_eq!(app.world().resource::<Signals>().started, 1);

        // One giant frame delta, as if the machine slept for eight hours.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(8 * 60 * 60));
        app.update();

        let signals = app.world().resource::<Signals>();
        assert_eq!(signals.completed, 0);
        assert_eq!(signals.break_due, 0);
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.phase(), focus::Phase::Working);
        assert_eq!(runtime.remaining(), Duration::from_secs(59));
        assert!(
            app.world()
                .resource::<crate::AppStore>()
                .0
                .focus_sessions()
                .is_empty()
        );

        // Catching up one real second at a time still completes exactly once.
        for _ in 0..59 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(1));
            app.update();
        }
        let signals = app.world().resource::<Signals>();
        assert_eq!(signals.completed, 1);
        assert_eq!(signals.break_due, 1);
        assert_eq!(
            app.world().resource::<Runtime>().phase(),
            focus::Phase::Break
        );
    }

    #[test]
    fn history_rows_show_the_planned_minutes_and_completion() {
        let row = format_session(store::FocusSession {
            started_at_ms: 1_700_000_000_000,
            planned_seconds: 1500,
            completed: true,
        });

        assert!(row.contains("25 min"));
        assert!(row.contains('✓'));
    }
}
