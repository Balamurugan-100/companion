mod animation_state;
mod breathing;
mod breathing_runtime;
mod bubbles;
mod character;
mod click;
mod cooldown;
mod dev_nudges;
mod dialogue;
mod focus;
mod focus_runtime;
mod ghost_mode;
mod grounding;
mod grounding_runtime;
mod hit_test;
mod lines;
mod notes;
mod platform;
mod pointer;
mod settings;
mod settings_panel;
mod store;
mod summary;
mod sync;
mod tasks;
mod tray;
mod visibility;
mod window_bounds;

use bevy::app::AppExit;
use bevy::ecs::system::NonSendMarker;
use bevy::log::{info, warn};
use bevy::prelude::*;
use bevy::window::{CompositeAlphaMode, PrimaryWindow, WindowLevel, WindowPlugin, WindowPosition};
use bevy::winit::{UpdateMode, WINIT_WINDOWS, WinitSettings};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton as TrayMouseButton, MouseButtonState, TrayIconEvent};
use winit::dpi::PhysicalPosition;
use winit::window::Window as WinitWindow;

const WINDOW_SIZE: u32 = 320;
const BOUNDS_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const ALL_WORKSPACES_CHECK_INTERVAL: Duration = Duration::from_secs(2);
const IDLE_FRAME_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Resource)]
struct WindowBoundsCheck(Timer);

/// Re-applies the macOS all-Spaces flags in case AeroSpace or macOS resets
/// them while workspaces are created or switched.
#[derive(Resource)]
struct AllWorkspacesCheck(Timer);

impl Default for AllWorkspacesCheck {
    fn default() -> Self {
        let mut timer = Timer::new(ALL_WORKSPACES_CHECK_INTERVAL, TimerMode::Repeating);
        timer.set_elapsed(ALL_WORKSPACES_CHECK_INTERVAL);
        Self(timer)
    }
}

impl Default for WindowBoundsCheck {
    fn default() -> Self {
        Self(Timer::new(BOUNDS_CHECK_INTERVAL, TimerMode::Repeating))
    }
}

#[derive(Resource)]
struct WaylandFallback(bool);

#[derive(Resource)]
struct CompanionVisibility(visibility::Visibility);

/// Settings loaded from the local store at startup; the panel edits and saves it.
#[derive(Resource)]
struct SettingsResource(settings::Settings);

/// The single open redb store, shared with the settings panel.
#[derive(Resource)]
struct AppStore(store::Store);

/// Opt-in GitHub sync state. It is plain data so the settings panel can render
/// it while a worker thread performs the actual git commands.
#[derive(Resource, Default)]
struct SyncState {
    config: sync::SyncConfig,
    local: sync::Document,
    remote: sync::Document,
    base: sync::Document,
    conflicts: Vec<sync::Conflict>,
    choices: BTreeMap<String, sync::Side>,
    status: Option<String>,
    pending_repo: Option<String>,
    /// Incremented whenever a background sync changes settings, so an open
    /// panel can refresh its draft instead of saving stale values back.
    generation: u64,
}

/// A git job runs off the Bevy threads so dialogs, timers, and the character
/// never wait on the network. The receiver is behind a Mutex because Bevy
/// resources must be `Sync`.
#[derive(Resource)]
struct SyncRunner {
    sender: Sender<SyncJobResult>,
    receiver: Mutex<Receiver<SyncJobResult>>,
    busy: bool,
    last_attempt: Instant,
}

#[derive(Clone)]
enum SyncJob {
    Sync {
        repo_url: String,
        mode: sync::SyncMode,
        local: sync::Document,
        base: sync::Document,
    },
    PushResolved {
        repo_url: String,
        document: sync::Document,
    },
}

enum SyncJobResult {
    Sync {
        result: Result<sync::SyncResult, String>,
    },
    PushResolved {
        result: Result<(String, sync::Document), String>,
    },
}

/// Asks the settings panel to open on the sync section after a fresh launch.
#[derive(Resource, Default)]
pub(crate) struct SettingsTabRequest(pub(crate) bool);

