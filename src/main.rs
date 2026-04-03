//! Simple 2D Golf Game built with Bevy.
//!
//! Controls:
//!   - Use the Golf / Shoot buttons to switch between swing and gun modes
//!   - Move the mouse or touch to aim
//!   - Tap / click in Golf mode to shoot the ball
//!   - Tap / click in Shoot mode to fire at zombies
//!   - Power = distance from ball to cursor (capped at MAX_AIM_DIST)
//!
//! There are 6 holes, with zombies chasing the ball from later holes onward.

use bevy::{asset::AssetMetaCheck, audio::{PlaybackMode, Volume}, prelude::*, window::PrimaryWindow};

// ─── Constants ───────────────────────────────────────────────────────────────

const WINDOW_W: f32 = 800.0;
const WINDOW_H: f32 = 600.0;

/// Radius of the golf ball (pixels).
const BALL_R: f32 = 10.0;
/// Radius of the hole cup (pixels).
const HOLE_R: f32 = 14.0;

/// Per-frame velocity multiplier (simulates friction/grass resistance).
const FRICTION: f32 = 0.985;
/// Ball is considered stopped when speed drops below this value.
const STOP_SPEED: f32 = 2.5;
/// Velocity retained after bouncing off a wall (0 = no bounce, 1 = elastic).
const BOUNCE: f32 = 0.55;

/// Maximum distance (pixels) the cursor can be from the ball to set full power.
const MAX_AIM_DIST: f32 = 140.0;
/// Maps aim distance to initial speed (pixels/second).
const MAX_SPEED: f32 = 620.0;

const TOTAL_HOLES: usize = 6;
const TOP_UI_EXCLUSION_HEIGHT: f32 = 58.0;
const GUN_AIM_DIST: f32 = 110.0;
const GUN_SPEED: f32 = 720.0;
const GUN_PROJECTILE_LIFETIME: f32 = 1.2;
const PROJECTILE_R: f32 = 5.0;
const ZOMBIE_R: f32 = 14.0;
const ZOMBIE_SPEED: f32 = 42.0;
const DEATH_STROKE_PENALTY: u32 = 10;

/// BPM of the background track — controls how fast the visuals pulse.
const PULSE_BPM: f32 = 120.0;

// ─── Colours ────────────────────────────────────────────────────────────────

const COLOR_BG: Color = Color::srgb(0.05, 0.20, 0.05);
const COLOR_COURSE: Color = Color::srgb(0.18, 0.58, 0.18);
const COLOR_OBSTACLE: Color = Color::srgb(0.35, 0.22, 0.10);
const COLOR_BALL: Color = Color::WHITE;
const COLOR_HOLE: Color = Color::BLACK;
const COLOR_FLAG_POLE: Color = Color::srgb(0.85, 0.85, 0.85);
const COLOR_FLAG: Color = Color::srgb(0.95, 0.15, 0.15);
const COLOR_AIM_ARROW: Color = Color::srgba(1.0, 0.88, 0.0, 0.90);
const COLOR_TEXT: Color = Color::WHITE;
const COLOR_GOLD: Color = Color::srgb(1.0, 0.88, 0.0);
const COLOR_ZOMBIE: Color = Color::srgb(0.36, 0.72, 0.22);
const COLOR_GUN_AIM: Color = Color::srgba(0.95, 0.22, 0.22, 0.90);
const COLOR_PROJECTILE: Color = Color::srgb(1.0, 0.75, 0.15);

// ─── States ──────────────────────────────────────────────────────────────────

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum GameState {
    /// Ball is stationary; player is aiming.
    #[default]
    Playing,
    /// Waiting between holes (timer + message overlay).
    HoleComplete,
    /// All holes finished.
    GameOver,
}

// ─── Components ──────────────────────────────────────────────────────────────

/// Marks the golf ball.  Holds physics state.
#[derive(Component)]
struct Ball {
    velocity: Vec2,
    moving: bool,
}

/// Marks the hole cup entity.  Carries the world-space position for fast lookup.
#[derive(Component)]
struct GolfHole {
    pos: Vec2,
}

/// Marks the aim-arrow sprite.
#[derive(Component)]
struct AimArrow;

/// Marks the gun aim line while in shoot mode.
#[derive(Component)]
struct GunAim;

/// Marks every entity that belongs to the currently loaded hole
/// (course, obstacles, ball, hole, flag …).  Used for bulk despawn.
#[derive(Component)]
struct CourseEntity;

/// Rectangular AABB used for obstacle collision.
#[derive(Component)]
struct ObstacleColl {
    pos: Vec2,
    half: Vec2,
}

/// Course boundary rectangle.
#[derive(Component)]
struct CourseColl {
    center: Vec2,
    half: Vec2,
}

/// The persistent HUD score line.
#[derive(Component)]
struct ScoreText;

/// Transient overlay shown after a hole is complete.
#[derive(Component)]
struct MessageText;

/// Overlay shown on the game-over screen.
#[derive(Component)]
struct GameOverText;

/// Marks the three on-screen volume buttons and their display text.
#[derive(Component)] struct VolumeDownButton;
#[derive(Component)] struct VolumeMuteButton;
#[derive(Component)] struct VolumeUpButton;
/// The text child inside the mute/display button that shows current level.
#[derive(Component)] struct VolumeDisplayText;

/// Marks the persistent background sprite so the pulse system can find it.
#[derive(Component)]
struct BackgroundEntity;

/// A shambling enemy that chases the golf ball.
#[derive(Component)]
struct Zombie;

/// Projectile fired from the ball's gun.
#[derive(Component)]
struct Projectile {
    velocity: Vec2,
    lifetime: f32,
}

/// Marks the action-mode buttons.
#[derive(Component)]
struct ActionModeButton(ActionMode);

/// Flips to true after the first user gesture so pulse_to_music only
/// runs once the music is actually playing.
#[derive(Resource, Default)]
struct MusicPlaying(bool);

// ─── Resources ───────────────────────────────────────────────────────────────

#[derive(Resource)]
struct MusicVolume {
    volume: f32,
    muted: bool,
}

impl Default for MusicVolume {
    fn default() -> Self {
        Self { volume: 0.5, muted: false }
    }
}

#[derive(Resource, Default)]
struct GameData {
    current_hole: usize,
    strokes: Vec<u32>,
    par: Vec<u32>,
}

/// Inserted when a hole finishes; drives the between-holes countdown.
#[derive(Resource)]
struct TransitionTimer(Timer);

