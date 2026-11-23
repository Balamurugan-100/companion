use crate::breathing_runtime::{self, BreathCommand, Status};
use crate::bubbles::{self, MoodSelected};
use crate::dialogue::Mood;
use crate::focus_runtime::{self, FocusCommand};
use crate::grounding_runtime::{self, GroundCommand};
use crate::lines;
use crate::settings::{self, Settings};
use crate::store;
use crate::summary;
use crate::sync;
use crate::ui_theme as theme;
use bevy::camera::RenderTarget;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::window::WindowRef;
use bevy_egui::{EguiContext, EguiSchedule, PrimaryEguiContext, egui};

/// Dedicated egui pass for the settings window (see bevy_egui's two_windows
/// example). UI systems must not run in `Update`.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SettingsPanelPass;

#[derive(Component)]
pub struct SettingsPanelWindow;

#[derive(Component)]
pub struct SettingsPanelCamera;

/// Spawns the normal (decorated, opaque) settings window on demand, or refocuses
/// it if it is already open. Closing it with the OS close button despawns the
/// window entity, so the next open also replaces the stale camera.
pub fn manage_window(
    mut opens: MessageReader<crate::OpenSettings>,
    mut commands: Commands,
    mut windows: Query<(Entity, &mut Window), With<SettingsPanelWindow>>,
    cameras: Query<Entity, With<SettingsPanelCamera>>,
) {
    for _ in opens.read() {
        if let Ok((_, mut window)) = windows.single_mut() {
            window.visible = true;
            window.focused = true;
            continue;
        }

        for camera in &cameras {
            commands.entity(camera).despawn();
        }
        let window_entity = commands
            .spawn((
                Window {
                    title: "Companion".into(),
                    resolution: (560, 720).into(),
                    ..default()
                },
                SettingsPanelWindow,
            ))
            .id();
        commands.spawn((
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.07, 0.09)),
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(window_entity)),
            EguiSchedule::new(SettingsPanelPass),
            SettingsPanelCamera,
        ));
    }
}