#[derive(Message)]
struct OpenSettings;

#[derive(Resource, Default)]
struct PendingActions(VecDeque<Action>);

/// Tracks the current pointer gesture over the character so a click can open
/// the setup panel without turning every drag into a panel open.
#[derive(Resource, Default)]
struct CharacterClicks {
    gesture: click::ClickTracker,
    drag: Option<DragStart>,
}

/// Where the pointer and window were when a possible drag began. Both values
/// are in logical screen points, matching AppKit's coordinate space.
#[derive(Clone, Copy)]
struct DragStart {
    cursor: Vec2,
    origin: Vec2,
}

/// Pointer state for one frame, in the coordinate spaces the gesture needs.
struct PointerSample {
    local: Option<Vec2>,
    global: Option<Vec2>,
    origin: Option<Vec2>,
}

#[derive(Debug, Clone, Copy)]
enum Action {
    Show,
    Hide,
    Toggle,
    Quit,
    OpenSettings,
}

#[derive(Resource)]
struct HotKeys {
    _manager: GlobalHotKeyManager,
}

fn main() {
    // macOS (muda/tray-icon) and global-hotkey require these to be created on
    // the process main thread; Bevy 0.20 runs Startup on a worker thread.
    let menu = tray::menu();
    let tray_icon = tray_icon::TrayIconBuilder::new()
        .with_icon(tray::icon())
        .with_menu(Box::new(menu.clone()))
        .with_menu_on_left_click(false)
        .with_tooltip("Companion")
        .build()
        .expect("failed to create tray icon");
    std::mem::forget(tray_icon);
    std::mem::forget(menu);

    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC);
    let manager = GlobalHotKeyManager::new().expect("failed to create hotkey manager");
    manager
        .register(hotkey)
        .expect("failed to register global hotkey");

    let store = store::open_default_store();
    let settings = store.load();
    let sync_state = initial_sync_state(&store, &settings);
    let (sync_sender, sync_receiver) = mpsc::channel();
    let wayland = platform::is_wayland();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Companion".into(),
                resolution: (WINDOW_SIZE, WINDOW_SIZE).into(),
                // Wayland compositors commonly reject transparent, undecorated,
                // always-on-top surfaces; fall back to a normal decorated window.
                transparent: !wayland,
                decorations: wayland,
                window_level: WindowLevel::AlwaysOnTop,
                composite_alpha_mode: CompositeAlphaMode::PostMultiplied,
                movable_by_window_background: true,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::reactive(IDLE_FRAME_INTERVAL),
            unfocused_mode: UpdateMode::reactive_low_power(IDLE_FRAME_INTERVAL),
        })
        .add_message::<OpenSettings>()
        .add_message::<bubbles::MoodSelected>()
        .add_message::<bubbles::TaskCompleted>()
        .add_message::<bubbles::UserInteracted>()
        .add_message::<bubbles::CompanionRestored>()
        .add_message::<bubbles::ReactionRequested>()
        .add_message::<dev_nudges::DevNudge>()
        .add_message::<character::Cheer>()
        .add_message::<focus_runtime::FocusCommand>()
        .add_message::<focus_runtime::FocusStarted>()
        .add_message::<focus_runtime::FocusCompleted>()
        .add_message::<focus_runtime::BreakDue>()
        .add_message::<breathing_runtime::BreathCommand>()
        .add_message::<grounding_runtime::GroundCommand>()
        .add_plugins(EguiPlugin::default())
        .insert_resource(WaylandFallback(wayland))
        .insert_resource(WindowBoundsCheck::default())
        .insert_resource(AllWorkspacesCheck::default())
        .insert_resource(CompanionVisibility(visibility::Visibility::Shown))
        .insert_resource(SettingsResource(settings))
        .insert_resource(AppStore(store))
        .insert_resource(sync_state)
        .insert_resource(SyncRunner {
            sender: sync_sender,
            receiver: Mutex::new(sync_receiver),
            busy: false,
            last_attempt: Instant::now(),
        })
        .insert_resource(SettingsTabRequest::default())
        .insert_resource(PendingActions::default())
        .insert_resource(CharacterClicks::default())
        .insert_resource(bubbles::ActiveBubble::default())
        .insert_resource(bubbles::DialogueRuntime::new())
        .insert_resource(dev_nudges::DevNudgeRuntime::default())
        .insert_resource(focus_runtime::Runtime::default())
        .insert_resource(breathing_runtime::Runtime::default())
        .insert_resource(grounding_runtime::Runtime::default())
        .init_schedule(settings_panel::SettingsPanelPass)
        .add_systems(settings_panel::SettingsPanelPass, settings_panel::ui_system)
        .add_systems(EguiPrimaryContextPass, bubbles::draw_bubble)
        .insert_resource(HotKeys { _manager: manager })
        .add_systems(Startup, character::setup)
        .add_systems(Startup, initialize_ghost_policy)
        .add_systems(Startup, prompt_sync_setup)
        .add_systems(Startup, start_initial_sync)
        .add_systems(
            Update,
            character::load_when_ready.run_if(character::load_pending),
        )
        .add_systems(Update, character::handle_click_cheer)
        .add_systems(
            Update,
            (
                focus_runtime::advance_timer,
                breathing_runtime::advance_session,
                grounding_runtime::advance_exercise,
                dev_nudges::advance,
                bubbles::update_dialogue,
                bubbles::advance_bubble,
                character::advance_animation,
                character::apply_transform,
            )
                .chain(),
        )
        .add_systems(Update, handle_tray_events)
        .add_systems(Update, handle_tray_menu_events)
        .add_systems(
            Update,
            settings_panel::manage_window.after(handle_pending_actions),
        )
        .add_systems(Update, handle_hotkey_events)
        .add_systems(Update, handle_pending_actions)
        .add_systems(
            Update,
            handle_character_click.before(settings_panel::manage_window),
        )
        .add_systems(Update, keep_window_on_screen)
        .add_systems(Update, maintain_all_workspaces)
        .add_systems(Update, apply_settings_opacity)
        .add_systems(Update, update_click_through)
        .add_systems(Update, poll_sync)
        .add_systems(Update, auto_sync_if_due.after(poll_sync))
        .run();
}

