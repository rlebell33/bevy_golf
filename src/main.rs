//! Simple 2D Golf Game built with Bevy.
//!
//! Controls:
//!   - Move the mouse to aim (arrow extends from ball toward cursor)
//!   - Click the left mouse button to shoot
//!   - Power = distance from ball to cursor (capped at MAX_AIM_DIST)
//!
//! There are 3 holes, each with increasing difficulty.

use bevy::{audio::{PlaybackMode, Volume}, prelude::*, window::PrimaryWindow};

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

const TOTAL_HOLES: usize = 3;

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

/// Marks the volume HUD text.
#[derive(Component)]
struct VolumeText;

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

// ─── Hole definitions ────────────────────────────────────────────────────────

struct HoleConfig {
    ball_start: Vec2,
    hole_pos: Vec2,
    par: u32,
    course_center: Vec2,
    course_size: Vec2,
    /// (center, half_size) pairs.
    obstacles: Vec<(Vec2, Vec2)>,
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
        },
    ]
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Bevy Golf 🏌️".into(),
                resolution: (WINDOW_W, WINDOW_H).into(),
                // The HTML canvas element targeted by Trunk / wasm-bindgen.
                canvas: Some("#bevy".into()),
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .init_resource::<GameData>()
        .init_resource::<MusicVolume>()
        // ── Startup ─────────────────────────────────────────────────────────
        .add_systems(
            Startup,
            (setup_camera, setup_background, setup_ui, setup_first_hole, setup_music).chain(),
        )
        // ── Per-frame (Playing) ──────────────────────────────────────────────
        .add_systems(
            Update,
            (
                aim_and_shoot.run_if(in_state(GameState::Playing)),
                move_ball.run_if(in_state(GameState::Playing)),
                update_score_text,
                volume_control,
                update_volume_text,
            ),
        )
        // ── Transition countdown ─────────────────────────────────────────────
        .add_systems(
            Update,
            tick_transition.run_if(resource_exists::<TransitionTimer>),
        )
        // ── State callbacks ──────────────────────────────────────────────────
        .add_systems(OnEnter(GameState::HoleComplete), on_hole_complete)
        .add_systems(OnEnter(GameState::GameOver), on_game_over)
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
            volume: Volume(0.5),
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
    ));
}

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Text2d::new("Hole 1 / 3  |  Par 2  |  Strokes: 0"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(COLOR_TEXT),
        Transform::from_xyz(-220.0, 270.0, 10.0),
        ScoreText,
    ));

    // Hint text at the bottom
    commands.spawn((
        Text2d::new("Click to shoot  |  [ ] volume  |  M mute"),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
        Transform::from_xyz(0.0, -270.0, 10.0),
    ));

    // Volume indicator (top-right)
    commands.spawn((
        Text2d::new("♪ 50%"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
        Transform::from_xyz(330.0, 270.0, 10.0),
        VolumeText,
    ));
}

fn setup_first_hole(
    mut commands: Commands,
    mut game_data: ResMut<GameData>,
) {
    let configs = hole_configs();
    game_data.strokes = vec![0u32; TOTAL_HOLES];
    game_data.par = configs.iter().map(|h| h.par).collect();
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

// ─── Aim and shoot ───────────────────────────────────────────────────────────

fn aim_and_shoot(
    mut ball_q: Query<(&mut Ball, &Transform)>,
    mut arrow_q: Query<(&mut Sprite, &mut Transform), (With<AimArrow>, Without<Ball>)>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut game_data: ResMut<GameData>,
) {
    let Ok((camera, cam_gt)) = camera_q.get_single() else {
        return;
    };
    let Ok(window) = window_q.get_single() else {
        return;
    };
    let Ok((mut ball, ball_tf)) = ball_q.get_single_mut() else {
        return;
    };
    let Ok((mut arrow_sprite, mut arrow_tf)) = arrow_q.get_single_mut() else {
        return;
    };

    // Hide arrow while ball is rolling.
    if ball.moving {
        if let Some(sz) = arrow_sprite.custom_size.as_mut() {
            sz.x = 0.0;
        }
        return;
    }

    // Convert cursor from viewport → world space.
    let cursor_world = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(cam_gt, c).ok());

    let Some(cursor) = cursor_world else {
        if let Some(sz) = arrow_sprite.custom_size.as_mut() {
            sz.x = 0.0;
        }
        return;
    };

    let ball_pos = ball_tf.translation.truncate();
    let delta = cursor - ball_pos;
    let raw_dist = delta.length();

    if raw_dist < 4.0 {
        if let Some(sz) = arrow_sprite.custom_size.as_mut() {
            sz.x = 0.0;
        }
        return;
    }

    let clamped_dist = raw_dist.min(MAX_AIM_DIST);
    let dir = delta / raw_dist;
    let angle = dir.y.atan2(dir.x);
    let arrow_center = ball_pos + dir * (clamped_dist / 2.0);

    // Update arrow sprite.
    if let Some(sz) = arrow_sprite.custom_size.as_mut() {
        sz.x = clamped_dist;
    }
    arrow_tf.translation = arrow_center.extend(3.0);
    arrow_tf.rotation = Quat::from_rotation_z(angle);

    // Shoot on left-click.
    if mouse.just_pressed(MouseButton::Left) {
        let speed = (clamped_dist / MAX_AIM_DIST) * MAX_SPEED;
        ball.velocity = dir * speed;
        ball.moving = true;

        // Count the stroke.
        let hole = game_data.current_hole;
        if hole < game_data.strokes.len() {
            game_data.strokes[hole] += 1;
        }

        // Immediately hide arrow.
        if let Some(sz) = arrow_sprite.custom_size.as_mut() {
            sz.x = 0.0;
        }
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

fn update_volume_text(
    vol: Res<MusicVolume>,
    mut text_q: Query<&mut Text2d, With<VolumeText>>,
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

fn on_game_over(mut commands: Commands, game_data: Res<GameData>) {
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

    let msg = format!(
        "🏆  Game Complete!\n\n{}\n\nTotal: {} strokes  —  {}",
        scorecard, total_strokes, diff_str
    );

    commands.spawn((
        Text2d::new(msg),
        TextFont {
            font_size: 24.0,
            ..default()
        },
        TextColor(COLOR_GOLD),
        Transform::from_xyz(0.0, 30.0, 20.0),
    ));
}