#[derive(Default)]
pub(crate) struct Draft {
    settings: Option<Settings>,
    status: Option<String>,
    task_text: String,
    note_title: String,
    note_body: String,
    selected_note: Option<u64>,
    sync_draft_loaded: bool,
    repo_url: String,
    sync_mode: sync::SyncMode,
    interval_hours: u64,
    sync_generation: u64,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelTab {
    #[default]
    Focus,
    Breathe,
    Ground,
    Mood,
    Tasks,
    Notes,
    Summary,
    Settings,
}

const TABS: [(&str, PanelTab); 8] = [
    ("Focus", PanelTab::Focus),
    ("Breathe", PanelTab::Breathe),
    ("Ground", PanelTab::Ground),
    ("Mood", PanelTab::Mood),
    ("Tasks", PanelTab::Tasks),
    ("Notes", PanelTab::Notes),
    ("Summary", PanelTab::Summary),
    ("Settings", PanelTab::Settings),
];

fn tab_bar(ui: &mut egui::Ui, tab: &mut PanelTab) {
    ui.horizontal_wrapped(|ui| {
        for (label, candidate) in TABS {
            if theme::tab_button(ui, label, *tab == candidate) {
                *tab = candidate;
            }
        }
    });
    ui.add_space(2.0);
}

/// The panel edits a local settings draft and writes to both the Bevy resource
/// and the redb store only when Save is pressed. Focus and breathing commands
/// travel to `Update` as typed messages because this UI runs in its own egui
/// pass.
pub fn ui_system(
    mut context: Single<&mut EguiContext, Without<PrimaryEguiContext>>,
    mut current: ResMut<crate::SettingsResource>,
    store: Res<crate::AppStore>,
    mut sync_state: ResMut<crate::SyncState>,
    mut runner: ResMut<crate::SyncRunner>,
    mut tab_request: ResMut<crate::SettingsTabRequest>,
    runtime: Res<focus_runtime::Runtime>,
    breath: Res<breathing_runtime::Runtime>,
    exercise: Res<grounding_runtime::Runtime>,
    mut mood_selected: MessageWriter<MoodSelected>,
    mut task_completed: MessageWriter<bubbles::TaskCompleted>,
    mut focus_commands: MessageWriter<FocusCommand>,
    mut breath_commands: MessageWriter<BreathCommand>,
    mut ground_commands: MessageWriter<GroundCommand>,
    mut draft: Local<Draft>,
    mut tab: Local<PanelTab>,
) {
    let Draft {
        settings,
        status,
        task_text,
        note_title,
        note_body,
        selected_note,
        sync_draft_loaded,
        repo_url,
        sync_mode,
        interval_hours,
        sync_generation,
    } = &mut *draft;
    let context = context.get_mut();

    if *sync_generation != sync_state.generation {
        *settings = Some(current.0.clone());
        *sync_generation = sync_state.generation;
    }
    let edit = settings.get_or_insert_with(|| current.0.clone());
    if tab_request.0 {
        *tab = PanelTab::Settings;
        tab_request.0 = false;
    }
    if !*sync_draft_loaded {
        *repo_url = sync_state.config.repo_url.clone();
        *sync_mode = sync_state.config.mode;
        *interval_hours = sync_state.config.interval_hours;
        *sync_draft_loaded = true;
    }

    egui::Window::new("Companion")
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(true)
        .collapsible(false)
        .show(context, |ui| {
            theme::apply_visuals(ui, current.0.calm_mode);
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    tab_bar(ui, &mut tab);
                    match *tab {
                        PanelTab::Focus => {
                            use crate::focus::Phase;

                            let label = focus_runtime::phase_label(runtime.phase()).to_uppercase();
                            let accent = match runtime.phase() {
                                Phase::Idle => theme::muted(),
                                Phase::Working => theme::accent(),
                                Phase::Paused => theme::warm(),
                                Phase::Break => theme::danger(),
                            };
                            theme::card(ui, |ui| {
                                theme::heading(ui, "Focus timer", "One thing at a time.");
                                ui.horizontal(|ui| {
                                    theme::chip(ui, &label, accent);
                                    ui.label(
                                        egui::RichText::new("session clock")
                                            .size(11.0)
                                            .color(theme::faint()),
                                    );
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(focus_runtime::format_remaining(
                                        runtime.remaining(),
                                    ))
                                    .size(38.0)
                                    .strong()
                                    .color(theme::text()),
                                );
                                if runtime.phase() != Phase::Idle {
                                    theme::progress(ui, runtime.progress(), accent);
                                }
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if theme::primary_button(ui, "Start") {
                                        focus_commands.write(FocusCommand::Start);
                                    }
                                    if theme::ghost_button(ui, "Pause") {
                                        focus_commands.write(FocusCommand::Pause);
                                    }
                                    if theme::ghost_button(ui, "Resume") {
                                        focus_commands.write(FocusCommand::Resume);
                                    }
                                    if theme::ghost_button(ui, "Reset") {
                                        focus_commands.write(FocusCommand::Reset);
                                    }
                                });
                            });

                            theme::card(ui, |ui| {
                                theme::heading(
                                    ui,
                                    "Recent sessions",
                                    "A quiet record of the work.",
                                );
                                let history = store.0.focus_sessions();
                                if history.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No completed sessions yet.")
                                            .color(theme::faint()),
                                    );
                                } else {
                                    for session in history.into_iter().take(8) {
                                        ui.label(
                                            egui::RichText::new(focus_runtime::format_session(
                                                session,
                                            ))
                                            .size(12.0)
                                            .color(theme::muted()),
                                        );
                                    }
                                }
                            });
                        }
                        PanelTab::Breathe => {
                            theme::card(ui, |ui| {
                                theme::heading(ui, "Guided breathing", "Follow the circle.");
                                ui.vertical_centered(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(140.0, 140.0),
                                        egui::Sense::hover(),
                                    );
                                    let center = rect.center();
                                    let radius = 56.0 * breath.scale();
                                    ui.painter().circle_filled(
                                        center,
                                        radius + 9.0,
                                        egui::Color32::from_rgba_premultiplied(
                                            theme::accent().r(),
                                            theme::accent().g(),
                                            theme::accent().b(),
                                            18,
                                        ),
                                    );
                                    ui.painter().circle_filled(
                                        center,
                                        radius,
                                        theme::accent_soft(),
                                    );
                                    ui.painter().circle_stroke(
                                        center,
                                        radius + 2.0,
                                        egui::Stroke::new(2.0, theme::accent()),
                                    );

                                    let phase = match breath.phase() {
                                        Some(phase) if breath.active() => {
                                            breathing_runtime::phase_label(phase).to_owned()
                                        }
                                        _ => "Ready when you are".to_owned(),
                                    };
                                    let phase = if breath.status() == Status::Paused {
                                        format!("{phase} · paused")
                                    } else {
                                        phase
                                    };
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new(phase).size(20.0).strong().color(
                                        if breath.active() {
                                            theme::accent()
                                        } else {
                                            theme::text()
                                        },
                                    ));
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} elapsed · {} left · {:.0}%",
                                            focus_runtime::format_remaining(breath.elapsed()),
                                            focus_runtime::format_remaining(breath.remaining()),
                                            breath.progress() * 100.0
                                        ))
                                        .size(11.0)
                                        .color(theme::muted()),
                                    );
                                });
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let start_label = if breath.status() == Status::Paused {
                                        "Resume"
                                    } else {
                                        "Start"
                                    };
                                    if theme::primary_button(ui, start_label) {
                                        breath_commands.write(BreathCommand::Start);
                                    }
                                    if theme::ghost_button(ui, "Pause") {
                                        breath_commands.write(BreathCommand::Pause);
                                    }
                                    if theme::ghost_button(ui, "Stop") {
                                        breath_commands.write(BreathCommand::Stop);
                                    }
                                });
                            });
                        }
                        PanelTab::Ground => {
                            theme::card(ui, |ui| {
                                theme::heading(
                                    ui,
                                    "Grounding",
                                    "A short reset when your head is loud.",
                                );
                                theme::chip(
                                    ui,
                                    &format!(
                                        "STEP {} / {}",
                                        exercise.index() + 1,
                                        exercise.count()
                                    ),
                                    theme::violet(),
                                );
                                ui.add_space(10.0);
                                let step = exercise.current();
                                ui.label(
                                    egui::RichText::new(step.cue)
                                        .size(24.0)
                                        .strong()
                                        .color(theme::text()),
                                );
                                ui.label(
                                    egui::RichText::new(step.detail)
                                        .size(13.0)
                                        .color(theme::muted()),
                                );
                                ui.add_space(8.0);
                                theme::progress(
                                    ui,
                                    (exercise.index() + 1) as f32 / exercise.count() as f32,
                                    theme::violet(),
                                );
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if theme::primary_button(ui, "Next") {
                                        ground_commands.write(GroundCommand::Next);
                                    }
                                    if theme::ghost_button(ui, "Reset") {
                                        ground_commands.write(GroundCommand::Reset);
                                    }
                                });
                                if exercise.at_end() {
                                    ui.label(
                                        egui::RichText::new(
                                            "That's the sequence. Reset to begin again.",
                                        )
                                        .size(11.0)
                                        .color(theme::faint()),
                                    );
                                }
                            });
                        }
                        PanelTab::Mood => {
                            theme::card(ui, |ui| {
                                theme::heading(
                                    ui,
                                    "Mood check-in",
                                    "Name it, then choose a next step.",
                                );
                                ui.horizontal_wrapped(|ui| {
                                    for (label, mood) in [
                                        ("Good", Mood::Good),
                                        ("Okay", Mood::Okay),
                                        ("Stressed", Mood::Stressed),
                                        ("Tired", Mood::Tired),
                                    ] {
                                        if theme::ghost_button(ui, label) {
                                            mood_selected.write(MoodSelected(mood));
                                            *status =
                                                Some(lines::for_mood(mood).suggestion.to_owned());
                                        }
                                    }
                                });
                            });
                            if let Some(status) = status {
                                theme::card(ui, |ui| {
                                    theme::heading(ui, "Suggested next step", "");
                                    ui.label(
                                        egui::RichText::new(status.as_str())
                                            .size(13.0)
                                            .color(theme::accent()),
                                    );
                                });
                            }
                        }
                        PanelTab::Tasks => {
                            theme::card(ui, |ui| {
                                theme::heading(ui, "Tasks", "Capture it, then let it go.");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(task_text)
                                            .hint_text("What needs doing?")
                                            .desired_width(340.0),
                                    );
                                    if theme::primary_button(ui, "Add")
                                        && store
                                            .0
                                            .add_task(task_text, store::Store::now_millis())
                                            .is_some()
                                    {
                                        task_text.clear();
                                    }
                                });
                            });
                            theme::card(ui, |ui| {
                                let tasks = store.0.tasks();
                                if tasks.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No tasks yet.").color(theme::faint()),
                                    );
                                } else {
                                    egui::ScrollArea::vertical()
                                        .max_height(300.0)
                                        .show(ui, |ui| {
                                            for task in tasks {
                                                ui.horizontal(|ui| {
                                                    let mut done = task.done;
                                                    let checkbox =
                                                        egui::Checkbox::new(&mut done, &task.text);
                                                    if ui
                                                        .add_enabled(!task.done, checkbox)
                                                        .clicked()
                                                        && done
                                                        && store.0.complete_task(
                                                            task.id,
                                                            store::Store::now_millis(),
                                                        )
                                                    {
                                                        task_completed
                                                            .write(bubbles::TaskCompleted);
                                                    }
                                                    if theme::ghost_button(ui, "×") {
                                                        store.0.delete_task(task.id);
                                                    }
                                                });
                                            }
                                        });
                                }
                            });
                        }
                        PanelTab::Notes => {
                            let notes = store.0.notes();
                            if selected_note.is_none()
                                && let Some(first) = notes.first()
                            {
                                *selected_note = Some(first.id);
                                *note_title = first.title.clone();
                                *note_body = first.body.clone();
                            }
                            theme::card(ui, |ui| {
                                theme::heading(ui, "Notes", "Small thoughts, kept locally.");
                                if theme::primary_button(ui, "New")
                                    && let Some(id) = store.0.upsert_note(
                                        None,
                                        "",
                                        "",
                                        store::Store::now_millis(),
                                    )
                                {
                                    *selected_note = Some(id);
                                    note_title.clear();
                                    note_body.clear();
                                }
                                egui::ScrollArea::vertical()
                                    .max_height(110.0)
                                    .show(ui, |ui| {
                                        for note in &notes {
                                            if ui
                                                .selectable_label(
                                                    *selected_note == Some(note.id),
                                                    note.display_title(),
                                                )
                                                .clicked()
                                            {
                                                *selected_note = Some(note.id);
                                                *note_title = note.title.clone();
                                                *note_body = note.body.clone();
                                            }
                                        }
                                    });
                                if notes.is_empty() {
                                    ui.label(
                                        egui::RichText::new(
                                            "No notes yet. Press New to write one.",
                                        )
                                        .color(theme::faint()),
                                    );
                                }
                            });
                            if let Some(id) = *selected_note {
                                theme::card(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut *note_title)
                                            .hint_text("Title")
                                            .desired_width(440.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::multiline(&mut *note_body)
                                            .desired_rows(5)
                                            .hint_text("Write anything…")
                                            .desired_width(440.0),
                                    );
                                    ui.horizontal(|ui| {
                                        if theme::primary_button(ui, "Save") {
                                            store.0.upsert_note(
                                                Some(id),
                                                note_title.as_str(),
                                                note_body.as_str(),
                                                store::Store::now_millis(),
                                            );
                                        }
                                        if theme::danger_button(ui, "Delete") {
                                            store.0.delete_note(id);
                                            *selected_note = None;
                                            note_title.clear();
                                            note_body.clear();
                                        }
                                    });
                                });
                            }
                        }
                        PanelTab::Summary => {
                            let day_start_ms = store::Store::start_of_today_ms();
                            let sessions = store.0.focus_sessions_since(day_start_ms);
                            let tasks = store.0.completed_tasks_since(day_start_ms);
                            let today = summary::aggregate(&sessions, &tasks, day_start_ms);

                            theme::card(ui, |ui| {
                                theme::heading(ui, "Today", "A gentle scoreboard.");
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(today.focus_minutes.to_string())
                                            .size(34.0)
                                            .strong()
                                            .color(theme::accent()),
                                    );
                                    ui.label(
                                        egui::RichText::new("min focused")
                                            .size(12.0)
                                            .color(theme::muted()),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(today.completed_tasks.to_string())
                                            .size(34.0)
                                            .strong()
                                            .color(theme::violet()),
                                    );
                                    ui.label(
                                        egui::RichText::new("tasks done")
                                            .size(12.0)
                                            .color(theme::muted()),
                                    );
                                });
                                if today.focus_minutes == 0 && today.completed_tasks == 0 {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(lines::summary_empty())
                                            .size(12.0)
                                            .color(theme::faint()),
                                    );
                                }
                            });
                        }
                        PanelTab::Settings => {
                            let pending_repo = sync_state.pending_repo.clone();
                            let conflicts = sync_state.conflicts.clone();

                            theme::card(ui, |ui| {
                                theme::heading(
                                    ui,
                                    "GitHub settings sync",
                                    "Off by default. Your repo, your credentials.",
                                );
                                if sync_state.config.repo_url.trim().is_empty() {
                                    theme::chip(ui, "OFFLINE", theme::muted());
                                    if !sync_state.config.setup_declined {
                                        ui.label(
                                            egui::RichText::new(
                                                "Add a repo to sync, or skip to stay offline.",
                                            )
                                            .size(12.0)
                                            .color(theme::muted()),
                                        );
                                        if theme::ghost_button(ui, "Not now") {
                                            sync_state.config.setup_declined = true;
                                            store.0.save_sync_config(&sync_state.config);
                                            *status = Some(
                                                "Sync is off. Turn it on here anytime.".to_owned(),
                                            );
                                        }
                                    }
                                } else {
                                    theme::chip(ui, "LINKED", theme::accent());
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Repository: {}",
                                            sync_state.config.repo_url
                                        ))
                                        .size(11.0)
                                        .color(theme::faint()),
                                    );
                                }
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(repo_url)
                                            .hint_text("https://github.com/you/repo.git")
                                            .desired_width(300.0),
                                    );
                                    if theme::primary_button(ui, "Use repo") {
                                        crate::apply_repo_change(
                                            &mut runner,
                                            &mut sync_state,
                                            &store.0,
                                            &current.0,
                                            repo_url,
                                            *sync_mode,
                                            *interval_hours,
                                        );
                                    }
                                    if !sync_state.config.repo_url.trim().is_empty()
                                        && theme::ghost_button(ui, "Sync now")
                                    {
                                        crate::sync_now(&mut runner, &mut sync_state);
                                    }
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(
                                        "If this repo changes, decide which side wins:",
                                    )
                                    .size(11.0)
                                    .color(theme::muted()),
                                );
                                ui.horizontal_wrapped(|ui| {
                                    ui.radio_value(
                                        sync_mode,
                                        sync::SyncMode::RebaseMerge,
                                        "Merge + resolve",
                                    );
                                    ui.radio_value(
                                        sync_mode,
                                        sync::SyncMode::PullLocal,
                                        "Remote wins",
                                    );
                                    ui.radio_value(
                                        sync_mode,
                                        sync::SyncMode::PushRemote,
                                        "Local wins",
                                    );
                                });
                                ui.label(
                                    egui::RichText::new("Automatic sync")
                                        .size(11.0)
                                        .color(theme::muted()),
                                );
                                ui.horizontal(|ui| {
                                    ui.radio_value(interval_hours, 12, "Twice a day (12h)");
                                    ui.radio_value(interval_hours, 24, "Once a day (24h)");
                                });
                                if let Some(status) = &sync_state.status {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(status.as_str())
                                            .size(11.0)
                                            .color(theme::accent()),
                                    );
                                }
                            });

                            if let Some(new_repo) = pending_repo {
                                theme::card(ui, |ui| {
                                    theme::heading(
                                        ui,
                                        "Repository changed",
                                        "Nothing is overwritten until you choose.",
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{}  →  {}",
                                            sync_state.config.repo_url, new_repo
                                        ))
                                        .size(11.0)
                                        .color(theme::warm()),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        if theme::ghost_button(ui, "Pull remote") {
                                            crate::confirm_repo_change(
                                                &mut runner,
                                                &mut sync_state,
                                                &store.0,
                                                &current.0,
                                                sync::SyncMode::PullLocal,
                                            );
                                        }
                                        if theme::ghost_button(ui, "Push local") {
                                            crate::confirm_repo_change(
                                                &mut runner,
                                                &mut sync_state,
                                                &store.0,
                                                &current.0,
                                                sync::SyncMode::PushRemote,
                                            );
                                        }
                                        if theme::primary_button(ui, "Rebase + merge") {
                                            crate::confirm_repo_change(
                                                &mut runner,
                                                &mut sync_state,
                                                &store.0,
                                                &current.0,
                                                sync::SyncMode::RebaseMerge,
                                            );
                                        }
                                    });
                                });
                            }

                            if !conflicts.is_empty() {
                                theme::card(ui, |ui| {
                                    theme::heading(
                                        ui,
                                        "Resolve conflicts",
                                        "Pick the winner for each field.",
                                    );
                                    for conflict in conflicts {
                                        ui.label(
                                            egui::RichText::new(&conflict.label)
                                                .strong()
                                                .color(theme::text()),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "local {}   ·   remote {}",
                                                conflict.local, conflict.remote
                                            ))
                                            .size(11.0)
                                            .color(theme::muted()),
                                        );
                                        ui.horizontal(|ui| {
                                            let mut choice = sync_state
                                                .choices
                                                .get(&conflict.key)
                                                .copied()
                                                .unwrap_or(sync::Side::Local);
                                            if ui
                                                .radio_value(
                                                    &mut choice,
                                                    sync::Side::Local,
                                                    "Use local",
                                                )
                                                .clicked()
                                            {
                                                sync_state.choices.insert(
                                                    conflict.key.clone(),
                                                    sync::Side::Local,
                                                );
                                            }
                                            if ui
                                                .radio_value(
                                                    &mut choice,
                                                    sync::Side::Remote,
                                                    "Use remote",
                                                )
                                                .clicked()
                                            {
                                                sync_state.choices.insert(
                                                    conflict.key.clone(),
                                                    sync::Side::Remote,
                                                );
                                            }
                                        });
                                    }
                                    if theme::primary_button(ui, "Apply choices and push") {
                                        crate::resolve_conflicts(&mut runner, &mut sync_state);
                                    }
                                });
                            }

                            theme::card(ui, |ui| {
                                theme::heading(ui, "Focus & comfort", "Timers that suit your day.");
                                ui.add(
                                    egui::Slider::new(&mut edit.focus_minutes, 1..=240)
                                        .text("Focus (minutes)"),
                                );
                                ui.add(
                                    egui::Slider::new(&mut edit.break_minutes, 1..=60)
                                        .text("Break (minutes)"),
                                );
                                ui.checkbox(&mut edit.calm_mode, "Calm mode");
                                ui.add(
                                    egui::Slider::new(
                                        &mut edit.window_opacity,
                                        settings::MIN_OPACITY..=settings::MAX_OPACITY,
                                    )
                                    .text("Window opacity"),
                                );
                            });

                            theme::card(ui, |ui| {
                                theme::heading(ui, "Breathing pacer", "Set your own rhythm.");
                                ui.add(
                                    egui::Slider::new(
                                        &mut edit.breath_inhale_secs,
                                        settings::BREATH_INHALE_RANGE.clone(),
                                    )
                                    .text("Inhale (secs)"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut edit.breath_hold_secs,
                                        settings::BREATH_HOLD_RANGE.clone(),
                                    )
                                    .text("Hold (secs)"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut edit.breath_exhale_secs,
                                        settings::BREATH_EXHALE_RANGE.clone(),
                                    )
                                    .text("Exhale (secs)"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut edit.breath_session_secs,
                                        settings::BREATH_SESSION_RANGE.clone(),
                                    )
                                    .text("Session (secs)"),
                                );
                            });

                            ui.horizontal(|ui| {
                                if theme::primary_button(ui, "Save") {
                                    edit.window_opacity =
                                        settings::clamp_opacity(edit.window_opacity);
                                    settings::clamp_breathing(edit);
                                    current.0 = edit.clone();
                                    store.0.save(&current.0);
                                    crate::mark_settings_saved(
                                        &store.0,
                                        &mut sync_state,
                                        &current.0,
                                    );
                                    *status = Some("Saved locally".to_owned());
                                }
                                if let Some(status) = status {
                                    ui.label(
                                        egui::RichText::new(status.as_str())
                                            .size(11.0)
                                            .color(theme::muted()),
                                    );
                                }
                            });
                        }
                    }
                });
        });
}