/// Loads sync metadata, giving an existing database sensible defaults so no
/// migration is needed. A fresh local document is stamped from current settings
/// and becomes the first common ancestor for later merges.
fn initial_sync_state(store: &store::Store, settings: &settings::Settings) -> SyncState {
    let config = store.load_sync_config();
    let mut local = store.load_sync_document(store::SYNC_LOCAL_KEY);
    let remote = store.load_sync_document(store::SYNC_REMOTE_KEY);
    let mut base = store.load_sync_document(store::SYNC_BASE_KEY);
    let conflicts = store.load_sync_conflicts();
    let now = sync::now_millis();

    if local.fields.is_empty() {
        local = sync::document_from_settings(settings, None, now, &config.device_id);
        store.save_sync_document(store::SYNC_LOCAL_KEY, &local);
    }
    if base.fields.is_empty() {
        base = local.clone();
        store.save_sync_document(store::SYNC_BASE_KEY, &base);
    }

    let mut choices = BTreeMap::new();
    for conflict in &conflicts {
        choices.insert(conflict.key.clone(), sync::Side::Local);
    }

    SyncState {
        config,
        local,
        remote,
        base,
        conflicts,
        choices,
        status: None,
        pending_repo: None,
        generation: 0,
    }
}

fn save_sync_documents(store: &store::Store, state: &SyncState) {
    store.save_sync_document(store::SYNC_LOCAL_KEY, &state.local);
    store.save_sync_document(store::SYNC_REMOTE_KEY, &state.remote);
    store.save_sync_document(store::SYNC_BASE_KEY, &state.base);
    store.save_sync_conflicts(&state.conflicts);
}

fn sync_interval(config: &sync::SyncConfig) -> Duration {
    Duration::from_secs(config.interval_hours.clamp(1, 24) * 60 * 60)
}

