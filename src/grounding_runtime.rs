use crate::grounding;
use bevy::prelude::*;

#[derive(Message)]
pub enum GroundCommand {
    Next,
    Reset,
}

/// Bevy-side view of the manual grounding exercise. The authored steps and
/// their controller stay pure in `grounding`.
#[derive(Resource, Default)]
pub struct Runtime {
    exercise: grounding::Exercise,
}

impl Runtime {
    pub fn current(&self) -> &'static grounding::Step {
        self.exercise.current()
    }

    pub fn index(&self) -> usize {
        self.exercise.index()
    }

    pub fn count(&self) -> usize {
        self.exercise.count()
    }

    pub fn at_end(&self) -> bool {
        self.exercise.at_end()
    }
}

/// Applies manual Next/Reset messages from the settings panel.
pub fn advance_exercise(mut runtime: ResMut<Runtime>, mut commands: MessageReader<GroundCommand>) {
    for command in commands.read() {
        match command {
            GroundCommand::Next => runtime.exercise.next(),
            GroundCommand::Reset => runtime.exercise.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_and_reset_messages_drive_the_step_controller() {
        let mut app = App::new();
        app.add_message::<GroundCommand>();
        app.init_resource::<Runtime>();
        app.add_systems(Update, advance_exercise);

        assert_eq!(app.world().resource::<Runtime>().index(), 0);

        app.world_mut().write_message(GroundCommand::Next);
        app.update();
        assert_eq!(app.world().resource::<Runtime>().index(), 1);

        app.world_mut().write_message(GroundCommand::Next);
        app.update();
        assert_eq!(app.world().resource::<Runtime>().index(), 2);

        app.world_mut().write_message(GroundCommand::Reset);
        app.update();
        let runtime = app.world().resource::<Runtime>();
        assert_eq!(runtime.index(), 0);
        assert!(!runtime.at_end());
    }

    #[test]
    fn next_never_moves_past_the_last_authored_step() {
        let mut app = App::new();
        app.add_message::<GroundCommand>();
        app.init_resource::<Runtime>();
        app.add_systems(Update, advance_exercise);

        for _ in 0..20 {
            app.world_mut().write_message(GroundCommand::Next);
            app.update();
        }

        let runtime = app.world().resource::<Runtime>();
        assert!(runtime.at_end());
        assert_eq!(runtime.index(), runtime.count() - 1);
    }
}