#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
enum ActionMode {
    #[default]
    Golf,
    Gun,
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
enum RunOutcome {
    #[default]
    Victory,
    Death,
}

// ─── Hole definitions ────────────────────────────────────────────────────────

struct HoleConfig {
    ball_start: Vec2,
    hole_pos: Vec2,
    par: u32,
    course_center: Vec2,
    course_size: Vec2,
    /// (center, half_size) pairs.
    obstacles: Vec<(Vec2, Vec2)>,
    zombies: Vec<Vec2>,
}

fn hole_configs() -> [HoleConfig; TOTAL_HOLES] {
    [
        // ── Hole 1 ── Straight shot, no obstacles, par 2 ──────────────────
        HoleConfig {
            ball_start: Vec2::new(-280.0, 0.0),
            hole_pos: Vec2::new(280.0, 0.0),
            par: 2,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(680.0, 130.0),
            obstacles: vec![],
            zombies: vec![],
        },
        // ── Hole 2 ── Two staggered pillars, par 3 ───────────────────────
        HoleConfig {
            ball_start: Vec2::new(-280.0, 0.0),
            hole_pos: Vec2::new(280.0, 0.0),
            par: 3,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(680.0, 170.0),
            obstacles: vec![
                (Vec2::new(-80.0, 35.0), Vec2::new(28.0, 50.0)),
                (Vec2::new(80.0, -35.0), Vec2::new(28.0, 50.0)),
            ],
            zombies: vec![Vec2::new(0.0, 58.0)],
        },
        // ── Hole 3 ── Three pillars in a zigzag, par 4 ───────────────────
        HoleConfig {
            ball_start: Vec2::new(-280.0, 55.0),
            hole_pos: Vec2::new(280.0, -55.0),
            par: 4,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(680.0, 185.0),
            obstacles: vec![
                (Vec2::new(-155.0, -22.0), Vec2::new(24.0, 62.0)),
                (Vec2::new(0.0, 42.0), Vec2::new(24.0, 62.0)),
                (Vec2::new(155.0, -22.0), Vec2::new(24.0, 62.0)),
            ],
            zombies: vec![Vec2::new(-20.0, -68.0), Vec2::new(140.0, 66.0)],
        },
        // ── Hole 4 ── Narrow lane with a gate, par 4 ─────────────────────
        HoleConfig {
            ball_start: Vec2::new(-290.0, -72.0),
            hole_pos: Vec2::new(275.0, 84.0),
            par: 4,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(700.0, 240.0),
            obstacles: vec![
                (Vec2::new(-95.0, 10.0), Vec2::new(28.0, 92.0)),
                (Vec2::new(55.0, -54.0), Vec2::new(28.0, 92.0)),
                (Vec2::new(190.0, 52.0), Vec2::new(24.0, 72.0)),
            ],
            zombies: vec![Vec2::new(-5.0, 86.0), Vec2::new(235.0, -70.0)],
        },
        // ── Hole 5 ── Two bends with central blockers, par 5 ──────────────
        HoleConfig {
            ball_start: Vec2::new(-300.0, 88.0),
            hole_pos: Vec2::new(285.0, -86.0),
            par: 5,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(710.0, 260.0),
            obstacles: vec![
                (Vec2::new(-165.0, -25.0), Vec2::new(26.0, 88.0)),
                (Vec2::new(-20.0, 55.0), Vec2::new(30.0, 68.0)),
                (Vec2::new(120.0, -56.0), Vec2::new(30.0, 68.0)),
                (Vec2::new(235.0, 34.0), Vec2::new(24.0, 76.0)),
            ],
            zombies: vec![
                Vec2::new(-70.0, -82.0),
                Vec2::new(90.0, 102.0),
                Vec2::new(245.0, -8.0),
            ],
        },
        // ── Hole 6 ── Final gauntlet, par 5 ───────────────────────────────
        HoleConfig {
            ball_start: Vec2::new(-310.0, 0.0),
            hole_pos: Vec2::new(305.0, 0.0),
            par: 5,
            course_center: Vec2::ZERO,
            course_size: Vec2::new(730.0, 290.0),
            obstacles: vec![
                (Vec2::new(-190.0, 72.0), Vec2::new(24.0, 68.0)),
                (Vec2::new(-110.0, -78.0), Vec2::new(24.0, 68.0)),
                (Vec2::new(-5.0, 0.0), Vec2::new(26.0, 112.0)),
                (Vec2::new(125.0, 82.0), Vec2::new(24.0, 68.0)),
                (Vec2::new(205.0, -82.0), Vec2::new(24.0, 68.0)),
            ],
            zombies: vec![
                Vec2::new(-205.0, -112.0),
                Vec2::new(-35.0, 120.0),
                Vec2::new(150.0, -118.0),
                Vec2::new(230.0, 118.0),
            ],
        },
    ]
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Bevy Golf 🏌️".into(),
                    resolution: (WINDOW_W, WINDOW_H).into(),
                    // The HTML canvas element targeted by Trunk / wasm-bindgen.
                    canvas: Some("#bevy".into()),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                // Prevent 404 errors on static hosts (GitHub Pages) by never
                // requesting .meta sidecar files alongside assets.
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
        )
        .init_state::<GameState>()
        .init_resource::<GameData>()
        .init_resource::<MusicVolume>()
        .init_resource::<MusicPlaying>()
        .init_resource::<ActionMode>()
        .init_resource::<RunOutcome>()
        // ── Startup ─────────────────────────────────────────────────────────
        .add_systems(
            Startup,
            (setup_camera, setup_background, setup_ui, setup_first_hole, setup_music).chain(),
        )
        // ── Per-frame (Playing) ──────────────────────────────────────────────
        .add_systems(
            Update,
            (
                (
                    update_aim_guides,
                    handle_golf_input,
                    handle_gun_input,
                    move_projectiles,
                    projectile_hits_zombies,
                    move_ball,
                    zombie_chase_ball,
                    detect_zombie_death,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
                update_score_text,
                volume_control,
                volume_button_interaction,
                action_mode_button_interaction,
                volume_button_visual,
                action_mode_button_visual,
                update_volume_text,
                detect_music_start,
                pulse_to_music.run_if(|mp: Res<MusicPlaying>| mp.0),
            ),
        )
        // ── Transition countdown ─────────────────────────────────────────────
        .add_systems(
            Update,
            tick_transition.run_if(resource_exists::<TransitionTimer>),
        )
        // ── State callbacks ──────────────────────────────────────────────────
        .add_systems(OnEnter(GameState::HoleComplete), on_hole_complete)
        .add_systems(OnEnter(GameState::GameOver), (cleanup_course_entities, on_game_over).chain())
        .add_systems(
            Update,
            (
                action_mode_keyboard_shortcuts.run_if(in_state(GameState::Playing)),
                restart_after_game_over.run_if(in_state(GameState::GameOver)),
            ),
        )
        .run();
}

// ─── Startup systems ─────────────────────────────────────────────────────────

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_music(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        AudioPlayer::new(asset_server.load("murderTrain.ogg")),
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::new(0.5),
            ..default()
        },
    ));
}