fn active_sync_job(state: &SyncState) -> SyncJob {
    SyncJob::Sync {
        repo_url: state.config.repo_url.clone(),
        mode: state.config.mode,
        local: state.local.clone(),
        base: state.base.clone(),
    }
}

/// Starts a git job in the background. All network and credential handling
/// stays inside `git`; the companion UI never blocks on it.
fn request_sync_job(runner: &mut SyncRunner, state: &mut SyncState, job: SyncJob) {
    if runner.busy {
        state.status = Some("A sync is already running.".to_owned());
        return;
    }
    let Some(root) = sync::sync_root() else {
        state.status = Some("No app data directory is available for sync.".to_owned());
        return;
    };

    runner.busy = true;
    runner.last_attempt = Instant::now();
    state.status = Some(match &job {
        SyncJob::Sync { .. } => "Syncing settings with GitHub…".to_owned(),
        SyncJob::PushResolved { .. } => "Pushing resolved settings…".to_owned(),
    });

    let sender = runner.sender.clone();
    std::thread::spawn(move || {
        let result = match job.clone() {
            SyncJob::Sync {
                repo_url,
                mode,
                local,
                base,
            } => {
                let call = sync::run_sync(&repo_url, mode, &local, &base, &root);
                SyncJobResult::Sync { result: call }
            }
            SyncJob::PushResolved { repo_url, document } => {
                let call = sync::push_resolved(&repo_url, &document, &root)
                    .map(|status| (status, document.clone()));
                SyncJobResult::PushResolved { result: call }
            }
        };
        let _ = sender.send(result);
    });
}

/// First launch with no repository opens the panel on the sync section. After
/// the user declines once, the prompt never nags again unless they opt in.
fn prompt_sync_setup(
    state: Res<SyncState>,
    mut pending: ResMut<PendingActions>,
    mut tab_request: ResMut<SettingsTabRequest>,
) {
    if state.config.repo_url.trim().is_empty() && !state.config.setup_declined {
        pending.0.push_back(Action::OpenSettings);
        tab_request.0 = true;
        info!("First launch: asking whether to set up settings sync.");
    }
}

fn start_initial_sync(mut runner: ResMut<SyncRunner>, mut state: ResMut<SyncState>) {
    if state.config.repo_url.trim().is_empty() {
        return;
    }
    let elapsed =
        Duration::from_millis(sync::now_millis().saturating_sub(state.config.last_sync_ms));
    if elapsed >= sync_interval(&state.config) {
        let job = active_sync_job(&state);
        request_sync_job(&mut runner, &mut state, job);
    }
}

fn auto_sync_if_due(mut runner: ResMut<SyncRunner>, mut state: ResMut<SyncState>) {
    if runner.busy || state.config.repo_url.trim().is_empty() {
        return;
    }
    let interval = sync_interval(&state.config);
    if runner.last_attempt.elapsed() < interval {
        return;
    }
    let elapsed =
        Duration::from_millis(sync::now_millis().saturating_sub(state.config.last_sync_ms));
    if elapsed >= interval {
        let job = active_sync_job(&state);
        request_sync_job(&mut runner, &mut state, job);
    }
}

