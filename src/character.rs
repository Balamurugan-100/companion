use crate::animation_state::{self, Event, State};
use crate::breathing_runtime;
use crate::bubbles::ReactionRequested;
use crate::dialogue::Reaction;
use crate::hit_test;
use crate::pointer;
use bevy::camera::ScalingMode;
use bevy::ecs::system::NonSendMarker;
use bevy::gltf::{Gltf, GltfAssetLabel};
use bevy::log::{error, info};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WINIT_WINDOWS;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use std::time::Duration;

/// Bundled at build/development time; there is no runtime download.
pub const CHARACTER_PATH: &str = "character.glb";

/// The Fox mesh is about 79 units tall and 155 units long in glTF space.
/// A 0.05 scale makes it ~4 units tall and ~7.75 units long, which fits the
/// 320x320 window with the orthographic view below.
const CHARACTER_SCALE: f32 = 0.05;
const VIEW_HEIGHT: f32 = 7.2;
const WALK_SPEED: f32 = 0.55;
const WALK_LIMIT: f32 = 0.4;
const MAX_YAW: f32 = 0.8;
const BLINK_INTERVAL: f32 = 4.5;
const BLINK_DURATION: f32 = 0.16;
const HAPPY_DURATION: f32 = 1.1;
const REST_DURATION: f32 = 2.8;
const WANDER_INTERVAL: f32 = 4.0;
const CLIP_TRANSITION: Duration = Duration::from_millis(150);

const IDLE_CLIP: &str = "Survey";
const WALK_CLIP: &str = "Walk";
const HAPPY_CLIP: &str = "Run";

#[derive(Resource)]
pub(crate) struct CharacterAssets {
    gltf: Handle<Gltf>,
    scene: Handle<WorldAsset>,
}

#[derive(Default, PartialEq)]
pub(crate) enum Outcome {
    #[default]
    Pending,
    Ready,
    Fallback,
}

#[derive(Resource, Default)]
pub(crate) struct LoadOutcome(pub(crate) Outcome);

impl LoadOutcome {
    fn pending(&self) -> bool {
        self.0 == Outcome::Pending
    }
}

/// Run condition for the one-shot asset load system.
pub fn load_pending(outcome: Option<Res<LoadOutcome>>) -> bool {
    outcome.is_some_and(|outcome| outcome.pending())
}

#[derive(Resource)]
pub(crate) struct Clips {
    idle: AnimationNodeIndex,
    walk: AnimationNodeIndex,
    happy: AnimationNodeIndex,
    graph: Handle<AnimationGraph>,
}

#[derive(Component)]
pub(crate) struct CharacterRoot;

#[derive(Message)]
pub(crate) struct Cheer;

#[derive(Resource)]
pub(crate) struct Runtime {
    state: State,
    x: f32,
    yaw: f32,
    walk_target: f32,
    walk_direction: f32,
    blink_timer: Timer,
    blink_pulse: Timer,
    happy_timer: Timer,
    rest_timer: Timer,
    wander_timer: Timer,
    playing: Option<State>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: State::Idle,
            x: 0.0,
            yaw: 0.0,
            walk_target: WALK_LIMIT,
            walk_direction: 1.0,
            blink_timer: Timer::from_seconds(BLINK_INTERVAL, TimerMode::Repeating),
            blink_pulse: Timer::from_seconds(BLINK_DURATION, TimerMode::Once),
            happy_timer: Timer::from_seconds(HAPPY_DURATION, TimerMode::Once),
            rest_timer: Timer::from_seconds(REST_DURATION, TimerMode::Once),
            wander_timer: Timer::from_seconds(WANDER_INTERVAL, TimerMode::Repeating),
            playing: None,
        }
    }
}

