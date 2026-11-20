use crate::cooldown::Cooldown;
use crate::dialogue::{self, CurrentBubble, Event, Mood, Reaction, TimeOfDay};
use crate::focus_runtime::{BreakDue, FocusCompleted, FocusStarted};
use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use bevy_egui::{EguiContexts, egui};
use chrono::{Local, Timelike};
use std::time::Duration;

const COOLDOWN_WINDOW: Duration = Duration::from_secs(15);
const CLOCK_CHECK_INTERVAL: f32 = 60.0;

#[derive(Message)]
pub struct MoodSelected(pub Mood);

#[derive(Message)]
pub struct TaskCompleted;

#[derive(Message)]
pub struct UserInteracted;

#[derive(Message)]
pub struct CompanionRestored;

#[derive(Message)]
pub struct ReactionRequested(pub Reaction);

#[derive(Resource, Default)]
pub struct ActiveBubble(pub Option<CurrentBubble>);

#[derive(Resource)]
pub struct DialogueRuntime {
    cooldown: Cooldown,
    idle: dialogue::IdleTracker,
    clock: Timer,
    time_of_day: Option<TimeOfDay>,
    started: bool,
    mood: Option<Mood>,
    last_cursor: Option<Vec2>,
}

impl DialogueRuntime {
    pub fn new() -> Self {
        Self {
            cooldown: Cooldown::default(),
            idle: dialogue::IdleTracker::default(),
            clock: Timer::from_seconds(CLOCK_CHECK_INTERVAL, TimerMode::Repeating),
            time_of_day: None,
            started: false,
            mood: None,
            last_cursor: None,
        }
    }

    fn note_interaction(&mut self) {
        self.idle.reset();
    }
}

fn local_hour() -> u32 {
    Local::now().hour()
}

/// Gathers this frame's typed signals, runs the pure rule engine and cooldown,
/// and hands the result to the character and the bubble overlay.
pub fn update_dialogue(
    time: Res<Time>,
    settings: Res<crate::SettingsResource>,
    mut runtime: ResMut<DialogueRuntime>,
    mut mood_selected: MessageReader<MoodSelected>,
    mut interacted: MessageReader<UserInteracted>,
    mut restored: MessageReader<CompanionRestored>,
    mut focus_started: MessageReader<FocusStarted>,
    mut focus_completed: MessageReader<FocusCompleted>,
    mut task_completed: MessageReader<TaskCompleted>,
    mut break_due: MessageReader<BreakDue>,
    mut dev_nudges: MessageReader<crate::dev_nudges::DevNudge>,
    buttons: Res<ButtonInput<MouseButton>>,
    primary: Query<(Entity, &Window), With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
    mut active: ResMut<ActiveBubble>,
    mut reactions: MessageWriter<ReactionRequested>,
) {
    let delta = time.delta();
    let mut events = Vec::new();
    let mut dev_nudge = None;

    for selected in mood_selected.read() {
        runtime.mood = Some(selected.0);
        events.push(Event::MoodAnswered);
        runtime.note_interaction();
    }
    for _ in interacted.read() {
        runtime.note_interaction();
    }
    for _ in restored.read() {
        events.push(Event::Restored);
        runtime.cooldown.reset();
    }
    for _ in focus_started.read() {
        events.push(Event::FocusStarted);
        runtime.note_interaction();
    }
    for _ in focus_completed.read() {
        events.push(Event::FocusCompleted);
        runtime.note_interaction();
    }
    for _ in task_completed.read() {
        events.push(Event::TaskCompleted);
        runtime.note_interaction();
    }
    for _ in break_due.read() {
        events.push(Event::BreakDue);
        runtime.note_interaction();
    }
    for nudge in dev_nudges.read() {
        dev_nudge = Some(nudge.0);
        events.push(Event::DevNudge);
        runtime.note_interaction();
    }
    if buttons.just_pressed(MouseButton::Left)
        || buttons.just_pressed(MouseButton::Right)
        || buttons.just_pressed(MouseButton::Middle)
    {
        runtime.note_interaction();
    }

    let cursor = primary.single().ok().and_then(|(entity, window)| {
        WINIT_WINDOWS.with_borrow(|winit_windows| {
            let wrapper = winit_windows.get_window(entity)?;
            crate::local_cursor(window, wrapper)
        })
    });
    if cursor != runtime.last_cursor {
        runtime.last_cursor = cursor;
        if cursor.is_some() {
            runtime.note_interaction();
        }
    }

    if runtime.idle.tick(delta) {
        events.push(Event::IdleThresholdReached);
    }

    runtime.clock.tick(delta);
    if !runtime.started {
        runtime.started = true;
        runtime.time_of_day = Some(TimeOfDay::from_hour(local_hour()));
        events.push(Event::Greeting);
    } else if runtime.clock.just_finished() {
        let current = TimeOfDay::from_hour(local_hour());
        if runtime.time_of_day != Some(current) {
            runtime.time_of_day = Some(current);
            events.push(Event::TimeChanged);
        }
    }

    let context = dialogue::Context {
        time_of_day: runtime.time_of_day.unwrap_or(TimeOfDay::Afternoon),
        idle: runtime.idle.elapsed(),
        mood: runtime.mood,
        calm_mode: settings.0.calm_mode,
        dev_nudge,
    };
    let Some(line) = dialogue::select(&events, context) else {
        return;
    };

    // Essential answers (mood, finished session) always speak and start a
    // fresh cooldown window so an automatic line cannot immediately follow.
    if events.iter().any(Event::bypasses_cooldown) {
        runtime.cooldown.emit(time.elapsed());
    } else if !runtime.cooldown.try_emit(time.elapsed(), COOLDOWN_WINDOW) {
        return;
    }

    active.0 = Some(CurrentBubble::new(line, dialogue::BUBBLE_LIFETIME));
    if line.reaction != Reaction::None {
        reactions.write(ReactionRequested(line.reaction));
    }
}