/// Applies a finished background job on the main thread and persists the new
/// metadata. This is the only place a network result can change local settings.
fn poll_sync(
    mut runner: ResMut<SyncRunner>,
    mut state: ResMut<SyncState>,
    mut settings: ResMut<SettingsResource>,
    store: Res<AppStore>,
) {
    let Ok(message) = runner
        .receiver
        .lock()
        .expect("sync receiver lock")
        .try_recv()
    else {
        return;
    };
    runner.busy = false;

    match message {
        SyncJobResult::Sync { result, .. } => match result {
            Ok(result) => {
                state.remote = result.remote_document.clone();
                state.local = result.final_document.clone();
                state.conflicts = result.conflicts.clone();
                state.choices.clear();
                let conflict_keys: Vec<String> = state
                    .conflicts
                    .iter()
                    .map(|conflict| conflict.key.clone())
                    .collect();
                for key in conflict_keys {
                    state.choices.insert(key, sync::Side::Local);
                }
                // On conflicts nothing was pushed, so the old base is still the
                // common ancestor. Otherwise the merged document is now remote.
                if state.conflicts.is_empty() {
                    state.base = state.local.clone();
                }
                settings.0 = result.settings.clone();
                store.0.save(&settings.0);
                save_sync_documents(&store.0, &state);
                state.config.last_sync_ms = sync::now_millis();
                store.0.save_sync_config(&state.config);
                state.status = Some(result.status.clone());
                state.generation = state.generation.wrapping_add(1);
                info!("Settings sync: {}", result.status);
            }
            Err(error) => {
                state.status = Some(format!("Sync failed: {error}"));
                warn!("Settings sync failed: {error}");
            }
        },
        SyncJobResult::PushResolved { result } => match result {
            Ok((status, document)) => {
                state.local = document.clone();
                state.remote = document.clone();
                state.base = document.clone();
                state.conflicts.clear();
                state.choices.clear();
                settings.0 = sync::settings_from_document(&document);
                store.0.save(&settings.0);
                save_sync_documents(&store.0, &state);
                state.config.last_sync_ms = sync::now_millis();
                store.0.save_sync_config(&state.config);
                state.status = Some(status.clone());
                state.generation = state.generation.wrapping_add(1);
                info!("Settings sync: {status}");
            }
            Err(error) => {
                state.status = Some(format!("Could not push resolved settings: {error}"));
                warn!("Could not push resolved settings: {error}");
            }
        },
    }
}

/// Records a panel save in the local sync document. Unchanged fields keep their
/// old timestamps, which is what makes later conflict detection accurate.
pub(crate) fn mark_settings_saved(
    store: &store::Store,
    state: &mut SyncState,
    settings: &settings::Settings,
) {
    state.local = sync::document_from_settings(
        settings,
        Some(&state.local),
        sync::now_millis(),
        &state.config.device_id,
    );
    store.save_sync_document(store::SYNC_LOCAL_KEY, &state.local);
}

/// Runs one sync immediately from the panel.
pub(crate) fn sync_now(runner: &mut SyncRunner, state: &mut SyncState) {
    if state.config.repo_url.trim().is_empty() {
        state.status = Some("Add a GitHub repository first.".to_owned());
        return;
    }
    let job = active_sync_job(state);
    request_sync_job(runner, state, job);
}

/// Applies a newly entered repository. An existing repository is never silently
/// replaced: the panel first asks which side should win.
pub(crate) fn apply_repo_change(
    runner: &mut SyncRunner,
    state: &mut SyncState,
    store: &store::Store,
    settings: &settings::Settings,
    repo_url: &str,
    mode: sync::SyncMode,
    interval_hours: u64,
) {
    let repo = repo_url.trim().to_owned();
    if repo.is_empty() {
        state.status = Some("Enter a GitHub repository URL first.".to_owned());
        return;
    }

    let previous = state.config.repo_url.trim().to_owned();
    if !previous.is_empty() && previous != repo {
        state.pending_repo = Some(repo);
        state.status = Some("Choose how to switch repositories.".to_owned());
        return;
    }

    let is_new = previous.is_empty();
    state.config.repo_url = repo;
    state.config.mode = mode;
    state.config.interval_hours = interval_hours.clamp(1, 24);
    state.config.setup_declined = false;
    state.pending_repo = None;
    if is_new {
        let now = sync::now_millis();
        state.local = sync::document_from_settings(settings, None, now, &state.config.device_id);
        state.remote = sync::Document::default();
        state.base = sync::Document::default();
        state.conflicts.clear();
        state.choices.clear();
        save_sync_documents(store, state);
    }
    store.save_sync_config(&state.config);

    let job = active_sync_job(state);
    request_sync_job(runner, state, job);
}

