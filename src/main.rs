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
use std::collections::VecDeque;
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
        .run();
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