impl Runtime {
    fn apply(&mut self, event: Event) {
        let before = self.state;
        self.state = animation_state::transition(before, event);
        if self.state == before {
            return;
        }
        match self.state {
            State::Blink => self.blink_pulse.reset(),
            State::Happy => self.happy_timer.reset(),
            State::Walk => self.wander_timer.reset(),
            State::Idle => {
                if before == State::Walk {
                    self.wander_timer.reset();
                }
            }
            State::Rest => self.rest_timer.reset(),
        }
    }
}

/// Spawns the camera, lights and the asynchronous character load.
pub fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(6.0, 3.5, 8.0).looking_at(Vec3::new(0.0, 1.8, -0.5), Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 1200.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 3000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // A cool rim light makes the Fox's silhouette and mesh details readable
    // against the transparent desktop background.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.55, 0.75, 1.0),
            illuminance: 1400.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-5.0, 4.0, -6.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y),
    ));
    commands.insert_resource(CharacterAssets {
        gltf: asset_server.load(CHARACTER_PATH),
        scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset(CHARACTER_PATH)),
    });
    commands.insert_resource(LoadOutcome::default());
    commands.insert_resource(Runtime::default());
}

/// Waits for the GLB and its Scene(0) sub-asset, then either starts the Fox or
/// falls back to a visible placeholder with a readable error.
pub fn load_when_ready(
    mut commands: Commands,
    assets: Res<CharacterAssets>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut outcome: ResMut<LoadOutcome>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !outcome.pending() {
        return;
    }

    let states = [
        asset_server.get_load_states(&assets.gltf),
        asset_server.get_load_states(&assets.scene),
    ];
    if states.iter().any(|state| {
        state
            .as_ref()
            .is_some_and(|(load, dependencies, recursive)| {
                load.is_failed() || dependencies.is_failed() || recursive.is_failed()
            })
    }) {
        error!(
            "Character asset '{CHARACTER_PATH}' failed to load; showing the fallback placeholder."
        );
        spawn_fallback(&mut commands, &mut meshes, &mut materials);
        outcome.0 = Outcome::Fallback;
        return;
    }
    if states.iter().any(|state| state.is_none())
        || !asset_server.is_loaded_with_dependencies(&assets.gltf)
        || !asset_server.is_loaded_with_dependencies(&assets.scene)
    {
        return;
    }

    let Some(gltf) = gltfs.get(&assets.gltf) else {
        report_invalid(
            &mut commands,
            "the Gltf asset is missing",
            &mut meshes,
            &mut materials,
        );
        outcome.0 = Outcome::Fallback;
        return;
    };
    let Some(idle) = gltf.named_animations.get(IDLE_CLIP) else {
        report_invalid(
            &mut commands,
            &format!(
                "clip '{IDLE_CLIP}' is missing (found: {:?})",
                gltf.named_animations.keys().collect::<Vec<_>>()
            ),
            &mut meshes,
            &mut materials,
        );
        outcome.0 = Outcome::Fallback;
        return;
    };
    let Some(walk) = gltf.named_animations.get(WALK_CLIP) else {
        report_invalid(
            &mut commands,
            &format!("clip '{WALK_CLIP}' is missing"),
            &mut meshes,
            &mut materials,
        );
        outcome.0 = Outcome::Fallback;
        return;
    };
    let Some(happy) = gltf.named_animations.get(HAPPY_CLIP) else {
        report_invalid(
            &mut commands,
            &format!("clip '{HAPPY_CLIP}' is missing"),
            &mut meshes,
            &mut materials,
        );
        outcome.0 = Outcome::Fallback;
        return;
    };

    let (graph, nodes) = AnimationGraph::from_clips([idle.clone(), walk.clone(), happy.clone()]);
    let [idle_node, walk_node, happy_node] = [nodes[0], nodes[1], nodes[2]];
    commands.insert_resource(Clips {
        idle: idle_node,
        walk: walk_node,
        happy: happy_node,
        graph: graphs.add(graph),
    });

    commands
        .spawn((
            WorldAssetRoot(assets.scene.clone()),
            Transform::from_scale(Vec3::splat(CHARACTER_SCALE)),
            CharacterRoot,
        ))
        .observe(start_animation);
    info!("Loaded character '{CHARACTER_PATH}' with Survey/Walk/Run clips.");
    outcome.0 = Outcome::Ready;
}