/// Completes a repository switch using the choice made in the panel.
pub(crate) fn confirm_repo_change(
    runner: &mut SyncRunner,
    state: &mut SyncState,
    store: &store::Store,
    settings: &settings::Settings,
    mode: sync::SyncMode,
) {
    let Some(repo) = state.pending_repo.take() else {
        return;
    };
    state.config.repo_url = repo;
    state.config.mode = mode;
    state.config.setup_declined = false;
    state.conflicts.clear();
    state.choices.clear();

    let now = sync::now_millis();
    state.local = sync::document_from_settings(settings, None, now, &state.config.device_id);
    state.remote = sync::Document::default();
    // An empty ancestor makes every differing field a conflict, so rebase-merge
    // genuinely asks the user which repository wins field by field.
    state.base = sync::Document::default();
    save_sync_documents(store, state);
    store.save_sync_config(&state.config);

    let job = active_sync_job(state);
    request_sync_job(runner, state, job);
}

/// Pushes the merged document after the user has picked local or remote for
/// every conflicting field.
pub(crate) fn resolve_conflicts(runner: &mut SyncRunner, state: &mut SyncState) {
    if state.conflicts.is_empty() {
        return;
    }
    let mut document = state.local.clone();
    sync::apply_choices(
        &mut document,
        &state.conflicts,
        &state.choices,
        &state.remote,
    );
    let job = SyncJob::PushResolved {
        repo_url: state.config.repo_url.clone(),
        document,
    };
    request_sync_job(runner, state, job);
}

fn handle_tray_events(
    mut pending: ResMut<PendingActions>,
    mut interacted: MessageWriter<bubbles::UserInteracted>,
) {
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if let TrayIconEvent::Click {
            button: TrayMouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            interacted.write(bubbles::UserInteracted);
            pending.0.push_back(Action::Toggle);
        }
    }
}

fn handle_tray_menu_events(
    mut pending: ResMut<PendingActions>,
    mut interacted: MessageWriter<bubbles::UserInteracted>,
) {
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if let Some(action) = tray::action_for(&event.id.0) {
            interacted.write(bubbles::UserInteracted);
            pending.0.push_back(match action {
                tray::MenuAction::Show => Action::Show,
                tray::MenuAction::Hide => Action::Hide,
                tray::MenuAction::Quit => Action::Quit,
                tray::MenuAction::Settings | tray::MenuAction::Mood => Action::OpenSettings,
            });
        }
    }
}

fn handle_hotkey_events(
    mut pending: ResMut<PendingActions>,
    mut interacted: MessageWriter<bubbles::UserInteracted>,
) {
    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if event.state == HotKeyState::Pressed {
            interacted.write(bubbles::UserInteracted);
            pending.0.push_back(Action::Toggle);
        }
    }
}

fn handle_pending_actions(
    mut pending: ResMut<PendingActions>,
    mut state: ResMut<CompanionVisibility>,
    mut primary: Query<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
    mut open_settings: MessageWriter<OpenSettings>,
    mut restored: MessageWriter<bubbles::CompanionRestored>,
) {
    while let Some(action) = pending.0.pop_front() {
        let was_hidden = state.0.hidden();
        match action {
            Action::Quit => {
                exit.write(AppExit::Success);
                return;
            }
            Action::OpenSettings => {
                open_settings.write(OpenSettings);
                continue;
            }
            Action::Show => {
                state.0 = visibility::apply(state.0, visibility::Command::Show);
            }
            Action::Hide => {
                state.0 = visibility::apply(state.0, visibility::Command::Hide);
            }
            Action::Toggle => {
                state.0 = visibility::apply(state.0, visibility::Command::Toggle);
            }
        }
        if let Ok(mut window) = primary.single_mut() {
            window.visible = !state.0.hidden();
        }
        info!(
            "Companion is now {}.",
            if state.0.hidden() { "hidden" } else { "shown" }
        );
        if was_hidden && !state.0.hidden() {
            restored.write(bubbles::CompanionRestored);
        }
    }
}