pub fn advance_bubble(time: Res<Time>, mut active: ResMut<ActiveBubble>) {
    let Some(bubble) = active.0.as_mut() else {
        return;
    };
    if bubble.advance(time.delta()) {
        active.0 = None;
    }
}

/// Compact, always-visible countdown. The rest phase is deliberately loud so
/// the companion keeps asking the user to step away even from a distance.
fn draw_timer(context: &egui::Context, focus: &crate::focus_runtime::Runtime) {
    use crate::focus::Phase;

    let (label, time, accent) = match focus.phase() {
        Phase::Idle => (
            "READY",
            "--:--".to_owned(),
            egui::Color32::from_rgb(150, 163, 176),
        ),
        Phase::Working => (
            "FOCUS",
            crate::focus_runtime::format_remaining(focus.remaining()),
            egui::Color32::from_rgb(116, 224, 196),
        ),
        Phase::Paused => (
            "PAUSED",
            crate::focus_runtime::format_remaining(focus.remaining()),
            egui::Color32::from_rgb(255, 205, 102),
        ),
        Phase::Break => (
            "REST NOW",
            crate::focus_runtime::format_remaining(focus.remaining()),
            egui::Color32::from_rgb(255, 142, 118),
        ),
    };

    egui::Area::new(egui::Id::new("companion.timer"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -8.0))
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(24, 26, 32, 225))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).color(accent).size(11.0).strong());
                        ui.label(
                            egui::RichText::new(time)
                                .color(egui::Color32::WHITE)
                                .size(18.0),
                        );
                    });
                    if focus.phase() != Phase::Idle {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(176.0, 4.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 2.0, egui::Color32::from_rgb(58, 64, 74));
                        let mut filled = rect;
                        filled.set_width(rect.width() * focus.progress());
                        ui.painter().rect_filled(filled, 2.0, accent);
                    }
                    if focus.phase() == Phase::Break {
                        ui.label(
                            egui::RichText::new("hydrate · unclench · look away")
                                .color(egui::Color32::from_rgb(220, 226, 232))
                                .size(10.0),
                        );
                    }
                });
        });
}

/// Draws the always-visible focus/rest HUD and the authored bubble over the
/// primary companion window. Both areas are non-interactive so they never
/// capture a click meant for the character.
pub fn draw_bubble(
    mut contexts: EguiContexts,
    active: Res<ActiveBubble>,
    focus: Res<crate::focus_runtime::Runtime>,
) -> Result {
    let context = contexts.ctx_mut()?;
    draw_timer(context, &focus);

    let Some(bubble) = active.0.as_ref() else {
        return Ok(());
    };
    egui::Area::new(egui::Id::new("companion.bubble"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 8.0))
        .interactable(false)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(30, 30, 36, 220))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(bubble.text)
                                .color(egui::Color32::WHITE)
                                .size(13.0),
                        );
                        if let Some(suggestion) = bubble.suggestion {
                            ui.label(
                                egui::RichText::new(suggestion)
                                    .color(egui::Color32::LIGHT_GRAY)
                                    .size(11.0),
                            );
                        }
                    });
                });
        });
    Ok(())
}