fn report_invalid(
    commands: &mut Commands,
    reason: &str,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    error!(
        "Character asset '{CHARACTER_PATH}' is invalid: {reason}; showing the fallback placeholder."
    );
    spawn_fallback(commands, meshes, materials);
}

fn spawn_fallback(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere { radius: 1.4 })),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.55, 0.25),
            ..default()
        })),
    ));
}

/// The glTF loader adds an `AnimationPlayer` to the animation root inside the
/// spawned scene; attach our graph and start the idle clip once it is ready.
fn start_animation(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    clips: Res<Clips>,
    mut runtime: ResMut<Runtime>,
) {
    for child in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let mut transitions = AnimationTransitions::new();
            transitions
                .play(&mut player, clips.idle, Duration::ZERO)
                .repeat();
            commands
                .entity(child)
                .insert(AnimationGraphHandle(clips.graph.clone()))
                .insert(transitions);
            runtime.playing = Some(State::Idle);
        }
    }
}

/// Left-clicking the character is the v1 "happy" trigger; there are no app
/// events (task completed, focus finished) to react to until later tickets.
pub fn handle_click_cheer(
    buttons: Res<ButtonInput<MouseButton>>,
    primary: Query<(Entity, &Window), With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
    mut cheers: MessageWriter<Cheer>,
) {
    let Ok((entity, window)) = primary.single() else {
        return;
    };
    if !window.visible || !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(wrapper) = winit_windows.get_window(entity) else {
            return;
        };
        let cursor = crate::local_cursor(window, wrapper);
        let area = hit_test::centered_area(
            window.width(),
            window.height(),
            hit_test::CHARACTER_DIAMETER,
        );
        if area.contains(cursor.map(|point| hit_test::Point {
            x: point.x,
            y: point.y,
        })) {
            cheers.write(Cheer);
        }
    });
}

/// Drives the pure state machine from timers, click reactions and the small
/// wander target, and switches animation clips when the chosen clip changes.
pub fn advance_animation(
    time: Res<Time>,
    settings: Res<crate::SettingsResource>,
    mut runtime: ResMut<Runtime>,
    mut cheers: MessageReader<Cheer>,
    mut reactions: MessageReader<ReactionRequested>,
    clips: Option<Res<Clips>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for _ in cheers.read() {
        runtime.apply(Event::Cheer);
    }
    for request in reactions.read() {
        let event = match request.0 {
            Reaction::Cheer => Some(Event::Cheer),
            Reaction::Blink => Some(Event::BlinkTick),
            Reaction::Walk => Some(Event::WanderTick),
            Reaction::Rest => Some(Event::RestTick),
            Reaction::None => None,
        };
        if let Some(event) = event {
            runtime.apply(event);
        }
    }

    let delta = animation_state::animation_delta(time.delta(), settings.0.calm_mode);
    let delta_secs = delta.as_secs_f32();
    runtime.blink_timer.tick(delta);
    runtime.blink_pulse.tick(delta);
    runtime.happy_timer.tick(delta);
    runtime.rest_timer.tick(delta);
    runtime.wander_timer.tick(delta);

    match runtime.state {
        State::Idle => {
            if runtime.blink_timer.just_finished() {
                // The Fox has no blink clip: the Blink state only drives the
                // squash/stretch pulse applied in `apply_transform`.
                runtime.apply(Event::BlinkTick);
            } else if runtime.wander_timer.just_finished() {
                runtime.walk_direction *= -1.0;
                runtime.walk_target = runtime.walk_direction * WALK_LIMIT;
                runtime.apply(Event::WanderTick);
            }
        }
        State::Walk => {
            let remaining = runtime.walk_target - runtime.x;
            let step = WALK_SPEED * delta_secs;
            if remaining.abs() <= step {
                runtime.x = runtime.walk_target;
                runtime.apply(Event::TargetReached);
            } else {
                runtime.x += remaining.signum() * step;
            }
        }
        State::Blink => {
            if runtime.blink_pulse.just_finished() {
                runtime.apply(Event::BlinkFinished);
                runtime.blink_timer.reset();
            }
        }
        State::Happy => {
            if runtime.happy_timer.just_finished() {
                runtime.apply(Event::HappyFinished);
                runtime.blink_timer.reset();
                runtime.wander_timer.reset();
            }
        }
        State::Rest => {
            if runtime.rest_timer.just_finished() {
                runtime.apply(Event::RestFinished);
                runtime.blink_timer.reset();
                runtime.wander_timer.reset();
            }
        }
    }

    let Some(clips) = clips else {
        return;
    };
    // Blink and Rest keep the current clip and only change the root transform.
    let wanted = match runtime.state {
        State::Blink | State::Rest => runtime.playing.unwrap_or(State::Idle),
        other => other,
    };
    if runtime.playing != Some(wanted) {
        let node = match wanted {
            State::Idle => clips.idle,
            State::Walk => clips.walk,
            State::Happy => clips.happy,
            State::Blink | State::Rest => unreachable!("not a clip"),
        };
        for (mut player, mut transitions) in &mut players {
            transitions
                .play(&mut player, node, CLIP_TRANSITION)
                .repeat();
        }
        runtime.playing = Some(wanted);
    }
}