/// A plain click on the character opens the setup panel. Movement past the
/// drag threshold moves the window with the pointer instead. Keeping the drag
/// in this system means movement stays visible to the gesture tracker, so a
/// drag is never mistaken for a click.
fn handle_character_click(
    buttons: Res<ButtonInput<MouseButton>>,
    mut clicks: ResMut<CharacterClicks>,
    primary: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut open_settings: MessageWriter<OpenSettings>,
    mut interacted: MessageWriter<bubbles::UserInteracted>,
    _non_send_marker: NonSendMarker,
) {
    let Ok((entity, window)) = primary.single() else {
        return;
    };
    if !window.visible {
        clicks.gesture.cancel();
        clicks.drag = None;
        return;
    }

    let sample = WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return PointerSample {
                local: window.cursor_position(),
                global: None,
                origin: None,
            };
        };
        let scale = wrapper.scale_factor();
        let global = platform::cursor_position().map(|(x, y)| Vec2::new(x as f32, y as f32));
        let origin = wrapper.outer_position().ok().map(|position| {
            let logical = position.to_logical::<f64>(scale);
            Vec2::new(logical.x as f32, logical.y as f32)
        });
        PointerSample {
            local: local_cursor(&window, &*wrapper),
            global,
            origin,
        }
    });

    let over = over_companion(companion_area(&window), sample.local);
    let now = Instant::now();

    if buttons.just_pressed(MouseButton::Left)
        && over
        && let Some(point) = sample.local
    {
        clicks.gesture.press(
            hit_test::Point {
                x: point.x,
                y: point.y,
            },
            now,
        );
        clicks.drag = sample
            .global
            .zip(sample.origin)
            .map(|(cursor, origin)| DragStart { cursor, origin });
    }

    if buttons.pressed(MouseButton::Left)
        && let Some(point) = sample.local
    {
        clicks.gesture.move_to(hit_test::Point {
            x: point.x,
            y: point.y,
        });
    }

    if clicks.gesture.is_dragged()
        && let Some(start) = clicks.drag
        && let Some(global) = sample.global
    {
        let target = start.origin + (global - start.cursor);
        WINIT_WINDOWS.with_borrow(|winit_windows| {
            if let Some(wrapper) = winit_windows.get_window(entity) {
                let scale = wrapper.scale_factor();
                wrapper.set_outer_position(PhysicalPosition::new(
                    (target.x as f64 * scale) as i32,
                    (target.y as f64 * scale) as i32,
                ));
            }
        });
    }

    if buttons.just_released(MouseButton::Left) {
        let clicked = sample
            .local
            .map(|point| {
                clicks.gesture.release(
                    hit_test::Point {
                        x: point.x,
                        y: point.y,
                    },
                    now,
                )
            })
            .unwrap_or_default();
        clicks.drag = None;
        if !clicked {
            clicks.gesture.cancel();
        }
        if clicked {
            interacted.write(bubbles::UserInteracted);
            open_settings.write(OpenSettings);
            info!("Companion clicked; opening the setup panel.");
        }
    }
}

/// Periodically pulls a drifting/off-screen window back onto an available monitor.
///
/// Runs at most once a second and only writes a position when our pure clamp
/// says the window has no meaningful overlap with any monitor. The actual
/// winit position is authoritative so this does not fight ordinary dragging.
fn keep_window_on_screen(
    time: Res<Time>,
    mut check: ResMut<WindowBoundsCheck>,
    fallback: Res<WaylandFallback>,
    mut primary: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
) {
    if fallback.0 || !check.0.tick(time.delta()).just_finished() {
        return;
    }
    let Ok((entity, mut window)) = primary.single_mut() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return;
        };
        let winit_window: &WinitWindow = &*wrapper;

        let Ok(outer) = winit_window.outer_position() else {
            // Wayland and some compositors do not expose this; skip the clamp.
            return;
        };
        let outer_size = winit_window.outer_size();
        let monitors: Vec<window_bounds::MonitorBounds> = winit_window
            .available_monitors()
            .map(|monitor| window_bounds::MonitorBounds {
                origin: vec2(monitor.position().x as f32, monitor.position().y as f32),
                size: vec2(monitor.size().width as f32, monitor.size().height as f32),
            })
            .collect();

        let position = vec2(outer.x as f32, outer.y as f32);
        let size = vec2(outer_size.width as f32, outer_size.height as f32);
        let clamped = window_bounds::clamp_window_position(position, size, &monitors);
        if clamped == position {
            return;
        }
        let target = IVec2::new(clamped.x.round() as i32, clamped.y.round() as i32);
        if window.position == WindowPosition::At(target) {
            // Bevy caches the last position it applied and will ignore an
            // identical value. Apply directly so a post-drag off-screen window
            // is also fixed.
            winit_window.set_outer_position(PhysicalPosition::new(target.x, target.y));
        } else {
            window.position = WindowPosition::At(target);
        }
    });
}

