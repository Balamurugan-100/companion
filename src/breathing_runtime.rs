use crate::{breathing, settings};
use bevy::prelude::*;
use std::time::Duration;

#[derive(Message)]
pub enum BreathCommand {
    Start,
    Pause,
    Stop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Running,
    Paused,
}

/// Bevy-side view of the pure pacer. All phase and timing decisions stay in
/// `breathing::Session`; this resource only applies panel commands and forwards
/// this frame's `Time` delta.
#[derive(Resource, Default)]
pub struct Runtime {
    session: Option<breathing::Session>,
    status: Status,
}

impl Runtime {
    pub fn status(&self) -> Status {
        self.status
    }

    /// A paused session is still active: the pacer holds its current shape.
    pub fn active(&self) -> bool {
        self.session().is_some()
    }

    pub fn session(&self) -> Option<&breathing::Session> {
        self.session.as_ref().filter(|session| !session.finished())
    }

    pub fn phase(&self) -> Option<breathing::Phase> {
        self.session().map(breathing::Session::phase)
    }

    pub fn progress(&self) -> f32 {
        self.session().map_or(0.0, breathing::Session::progress)
    }

    pub fn elapsed(&self) -> Duration {
        self.session()
            .map_or(Duration::ZERO, breathing::Session::elapsed)
    }

    pub fn remaining(&self) -> Duration {
        self.session()
            .map_or(Duration::ZERO, breathing::Session::remaining)
    }

    /// Neutral when no session is active, so the character needs no branch.
    pub fn scale(&self) -> f32 {
        self.session().map_or(1.0, breathing::Session::scale)
    }
}

fn config(settings: &settings::Settings) -> breathing::Config {
    breathing::Config {
        inhale: Duration::from_secs(u64::from(settings.breath_inhale_secs)),
        hold: Duration::from_secs(u64::from(settings.breath_hold_secs)),
        exhale: Duration::from_secs(u64::from(settings.breath_exhale_secs)),
        session_length: Duration::from_secs(u64::from(settings.breath_session_secs)),
    }
}

/// Applies panel commands, then advances the session with this frame's delta.
pub fn advance_session(
    time: Res<Time>,
    settings: Res<crate::SettingsResource>,
    mut runtime: ResMut<Runtime>,
    mut commands: MessageReader<BreathCommand>,
) {
    for command in commands.read() {
        match command {
            BreathCommand::Start => {
                // Start resumes a paused session; a fresh one starts at zero.
                if runtime.status != Status::Paused {
                    runtime.session = Some(breathing::Session::new(config(&settings.0)));
                }
                runtime.status = Status::Running;
            }
            BreathCommand::Pause => {
                if runtime.status == Status::Running {
                    runtime.status = Status::Paused;
                }
            }
            BreathCommand::Stop => {
                runtime.session = None;
                runtime.status = Status::Idle;
            }
        }
    }

    if runtime.status != Status::Running {
        return;
    }
    if let Some(session) = runtime.session.as_mut() {
        session.tick(time.delta());
        if session.finished() {
            runtime.status = Status::Idle;
        }
    }
}

pub fn phase_label(phase: breathing::Phase) -> &'static str {
    match phase {
        breathing::Phase::Inhale => "Breathe in",
        breathing::Phase::Hold => "Hold",
        breathing::Phase::Exhale => "Breathe out",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breathing::Phase;
    use crate::settings;
    use std::time::Duration;

    #[derive(Resource, Default)]
    struct Mirror {
        scale: f32,
        active: bool,
        phase: Option<Phase>,
    }

    fn mirror(runtime: Res<Runtime>, mut observed: ResMut<Mirror>) {
        observed.scale = runtime.scale();
        observed.active = runtime.active();
        observed.phase = runtime.phase();
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_message::<BreathCommand>();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(crate::SettingsResource(settings::Settings {
            breath_inhale_secs: 2,
            breath_hold_secs: 0,
            breath_exhale_secs: 2,
            breath_session_secs: 8,
            ..settings::defaults()
        }));
        app.insert_resource(Runtime::default());
        app.init_resource::<Mirror>();
        app.add_systems(Update, (advance_session, mirror).chain());
        app
    }

    fn step(app: &mut App, seconds: u64) {
        for _ in 0..seconds {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(1));
            app.update();
        }
    }

    #[test]
    fn commands_start_pause_resume_and_stop_the_pacer() {
        let mut app = test_app();
        app.update();
        assert!(!app.world().resource::<Runtime>().active());
        assert_eq!(app.world().resource::<Mirror>().scale, 1.0);

        app.world_mut().write_message(BreathCommand::Start);
        app.update();
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.status(), Status::Running);
        assert_eq!(runtime.phase(), Some(Phase::Inhale));
        assert_eq!(runtime.elapsed(), Duration::ZERO);
        assert!(app.world().resource::<Mirror>().active);

        step(&mut app, 2);
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.phase(), Some(Phase::Exhale));
        let expanded = runtime.scale();
        assert!(expanded > 1.0);

        app.world_mut().write_message(BreathCommand::Pause);
        app.update();
        let paused_elapsed = app.world().resource::<Runtime>().elapsed();
        step(&mut app, 3);
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.status(), Status::Paused);
        assert_eq!(runtime.elapsed(), paused_elapsed);

        app.world_mut().write_message(BreathCommand::Start);
        app.update();
        assert_eq!(app.world().resource::<Runtime>().status(), Status::Running);
        let resumed_elapsed = app.world().resource::<Runtime>().elapsed();
        step(&mut app, 1);
        assert!(app.world().resource::<Runtime>().elapsed() > resumed_elapsed);

        app.world_mut().write_message(BreathCommand::Stop);
        app.update();
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.status(), Status::Idle);
        assert!(!runtime.active());
        assert_eq!(runtime.scale(), 1.0);
        assert_eq!(app.world().resource::<Mirror>().scale, 1.0);
    }

    #[test]
    fn the_scale_exposed_to_the_character_changes_with_the_phase() {
        let mut app = test_app();
        app.world_mut().write_message(BreathCommand::Start);
        app.update();

        step(&mut app, 2);
        let expanded = app.world().resource::<Mirror>().scale;
        step(&mut app, 2);
        let contracted = app.world().resource::<Mirror>().scale;

        assert!(expanded > contracted);
        assert!(expanded > 1.0);
        assert!((contracted - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_sleep_wake_delta_advances_at_most_one_capped_tick() {
        let mut app = test_app();
        app.world_mut().write_message(BreathCommand::Start);
        app.update();

        // One eight-hour frame delta must not wrap the pacer through cycles.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(8 * 60 * 60));
        app.update();

        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.status(), Status::Running);
        assert_eq!(runtime.elapsed(), breathing::MAX_TICK);
        assert_eq!(runtime.phase(), Some(Phase::Inhale));
        assert!(runtime.scale() > 1.0);
        assert!(runtime.scale() <= breathing::MAX_SCALE + f32::EPSILON);

        // Normal stepping resumes from where it stopped.
        step(&mut app, 2);
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.elapsed(), Duration::from_secs(3));
        assert_eq!(runtime.phase(), Some(Phase::Exhale));
        assert_eq!(runtime.status(), Status::Running);
    }

    #[test]
    fn finishing_a_session_stops_the_modulation() {
        let mut app = test_app();
        app.world_mut().write_message(BreathCommand::Start);
        app.update();

        step(&mut app, 8);

        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.status(), Status::Idle);
        assert!(!runtime.active());
        assert_eq!(runtime.scale(), 1.0);
    }
}