/// Applies the wander offset, pointer yaw, and the composed vertical scale
/// (blink × rest × breathing) to the character root.
pub fn apply_transform(
    time: Res<Time>,
    mut runtime: ResMut<Runtime>,
    breath: Res<breathing_runtime::Runtime>,
    primary: Query<(Entity, &Window), With<PrimaryWindow>>,
    _non_send_marker: NonSendMarker,
    mut roots: Query<&mut Transform, With<CharacterRoot>>,
) {
    let Ok((entity, window)) = primary.single() else {
        return;
    };
    let cursor = WINIT_WINDOWS.with_borrow(|winit_windows| {
        let wrapper = winit_windows.get_window(entity)?;
        crate::local_cursor(window, wrapper)
    });
    let wanted_yaw = match cursor {
        Some(point) => pointer::yaw_from_pointer_offset(
            point.x - window.width() / 2.0,
            window.width() / 2.0,
            MAX_YAW,
        ),
        None => 0.0,
    };
    let smoothing = 1.0 - (-9.0 * time.delta_secs()).exp();
    runtime.yaw += (wanted_yaw - runtime.yaw) * smoothing;
    let yaw = runtime.yaw;

    let blink_progress = if runtime.state == State::Blink {
        (runtime.blink_pulse.elapsed_secs() / BLINK_DURATION).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let vertical = animation_state::composed_vertical_scale(
        blink_progress,
        runtime.state == State::Rest,
        breath.scale(),
    );
    let horizontal = 1.0 / vertical.sqrt();

    for mut transform in &mut roots {
        transform.translation.x = runtime.x;
        transform.rotation = Quat::from_rotation_y(yaw);
        transform.scale = Vec3::new(horizontal, vertical, horizontal) * CHARACTER_SCALE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;

    fn elapsed_blink_timer(calm: bool) -> Duration {
        let mut app = App::new();
        app.add_message::<Cheer>();
        app.add_message::<ReactionRequested>();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(crate::SettingsResource(settings::Settings {
            calm_mode: calm,
            ..settings::defaults()
        }));
        app.insert_resource(Runtime::default());
        app.add_systems(Update, advance_animation);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(4));
        app.update();

        app.world().resource::<Runtime>().blink_timer.elapsed()
    }

    #[test]
    fn calm_settings_slow_the_character_animation_timers() {
        assert_eq!(elapsed_blink_timer(false), Duration::from_secs(4));
        assert_eq!(elapsed_blink_timer(true), Duration::from_millis(2400));
    }
}