fn setup_background(mut commands: Commands) {
    commands.spawn((
        Sprite {
            color: COLOR_BG,
            custom_size: Some(Vec2::new(WINDOW_W, WINDOW_H)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, -10.0),
        BackgroundEntity,
    ));
}

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Text2d::new("Hole 1 / 6  |  Par 2  |  Strokes: 0"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(COLOR_TEXT),
        Transform::from_xyz(0.0, 270.0, 10.0),
        ScoreText,
    ));

    // Hint text at the bottom
    commands.spawn((
        Text2d::new("Golf: swing ball [G]  |  Shoot: fire at zombies [S]  |  Tap/click to aim"),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Transform::from_xyz(0.0, -270.0, 10.0),
    ));

    // ── Action mode buttons (touch-friendly gameplay controls) ─────────────
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(8.0),
            top: Val::Px(8.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(6.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|parent| {
            spawn_action_mode_button(parent, "Golf", ActionMode::Golf);
            spawn_action_mode_button(parent, "Shoot", ActionMode::Gun);
        });

    // ── Volume buttons (Bevy UI — respond to both click and touch) ──────────
    // Layout: [−]  [♪ 50%]  [+]  anchored to the top-right corner.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(8.0),
            top: Val::Px(8.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|parent| {
            // ── Vol-down button ─────────────────────────────────────────────
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(32.0),
                        height: Val::Px(32.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
                    BorderRadius::all(Val::Px(5.0)),
                    BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.75)),
                    VolumeDownButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("−"),
                        TextFont { font_size: 20.0, ..default() },
                        TextColor(COLOR_TEXT),
                    ));
                });

            // ── Display / mute button ───────────────────────────────────────
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(68.0),
                        height: Val::Px(32.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
                    BorderRadius::all(Val::Px(5.0)),
                    BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.75)),
                    VolumeMuteButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("♪ 50%"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(COLOR_TEXT),
                        VolumeDisplayText,
                    ));
                });

            // ── Vol-up button ───────────────────────────────────────────────
            parent
                .spawn((
                    Button,
                    Node {
                        width: Val::Px(32.0),
                        height: Val::Px(32.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
                    BorderRadius::all(Val::Px(5.0)),
                    BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.75)),
                    VolumeUpButton,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("+"),
                        TextFont { font_size: 20.0, ..default() },
                        TextColor(COLOR_TEXT),
                    ));
                });
        });
}

fn spawn_action_mode_button(
    parent: &mut ChildBuilder,
    label: &'static str,
    mode: ActionMode,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(74.0),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
            BorderRadius::all(Val::Px(5.0)),
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.75)),
            ActionModeButton(mode),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(COLOR_TEXT),
            ));
        });
}

fn setup_first_hole(
    mut commands: Commands,
    mut game_data: ResMut<GameData>,
    mut action_mode: ResMut<ActionMode>,
    mut outcome: ResMut<RunOutcome>,
) {
    let configs = hole_configs();
    game_data.strokes = vec![0u32; TOTAL_HOLES];
    game_data.par = configs.iter().map(|h| h.par).collect();
    *action_mode = ActionMode::Golf;
    *outcome = RunOutcome::Victory;
    spawn_hole(&mut commands, 0);
}

// ─── Hole spawn / despawn ────────────────────────────────────────────────────

