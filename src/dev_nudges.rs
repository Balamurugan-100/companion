//! Small authored, offline "developer brain" nudges.
//!
//! These are deterministic selections from static lines. There is no AI, no
//! random web service and no telemetry: a tiny local xorshift keeps the picks
//! varied without adding a dependency.

use crate::lines;
use bevy::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// A nudge produced by the local timer. The payload is one of the authored
/// lines in [`crate::lines::DEV_NUDGES`].
#[derive(Message)]
pub struct DevNudge(pub &'static str);

/// Deterministic picker with no immediate repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selector {
    state: u64,
    last: Option<usize>,
}

impl Selector {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
            last: None,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    /// Picks the next authored line, avoiding an immediate repeat.
    pub fn next(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let mut index = (self.next_u64() % count as u64) as usize;
        if count > 1 && Some(index) == self.last {
            index = (index + 1) % count;
        }
        self.last = Some(index);
        Some(index)
    }

    /// Four to seven minutes before the next nudge.
    pub fn interval_secs(&mut self) -> f32 {
        240.0 + (self.next_u64() % 241) as f32
    }
}

#[derive(Resource)]
pub struct DevNudgeRuntime {
    timer: Timer,
    selector: Selector,
}

impl DevNudgeRuntime {
    pub fn new(seed: u64) -> Self {
        let mut selector = Selector::new(seed);
        let first_interval = selector.interval_secs();
        Self {
            timer: Timer::from_seconds(first_interval, TimerMode::Once),
            selector,
        }
    }
}

impl Default for DevNudgeRuntime {
    fn default() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(1);
        Self::new(nanos ^ u64::from(std::process::id()))
    }
}

/// Fires at a calm, irregular pace while the companion is visible and calm
/// mode is off. Calm mode and a hidden companion pause the nudge clock.
pub fn advance(
    time: Res<Time>,
    settings: Res<crate::SettingsResource>,
    visibility: Res<crate::CompanionVisibility>,
    mut runtime: ResMut<DevNudgeRuntime>,
    mut nudges: MessageWriter<DevNudge>,
) {
    if settings.0.calm_mode || visibility.0.hidden() {
        return;
    }
    runtime.timer.tick(time.delta());
    if !runtime.timer.just_finished() {
        return;
    }

    if let Some(index) = runtime.selector.next(lines::DEV_NUDGES.len()) {
        nudges.write(DevNudge(lines::for_dev_nudge(index)));
    }
    let interval = runtime.selector.interval_secs();
    runtime.timer = Timer::from_seconds(interval, TimerMode::Once);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;
    use crate::visibility::Visibility;
    use std::time::Duration;

    #[derive(Resource, Default)]
    struct Received(Vec<&'static str>);

    fn record(mut reader: MessageReader<DevNudge>, mut received: ResMut<Received>) {
        for nudge in reader.read() {
            received.0.push(nudge.0);
        }
    }

    fn harness(seed: u64, calm: bool, hidden: bool) -> App {
        let mut app = App::new();
        app.add_message::<DevNudge>();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(crate::SettingsResource(settings::Settings {
            calm_mode: calm,
            ..settings::defaults()
        }));
        app.insert_resource(crate::CompanionVisibility(if hidden {
            Visibility::Hidden
        } else {
            Visibility::Shown
        }));
        app.insert_resource(DevNudgeRuntime::new(seed));
        app.init_resource::<Received>();
        app.add_systems(Update, (advance, record).chain());
        app
    }

    #[test]
    fn the_same_seed_picks_the_same_lines() {
        let mut first = Selector::new(42);
        let mut second = Selector::new(42);
        let count = lines::DEV_NUDGES.len();

        assert_eq!(first.next(count), second.next(count));
        assert_eq!(first.next(count), second.next(count));
    }

    #[test]
    fn a_selector_never_repeats_the_previous_line_immediately() {
        let mut selector = Selector::new(7);
        let count = lines::DEV_NUDGES.len();
        let mut previous = selector.next(count);

        for _ in 0..50 {
            let current = selector.next(count);
            assert_ne!(current, previous);
            previous = current;
        }
    }

    #[test]
    fn an_empty_line_list_produces_no_selection() {
        assert_eq!(Selector::new(1).next(0), None);
    }

    #[test]
    fn intervals_stay_in_the_authored_four_to_seven_minute_window() {
        let mut selector = Selector::new(99);
        for _ in 0..100 {
            let interval = selector.interval_secs();
            assert!((240.0..=480.0).contains(&interval));
        }
    }

    #[test]
    fn a_fired_timer_writes_one_authored_nudge() {
        let mut app = harness(11, false, false);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(480));
        app.update();

        let received = &app.world().resource::<Received>().0;
        assert_eq!(received.len(), 1);
        assert!(lines::DEV_NUDGES.contains(&received[0]));
    }

    #[test]
    fn calm_mode_and_hidden_companions_do_not_nudge() {
        for (calm, hidden) in [(true, false), (false, true), (true, true)] {
            let mut app = harness(11, calm, hidden);
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs(480));
            app.update();

            assert!(app.world().resource::<Received>().0.is_empty());
        }
    }
}