/// Applies the accessory activation policy once at startup. winit may reset
/// the policy after launch, so it is re-applied by `maintain_all_workspaces`.
fn initialize_ghost_policy(_non_send_marker: NonSendMarker) {
    if !platform::enforce_ghost_activation_policy() {
        warn!("Could not set accessory activation policy; AeroSpace may still manage this window.");
    }
}

/// Applies the saved window opacity. Ghost mode never dims the companion.
fn apply_settings_opacity(
    settings: Res<SettingsResource>,
    primary: Query<Entity, With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
    mut applied: Local<Option<f32>>,
) {
    let desired = settings.0.window_opacity;
    if *applied == Some(desired) {
        return;
    }
    let Ok(entity) = primary.single() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return;
        };
        if platform::set_window_alpha(&*wrapper, ghost_mode::window_alpha(desired)) {
            *applied = Some(desired);
        }
    });
}

/// Keeps the primary companion window on every macOS Space/AeroSpace
/// workspace. This is a no-op on other platforms.
fn maintain_all_workspaces(
    time: Res<Time>,
    mut check: ResMut<AllWorkspacesCheck>,
    primary: Query<Entity, With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
    mut configured: Local<bool>,
) {
    if !check.0.tick(time.delta()).just_finished() {
        return;
    }
    // The activation policy is the important part for AeroSpace: it only
    // manages apps whose policy is `.Regular`. Reapply it because winit can
    // reset it after launch.
    let accessory = platform::enforce_ghost_activation_policy();
    let Ok(entity) = primary.single() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return;
        };
        let all_spaces = platform::configure_all_workspaces(&*wrapper);
        if !*configured {
            if accessory && all_spaces {
                info!(
                    "Companion is an accessory ghost visible on all macOS Spaces/AeroSpace workspaces."
                );
            } else if accessory {
                warn!(
                    "All-Spaces flags unavailable; accessory policy still keeps AeroSpace from moving the window."
                );
            } else {
                warn!(
                    "Could not set accessory policy; AeroSpace may still manage this window."
                );
            }
            *configured = true;
        }
    });
}

fn update_click_through(
    state: Res<CompanionVisibility>,
    fallback: Res<WaylandFallback>,
    primary: Query<(Entity, &Window), With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
) {
    if fallback.0 {
        // Wayland does not support cursor hit-test regions; leave the window
        // fully interactive instead of silently losing all clicks.
        return;
    }
    let Ok((entity, window)) = primary.single() else {
        return;
    };
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return;
        };
        let winit_window: &WinitWindow = &*wrapper;
        let cursor = local_cursor(window, winit_window);
        let capture = !state.0.hidden() && over_companion(companion_area(window), cursor);
        let _ = winit_window.set_cursor_hittest(capture);
    });
}

pub(crate) fn local_cursor(window: &Window, winit_window: &WinitWindow) -> Option<Vec2> {
    if let Some((x, y)) = platform::cursor_position() {
        let scale = winit_window.scale_factor();
        let outer = winit_window.outer_position().ok()?.to_logical::<f64>(scale);
        return Some(Vec2::new((x - outer.x) as f32, (y - outer.y) as f32));
    }
    window.cursor_position()
}

fn companion_area(window: &Window) -> hit_test::Rect {
    hit_test::centered_area(
        window.width(),
        window.height(),
        hit_test::CHARACTER_DIAMETER,
    )
}

fn over_companion(area: hit_test::Rect, cursor: Option<Vec2>) -> bool {
    hit_test::decision(
        area,
        cursor.map(|point| hit_test::Point {
            x: point.x,
            y: point.y,
        }),
    ) == hit_test::ClickThrough::Capture
}