fn spawn_hole(
    commands: &mut Commands,
    hole_idx: usize,
) {
    let configs = hole_configs();
    let h = &configs[hole_idx];

    // Fairway rectangle ─────────────────────────────────────────────────────
    commands.spawn((
        Sprite {
            color: COLOR_COURSE,
            custom_size: Some(h.course_size),
            ..default()
        },
        Transform::from_xyz(h.course_center.x, h.course_center.y, -1.0),
        CourseEntity,
        CourseColl {
            center: h.course_center,
            half: h.course_size / 2.0,
        },
    ));

    // Obstacles ─────────────────────────────────────────────────────────────
    for &(obs_pos, obs_half) in &h.obstacles {
        commands.spawn((
            Sprite {
                color: COLOR_OBSTACLE,
                custom_size: Some(obs_half * 2.0),
                ..default()
            },
            Transform::from_xyz(obs_pos.x, obs_pos.y, 0.0),
            CourseEntity,
            ObstacleColl {
                pos: obs_pos,
                half: obs_half,
            },
        ));
    }

    // Hole cup (black circle) ───────────────────────────────────────────────
    commands.spawn((
        Sprite {
            color: COLOR_HOLE,
            custom_size: Some(Vec2::splat(HOLE_R * 2.0)),
            ..default()
        },
        Transform::from_xyz(h.hole_pos.x, h.hole_pos.y, 0.5),
        CourseEntity,
        GolfHole { pos: h.hole_pos },
    ));

    // Flag pole ─────────────────────────────────────────────────────────────
    commands.spawn((
        Sprite {
            color: COLOR_FLAG_POLE,
            custom_size: Some(Vec2::new(3.0, 32.0)),
            ..default()
        },
        Transform::from_xyz(h.hole_pos.x, h.hole_pos.y + 16.0, 1.0),
        CourseEntity,
    ));

    // Flag ──────────────────────────────────────────────────────────────────
    commands.spawn((
        Sprite {
            color: COLOR_FLAG,
            custom_size: Some(Vec2::new(14.0, 9.0)),
            ..default()
        },
        Transform::from_xyz(h.hole_pos.x + 7.0, h.hole_pos.y + 28.0, 1.0),
        CourseEntity,
    ));

    // Ball (white circle) ───────────────────────────────────────────────────
    commands.spawn((
        Sprite {
            color: COLOR_BALL,
            custom_size: Some(Vec2::splat(BALL_R * 2.0)),
            ..default()
        },
        Transform::from_xyz(h.ball_start.x, h.ball_start.y, 2.0),
        CourseEntity,
        Ball {
            velocity: Vec2::ZERO,
            moving: false,
        },
    ));

    // Aim arrow (zero-width = invisible until the player aims) ──────────────
    commands.spawn((
        Sprite {
            color: COLOR_AIM_ARROW,
            custom_size: Some(Vec2::new(0.0, 5.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0),
        CourseEntity,
        AimArrow,
    ));

    commands.spawn((
        Sprite {
            color: COLOR_GUN_AIM,
            custom_size: Some(Vec2::new(0.0, 4.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0),
        CourseEntity,
        GunAim,
    ));

    for &zombie_pos in &h.zombies {
        commands.spawn((
            Sprite {
                color: COLOR_ZOMBIE,
                custom_size: Some(Vec2::splat(ZOMBIE_R * 2.0)),
                ..default()
            },
            Transform::from_xyz(zombie_pos.x, zombie_pos.y, 1.5),
            CourseEntity,
            Zombie,
        ));
    }

    // Hole number label ─────────────────────────────────────────────────────
    let label = format!("Hole {}  —  Par {}", hole_idx + 1, h.par);
    commands.spawn((
        Text2d::new(label),
        TextFont {
            font_size: 17.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
        Transform::from_xyz(h.course_center.x, h.course_center.y - h.course_size.y / 2.0 - 18.0, 5.0),
        CourseEntity,
    ));
}

// ─── Input helpers ────────────────────────────────────────────────────────────

fn pointer_is_in_gameplay_area(pointer: Vec2) -> bool {
    pointer.y >= TOP_UI_EXCLUSION_HEIGHT
}

fn current_gameplay_pointer(window: &Window, touches: &Touches) -> Option<Vec2> {
    window
        .cursor_position()
        .filter(|&cursor| pointer_is_in_gameplay_area(cursor))
        .or_else(|| {
            touches
                .iter()
                .find_map(|touch| pointer_is_in_gameplay_area(touch.position()).then_some(touch.position()))
        })
}

fn gameplay_pointer_just_pressed(
    window: &Window,
    mouse: &ButtonInput<MouseButton>,
    touches: &Touches,
) -> bool {
    let mouse_pressed = mouse.just_pressed(MouseButton::Left)
        && window
            .cursor_position()
            .is_some_and(pointer_is_in_gameplay_area);
    let touch_pressed = touches
        .iter_just_pressed()
        .any(|touch| pointer_is_in_gameplay_area(touch.position()));
    mouse_pressed || touch_pressed
}

fn pointer_world_pos(
    window: &Window,
    camera: &Camera,
    cam_gt: &GlobalTransform,
    touches: &Touches,
) -> Option<Vec2> {
    current_gameplay_pointer(window, touches)
        .and_then(|cursor| camera.viewport_to_world_2d(cam_gt, cursor).ok())
}

fn hide_aim(sprite: &mut Sprite) {
    if let Some(size) = sprite.custom_size.as_mut() {
        size.x = 0.0;
    }
}

fn update_aim_guides(
    ball_q: Query<(&Ball, &Transform)>,
    mut golf_aim_q: Query<(&mut Sprite, &mut Transform), (With<AimArrow>, Without<GunAim>, Without<Ball>)>,
    mut gun_aim_q: Query<(&mut Sprite, &mut Transform), (With<GunAim>, Without<AimArrow>, Without<Ball>)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    touches: Res<Touches>,
    action_mode: Res<ActionMode>,
) {
    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = window_q.get_single() else {
        return;
    };
    let Ok((ball, ball_tf)) = ball_q.get_single() else {
        return;
    };
    let Ok((mut golf_sprite, mut golf_tf)) = golf_aim_q.get_single_mut() else {
        return;
    };
    let Ok((mut gun_sprite, mut gun_tf)) = gun_aim_q.get_single_mut() else {
        return;
    };

    let Some(cursor) = pointer_world_pos(window, camera, cam_gt, &touches) else {
        hide_aim(&mut golf_sprite);
        hide_aim(&mut gun_sprite);
        return;
    };

    let ball_pos = ball_tf.translation.truncate();
    let delta = cursor - ball_pos;
    let raw_dist = delta.length();

    if raw_dist < 4.0 {
        hide_aim(&mut golf_sprite);
        hide_aim(&mut gun_sprite);
        return;
    }

    let dir = delta / raw_dist;
    let angle = dir.y.atan2(dir.x);
    let golf_dist = raw_dist.min(MAX_AIM_DIST);
    let gun_dist = raw_dist.min(GUN_AIM_DIST);

    if *action_mode == ActionMode::Golf && !ball.moving {
        if let Some(size) = golf_sprite.custom_size.as_mut() {
            size.x = golf_dist;
        }
        golf_tf.translation = (ball_pos + dir * (golf_dist / 2.0)).extend(3.0);
        golf_tf.rotation = Quat::from_rotation_z(angle);
        hide_aim(&mut gun_sprite);
    } else if *action_mode == ActionMode::Gun {
        if let Some(size) = gun_sprite.custom_size.as_mut() {
            size.x = gun_dist;
        }
        gun_tf.translation = (ball_pos + dir * (gun_dist / 2.0)).extend(3.0);
        gun_tf.rotation = Quat::from_rotation_z(angle);
        hide_aim(&mut golf_sprite);
    } else {
        hide_aim(&mut golf_sprite);
        hide_aim(&mut gun_sprite);
    }
}

// ─── Golf and gun input ───────────────────────────────────────────────────────

fn handle_golf_input(
    mut ball_q: Query<(&mut Ball, &Transform)>,
    mut arrow_q: Query<&mut Sprite, (With<AimArrow>, Without<GunAim>)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    action_mode: Res<ActionMode>,
    mut game_data: ResMut<GameData>,
) {
    if *action_mode != ActionMode::Golf {
        return;
    }

    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = window_q.get_single() else {
        return;
    };
    let Ok((mut ball, ball_tf)) = ball_q.get_single_mut() else {
        return;
    };
    if ball.moving {
        return;
    }

    let Some(cursor) = pointer_world_pos(window, camera, cam_gt, &touches) else {
        return;
    };
    if !gameplay_pointer_just_pressed(window, &mouse, &touches) {
        return;
    }

    let ball_pos = ball_tf.translation.truncate();
    let delta = cursor - ball_pos;
    let raw_dist = delta.length();
    if raw_dist < 4.0 {
        return;
    }

    let clamped_dist = raw_dist.min(MAX_AIM_DIST);
    let dir = delta / raw_dist;
    let speed = (clamped_dist / MAX_AIM_DIST) * MAX_SPEED;
    ball.velocity = dir * speed;
    ball.moving = true;

    let hole = game_data.current_hole;
    if hole < game_data.strokes.len() {
        game_data.strokes[hole] += 1;
    }

    if let Ok(mut arrow_sprite) = arrow_q.get_single_mut() {
        hide_aim(&mut arrow_sprite);
    }
}

fn handle_gun_input(
    mut commands: Commands,
    ball_q: Query<&Transform, With<Ball>>,
    mut gun_aim_q: Query<&mut Sprite, (With<GunAim>, Without<AimArrow>)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    action_mode: Res<ActionMode>,
) {
    if *action_mode != ActionMode::Gun {
        return;
    }

    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = window_q.get_single() else {
        return;
    };
    let Ok(ball_tf) = ball_q.get_single() else {
        return;
    };
    let Some(cursor) = pointer_world_pos(window, camera, cam_gt, &touches) else {
        return;
    };
    if !gameplay_pointer_just_pressed(window, &mouse, &touches) {
        return;
    }

    let ball_pos = ball_tf.translation.truncate();
    let delta = cursor - ball_pos;
    let len = delta.length();
    if len < 4.0 {
        return;
    }

    let dir = delta / len;
    commands.spawn((
        Sprite {
            color: COLOR_PROJECTILE,
            custom_size: Some(Vec2::splat(PROJECTILE_R * 2.0)),
            ..default()
        },
        Transform::from_xyz(ball_pos.x, ball_pos.y, 2.5),
        CourseEntity,
        Projectile {
            velocity: dir * GUN_SPEED,
            lifetime: GUN_PROJECTILE_LIFETIME,
        },
    ));

    if let Ok(mut gun_aim_sprite) = gun_aim_q.get_single_mut() {
        hide_aim(&mut gun_aim_sprite);
    }
}

// ─── Ball movement ───────────────────────────────────────────────────────────

fn move_ball(
    mut ball_q: Query<(&mut Ball, &mut Transform)>,
    hole_q: Query<&GolfHole>,
    obstacle_q: Query<&ObstacleColl>,
    course_q: Query<&CourseColl>,
    game_data: Res<GameData>,
    mut next_state: ResMut<NextState<GameState>>,
    time: Res<Time>,
) {
    let Ok((mut ball, mut ball_tf)) = ball_q.get_single_mut() else {
        return;
    };
    if !ball.moving {
        return;
    }

    let dt = time.delta_secs();
    let mut pos = ball_tf.translation.truncate();

    // Apply friction.
    ball.velocity *= FRICTION;

    // Integrate position.
    pos += ball.velocity * dt;

    // Bounce off course boundaries.
    for course in &course_q {
        bounce_course_bounds(&mut pos, &mut ball.velocity, course.center, course.half);
    }

    // Bounce off obstacles.
    for obs in &obstacle_q {
        resolve_obstacle_collision(&mut pos, &mut ball.velocity, obs.pos, obs.half);
    }

    ball_tf.translation = pos.extend(ball_tf.translation.z);

    // Check if ball is in the hole.
    for golf_hole in &hole_q {
        if pos.distance(golf_hole.pos) < HOLE_R * 0.85 {
            ball.moving = false;
            ball.velocity = Vec2::ZERO;
            // Snap ball into the hole.
            ball_tf.translation = golf_hole.pos.extend(ball_tf.translation.z);

            // Only record completion if we haven't already transitioned.
            if game_data.current_hole < TOTAL_HOLES {
                next_state.set(GameState::HoleComplete);
            }
            return;
        }
    }

    // Stop when speed is negligible.
    if ball.velocity.length() < STOP_SPEED {
        ball.velocity = Vec2::ZERO;
        ball.moving = false;
    }
}

fn move_projectiles(
    mut commands: Commands,
    mut projectile_q: Query<(Entity, &mut Projectile, &mut Transform)>,
    course_q: Query<&CourseColl>,
    time: Res<Time>,
) {
    let Ok(course) = course_q.get_single() else {
        return;
    };
    let min = course.center - course.half;
    let max = course.center + course.half;

    for (entity, mut projectile, mut transform) in &mut projectile_q {
        projectile.lifetime -= time.delta_secs();
        let pos = transform.translation.truncate() + projectile.velocity * time.delta_secs();
        transform.translation = pos.extend(transform.translation.z);

        let out_of_bounds = pos.x < min.x || pos.x > max.x || pos.y < min.y || pos.y > max.y;
        if projectile.lifetime <= 0.0 || out_of_bounds {
            commands.entity(entity).despawn();
        }
    }
}

fn projectile_hits_zombies(
    mut commands: Commands,
    projectile_q: Query<(Entity, &Transform), With<Projectile>>,
    zombie_q: Query<(Entity, &Transform), With<Zombie>>,
) {
    let mut spent_projectiles = Vec::new();
    let mut dead_zombies = Vec::new();

    for (projectile_entity, projectile_tf) in &projectile_q {
        let projectile_pos = projectile_tf.translation.truncate();
        for (zombie_entity, zombie_tf) in &zombie_q {
            let zombie_pos = zombie_tf.translation.truncate();
            if projectile_pos.distance(zombie_pos) <= PROJECTILE_R + ZOMBIE_R {
                spent_projectiles.push(projectile_entity);
                dead_zombies.push(zombie_entity);
                break;
            }
        }
    }

    spent_projectiles.sort_unstable();
    spent_projectiles.dedup();
    dead_zombies.sort_unstable();
    dead_zombies.dedup();

    for entity in spent_projectiles {
        commands.entity(entity).despawn();
    }
    for entity in dead_zombies {
        commands.entity(entity).despawn();
    }
}

fn zombie_chase_ball(
    ball_q: Query<&Transform, (With<Ball>, Without<Zombie>)>,
    mut zombie_q: Query<&mut Transform, With<Zombie>>,
    obstacle_q: Query<&ObstacleColl>,
    course_q: Query<&CourseColl>,
    time: Res<Time>,
) {
    let Ok(ball_tf) = ball_q.get_single() else {
        return;
    };
    let ball_pos = ball_tf.translation.truncate();
    let Ok(course) = course_q.get_single() else {
        return;
    };

    for mut zombie_tf in &mut zombie_q {
        let pos = zombie_tf.translation.truncate();
        let delta = ball_pos - pos;
        let len = delta.length();
        if len <= f32::EPSILON {
            continue;
        }

        let step = delta.normalize() * ZOMBIE_SPEED * time.delta_secs();
        let mut new_pos = pos + step;

        let min = course.center - course.half + Vec2::splat(ZOMBIE_R);
        let max = course.center + course.half - Vec2::splat(ZOMBIE_R);
        new_pos.x = new_pos.x.clamp(min.x, max.x);
        new_pos.y = new_pos.y.clamp(min.y, max.y);

        for obstacle in &obstacle_q {
            new_pos = push_circle_out_of_aabb(new_pos, ZOMBIE_R, obstacle.pos, obstacle.half);
        }

        zombie_tf.translation.x = new_pos.x;
        zombie_tf.translation.y = new_pos.y;
    }
}

fn detect_zombie_death(
    ball_q: Query<&Transform, (With<Ball>, Without<Zombie>)>,
    zombie_q: Query<&Transform, With<Zombie>>,
    mut game_data: ResMut<GameData>,
    mut outcome: ResMut<RunOutcome>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if hole_completion_pending(&next_state) {
        return;
    }

    let Ok(ball_tf) = ball_q.get_single() else {
        return;
    };
    let ball_pos = ball_tf.translation.truncate();

    let touched_ball = zombie_q
        .iter()
        .any(|zombie_tf| zombie_tf.translation.truncate().distance(ball_pos) <= BALL_R + ZOMBIE_R - 2.0);

    if !touched_ball {
        return;
    }

    *outcome = RunOutcome::Death;
    apply_death_penalty(&mut game_data);
    next_state.set(GameState::GameOver);
}

fn hole_completion_pending(next_state: &NextState<GameState>) -> bool {
    matches!(next_state, NextState::Pending(GameState::HoleComplete))
}

// ─── Collision helpers ────────────────────────────────────────────────────────

/// Keep ball inside the rectangular course bounds; reflect velocity on contact.
fn bounce_course_bounds(pos: &mut Vec2, vel: &mut Vec2, center: Vec2, half: Vec2) {
    let min = center - half + Vec2::splat(BALL_R);
    let max = center + half - Vec2::splat(BALL_R);

    if pos.x < min.x {
        pos.x = min.x;
        vel.x = vel.x.abs() * BOUNCE;
    } else if pos.x > max.x {
        pos.x = max.x;
        vel.x = -vel.x.abs() * BOUNCE;
    }

    if pos.y < min.y {
        pos.y = min.y;
        vel.y = vel.y.abs() * BOUNCE;
    } else if pos.y > max.y {
        pos.y = max.y;
        vel.y = -vel.y.abs() * BOUNCE;
    }
}

/// Circle–AABB collision: push ball out and reflect velocity.
fn resolve_obstacle_collision(pos: &mut Vec2, vel: &mut Vec2, obs_center: Vec2, obs_half: Vec2) {
    // Closest point on the AABB to the ball centre.
    let closest = Vec2::new(
        pos.x.clamp(obs_center.x - obs_half.x, obs_center.x + obs_half.x),
        pos.y.clamp(obs_center.y - obs_half.y, obs_center.y + obs_half.y),
    );
    let diff = *pos - closest;
    let dist_sq = diff.length_squared();

    if dist_sq > 0.0 && dist_sq < BALL_R * BALL_R {
        let dist = dist_sq.sqrt();
        let normal = diff / dist;
        // Push ball to the surface.
        *pos = closest + normal * BALL_R;
        // Reflect and attenuate.
        let dot = vel.dot(normal);
        if dot < 0.0 {
            *vel -= (1.0 + BOUNCE) * dot * normal;
        }
    }
}

fn push_circle_out_of_aabb(pos: Vec2, radius: f32, obs_center: Vec2, obs_half: Vec2) -> Vec2 {
    let closest = Vec2::new(
        pos.x.clamp(obs_center.x - obs_half.x, obs_center.x + obs_half.x),
        pos.y.clamp(obs_center.y - obs_half.y, obs_center.y + obs_half.y),
    );
    let diff = pos - closest;
    let dist_sq = diff.length_squared();
    let radius_sq = radius * radius;

    if dist_sq == 0.0 {
        let dx = (obs_half.x + radius) - (pos.x - obs_center.x).abs();
        let dy = (obs_half.y + radius) - (pos.y - obs_center.y).abs();
        if dx < dy {
            let sign = if pos.x >= obs_center.x { 1.0 } else { -1.0 };
            Vec2::new(obs_center.x + sign * (obs_half.x + radius), pos.y)
        } else {
            let sign = if pos.y >= obs_center.y { 1.0 } else { -1.0 };
            Vec2::new(pos.x, obs_center.y + sign * (obs_half.y + radius))
        }
    } else if dist_sq < radius_sq {
        let dist = dist_sq.sqrt();
        let normal = diff / dist;
        closest + normal * radius
    } else {
        pos
    }
}

fn apply_death_penalty(game_data: &mut GameData) {
    for strokes in game_data.strokes.iter_mut().skip(game_data.current_hole) {
        *strokes += DEATH_STROKE_PENALTY;
    }
}

// ─── HUD update ──────────────────────────────────────────────────────────────

fn update_score_text(
    game_data: Res<GameData>,
    mut text_q: Query<&mut Text2d, With<ScoreText>>,
) {
    let Ok(mut text) = text_q.get_single_mut() else {
        return;
    };
    let hole = game_data.current_hole;
    let strokes = game_data.strokes.get(hole).copied().unwrap_or(0);
    let par = game_data.par.get(hole).copied().unwrap_or(0);
    text.0 = format!(
        "Hole {} / {}  |  Par {}  |  Strokes: {}",
        hole + 1,
        TOTAL_HOLES,
        par,
        strokes,
    );
}

// ─── Music-reactive visuals ──────────────────────────────────────────────────

fn detect_music_start(
    mut music_playing: ResMut<MusicPlaying>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    touches: Res<Touches>,
) {
    if music_playing.0 { return; }
    if mouse.get_just_pressed().next().is_some()
        || keyboard.get_just_pressed().next().is_some()
        || touches.iter_just_pressed().next().is_some()
    {
        music_playing.0 = true;
    }
}

fn pulse_to_music(
    time: Res<Time>,
    mut bg_q: Query<
        (&mut Sprite, &mut Transform),
        (With<BackgroundEntity>, Without<CourseColl>, Without<ObstacleColl>),
    >,
    mut fairway_q: Query<
        &mut Sprite,
        (With<CourseColl>, Without<ObstacleColl>, Without<BackgroundEntity>),
    >,
    mut obstacle_q: Query<
        (&mut Sprite, &mut Transform),
        (With<ObstacleColl>, Without<CourseColl>, Without<BackgroundEntity>),
    >,
    mut ball_q: Query<
        (&mut Sprite, &mut Transform),
        (With<Ball>, Without<BackgroundEntity>, Without<CourseColl>, Without<ObstacleColl>, Without<GolfHole>),
    >,
    mut hole_q: Query<
        (&mut Sprite, &mut Transform),
        (With<GolfHole>, Without<BackgroundEntity>, Without<CourseColl>, Without<ObstacleColl>, Without<Ball>),
    >,
) {
    let t = time.elapsed_secs();
    let freq = PULSE_BPM / 60.0;
    // powf(4) sharpens the sine peak into a quick "thump" rather than a
    // smooth wave, so the pulse feels more like a real beat hit.
    let beat = ((t * freq * std::f32::consts::TAU).sin() * 0.5 + 0.5).powf(4.0);

    // Background: brightness flash + very subtle scale breathe
    for (mut sprite, mut transform) in &mut bg_q {
        sprite.color = Color::srgb(
            COLOR_BG.to_srgba().red   + beat * 0.05,
            COLOR_BG.to_srgba().green + beat * 0.10,
            COLOR_BG.to_srgba().blue  + beat * 0.07,
        );
        let scale = 1.0 + beat * 0.02;
        transform.scale = Vec3::splat(scale);
    }

    // Fairway: green brightness pulse
    for mut sprite in &mut fairway_q {
        sprite.color = Color::srgb(
            COLOR_COURSE.to_srgba().red   + beat * 0.08,
            COLOR_COURSE.to_srgba().green + beat * 0.14,
            COLOR_COURSE.to_srgba().blue  + beat * 0.08,
        );
    }

    // Obstacles: warm colour shift + scale pop
    for (mut sprite, mut transform) in &mut obstacle_q {
        sprite.color = Color::srgb(
            COLOR_OBSTACLE.to_srgba().red   + beat * 0.25,
            COLOR_OBSTACLE.to_srgba().green + beat * 0.08,
            COLOR_OBSTACLE.to_srgba().blue  + beat * 0.02,
        );
        let scale = 1.0 + beat * 0.10;
        transform.scale = Vec3::new(scale, scale, 1.0);
    }

    // Ball: scale pop + warm white-to-gold tint
    for (mut sprite, mut transform) in &mut ball_q {
        sprite.color = Color::srgb(
            1.0,
            1.0 - beat * 0.15,
            1.0 - beat * 0.55,
        );
        let scale = 1.0 + beat * 0.20;
        transform.scale = Vec3::splat(scale);
    }

    // Hole cup: purple glow + scale pulse
    for (mut sprite, mut transform) in &mut hole_q {
        sprite.color = Color::srgb(beat * 0.45, 0.0, beat * 0.55);
        let scale = 1.0 + beat * 0.15;
        transform.scale = Vec3::splat(scale);
    }
}

// ─── Volume control ──────────────────────────────────────────────────────────

fn volume_control(
    keyboard: Res<ButtonInput<KeyCode>>,
    music_q: Query<&AudioSink>,
    mut vol: ResMut<MusicVolume>,
) {
    let changed = if keyboard.just_pressed(KeyCode::BracketLeft) {
        vol.volume = (vol.volume - 0.1).max(0.0);
        vol.muted = false;
        true
    } else if keyboard.just_pressed(KeyCode::BracketRight) {
        vol.volume = (vol.volume + 0.1).min(1.0);
        vol.muted = false;
        true
    } else if keyboard.just_pressed(KeyCode::KeyM) {
        vol.muted = !vol.muted;
        true
    } else {
        false
    };

    if changed {
        let effective = if vol.muted { 0.0 } else { vol.volume };
        for sink in &music_q {
            sink.set_volume(effective);
        }
    }
}

fn action_mode_shortcut(
    keyboard: &ButtonInput<KeyCode>,
    game_state: &GameState,
) -> Option<ActionMode> {
    if *game_state != GameState::Playing {
        return None;
    }

    if keyboard.just_pressed(KeyCode::KeyG) {
        Some(ActionMode::Golf)
    } else if keyboard.just_pressed(KeyCode::KeyS) {
        Some(ActionMode::Gun)
    } else {
        None
    }
}

fn action_mode_keyboard_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    game_state: Res<State<GameState>>,
    mut action_mode: ResMut<ActionMode>,
) {
    if let Some(next_mode) = action_mode_shortcut(&keyboard, game_state.get()) {
        *action_mode = next_mode;
    }
}

fn update_volume_text(
    vol: Res<MusicVolume>,
    mut text_q: Query<&mut Text, With<VolumeDisplayText>>,
) {
    if !vol.is_changed() {
        return;
    }
    let Ok(mut text) = text_q.get_single_mut() else {
        return;
    };
    text.0 = if vol.muted {
        "♪ OFF".to_string()
    } else {
        format!("♪ {}%", (vol.volume * 100.0).round() as u32)
    };
}

/// Handles taps/clicks on the on-screen volume buttons.
fn volume_button_interaction(
    down_q: Query<&Interaction, (Changed<Interaction>, With<VolumeDownButton>)>,
    up_q: Query<&Interaction, (Changed<Interaction>, With<VolumeUpButton>)>,
    mute_q: Query<&Interaction, (Changed<Interaction>, With<VolumeMuteButton>)>,
    music_q: Query<&AudioSink>,
    mut vol: ResMut<MusicVolume>,
) {
    let mut changed = false;

    for interaction in &down_q {
        if *interaction == Interaction::Pressed {
            vol.volume = (vol.volume - 0.1).max(0.0);
            vol.muted = false;
            changed = true;
        }
    }
    for interaction in &up_q {
        if *interaction == Interaction::Pressed {
            vol.volume = (vol.volume + 0.1).min(1.0);
            vol.muted = false;
            changed = true;
        }
    }
    for interaction in &mute_q {
        if *interaction == Interaction::Pressed {
            vol.muted = !vol.muted;
            changed = true;
        }
    }

    if changed {
        let effective = if vol.muted { 0.0 } else { vol.volume };
        for sink in &music_q {
            sink.set_volume(effective);
        }
    }
}

/// Gives visual press/hover feedback on the three volume buttons.
fn volume_button_visual(
    mut query: Query<
        (&Interaction, &mut BackgroundColor),
        Or<(With<VolumeDownButton>, With<VolumeUpButton>, With<VolumeMuteButton>)>,
    >,
) {
    for (interaction, mut bg) in &mut query {
        *bg = match interaction {
            Interaction::Pressed  => BackgroundColor(Color::srgba(0.45, 0.45, 0.45, 0.95)),
            Interaction::Hovered  => BackgroundColor(Color::srgba(0.25, 0.25, 0.25, 0.90)),
            Interaction::None     => BackgroundColor(Color::srgba(0.10, 0.10, 0.10, 0.75)),
        };
    }
}

fn action_mode_button_interaction(
    query: Query<(&Interaction, &ActionModeButton), Changed<Interaction>>,
    mut action_mode: ResMut<ActionMode>,
) {
    for (interaction, button) in &query {
        if *interaction == Interaction::Pressed {
            *action_mode = button.0;
        }
    }
}

fn action_mode_button_visual(
    action_mode: Res<ActionMode>,
    mut query: Query<(&Interaction, &ActionModeButton, &mut BackgroundColor)>,
) {
    for (interaction, button, mut bg) in &mut query {
        let active = button.0 == *action_mode;
        *bg = match (*interaction, active) {
            (Interaction::Pressed, _) => BackgroundColor(Color::srgba(0.55, 0.55, 0.55, 0.95)),
            (Interaction::Hovered, true) => BackgroundColor(Color::srgba(0.25, 0.45, 0.25, 0.95)),
            (Interaction::Hovered, false) => BackgroundColor(Color::srgba(0.25, 0.25, 0.25, 0.90)),
            (Interaction::None, true) => BackgroundColor(Color::srgba(0.15, 0.38, 0.15, 0.85)),
            (Interaction::None, false) => BackgroundColor(Color::srgba(0.10, 0.10, 0.10, 0.75)),
        };
    }
}

// ─── Hole-complete callback ───────────────────────────────────────────────────

fn on_hole_complete(mut commands: Commands, game_data: Res<GameData>) {
    let hole = game_data.current_hole;
    let strokes = game_data.strokes.get(hole).copied().unwrap_or(0);
    let par = game_data.par.get(hole).copied().unwrap_or(0);

    let verdict = match strokes.cmp(&par) {
        std::cmp::Ordering::Less => "🦅 Under par!",
        std::cmp::Ordering::Equal => "Par!",
        std::cmp::Ordering::Greater => "Over par",
    };

    let msg = format!(
        "Hole {} complete!\nStrokes: {}   Par: {}\n{}\n\nNext hole in 3 s …",
        hole + 1,
        strokes,
        par,
        verdict,
    );

    commands.spawn((
        Text2d::new(msg),
        TextFont {
            font_size: 26.0,
            ..default()
        },
        TextColor(COLOR_GOLD),
        Transform::from_xyz(0.0, 30.0, 20.0),
        MessageText,
    ));

    commands.insert_resource(TransitionTimer(Timer::from_seconds(3.0, TimerMode::Once)));
}

// ─── Transition tick ─────────────────────────────────────────────────────────

fn tick_transition(
    mut commands: Commands,
    mut timer: ResMut<TransitionTimer>,
    time: Res<Time>,
    mut game_data: ResMut<GameData>,
    mut next_state: ResMut<NextState<GameState>>,
    course_q: Query<Entity, With<CourseEntity>>,
    msg_q: Query<Entity, With<MessageText>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.finished() {
        return;
    }

    commands.remove_resource::<TransitionTimer>();

    // Remove the overlay message.
    for e in &msg_q {
        commands.entity(e).despawn();
    }

    let next_hole = game_data.current_hole + 1;

    if next_hole >= TOTAL_HOLES {
        next_state.set(GameState::GameOver);
    } else {
        // Despawn current hole entities.
        for e in &course_q {
            commands.entity(e).despawn();
        }
        game_data.current_hole = next_hole;
        spawn_hole(&mut commands, next_hole);
        next_state.set(GameState::Playing);
    }
}

// ─── Game-over screen ────────────────────────────────────────────────────────

fn cleanup_course_entities(
    mut commands: Commands,
    course_q: Query<Entity, With<CourseEntity>>,
) {
    for entity in &course_q {
        commands.entity(entity).despawn();
    }
}

fn restart_requested_on_game_over(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.just_pressed(KeyCode::KeyR) || keyboard.just_pressed(KeyCode::Enter)
}

fn reset_run(
    commands: &mut Commands,
    game_data: &mut GameData,
    action_mode: &mut ActionMode,
    outcome: &mut RunOutcome,
) {
    let configs = hole_configs();
    game_data.current_hole = 0;
    game_data.strokes = vec![0u32; TOTAL_HOLES];
    game_data.par = configs.iter().map(|h| h.par).collect();
    *action_mode = ActionMode::Golf;
    *outcome = RunOutcome::Victory;
    spawn_hole(commands, 0);
}

fn restart_after_game_over(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut game_data: ResMut<GameData>,
    mut action_mode: ResMut<ActionMode>,
    mut outcome: ResMut<RunOutcome>,
    mut next_state: ResMut<NextState<GameState>>,
    game_over_q: Query<Entity, With<GameOverText>>,
) {
    if !restart_requested_on_game_over(&keyboard) {
        return;
    }

    for entity in &game_over_q {
        commands.entity(entity).despawn();
    }

    reset_run(&mut commands, &mut game_data, &mut action_mode, &mut outcome);
    next_state.set(GameState::Playing);
}

fn game_over_message(game_data: &GameData, outcome: RunOutcome) -> String {
    let total_strokes: u32 = game_data.strokes.iter().sum();
    let total_par: u32 = game_data.par.iter().sum();
    let diff = total_strokes as i32 - total_par as i32;

    let diff_str = match diff.cmp(&0) {
        std::cmp::Ordering::Less => format!("{} under par", -diff),
        std::cmp::Ordering::Equal => "Even par".to_string(),
        std::cmp::Ordering::Greater => format!("{} over par", diff),
    };

    let scorecard: String = game_data
        .strokes
        .iter()
        .zip(game_data.par.iter())
        .enumerate()
        .map(|(i, (&s, &p))| format!("  Hole {}:  {} strokes  (par {})", i + 1, s, p))
        .collect::<Vec<_>>()
        .join("\n");

    let restart_hint = "\n\nPress R or Enter to restart.";
    let (title, summary) = match outcome {
        RunOutcome::Victory => (
            "🏆  Game Complete!",
            format!("Total: {} strokes  —  {}", total_strokes, diff_str),
        ),
        RunOutcome::Death => (
            "💀  You have died!",
            format!(
                "Zombies reached the ball.\nDeath penalty: +{} strokes for each remaining hole.\n\nTotal: {} strokes  —  {}",
                DEATH_STROKE_PENALTY,
                total_strokes,
                diff_str
            ),
        ),
    };

    format!("{title}\n\n{scorecard}\n\n{summary}{restart_hint}")
}

fn on_game_over(
    mut commands: Commands,
    game_data: Res<GameData>,
    outcome: Res<RunOutcome>,
) {
    commands.spawn((
        Text2d::new(game_over_message(&game_data, *outcome)),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(COLOR_GOLD),
        Transform::from_xyz(0.0, 30.0, 20.0),
        GameOverText,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn death_penalty_applies_to_current_and_remaining_holes() {
        let mut game_data = GameData {
            current_hole: 2,
            strokes: vec![2, 3, 1, 0, 0, 0],
            par: vec![2, 3, 4, 4, 5, 5],
        };

        apply_death_penalty(&mut game_data);

        assert_eq!(game_data.strokes, vec![2, 3, 11, 10, 10, 10]);
    }

    #[test]
    fn gameplay_area_excludes_top_ui_strip() {
        assert!(pointer_is_in_gameplay_area(Vec2::new(20.0, TOP_UI_EXCLUSION_HEIGHT)));
        assert!(!pointer_is_in_gameplay_area(Vec2::new(20.0, TOP_UI_EXCLUSION_HEIGHT - 1.0)));
    }

    #[test]
    fn circle_pushes_out_of_obstacle() {
        let adjusted = push_circle_out_of_aabb(
            Vec2::new(5.0, 0.0),
            10.0,
            Vec2::ZERO,
            Vec2::new(8.0, 8.0),
        );

        assert!(adjusted.x >= 18.0 || adjusted.y.abs() >= 18.0);
    }

    #[test]
    fn hole_completion_pending_only_for_hole_complete_state() {
        assert!(hole_completion_pending(&NextState::Pending(GameState::HoleComplete)));
        assert!(!hole_completion_pending(&NextState::Pending(GameState::GameOver)));
        assert!(!hole_completion_pending(&NextState::Unchanged));
    }

    #[test]
    fn action_mode_shortcuts_only_apply_during_playing() {
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyG);
        assert_eq!(
            action_mode_shortcut(&keyboard, &GameState::Playing),
            Some(ActionMode::Golf)
        );
        assert_eq!(action_mode_shortcut(&keyboard, &GameState::HoleComplete), None);

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyS);
        assert_eq!(
            action_mode_shortcut(&keyboard, &GameState::Playing),
            Some(ActionMode::Gun)
        );
        assert_eq!(action_mode_shortcut(&keyboard, &GameState::GameOver), None);
    }

    #[test]
    fn restart_available_for_all_game_over_outcomes() {
        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::KeyR);
        assert!(restart_requested_on_game_over(&keyboard));

        let mut keyboard = ButtonInput::<KeyCode>::default();
        keyboard.press(KeyCode::Enter);
        assert!(restart_requested_on_game_over(&keyboard));
    }

    #[test]
    fn game_over_messages_include_restart_hint_for_both_outcomes() {
        let game_data = GameData {
            current_hole: 3,
            strokes: vec![2, 3, 5, 14, 10, 10],
            par: vec![2, 3, 4, 4, 5, 5],
        };

        let death_msg = game_over_message(&game_data, RunOutcome::Death);
        let victory_msg = game_over_message(&game_data, RunOutcome::Victory);

        assert!(death_msg.contains("Press R or Enter to restart."));
        assert!(victory_msg.contains("Press R or Enter to restart."));
        assert!(death_msg.contains("You have died!"));
        assert!(death_msg.contains("Death penalty"));
    }
}
