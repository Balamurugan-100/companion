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
                    resolution: (520, 640).into(),
                    ..default()
                },
                SettingsPanelWindow,
            ))
            .id();
        commands.spawn((
            Camera2d,
            Camera {
                clear_color: ClearColorConfig::Custom(Color::srgb(0.09, 0.09, 0.11)),
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

/// Calm mode dims this panel only: text becomes a soft blue-grey, weak text
/// fades further, and widget fills flatten. `Ui::visuals_mut` is scoped to this
/// Ui and its children, and each frame rebuilds the Ui from the global style,
/// so switching calm mode off restores the normal look with no global mutation.
fn apply_calm_visuals(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.override_text_color = Some(egui::Color32::from_rgb(178, 190, 198));
    visuals.weak_text_alpha = 0.55;
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(40, 46, 52);
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(48, 56, 64);
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
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            if current.0.calm_mode {
                apply_calm_visuals(ui);
            }
            ui.horizontal(|ui| {
                for (label, candidate) in [
                    ("Focus", PanelTab::Focus),
                    ("Breathe", PanelTab::Breathe),
                    ("Ground", PanelTab::Ground),
                    ("Mood", PanelTab::Mood),
                    ("Tasks", PanelTab::Tasks),
                    ("Notes", PanelTab::Notes),
                    ("Summary", PanelTab::Summary),
                    ("Settings", PanelTab::Settings),
                ] {
                    if ui.selectable_label(*tab == candidate, label).clicked() {
                        *tab = candidate;
                    }
                }
            });
            ui.separator();

            match *tab {
                PanelTab::Focus => {
                    ui.heading("Focus timer");
                    ui.label(format!(
                        "{} · {}",
                        focus_runtime::phase_label(runtime.phase()),
                        focus_runtime::format_remaining(runtime.remaining())
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Start").clicked() {
                            focus_commands.write(FocusCommand::Start);
                        }
                        if ui.button("Pause").clicked() {
                            focus_commands.write(FocusCommand::Pause);
                        }
                        if ui.button("Resume").clicked() {
                            focus_commands.write(FocusCommand::Resume);
                        }
                        if ui.button("Reset").clicked() {
                            focus_commands.write(FocusCommand::Reset);
                        }
                    });
                    ui.separator();
                    ui.label("Recent sessions");
                    let history = store.0.focus_sessions();
                    if history.is_empty() {
                        ui.label("No completed sessions yet.");
                    } else {
                        for session in history.into_iter().take(8) {
                            ui.label(focus_runtime::format_session(session));
                        }
                    }
                }
                PanelTab::Breathe => {
                    ui.heading("Guided breathing");
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(120.0, 120.0), egui::Sense::hover());
                    let radius = 48.0 * breath.scale();
                    let painter = ui.painter();
                    painter.circle_filled(
                        rect.center(),
                        radius,
                        egui::Color32::from_rgb(38, 88, 118),
                    );
                    painter.circle_stroke(
                        rect.center(),
                        radius + 2.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 220, 255)),
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
                    ui.label(egui::RichText::new(phase).size(18.0));
                    ui.label(format!(
                        "{} elapsed · {} left · {:.0}% of phase",
                        focus_runtime::format_remaining(breath.elapsed()),
                        focus_runtime::format_remaining(breath.remaining()),
                        breath.progress() * 100.0
                    ));
                    ui.horizontal(|ui| {
                        let start_label = if breath.status() == Status::Paused {
                            "Resume"
                        } else {
                            "Start"
                        };
                        if ui.button(start_label).clicked() {
                            breath_commands.write(BreathCommand::Start);
                        }
                        if ui.button("Pause").clicked() {
                            breath_commands.write(BreathCommand::Pause);
                        }
                        if ui.button("Stop").clicked() {
                            breath_commands.write(BreathCommand::Stop);
                        }
                    });
                }
                PanelTab::Ground => {
                    ui.heading("Grounding");
                    let step = exercise.current();
                    ui.label(format!(
                        "Step {} of {}",
                        exercise.index() + 1,
                        exercise.count()
                    ));
                    ui.label(egui::RichText::new(step.cue).size(22.0));
                    ui.label(step.detail);
                    ui.horizontal(|ui| {
                        if ui.button("Next").clicked() {
                            ground_commands.write(GroundCommand::Next);
                        }
                        if ui.button("Reset").clicked() {
                            ground_commands.write(GroundCommand::Reset);
                        }
                    });
                    if exercise.at_end() {
                        ui.label("That's the sequence. Reset to begin again.");
                    }
                }
                PanelTab::Mood => {
                    ui.heading("Mood check-in");
                    ui.horizontal(|ui| {
                        for (label, mood) in [
                            ("Good", Mood::Good),
                            ("Okay", Mood::Okay),
                            ("Stressed", Mood::Stressed),
                            ("Tired", Mood::Tired),
                        ] {
                            if ui.button(label).clicked() {
                                mood_selected.write(MoodSelected(mood));
                                *status = Some(lines::for_mood(mood).suggestion.to_owned());
                            }
                        }
                    });
                    if let Some(status) = status {
                        ui.label(status.as_str());
                    }
                }
                PanelTab::Tasks => {
                    ui.heading("Tasks");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(task_text);
                        if ui.button("Add").clicked()
                            && store
                                .0
                                .add_task(task_text, store::Store::now_millis())
                                .is_some()
                        {
                            task_text.clear();
                        }
                    });
                    ui.separator();
                    let tasks = store.0.tasks();
                    if tasks.is_empty() {
                        ui.label("No tasks yet.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(210.0)
                            .show(ui, |ui| {
                                for task in tasks {
                                    ui.horizontal(|ui| {
                                        let mut done = task.done;
                                        let checkbox = egui::Checkbox::new(&mut done, &task.text);
                                        if ui.add_enabled(!task.done, checkbox).clicked()
                                            && done
                                            && store
                                                .0
                                                .complete_task(task.id, store::Store::now_millis())
                                        {
                                            task_completed.write(bubbles::TaskCompleted);
                                        }
                                        if ui.button("Delete").clicked() {
                                            store.0.delete_task(task.id);
                                        }
                                    });
                                }
                            });
                    }
                }
                PanelTab::Notes => {
                    ui.heading("Notes");
                    let notes = store.0.notes();
                    if selected_note.is_none()
                        && let Some(first) = notes.first()
                    {
                        *selected_note = Some(first.id);
                        *note_title = first.title.clone();
                        *note_body = first.body.clone();
                    }
                    if ui.button("New").clicked()
                        && let Some(id) =
                            store
                                .0
                                .upsert_note(None, "", "", store::Store::now_millis())
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
                        ui.label("No notes yet. Press New to write one.");
                    }
                    ui.separator();
                    if let Some(id) = *selected_note {
                        ui.add(egui::TextEdit::singleline(&mut *note_title).hint_text("Title"));
                        ui.add(
                            egui::TextEdit::multiline(&mut *note_body)
                                .desired_rows(4)
                                .hint_text("Note"),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                store.0.upsert_note(
                                    Some(id),
                                    note_title.as_str(),
                                    note_body.as_str(),
                                    store::Store::now_millis(),
                                );
                            }
                            if ui.button("Delete").clicked() {
                                store.0.delete_note(id);
                                *selected_note = None;
                                note_title.clear();
                                note_body.clear();
                            }
                        });
                    }
                }
                PanelTab::Summary => {
                    ui.heading("Today");
                    let day_start_ms = store::Store::start_of_today_ms();
                    let sessions = store.0.focus_sessions_since(day_start_ms);
                    let tasks = store.0.completed_tasks_since(day_start_ms);
                    let today = summary::aggregate(&sessions, &tasks, day_start_ms);
                    ui.label(format!("Focus · {} min", today.focus_minutes));
                    ui.label(format!("Tasks done · {}", today.completed_tasks));
                    if today.focus_minutes == 0 && today.completed_tasks == 0 {
                        ui.label(lines::summary_empty());
                    }
                }
                PanelTab::Settings => {
                    ui.heading("GitHub settings sync");
                    if sync_state.config.repo_url.trim().is_empty()
                        && !sync_state.config.setup_declined
                    {
                        ui.label(
                            "First run: add a GitHub repository to sync settings, or skip to stay offline.",
                        );
                        if ui.button("Not now").clicked() {
                            sync_state.config.setup_declined = true;
                            store.0.save_sync_config(&sync_state.config);
                            *status = Some("Sync is off. Turn it on here anytime.".to_owned());
                        }
                    }

                    ui.horizontal(|ui| {
                        ui.label("Repo");
                        ui.text_edit_singleline(repo_url);
                    });
                    ui.label("When this repo changes, decide which side wins:");
                    ui.horizontal(|ui| {
                        ui.radio_value(sync_mode, sync::SyncMode::RebaseMerge, "Merge + resolve");
                        ui.radio_value(sync_mode, sync::SyncMode::PullLocal, "Remote wins");
                        ui.radio_value(sync_mode, sync::SyncMode::PushRemote, "Local wins");
                    });
                    ui.label("Automatic sync:");
                    ui.horizontal(|ui| {
                        ui.radio_value(interval_hours, 12, "Twice a day (12h)");
                        ui.radio_value(interval_hours, 24, "Once a day (24h)");
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Use repo").clicked() {
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
                            && ui.button("Sync now").clicked()
                        {
                            crate::sync_now(&mut runner, &mut sync_state);
                        }
                    });
                    if !sync_state.config.repo_url.trim().is_empty() {
                        ui.label(format!("Configured: {}", sync_state.config.repo_url));
                    }
                    if let Some(status) = &sync_state.status {
                        ui.label(status.as_str());
                    }

                    let pending_repo = sync_state.pending_repo.clone();
                    if let Some(new_repo) = pending_repo {
                        ui.separator();
                        ui.heading("Repository changed");
                        ui.label(format!("Current: {}", sync_state.config.repo_url));
                        ui.label(format!("New: {new_repo}"));
                        ui.label("Choose before anything is overwritten:");
                        ui.horizontal(|ui| {
                            if ui.button("Pull remote").clicked() {
                                crate::confirm_repo_change(
                                    &mut runner,
                                    &mut sync_state,
                                    &store.0,
                                    &current.0,
                                    sync::SyncMode::PullLocal,
                                );
                            }
                            if ui.button("Push local").clicked() {
                                crate::confirm_repo_change(
                                    &mut runner,
                                    &mut sync_state,
                                    &store.0,
                                    &current.0,
                                    sync::SyncMode::PushRemote,
                                );
                            }
                            if ui.button("Rebase + merge").clicked() {
                                crate::confirm_repo_change(
                                    &mut runner,
                                    &mut sync_state,
                                    &store.0,
                                    &current.0,
                                    sync::SyncMode::RebaseMerge,
                                );
                            }
                        });
                    }

                    if !sync_state.conflicts.is_empty() {
                        ui.separator();
                        ui.heading("Resolve settings conflicts");
                        ui.label("Pick which side wins for each setting.");
                        let conflicts = sync_state.conflicts.clone();
                        for conflict in conflicts {
                            ui.label(&conflict.label);
                            ui.label(format!(
                                "Local: {}    Remote: {}",
                                conflict.local, conflict.remote
                            ));
                            ui.horizontal(|ui| {
                                let mut choice = sync_state
                                    .choices
                                    .get(&conflict.key)
                                    .copied()
                                    .unwrap_or(sync::Side::Local);
                                if ui
                                    .radio_value(&mut choice, sync::Side::Local, "Use local")
                                    .clicked()
                                {
                                    sync_state
                                        .choices
                                        .insert(conflict.key.clone(), sync::Side::Local);
                                }
                                if ui
                                    .radio_value(&mut choice, sync::Side::Remote, "Use remote")
                                    .clicked()
                                {
                                    sync_state
                                        .choices
                                        .insert(conflict.key.clone(), sync::Side::Remote);
                                }
                            });
                        }
                        if ui.button("Apply choices and push").clicked() {
                            crate::resolve_conflicts(&mut runner, &mut sync_state);
                        }
                    }

                    ui.separator();
                    ui.heading("Timers and comfort");
                    ui.add(
                        egui::Slider::new(&mut edit.focus_minutes, 1..=240).text("Focus (minutes)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut edit.break_minutes, 1..=60).text("Break (minutes)"),
                    );
                    ui.checkbox(&mut edit.calm_mode, "Calm mode");
                    ui.add(
                        egui::Slider::new(
                            &mut edit.window_opacity,
                            settings::MIN_OPACITY..=settings::MAX_OPACITY,
                        )
                        .text("Window opacity"),
                    );
                    ui.label("Breathing pacer");
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

                    if ui.button("Save").clicked() {
                        edit.window_opacity = settings::clamp_opacity(edit.window_opacity);
                        settings::clamp_breathing(edit);
                        current.0 = edit.clone();
                        store.0.save(&current.0);
                        crate::mark_settings_saved(&store.0, &mut sync_state, &current.0);
                        *status = Some("Saved locally".to_owned());
                    }
                    if let Some(status) = status {
                        ui.label(status.as_str());
                    }
                }
            }
        });
}
