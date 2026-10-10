//! LEGO Star Wars: The Complete Saga character engine on Bevy.
//!
//! Reads the player's own game archive at start-up, then runs the rules in `lsw_sim` and draws
//! the result. Run with `cargo run -p lsw` from the folder holding the `.obb`, or pass the
//! archive path as the first argument. `--demo` plays a scripted input sequence, saves
//! screenshots to `logs/` and exits.

mod data;
mod obj;
mod skin;
mod testarea;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::camera::Hdr;
use data::GameData;
use lsw_sim::camera::{GameCamera, Rail};
use lsw_sim::clips::Action;
use lsw_sim::player::{Event, Input, Player};
use skin::DrawMesh;
use std::collections::HashMap;
use std::io::Write;
use tt_formats::an3;
use tt_formats::ghg::{mul, Mat4 as RowMat};

const DEFAULT_ARCHIVE: &str = "main.1063.com.wb.lego.tcs.obb";
const DEFAULT_LEVEL: &str = "levels\\episode_i\\negotiations\\negotiations_a";
const DEMO_DT: f32 = 1.0 / 60.0;

struct PosedMesh {
    draw: DrawMesh,
    handle: Handle<Mesh>,
}

struct SaberDraw {
    hilt: Vec<PosedMesh>,
    blade: Vec<PosedMesh>,
    /// The same blade drawn fatter and see-through: the coloured glow around the pale core.
    glow: Vec<PosedMesh>,
    centre: [f32; 3],
    blade_entities: Vec<Entity>,
    /// Axis the blade runs along and the coordinate of its hilt end, in the saber's own space.
    axis: usize,
    base: f32,
    locator: RowMat,
    locator_joint: usize,
}

#[derive(Resource)]
struct Game {
    data: GameData,
    player: Player,
    camera: GameCamera,
    meshes: Vec<PosedMesh>,
    saber: Option<SaberDraw>,
    test_lights: Vec<lsw_sim::lighting::Light>,
    combat: lsw_sim::combat::Combat,
    enemy_draw: Vec<EnemyDraw>,
    bolt_entities: Vec<Entity>,
    /// Put enemies in this level (the built-in test area only).
    spawn_enemies: bool,
    /// Use the chase camera (model-file levels only).
    chase: bool,
    /// Free-flight mode (toggled with F): no gravity or collision, for looking around.
    fly: bool,
    /// Up/down input and the speed boost while flying.
    fly_vertical: f32,
    fly_boost: bool,
    /// Camera rail set the player was last inside.
    sock: Option<usize>,
    /// Whether scenery hides the player from the camera (the game's own test).
    hidden: lsw_sim::camera::HiddenCheck,
    input: Input,
    held: (bool, bool, bool),
    log: Option<std::fs::File>,
    log_timer: f32,
    clock: f32,
    demo: Option<Demo>,
}

#[derive(Resource, Default)]
struct Sounds {
    clips: HashMap<&'static str, Handle<AudioSource>>,
    hum: Option<Entity>,
    swing: usize,
}

#[derive(Component)]
struct CharacterRoot;

/// An upside-down copy of the character under a reflective floor. The level supplies its own
/// mirrored scenery; characters are mirrored by the game at run time.
#[derive(Component)]
struct MirrorRoot;

#[derive(Component)]
struct Hud;

/// The round shadow the game draws under a character.
#[derive(Component)]
struct BlobShadow;

/// A scripted run used to check the build without a person at the controls.
struct Demo {
    /// (start, end, stick x, stick z, jump, action, special)
    script: Vec<(f32, f32, f32, f32, bool, bool, bool)>,
    shots: Vec<(f32, &'static str)>,
    next_shot: usize,
    end: f32,
}

impl Demo {
    fn new() -> Self {
        let s = |a: f32, b: f32, x: f32, z: f32| (a, b, x, z, false, false, false);
        let jump = |a: f32, b: f32, x: f32, z: f32| (a, b, x, z, true, false, false);
        let action = |a: f32| (a, a + 0.05, 0.0, 0.0, false, true, false);
        Self {
            script: vec![
                s(1.0, 3.0, 1.0, 0.0),
                s(3.0, 4.5, 0.0, 1.0),
                s(4.5, 5.5, -0.7, 0.0),
                jump(6.0, 6.3, 0.0, 0.0),
                jump(7.5, 7.7, 0.0, 0.0),
                jump(7.9, 8.1, 0.0, 0.0),
                action(8.3),
                action(10.5),
                action(12.5),
                action(13.1),
                action(13.75),
                s(16.0, 17.0, 1.0, 0.0),
                jump(17.0, 17.2, 1.0, 0.0),
                (17.25, 17.3, 1.0, 0.0, false, true, false),
                (19.5, 19.55, 0.0, 0.0, false, false, true),
                // Hold the Force on the door, then walk at it.
                (22.0, 23.6, 0.0, 0.0, false, false, true),
            ],
            shots: vec![
                (0.8, "01_idle"),
                (2.5, "02_run_right"),
                (4.2, "03_run_away"),
                (5.3, "04_walk_left"),
                (6.3, "05_jump"),
                (8.15, "06_double_jump"),
                (8.5, "07_slam"),
                (9.0, "08_slam_land"),
                (11.0, "09_saber_draw"),
                (12.0, "10_saber_idle"),
                (12.8, "11_combo_1"),
                (13.5, "12_combo_2"),
                (14.2, "13_combo_3"),
                (17.45, "14_lunge"),
                (19.0, "15_after_lunge"),
                (21.0, "16_saber_away"),
                (23.0, "17_force_hold"),
                (25.0, "18_door_open"),
                (27.4, "19_through_door"),
            ],
            next_shot: 0,
            end: 28.0,
        }
    }

    fn input(&self, t: f32) -> (f32, f32, bool, bool, bool) {
        let mut out = (0.0, 0.0, false, false, false);
        for &(start, end, x, z, jump, action, special) in &self.script {
            if t >= start && t < end {
                out = (x, z, out.2 || jump, out.3 || action, out.4 || special);
            }
        }
        out
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|a| a == "--demo");
    // With no option the built-in test area is used. `--lego` loads the game's own first
    // level, `--level=<folder inside the archive>` another one, `--flat` a bare floor.
    let lego = args.iter().any(|a| a == "--lego" || a.starts_with("--level="));
    let bare = args.iter().any(|a| a == "--flat");
    let flat = bare || !lego;
    let level = args.iter().find_map(|a| a.strip_prefix("--level=")).unwrap_or(DEFAULT_LEVEL).to_string();
    // `--obj=<file.obj>` uses a plain model as the level; `--obj-scale=` sets how many game
    // units one model unit is (a minifig is 0.42 game units tall).
    let obj_path = args.iter().find_map(|a| a.strip_prefix("--obj=")).map(std::path::PathBuf::from);
    let obj_scale: f32 = args.iter().find_map(|a| a.strip_prefix("--obj-scale=")).and_then(|s| s.parse().ok()).unwrap_or(0.05);
    let flat = flat || obj_path.is_some();
    let archive = args.iter().find(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| DEFAULT_ARCHIVE.to_string());

    let _ = std::fs::create_dir_all("logs");
    let mut log = std::fs::File::create("logs/lsw.log").ok();
    let mut notes = Vec::new();
    let data = match GameData::load(&archive, "quigonjinn", (!flat).then_some(level.as_str()), &mut notes) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Could not load the game data: {e}");
            eprintln!("Run from the folder that holds {DEFAULT_ARCHIVE}, or pass its path as the first argument.");
            std::process::exit(1);
        }
    };
    for line in &notes {
        println!("{line}");
        if let Some(file) = log.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }

    let mut data = data;
    if let Some(path) = &obj_path {
        match obj::load(path, obj_scale) {
            Ok(scene) => {
                let mut world = lsw_sim::world::World::new(scene.triangles.iter().copied());
                // Start on the highest ground above the model's origin, or failing that on the
                // highest walkable triangle anywhere.
                let top = scene.triangles.iter().flat_map(|t| t.0).fold(f32::MIN, |m, p| m.max(p[1]));
                let start = world.floor_between(0.0, 0.0, top + 1.0, -1.0e9).map(|y| lsw_sim::Vec3::new(0.0, y, 0.0)).or_else(|| {
                    scene
                        .triangles
                        .iter()
                        .filter(|t| t.1[1] > 0.9)
                        .max_by(|a, b| a.0[0][1].total_cmp(&b.0[0][1]))
                        .map(|t| lsw_sim::Vec3::new((t.0[0][0] + t.0[1][0] + t.0[2][0]) / 3.0, t.0[0][1].max(t.0[1][1]).max(t.0[2][1]), (t.0[0][2] + t.0[1][2] + t.0[2][2]) / 3.0))
                });
                world.start = start;
                println!(
                    "model level {}: {} collision triangles ({}), {} materials, scale {obj_scale}, start {start:?}",
                    path.display(),
                    scene.triangles.len(),
                    if scene.has_collision_shell { "the file's own collision shell, not drawn" } else { "the drawn geometry" },
                    scene.batches.len()
                );
                data.custom = Some(data::CustomLevel { scene, world });
            }
            Err(e) => {
                eprintln!("Could not load the model level: {e}");
                std::process::exit(1);
            }
        }
    }
    // In a model level only, the character runs faster than his LEGO speeds (`--obj-speed=`,
    // default 5 times). The game's own level keeps the real numbers.
    if obj_path.is_some() && data.custom.is_some() {
        let factor: f32 = args.iter().find_map(|a| a.strip_prefix("--obj-speed=")).and_then(|s| s.parse().ok()).unwrap_or(5.0);
        let stats = &mut data.character.stats;
        stats.tiptoe_speed *= factor;
        stats.walk_speed *= factor;
        stats.run_speed *= factor;
        println!("model level: movement speeds multiplied by {factor} (run {} units a second)", stats.run_speed);
    }
    // The built-in test area: this project's own ramps, stairs and platforms.
    if obj_path.is_none() && !lego && !bare {
        let scene = testarea::build();
        let mut world = lsw_sim::world::World::new(scene.triangles.iter().copied());
        world.start = Some(lsw_sim::Vec3::ZERO);
        println!("test area: {} collision triangles", scene.triangles.len());
        if demo {
            test_area_report(&world, &data.character.stats, &data.character.clips);
        }
        data.custom = Some(data::CustomLevel { scene, world });
    }
    let mut player = Player::new(data.character.stats.clone());
    if let Some(start) = data.world().and_then(|w| w.start) {
        player.pos = start;
        player.floor_y = start.y;
    }
    // `--start=x,z` puts the character somewhere else on the ground (for trying one spot).
    if let Some((x, z)) = args.iter().find_map(|a| a.strip_prefix("--start=")).and_then(|s| s.split_once(',')) {
        if let (Ok(x), Ok(z), Some(world)) = (x.trim().parse::<f32>(), z.trim().parse::<f32>(), data.world()) {
            let y = world.floor_between(x, z, 1.0e6, -1.0e6).unwrap_or(0.0);
            player.pos = lsw_sim::Vec3::new(x, y, z);
            player.floor_y = y;
        }
    }
    let game = Game {
        sock: None,
        hidden: Default::default(),
        combat: lsw_sim::combat::Combat::default(),
        enemy_draw: Vec::new(),
        bolt_entities: Vec::new(),
        spawn_enemies: obj_path.is_none() && !lego && !bare,
        chase: obj_path.is_some(),
        fly: false,
        fly_vertical: 0.0,
        fly_boost: false,
        test_lights: lsw_sim::lighting::test_lights(),
        data,
        player,
        camera: GameCamera::new(Rail::default()),
        meshes: Vec::new(),
        saber: None,
        input: Input::default(),
        held: (false, false, false),
        log,
        log_timer: 0.0,
        clock: 0.0,
        demo: demo.then(Demo::new),
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "LEGO Star Wars engine on Bevy - Qui-Gon Jinn".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(if flat { Color::srgb(0.45, 0.66, 0.9) } else { Color::BLACK }))
        .insert_resource(game)
        .init_resource::<Sounds>()
        .add_systems(Startup, setup)
        .add_systems(Update, (read_input, simulate, pose_character, place_shadow, place_camera, pose_enemies, draw_bolts, update_hud, place_hidden_marker, beat_hearts, spin_coin, move_force_pieces, run_demo).chain())
        .run();
}

/// Walk and jump a character through the test area with no window, and print what happened.
/// This is the numeric check of the movement rules against known shapes.
fn test_area_report(world: &lsw_sim::world::World, stats: &lsw_sim::player::Stats, clips: &lsw_sim::clips::Clips) {
    let dt = 1.0 / 60.0;
    // `push` is how long the stick is held; after that he is left to come to rest.
    let run = |start: lsw_sim::Vec3, stick: (f32, f32), push: f32, seconds: f32, jumps: &[f32]| {
        let mut player = Player::new(stats.clone());
        player.pos = start;
        let (mut top, mut t, mut held) = (start.y, 0.0f32, false);
        while t < seconds {
            let jump = jumps.iter().any(|&at| t >= at && t < at + 0.1);
            let stick = if t < push { stick } else { (0.0, 0.0) };
            let input = Input { stick_x: stick.0, stick_z: stick.1, jump_pressed: jump && !held, jump_held: jump, ..Default::default() };
            held = jump;
            player.step_in(&input, 0, clips, Some(world), dt);
            top = top.max(player.pos.y);
            t += dt;
        }
        (player.pos, top, player.on_ground)
    };
    for (i, angle) in testarea::RAMP_ANGLES.iter().enumerate() {
        let x = -11.75 + i as f32 * 4.0 + 1.5;
        let (_, top, _) = run(lsw_sim::Vec3::new(x, 0.0, 4.5), (0.0, 1.0), 14.0, 14.0, &[]);
        println!("ramp {angle:>2} degrees: reached height {top:.2} of 1.50 ({})", if top > 1.45 { "climbed" } else { "stopped" });
    }
    for (i, height) in testarea::STEP_HEIGHTS.iter().enumerate() {
        let x = -6.0 + i as f32 * 4.5 + 1.5;
        let (_, top, _) = run(lsw_sim::Vec3::new(x, 0.0, -5.0), (0.0, -1.0), 8.0, 8.0, &[]);
        println!("stairs with {height:.2} steps: reached height {top:.2} of {:.2}", height * 10.0);
    }
    for (i, height) in testarea::PLATFORM_HEIGHTS.iter().enumerate() {
        let z = -6.0 + i as f32 * 3.0 + 1.0;
        let start = lsw_sim::Vec3::new(13.2, 0.0, z);
        let (single, _, _) = run(start, (1.0, 0.0), 1.2, 3.0, &[0.3]);
        let (double, _, _) = run(start, (1.0, 0.0), 1.2, 3.0, &[0.3, 0.55]);
        let on = |p: lsw_sim::Vec3| if (p.y - height).abs() < 0.02 { "lands on it" } else { "does not make it" };
        println!("platform {height:.1} high: single jump {}, double jump {}", on(single), on(double));
    }
    let mut z = -12.0;
    for gap in testarea::GAP_WIDTHS {
        // From the middle of the slab before the gap: run, then jump near its edge.
        let start = lsw_sim::Vec3::new(-18.0, 0.4, z + 0.5);
        let run_up = (2.5 - 0.15) / stats.run_speed;
        let (single, _, _) = run(start, (0.0, 1.0), run_up + 1.2, run_up + 3.0, &[run_up]);
        let (double, _, _) = run(start, (0.0, 1.0), run_up + 1.4, run_up + 3.0, &[run_up, run_up + 0.3]);
        let over = |p: lsw_sim::Vec3| if p.y > 0.39 && p.z > z + 3.0 + gap - 0.05 { "clears it" } else { "falls short" };
        println!("gap {gap:.1} wide: single jump {}, double jump {}", over(single), over(double));
        z += 3.0 + gap;
    }
    // Camera: stand the character on the ground all over the area and see whether the camera's
    // resting place has scenery over it (it is inside or under something).
    let camera = GameCamera::new(Rail::default());
    let (mut spots, mut plain_inside, mut lifted_inside) = (0, 0, 0);
    for xi in -44..=44 {
        for zi in -44..=60 {
            let (x, z) = (xi as f32 * 0.5, zi as f32 * 0.5);
            let Some(y) = world.floor_between(x, z, 0.05, -0.05) else { continue };
            let feet = lsw_sim::Vec3::new(x, y, z);
            let o = camera.rail.offset;
            let plain = lsw_sim::Vec3::new(x + o.x, y + stats.height * 0.5 + o.y, z + o.z);
            let (lifted, _) = camera.stand_in_goal(feet, stats.height, world);
            let covered = |p: lsw_sim::Vec3| world.segment_hit(lsw_sim::Vec3::new(p.x, p.y + 20.0, p.z), p).is_some();
            spots += 1;
            plain_inside += covered(plain) as u32;
            lifted_inside += covered(lifted) as u32;
        }
    }
    println!("camera: of {spots} standing spots, the plain offset put the camera inside scenery at {plain_inside}; with the lift, {lifted_inside}");
}

fn make_image(texture: &tt_formats::atc::Texture) -> Image {
    let mut image = Image::new(
        Extent3d { width: texture.width, height: texture.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        texture.rgba[..(texture.width * texture.height * 4) as usize].to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The constructor only accepts the top level; the full mip chain goes in afterwards.
    image.data = Some(texture.rgba.clone());
    image.texture_descriptor.mip_level_count = texture.mip_count;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn make_materials(
    model: &tt_formats::ghg::Model,
    textures: &[Option<tt_formats::atc::Texture>],
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<Handle<StandardMaterial>> {
    let handles: Vec<Option<(Handle<Image>, bool)>> =
        textures.iter().map(|t| t.as_ref().map(|t| (images.add(make_image(t)), t.has_alpha))).collect();
    model
        .materials
        .iter()
        .map(|m| {
            let texture = m.diffuse.and_then(|i| handles.get(i).cloned().flatten());
            let has_alpha = texture.as_ref().is_some_and(|t| t.1);
            materials.add(StandardMaterial {
                // Material colours in the files are display-space values.
                base_color: Color::srgba(m.colour[0], m.colour[1], m.colour[2], 1.0),
                base_color_texture: texture.map(|t| t.0),
                // Lighting is worked out per vertex by the game's own rules and arrives as
                // vertex colour, so the renderer adds none of its own.
                unlit: true,
                // The mirrored copy under a reflective floor is seen from its inside-out side.
                double_sided: true,
                cull_mode: None,
                alpha_mode: if has_alpha { AlphaMode::Blend } else { AlphaMode::Opaque },
                // Printed faces sit on top of the head surface.
                depth_bias: if has_alpha { 8.0 } else { 0.0 },
                ..default()
            })
        })
        .collect()
}

fn build_mesh(draw: &DrawMesh, positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32, 1.0, 1.0, 1.0]; positions.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, draw.uvs.clone());
    mesh.insert_indices(Indices::U32(draw.indices.clone()));
    mesh
}

fn blade_colour(name: &str) -> LinearRgba {
    match name {
        "red" => LinearRgba::rgb(9.0, 0.5, 0.4),
        "blue" => LinearRgba::rgb(0.5, 1.8, 9.0),
        "purple" => LinearRgba::rgb(5.0, 0.6, 9.0),
        _ => LinearRgba::rgb(0.5, 6.0, 0.8),
    }
}

#[derive(Component)]
struct StudDigits;

#[derive(Resource)]
struct HudFont {
    image: Handle<Image>,
    height: f32,
    glyphs: HashMap<char, [f32; 3]>,
}

/// Spawn `text` as a row of glyph images from the game font under `parent`.
fn spawn_glyphs(commands: &mut Commands, font: &HudFont, parent: Entity, text: &str, size: f32, colour: Color) {
    let scale = size / font.height;
    for c in text.chars() {
        let Some(&[x, y, width]) = font.glyphs.get(&c) else { continue };
        commands.spawn((
            ImageNode {
                image: font.image.clone(),
                rect: Some(Rect::new(x - width, y, x, y + font.height)),
                color: colour,
                ..default()
            },
            Node { width: Val::Px(width * scale), height: Val::Px(size), ..default() },
            ChildOf(parent),
        ));
    }
}

// Where the game puts each HUD piece, in its "panel" units (x right, y up, 0 at screen centre).
// Values are the game's own constants.
const PANEL_ICON_X: f32 = 0.775;
const PANEL_ICON_SIZE: f32 = 0.16;
const PANEL_STATS_Y: f32 = 0.745;
const PANEL_HEARTS_X: f32 = 0.65;
const PANEL_HEART_Y: f32 = -0.045;
const PANEL_HEART_SCALE: f32 = 0.195;
const PANEL_COIN_X: f32 = 0.65;
const PANEL_COIN_Y: f32 = 0.045;
const PANEL_COIN_SCALE: f32 = 0.35;
const PANEL_SCORE_X: f32 = 0.61;

/// The fixed camera the HUD pieces are drawn with.
#[derive(Component)]
struct HudCamera;

/// One heart of the health row; the last full one beats.
#[derive(Component)]
struct HudHeart {
    index: u32,
    scale: f32,
}

/// One droid's drawable pieces. When it is destroyed each piece flies off on its own.
struct EnemyDraw {
    root: Entity,
    parts: Vec<EnemyPart>,
    broken: bool,
}

struct EnemyPart {
    posed: PosedMesh,
    entity: Entity,
    centre: Vec3,
    velocity: Vec3,
    axis: Vec3,
}

/// A slot in the pool of bolt visuals.
#[derive(Component)]
struct BoltVisual(usize);

/// Spawn the droids of the test area: their meshes, and one visual per possible bolt.
fn spawn_enemies(game: &mut Game, commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) {
    let Some(droid) = &game.data.droid else { return };
    // Placed by hand around the start of the test area, all facing it.
    for (x, z) in [(-3.0f32, 3.5f32), (3.0, 3.5), (0.0, 5.0), (8.0, -2.0), (-8.0, -3.0)] {
        game.combat.add_enemy(lsw_sim::Vec3::new(x, 0.0, z), lsw_sim::atan2d(-x, -z));
    }
    let joints = &droid.model.joints;
    let world = an3::bind_matrices(joints);
    let skin_mats = skin::skin_matrices(joints, &world);
    let handles = make_materials(&droid.model, &droid.textures, images, materials);
    for _ in 0..game.combat.enemies.len() {
        let root = commands.spawn((Transform::default(), Visibility::default())).id();
        let mut parts = Vec::new();
        for &layer in &droid.layers {
            for piece in &droid.model.layers[layer].pieces {
                for mesh in piece.meshes.iter().filter(|m| !m.indices.is_empty()) {
                    let draw = DrawMesh::new(piece, mesh, joints);
                    let (positions, normals) = draw.pose(&world, &skin_mats, None);
                    let handle = meshes.add(build_mesh(&draw, positions, normals));
                    let material = handles.get(draw.material).cloned().unwrap_or_default();
                    let entity = commands
                        .spawn((Mesh3d(handle.clone()), MeshMaterial3d(material), NoFrustumCulling, Transform::default(), ChildOf(root)))
                        .id();
                    parts.push(EnemyPart { posed: PosedMesh { draw, handle }, entity, centre: Vec3::ZERO, velocity: Vec3::ZERO, axis: Vec3::Y });
                }
            }
        }
        game.enemy_draw.push(EnemyDraw { root, parts, broken: false });
    }
    // Blaster bolts: a short glowing rod (this project's shape; the colour is the bolt type's red).
    let rod = meshes.add(Capsule3d::new(0.016, 0.2));
    let glow = materials.add(StandardMaterial { base_color: Color::LinearRgba(LinearRgba::rgb(9.0, 0.7, 0.5)), unlit: true, ..default() });
    for i in 0..24 {
        let entity = commands
            .spawn((BoltVisual(i), Mesh3d(rod.clone()), MeshMaterial3d(glow.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster))
            .id();
        game.bolt_entities.push(entity);
    }
}

/// Pose every droid from its clip, or throw its pieces about once it has been destroyed.
fn pose_enemies(mut game: ResMut<Game>, mut meshes: ResMut<Assets<Mesh>>, mut transforms: Query<&mut Transform>, mut visibility: Query<&mut Visibility>) {
    let game = &mut *game;
    let Some(droid) = &game.data.droid else { return };
    let joints = &droid.model.joints;
    let lights: &[lsw_sim::lighting::Light] = game.data.level.as_ref().map_or(&game.test_lights, |l| &l.lights);
    for (enemy, draw) in game.combat.enemies.iter().zip(game.enemy_draw.iter_mut()) {
        let rotation = Quat::from_rotation_y(-(lsw_sim::ang_to_radians(enemy.facing) + std::f32::consts::PI));
        if let Ok(mut root) = transforms.get_mut(draw.root) {
            root.translation = Vec3::new(enemy.pos.x, enemy.pos.y, -enemy.pos.z);
            root.rotation = rotation;
        }
        if enemy.alive {
            let world = match droid.animations.get(&enemy.anim.action) {
                Some(clip) => an3::world_matrices(joints, &clip.sample_nodes(enemy.anim.time)),
                None => an3::bind_matrices(joints),
            };
            let skin_mats = skin::skin_matrices(joints, &world);
            let middle = lsw_sim::Vec3::new(enemy.pos.x, enemy.pos.y + 0.21, enemy.pos.z);
            let lighting = lsw_sim::lighting::light_object(lights, middle, game.camera.yaw, game.camera.pitch).rotated_into(enemy.facing.wrapping_add(0x8000));
            for part in &draw.parts {
                let (positions, normals) = part.posed.draw.pose(&world, &skin_mats, None);
                let colours: Vec<[f32; 4]> = normals
                    .iter()
                    .map(|n| {
                        let c = lighting.shade(lsw_sim::Vec3::new(n[0], n[1], -n[2]));
                        [c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2), 1.0]
                    })
                    .collect();
                if let Some(mut mesh) = meshes.get_mut(&part.posed.handle) {
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
                }
            }
            continue;
        }
        // Destroyed: a battle droid falls to pieces. The pieces are the model's own parts; how
        // they fly (speeds, spin, gravity of -5) is this project's stand-in for the game's debris.
        if !draw.broken {
            draw.broken = true;
            let blow = rotation.inverse() * Vec3::new(enemy.blow.x, 0.0, -enemy.blow.z);
            for (i, part) in draw.parts.iter_mut().enumerate() {
                let positions = meshes.get(&part.posed.handle).and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION)).and_then(|a| a.as_float3()).map(|p| p.to_vec()).unwrap_or_default();
                let count = positions.len().max(1) as f32;
                part.centre = positions.iter().fold(Vec3::ZERO, |acc, p| acc + Vec3::from(*p) / count);
                let spread = ((i * 37 % 11) as f32 / 11.0, (i * 53 % 7) as f32 / 7.0);
                let outward = Vec3::new(part.centre.x, 0.0, part.centre.z).normalize_or_zero();
                part.velocity = outward * (0.3 + spread.0 * 0.5) + blow * (0.6 + spread.1 * 0.6) + Vec3::Y * (1.0 + spread.0);
                part.axis = Vec3::new(spread.0 - 0.5, 0.3, spread.1 - 0.5).normalize_or_zero();
            }
        }
        let t = enemy.dead_time;
        for part in &draw.parts {
            let mut offset = part.velocity * t + Vec3::new(0.0, -2.5 * t * t, 0.0);
            let floor = 0.03 - part.centre.y;
            let landed = offset.y < floor;
            if landed {
                // Time of touching down, to stop sliding and spinning from then on.
                let (a, b, c) = (-2.5, part.velocity.y, -floor);
                let land = (-b - (b * b - 4.0 * a * c).max(0.0).sqrt()) / (2.0 * a);
                offset = part.velocity * land;
                offset.y = floor;
            }
            let spin = if landed { 0.0 } else { t * 7.0 };
            let turn = Quat::from_axis_angle(if part.axis == Vec3::ZERO { Vec3::Y } else { part.axis }, spin);
            if let Ok(mut transform) = transforms.get_mut(part.entity) {
                transform.rotation = turn;
                transform.translation = part.centre + offset - turn * part.centre;
            }
            if let Ok(mut visible) = visibility.get_mut(part.entity) {
                // Pieces blink for the last second, then go.
                *visible = if t > 4.0 || (t > 3.0 && (t * 12.0) as i32 % 2 == 0) { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
    }
}

fn draw_bolts(game: Res<Game>, mut visuals: Query<(&BoltVisual, &mut Transform, &mut Visibility)>) {
    for (slot, mut transform, mut visible) in &mut visuals {
        match game.combat.bolts.get(slot.0) {
            Some(bolt) => {
                let direction = Vec3::new(bolt.dir.x, bolt.dir.y, -bolt.dir.z).normalize_or_zero();
                transform.translation = Vec3::new(bolt.pos.x, bolt.pos.y, -bolt.pos.z);
                transform.rotation = Quat::from_rotation_arc(Vec3::Y, if direction == Vec3::ZERO { Vec3::Y } else { direction });
                *visible = Visibility::Inherited;
            }
            None => *visible = Visibility::Hidden,
        }
    }
}

/// Turn one of the game's prop models into drawable meshes on the HUD layer.
fn spawn_hud_piece(
    commands: &mut Commands,
    scene: &(tt_formats::ghg::Model, Vec<Option<tt_formats::atc::Texture>>),
    name: &str,
    transform: Transform,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Option<(Entity, Vec<Handle<StandardMaterial>>)> {
    let (model, textures) = scene;
    let mut handles = Vec::new();
    let piece = model.specials.iter().find(|p| p.name.eq_ignore_ascii_case(name))?;
    let parent = commands.spawn((transform, Visibility::default(), bevy::camera::visibility::RenderLayers::layer(1))).id();
    // HUD pieces are lit from the viewer's side. This fixed light is this project's estimate.
    let light = lsw_sim::lighting::ObjectLight {
        ambient: [0.6; 3],
        directional: [
            (lsw_sim::Vec3::new(-0.33, 0.5, -0.8), [0.5; 3]),
            (lsw_sim::Vec3::ZERO, [0.0; 3]),
            (lsw_sim::Vec3::ZERO, [0.0; 3]),
        ],
    };
    for source in &piece.meshes {
        let Some(material) = model.materials.get(source.material) else { continue };
        let texture = material.diffuse.and_then(|i| textures.get(i)).and_then(|t| t.as_ref());
        let has_alpha = texture.is_some_and(|t| t.has_alpha);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, source.vertices.iter().map(|v| [v.position[0], v.position[1], -v.position[2]]).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, source.vertices.iter().map(|v| [v.normal[0], v.normal[1], -v.normal[2]]).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, source.vertices.iter().map(|v| v.uv).collect::<Vec<_>>());
        let colours: Vec<[f32; 4]> = source
            .vertices
            .iter()
            .map(|v| {
                let lit = if source.has_normals { light.shade(lsw_sim::Vec3::new(v.normal[0], v.normal[1], v.normal[2])) } else { [1.0; 3] };
                let c = [0, 1, 2].map(|k| (v.colour[k] as f32 / 127.0 * lit[k]).clamp(0.0, 2.0).powf(2.2));
                [c[0], c[1], c[2], 1.0]
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.insert_indices(Indices::U32(source.indices.clone()));
        let blended = has_alpha || (material.alpha_mode != 0 && material.opacity < 0.999);
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgba(material.colour[0], material.colour[1], material.colour[2], material.opacity.clamp(0.0, 1.0)),
            base_color_texture: texture.map(|t| images.add(make_image(t))),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            alpha_mode: if blended { AlphaMode::Blend } else { AlphaMode::Opaque },
            ..default()
        });
        handles.push(handle.clone());
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(handle),
            NoFrustumCulling,
            bevy::light::NotShadowCaster,
            bevy::camera::visibility::RenderLayers::layer(1),
            ChildOf(parent),
        ));
    }
    Some((parent, handles))
}

/// The stud icon's spin: its material steps through the game's 32 frames.
#[derive(Resource)]
struct CoinSpin {
    frames: Vec<Handle<Image>>,
    materials: Vec<Handle<StandardMaterial>>,
}

fn spin_coin(time: Res<Time>, spin: Option<Res<CoinSpin>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(spin) = spin else { return };
    if spin.frames.is_empty() {
        return;
    }
    // The script advances one frame every second game tick; 60 ticks a second is assumed.
    let frame = (time.elapsed_secs() * 30.0) as usize % spin.frames.len();
    for handle in &spin.materials {
        if let Some(mut material) = materials.get_mut(handle) {
            if material.base_color_texture.as_ref() != Some(&spin.frames[frame]) {
                material.base_color_texture = Some(spin.frames[frame].clone());
            }
        }
    }
}

/// The in-game HUD, drawn the way the game does: small 3D models from the shared props scene,
/// placed in front of a camera at fixed panel positions, plus the stud count in the game font.
fn spawn_hud(
    commands: &mut Commands,
    data: &GameData,
    hearts: u32,
    fov: f32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let aspect = 1280.0 / 720.0;
    // The game's panel scale: positions are multiplied by these and placed one unit ahead.
    let panel_fov = fov / 0.75;
    let mul_y = panel_fov / ((1.0 - panel_fov) * 0.22 + 2.545);
    let mul_x = aspect * mul_y;
    commands.spawn((
        HudCamera,
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
        Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Projection::Perspective(PerspectiveProjection { fov, near: 0.05, far: 10.0, ..default() }),
        Transform::default(),
        bevy::camera::visibility::RenderLayers::layer(1),
    ));
    let place = |x: f32, y: f32, z: f32, scale: f32| Transform::from_xyz(x * mul_x, y * mul_y, -z).with_scale(Vec3::splat(scale));
    if let Some(things) = &data.hud.things {
        spawn_hud_piece(commands, things, "icon_back_blue", place(-PANEL_ICON_X, PANEL_STATS_Y, 1.0, PANEL_ICON_SIZE), meshes, materials, images);
        let coin = spawn_hud_piece(
            commands,
            things,
            "silver_pan_coin1",
            place(-PANEL_COIN_X, PANEL_STATS_Y + PANEL_COIN_Y, 1.0, PANEL_COIN_SCALE),
            meshes,
            materials,
            images,
        );
        if let Some((_, coin_materials)) = coin {
            let frames = data.hud.coin_frames.iter().map(|t| images.add(make_image(t))).collect();
            commands.insert_resource(CoinSpin { frames, materials: coin_materials });
        }
        // Hearts step right by 0.35 of their scale, tightened to 0.85 of that on widescreen.
        for i in 0..hearts {
            let x = -PANEL_HEARTS_X + i as f32 * PANEL_HEART_SCALE * 0.35 * 0.85;
            let transform = place(x, PANEL_STATS_Y + PANEL_HEART_Y, 1.001 - i as f32 * 0.0005, PANEL_HEART_SCALE);
            if let Some((entity, _)) = spawn_hud_piece(commands, things, "flat_heart", transform, meshes, materials, images) {
                commands.entity(entity).insert(HudHeart { index: i, scale: PANEL_HEART_SCALE });
            }
        }
    }
    if let Some(portrait) = &data.hud.portrait {
        // The portrait is the scene's object named after the icon itself (the other is a variant).
        let name = data.character.config.str("icon").unwrap_or("");
        spawn_hud_piece(commands, portrait, name, place(-PANEL_ICON_X, PANEL_STATS_Y, 0.998, PANEL_ICON_SIZE), meshes, materials, images);
    }
    // Stud count, in the game font, at the game's score position. The glyph size on screen is
    // an estimate (the text routine's scale-to-size rule was not traced).
    if let Some(font) = &data.hud.font {
        let half_height = (fov * 0.5).tan();
        let left = 50.0 + 50.0 * (-PANEL_SCORE_X * mul_x) / (half_height * aspect);
        let top = 50.0 - 50.0 * ((PANEL_STATS_Y + PANEL_COIN_Y) * mul_y) / half_height;
        let size = 720.0 * 0.045;
        let digits = commands
            .spawn((
                StudDigits,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(left),
                    top: Val::Percent(top),
                    margin: UiRect::top(Val::Px(-size * 0.5)),
                    ..default()
                },
            ))
            .id();
        let hud_font = HudFont { image: images.add(make_image(&font.texture)), height: font.height, glyphs: font.glyphs.clone() };
        spawn_glyphs(commands, &hud_font, digits, "0", size, Color::srgb(1.0, 0.75, 0.0));
        // The arrow the game floats over a player that scenery hides from the camera: the
        // font's own up-arrow glyph (character 0xAC) in player one's colour.
        if hud_font.glyphs.contains_key(&HIDDEN_ARROW) {
            let marker = commands
                .spawn((HiddenMarker, Visibility::Hidden, Node { position_type: PositionType::Absolute, ..default() }))
                .id();
            // The font lists this glyph as 12 wide, but the glyphs are packed about 39 apart
            // on that row and the matching down arrow (0xA3) is 38 wide, so the picture is
            // taken to be as wide as the down arrow's. Inferred, not from the game's code.
            let [right, top, listed] = hud_font.glyphs[&HIDDEN_ARROW];
            let width = hud_font.glyphs.get(&'\u{a3}').map_or(listed, |g| g[2]);
            commands.spawn((
                ImageNode {
                    image: hud_font.image.clone(),
                    rect: Some(Rect::new(right - width, top, right, top + font.height)),
                    color: Color::srgb_u8(0, 127, 255),
                    ..default()
                },
                Node { width: Val::Px(width * HIDDEN_ARROW_SIZE / font.height), height: Val::Px(HIDDEN_ARROW_SIZE), ..default() },
                ChildOf(marker),
            ));
        } else {
            println!("hud: the font has no up-arrow glyph, so the hidden-player marker is not drawn");
        }
        commands.insert_resource(hud_font);
    }
}

/// The game's up-arrow character.
const HIDDEN_ARROW: char = '\u{ac}';
/// Size on screen in pixels: an estimate (the game gives a text scale of 3, not traced to pixels).
const HIDDEN_ARROW_SIZE: f32 = 44.0;

#[derive(Component)]
struct HiddenMarker;

/// Float the arrow over the character's head while scenery hides them from the camera. Its
/// opacity pulses twice a second between 32/128 and full, as in the game. Placing it at the
/// top of the head is an estimate.
fn place_hidden_marker(
    game: Res<Game>,
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<HudCamera>)>,
    mut markers: Query<(&mut Node, &mut Visibility, &Children), With<HiddenMarker>>,
    mut glyphs: Query<(&mut ImageNode, &Node), Without<HiddenMarker>>,
) {
    let p = game.player.pos;
    let head = Vec3::new(p.x, p.y + game.player.stats.height, -p.z);
    let spot = cameras.iter().next().and_then(|(camera, transform)| camera.world_to_viewport(transform, head).ok());
    let phase = (time.elapsed_secs() % 0.5) * 2.0;
    let alpha = ((80.0 + 48.0 * (phase * std::f32::consts::TAU).sin()) / 128.0).clamp(0.0, 1.0);
    for (mut node, mut visibility, children) in &mut markers {
        let Some(spot) = spot.filter(|_| game.hidden.hidden) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        let mut width = 0.0;
        for child in children.iter() {
            if let Ok((mut image, glyph)) = glyphs.get_mut(child) {
                image.color.set_alpha(alpha);
                if let Val::Px(w) = glyph.width {
                    width += w;
                }
            }
        }
        node.left = Val::Px(spot.x - width * 0.5);
        node.top = Val::Px(spot.y - HIDDEN_ARROW_SIZE);
    }
}

/// The last full heart beats twice a second, as in the game.
fn beat_hearts(game: Res<Game>, time: Res<Time>, mut hearts: Query<(&HudHeart, &mut Transform, &mut Visibility)>) {
    let phase = (time.elapsed_secs() % 0.5) * 2.0;
    let left = game.player.hit_points;
    for (heart, mut transform, mut visible) in &mut hearts {
        // Lost hearts are simply not shown (the game draws them faded).
        *visible = if heart.index < left { Visibility::Inherited } else { Visibility::Hidden };
        // With one heart left the beat is stronger.
        let pulse = if heart.index + 1 == left { (if left == 1 { 0.5 } else { 0.2 }) * (1.0 - phase) + 1.0 } else { 1.0 };
        transform.scale = Vec3::new(heart.scale * pulse, heart.scale * pulse, heart.scale);
    }
}
fn ground_image() -> Image {
    // A plain tiled floor, one tile per game unit, so distances can be judged by eye.
    let size = 128usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let edge = x < 2 || y < 2;
            let shade = if edge { 92 } else if (x / 64 + y / 64) % 2 == 0 { 138 } else { 128 };
            data.extend_from_slice(&[shade, shade, (shade as f32 * 0.97) as u8, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Level scenery: every placed mesh baked into world space and merged per material. Levels
/// carry their lighting in vertex colours (0x7F is full brightness), so they are drawn unlit.
fn spawn_level(
    commands: &mut Commands,
    level: &data::LevelData,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let model = &level.model;
    let textures: Vec<Option<(Handle<Image>, bool)>> =
        level.textures.iter().map(|t| t.as_ref().map(|t| (images.add(make_image(t)), t.has_alpha))).collect();
    struct Batch {
        positions: Vec<[f32; 3]>,
        colours: Vec<[f32; 4]>,
        normals: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
    }
    // Pieces of Force objects are kept as separate movable groups (key: object, piece);
    // everything else is merged per material (key: none).
    let force_piece: HashMap<String, usize> = level
        .force_objects
        .iter()
        .enumerate()
        .flat_map(|(i, o)| o.pieces.iter().map(move |p| (p.to_ascii_lowercase(), i)))
        .collect();
    let mut batches: HashMap<(Option<(usize, usize)>, usize), Batch> = HashMap::new();
    let to_linear = |c: f32| c.clamp(0.0, 2.0).powf(2.2);
    for placed in &model.statics {
        // Scenery with baked lighting has no normals or carries the extra baked-light stream
        // (descriptor bit 0x4000000). Everything else is a free-standing LEGO piece that the
        // game lights like a character, from the light table at the piece's position.
        let descriptor = model.materials.get(placed.mesh.material).map_or(0, |m| m.vertex_descriptor);
        let lit = placed.mesh.has_normals && descriptor & 0x0400_0000 == 0;
        let lighting = lit.then(|| {
            let mut centre = [0.0f32; 3];
            for v in &placed.mesh.vertices {
                let p = tt_formats::ghg::transform_point(&placed.matrix, v.position);
                for k in 0..3 {
                    centre[k] += p[k] / placed.mesh.vertices.len() as f32;
                }
            }
            // Camera-relative lights are frozen at a typical view for these fixed pieces.
            lsw_sim::lighting::light_object(&level.lights, lsw_sim::Vec3::new(centre[0], centre[1], centre[2]), 0, 0x1000)
        });
        // Collision helper objects (named "terr...") are not drawn. The visibility byte is
        // not a safe test: it is also 0 on real scenery such as walls.
        if placed.name.to_ascii_lowercase().starts_with("terr") {
            continue;
        }
        let group = force_piece.get(&placed.name.to_ascii_lowercase()).map(|&object| (object, placed.object));
        let batch = batches
            .entry((group, placed.mesh.material))
            .or_insert_with(|| Batch { positions: Vec::new(), colours: Vec::new(), normals: Vec::new(), uvs: Vec::new(), indices: Vec::new() });
        let base = batch.positions.len() as u32;
        for v in &placed.mesh.vertices {
            let p = tt_formats::ghg::transform_point(&placed.matrix, v.position);
            batch.positions.push([p[0], p[1], -p[2]]);
            let n = tt_formats::ghg::transform_dir(&placed.matrix, v.normal);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let light = match &lighting {
                Some(l) if len > 1e-6 => l.shade(lsw_sim::Vec3::new(n[0] / len, n[1] / len, n[2] / len)),
                _ => [1.0; 3],
            };
            let c = [0, 1, 2].map(|k| to_linear(v.colour[k] as f32 / 127.0 * light[k]));
            batch.colours.push([c[0], c[1], c[2], (v.colour[3] as f32 / 127.0).min(1.0)]);
            batch.uvs.push(v.uv);
            batch.normals.push(if placed.mesh.has_normals && len > 1e-6 { [n[0] / len, n[1] / len, -n[2] / len] } else { [0.0, 1.0, 0.0] });
        }
        batch.indices.extend(placed.mesh.indices.iter().map(|i| i + base));
    }
    for ((group, material_index), mut batch) in batches {
        // A Force piece moves away from the middle of its object, sideways and a little up.
        let force = group.map(|(object, _)| {
            let centre = level.force_objects[object].centre;
            let count = batch.positions.len().max(1) as f32;
            let mean = batch.positions.iter().fold([0.0f32; 3], |acc, p| [acc[0] + p[0] / count, acc[1] + p[1] / count, acc[2] + p[2] / count]);
            // Renderer z is the game's -z.
            let side = mean[2] + centre.z;
            let lift = (mean[1] - centre.y).max(0.0);
            ForcePiece { object, offset: Vec3::new(0.0, 0.25 + lift * 0.6, if side >= 0.0 { 0.75 } else { -0.75 }) }
        });
        let Some(material) = model.materials.get(material_index) else { continue };
        let texture = material.diffuse.and_then(|i| textures.get(i).cloned().flatten());
        let has_alpha = texture.as_ref().is_some_and(|t| t.1);
        // Glossy materials reflect an environment map in the game. That is not drawn yet; as a
        // stand-in the map's average colour is added to the base colour so these surfaces are
        // not left at their dark base (the half strength is an estimate).
        let mut colour = material.colour;
        if let Some(env) = material.env_map.and_then(|i| level.textures.get(i)).and_then(|t| t.as_ref()) {
            let pixels = (env.width * env.height) as usize;
            let mut sum = [0.0f32; 3];
            for px in env.rgba[..pixels * 4].chunks_exact(4) {
                for k in 0..3 {
                    sum[k] += px[k] as f32 / 255.0;
                }
            }
            for k in 0..3 {
                colour[k] += 0.5 * sum[k] / pixels.max(1) as f32;
            }
        }
        let blended = material.alpha_mode != 0 && (material.opacity < 0.999 || has_alpha || batch.colours.iter().any(|c| c[3] < 0.99));
        if !blended {
            for colour in &mut batch.colours {
                colour[3] = 1.0;
            }
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, batch.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, batch.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, batch.colours);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, batch.uvs);
        mesh.insert_indices(Indices::U32(batch.indices));
        let mut entity = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(colour[0], colour[1], colour[2], material.opacity.clamp(0.0, 1.0)),
                base_color_texture: texture.map(|t| t.0),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                // See-through overlays (soft shadows, glass) blend; cut-out textures are masked.
                alpha_mode: if blended { AlphaMode::Blend } else if has_alpha { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque },
                depth_bias: if blended { 4.0 } else { 0.0 },
                ..default()
            })),
            NoFrustumCulling,
            bevy::light::NotShadowCaster,
        ));
        if let Some(force) = force {
            entity.insert((force, Transform::default()));
        }
    }
}

/// A scene piece that a Force object moves. `offset` is where it ends up, relative to its
/// starting place. (The game stores the real motion; it is not decoded, so this is a stand-in.)
#[derive(Component)]
struct ForcePiece {
    object: usize,
    offset: Vec3,
}

fn move_force_pieces(game: Res<Game>, time: Res<Time>, mut pieces: Query<(&ForcePiece, &mut Transform)>) {
    let Some(level) = &game.data.level else { return };
    for (piece, mut transform) in &mut pieces {
        let Some(object) = level.force_objects.get(piece.object) else { continue };
        // While the Force is held the bricks tremble; once done they drift aside.
        let eased = object.open * object.open * (3.0 - 2.0 * object.open);
        let shake = if object.held > 0.0 && !object.done {
            let t = time.elapsed_secs() * 31.0 + piece.offset.y * 40.0;
            Vec3::new(t.sin(), (t * 1.3).cos(), (t * 0.7).sin()) * 0.008 * (object.held / FORCE_HOLD_SECONDS).min(1.0)
        } else {
            Vec3::ZERO
        };
        transform.translation = piece.offset * eased + shake;
    }
}

/// How close to a Force object the player must be, and how long the button is held. Both are
/// estimates: the game's own values were not traced.
const FORCE_RANGE: f32 = 2.0;
const FORCE_HOLD_SECONDS: f32 = 1.0;

/// Draw a plain model level: one mesh per material, textures straight from their image files,
/// shown at full brightness (the model carries no lighting).
fn spawn_model_level(
    commands: &mut Commands,
    scene: &obj::ObjScene,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    let mut loaded: HashMap<std::path::PathBuf, Option<(Handle<Image>, bool)>> = HashMap::new();
    // Untextured batches (the built-in test area) use this project's tile image, tinted.
    let mut tile_image = ground_image();
    add_mips(&mut tile_image);
    let tile = images.add(tile_image);
    for batch in &scene.batches {
        if batch.indices.is_empty() {
            continue;
        }
        let texture = batch.texture.as_ref().and_then(|path| {
            loaded
                .entry(path.clone())
                .or_insert_with(|| {
                    let bytes = std::fs::read(path).ok()?;
                    let image = Image::from_buffer(
                        &bytes,
                        bevy::image::ImageType::Extension("png"),
                        bevy::image::CompressedImageFormats::NONE,
                        true,
                        sampler.clone(),
                        RenderAssetUsages::RENDER_WORLD,
                    )
                    .ok()?;
                    let mut image = image;
                    let has_alpha = image.data.as_ref().is_some_and(|d| d.chunks_exact(4).any(|p| p[3] < 128));
                    add_mips(&mut image);
                    Some((images.add(image), has_alpha))
                })
                .clone()
        });
        let has_alpha = texture.as_ref().is_some_and(|t| t.1);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, batch.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, batch.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, batch.uvs.clone());
        mesh.insert_indices(Indices::U32(batch.indices.clone()));
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                // The tile image is mid-grey, so flat tints are lifted to stay readable.
                base_color: if batch.texture.is_none() {
                    Color::srgb((batch.colour[0] * 1.6).min(1.0), (batch.colour[1] * 1.6).min(1.0), (batch.colour[2] * 1.6).min(1.0))
                } else {
                    Color::srgb(batch.colour[0], batch.colour[1], batch.colour[2])
                },
                base_color_texture: Some(texture.map_or_else(|| tile.clone(), |t| t.0)),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                alpha_mode: if has_alpha { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque },
                ..default()
            })),
            NoFrustumCulling,
            bevy::light::NotShadowCaster,
        ));
    }
}

/// Give an 8-bit RGBA image the chain of half-size copies that stops it sparkling when it is
/// drawn small. Each level averages 2x2 pixels of the one before.
fn add_mips(image: &mut Image) {
    let (mut w, mut h) = (image.texture_descriptor.size.width as usize, image.texture_descriptor.size.height as usize);
    let Some(base) = image.data.clone() else { return };
    if base.len() != w * h * 4 || !matches!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm) {
        return;
    }
    let mut all = base.clone();
    let mut level = base;
    let mut count = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                        sum += level[(sy * w + sx) * 4 + c] as u32;
                    }
                    next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        all.extend_from_slice(&next);
        level = next;
        (w, h) = (nw, nh);
        count += 1;
    }
    image.data = Some(all);
    image.texture_descriptor.mip_level_count = count;
}

fn setup(
    mut commands: Commands,
    mut game: ResMut<Game>,
    mut sounds: ResMut<Sounds>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut audio: ResMut<Assets<AudioSource>>,
) {
    let game = &mut *game;
    let character = &game.data.character;
    let joints = &character.model.joints;
    let world = an3::bind_matrices(joints);
    let skin_mats = skin::skin_matrices(joints, &world);
    let handles = make_materials(&character.model, &character.textures, &mut images, &mut materials);

    let root = commands.spawn((CharacterRoot, Transform::default(), Visibility::default())).id();
    let mirror = game
        .data
        .level
        .as_ref()
        .is_some_and(|l| l.reflective)
        .then(|| commands.spawn((MirrorRoot, Transform::default(), Visibility::default())).id());
    for &layer in &character.layers {
        for piece in &character.model.layers[layer].pieces {
            for mesh in &piece.meshes {
                if mesh.indices.is_empty() {
                    continue;
                }
                let draw = DrawMesh::new(piece, mesh, joints);
                let (positions, normals) = draw.pose(&world, &skin_mats, None);
                let handle = meshes.add(build_mesh(&draw, positions, normals));
                let material = handles.get(draw.material).cloned().unwrap_or_default();
                if let Some(mirror) = mirror {
                    commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material.clone()), NoFrustumCulling, ChildOf(mirror)));
                }
                commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material), NoFrustumCulling, ChildOf(root)));
                game.meshes.push(PosedMesh { draw, handle });
            }
        }
    }

    // The saber: a hilt and a blade hanging off the weapon locator of the right hand.
    if let Some(saber) = &game.data.saber {
        let locator_index = character.config.f32("weapon_locator").unwrap_or(0.0) as usize;
        if let Some(Some(locator)) = character.model.locators.get(locator_index) {
            let saber_handles = make_materials(&saber.model, &saber.textures, &mut images, &mut materials);
            let piece = &saber.model.layers[saber.piece.0].pieces[saber.piece.1];
            let colour = blade_colour(character.config.str("streak").unwrap_or("green"));
            // A pale core with the blade colour as a wide additive glow, as in the game.
            let blade_material = materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::rgb(1.0 + colour.red * 0.4, 1.0 + colour.green * 0.4, 1.0 + colour.blue * 0.4)),
                unlit: true,
                ..default()
            });
            let glow_material = materials.add(StandardMaterial {
                base_color: Color::LinearRgba(LinearRgba::new(colour.red * 0.16, colour.green * 0.16, colour.blue * 0.16, 1.0)),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            let attach = mul(&locator.matrix, &world[locator.joint]);
            let mut draw =
                SaberDraw { hilt: Vec::new(), blade: Vec::new(), glow: Vec::new(), centre: [0.0; 3], blade_entities: Vec::new(), axis: 1, base: 0.0, locator: locator.matrix, locator_joint: locator.joint };
            let mut hilt_centre = [0.0f32; 3];
            for mesh in &piece.meshes {
                let is_blade = saber.model.materials.get(mesh.material).is_some_and(|m| m.diffuse.is_none());
                let posed = DrawMesh::new(piece, mesh, &saber.model.joints);
                let (positions, normals) = posed.pose(&world, &skin_mats, Some(&attach));
                let handle = meshes.add(build_mesh(&posed, positions, normals));
                let material =
                    if is_blade { blade_material.clone() } else { saber_handles.get(mesh.material).cloned().unwrap_or_default() };
                if let (Some(mirror), false) = (mirror, is_blade) {
                    commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material.clone()), NoFrustumCulling, ChildOf(mirror)));
                }
                let entity =
                    commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material), NoFrustumCulling, ChildOf(root))).id();
                let (lo, hi) = posed.rest_bounds();
                if is_blade {
                    let glow_draw = DrawMesh::new(piece, mesh, &saber.model.joints);
                    let (gp, gn) = glow_draw.pose(&world, &skin_mats, Some(&attach));
                    let glow_handle = meshes.add(build_mesh(&glow_draw, gp, gn));
                    let glow_entity = commands
                        .spawn((Mesh3d(glow_handle.clone()), MeshMaterial3d(glow_material.clone()), NoFrustumCulling, ChildOf(root)))
                        .id();
                    draw.blade_entities.push(glow_entity);
                    draw.glow.push(PosedMesh { draw: glow_draw, handle: glow_handle });
                    draw.centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
                    draw.axis = (0..3).max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b]))).unwrap_or(1);
                    draw.base = lo[draw.axis];
                    draw.blade_entities.push(entity);
                    draw.blade.push(PosedMesh { draw: posed, handle });
                } else {
                    hilt_centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
                    draw.hilt.push(PosedMesh { draw: posed, handle });
                }
            }
            // The blade grows from whichever of its ends is nearer the hilt.
            if let Some(blade) = draw.blade.first() {
                let (lo, hi) = blade.draw.rest_bounds();
                let k = draw.axis;
                draw.base = if (hilt_centre[k] - lo[k]).abs() < (hilt_centre[k] - hi[k]).abs() { lo[k] } else { hi[k] };
            }
            game.saber = Some(draw);
        }
    }

    if let Some(level) = &game.data.level {
        spawn_level(&mut commands, level, &mut meshes, &mut materials, &mut images);
    }
    if let Some(custom) = &game.data.custom {
        spawn_model_level(&mut commands, &custom.scene, &mut meshes, &mut materials, &mut images);
    }
    // A flat test floor (this project's own art, not from the game), when no level is loaded.
    let ground_size = 120.0;
    if game.data.level.is_none() && game.data.custom.is_none() {
        commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(ground_size, ground_size))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(images.add(ground_image())),
            unlit: true,
            uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(ground_size / 2.0)),
            ..default()
        })),
        ));
    }


    commands.spawn((
        Camera3d::default(),
        Hdr,
        // The game multiplies texture by light and shows the result as is.
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        // Glow only from things brighter than white (the saber blade), so it cannot haze the scene.
        Bloom {
            intensity: 0.12,
            prefilter: bevy::post_process::bloom::BloomPrefilter { threshold: 1.2, threshold_softness: 0.1 },
            composite_mode: bevy::post_process::bloom::BloomCompositeMode::Additive,
            ..Bloom::NATURAL
        },
        Projection::Perspective(PerspectiveProjection { fov: game.camera.rail.fov, near: 0.05, far: 3000.0, ..default() }),
        Transform::default(),
    ));

    // Blob shadow: the level file asks for alpha 80 of 255.
    commands.spawn((
        BlobShadow,
        Mesh3d(meshes.add(Circle::new(1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.0, 0.0, 0.0, 80.0 / 255.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::default(),
        bevy::light::NotShadowCaster,
    ));

    // Debug read-out and controls, bottom-left (F1 hides it).
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(8.0), left: Val::Px(10.0), ..default() },
    ));
    if game.spawn_enemies {
        spawn_enemies(game, &mut commands, &mut meshes, &mut materials, &mut images);
    }
    spawn_hud(&mut commands, &game.data, game.player.stats.hit_points, game.camera.rail.fov, &mut meshes, &mut materials, &mut images);

    for (name, bytes) in &game.data.sounds {
        sounds.clips.insert(name, audio.add(AudioSource { bytes: bytes.clone().into() }));
    }
}

fn read_input(
    mut game: ResMut<Game>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
) {
    let game = &mut *game;
    let (mut x, mut z, mut jump, mut action, mut special);
    if let Some(demo) = &game.demo {
        (x, z, jump, action, special) = demo.input(game.clock);
        // Demo only, in a model level: fly up and forwards for two seconds, then drop.
        if game.data.custom.is_some() {
            game.fly = (22.0..24.0).contains(&game.clock);
            if game.fly {
                game.fly_vertical = 1.0;
                game.fly_boost = false;
                z = 1.0;
            }
        }
        // Demo only: stand in front of the first Force door, then walk straight at it.
        if let Some(door) = game.data.level.as_ref().and_then(|l| l.force_objects.first()) {
            let start = game.data.level.as_ref().and_then(|l| l.world.start).unwrap_or(lsw_sim::Vec3::ZERO);
            let (dx, dz) = (door.centre.x - start.x, door.centre.z - start.z);
            let len = (dx * dx + dz * dz).sqrt().max(1e-6);
            if (21.9..21.95).contains(&game.clock) {
                game.player.pos = lsw_sim::Vec3::new(door.centre.x - dx / len * 1.0, start.y, door.centre.z - dz / len * 1.0);
                game.player.vel = lsw_sim::Vec3::ZERO;
            }
            if (24.2..27.5).contains(&game.clock) {
                let heading = lsw_sim::atan2d(dx, dz).wrapping_sub(game.camera.yaw);
                x = lsw_sim::ang_sin(heading);
                z = lsw_sim::ang_cos(heading);
            }
        }
    } else {
        let key = |codes: &[KeyCode]| codes.iter().any(|&c| keys.pressed(c));
        x = key(&[KeyCode::KeyD, KeyCode::ArrowRight]) as i32 as f32 - key(&[KeyCode::KeyA, KeyCode::ArrowLeft]) as i32 as f32;
        z = key(&[KeyCode::KeyW, KeyCode::ArrowUp]) as i32 as f32 - key(&[KeyCode::KeyS, KeyCode::ArrowDown]) as i32 as f32;
        // Keys are all-or-nothing, so two modifiers stand in for a half-pushed stick.
        let scale = if key(&[KeyCode::ControlLeft, KeyCode::ControlRight]) {
            0.4
        } else if key(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            0.7
        } else {
            1.0
        };
        x *= scale;
        z *= scale;
        jump = keys.pressed(KeyCode::Space);
        action = key(&[KeyCode::KeyJ, KeyCode::Enter]) || mouse.pressed(MouseButton::Left);
        special = key(&[KeyCode::KeyK, KeyCode::KeyE]) || mouse.pressed(MouseButton::Right);
        // Free flight: F (or the top face button) toggles it. While flying, Space rises,
        // Ctrl or C sinks and Shift goes faster, so those keys stop being walk/tiptoe.
        let toggle = keys.just_pressed(KeyCode::KeyF) || gamepads.iter().any(|p| p.just_pressed(GamepadButton::North));
        if toggle {
            game.fly = !game.fly;
        }
        if game.fly {
            if scale != 1.0 {
                x /= scale;
                z /= scale;
            }
            game.fly_vertical = keys.pressed(KeyCode::Space) as i32 as f32 - key(&[KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::KeyC]) as i32 as f32;
            game.fly_boost = key(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            for pad in &gamepads {
                game.fly_vertical += pad.pressed(GamepadButton::South) as i32 as f32 - pad.pressed(GamepadButton::East) as i32 as f32;
                game.fly_boost |= pad.pressed(GamepadButton::RightTrigger);
            }
        }
        for pad in &gamepads {
            let stick = pad.left_stick();
            if stick.length_squared() > x * x + z * z {
                x = stick.x;
                z = stick.y;
            }
            jump |= pad.pressed(GamepadButton::South);
            action |= pad.pressed(GamepadButton::West);
            special |= pad.pressed(GamepadButton::East);
        }
    }
    game.input = Input {
        stick_x: x,
        stick_z: z,
        jump_pressed: jump && !game.held.0,
        jump_held: jump,
        action_pressed: action && !game.held.1,
        action_held: action,
        special_pressed: special && !game.held.2,
        incoming_bolt: None,
    };
    game.held = (jump, action, special);
}

fn simulate(mut commands: Commands, mut game: ResMut<Game>, mut sounds: ResMut<Sounds>, time: Res<Time>) {
    let game = &mut *game;
    let dt = if game.demo.is_some() { DEMO_DT } else { time.delta_secs().min(1.0 / 20.0) };
    if dt <= 0.0 {
        return;
    }
    game.clock += dt;
    let mut input = game.input;
    // The Force: holding the special button near a Force object works on it instead of
    // putting the saber away. When the hold completes its blocker's collision is removed.
    let special_held = game.held.2;
    let feet = game.player.pos;
    let mut completed = Vec::new();
    if let Some(level) = game.data.level.as_mut() {
        for object in &mut level.force_objects {
            let near = !object.done && feet.distance(object.centre) < FORCE_RANGE;
            if near && special_held && game.player.on_ground {
                input.special_pressed = false;
                object.held += dt;
                if object.held >= FORCE_HOLD_SECONDS {
                    object.done = true;
                    completed.push(object.name.clone());
                    for &blocker in &object.blockers {
                        level.world.remove_object(blocker);
                    }
                }
            } else if !object.done {
                object.held = (object.held - dt * 2.0).max(0.0);
            }
            if object.done {
                object.open = (object.open + dt / 0.8).min(1.0);
            }
        }
    }
    for name in completed {
        if let Some(file) = game.log.as_mut() {
            let _ = writeln!(file, "{:8.3} event ForceComplete({name})", game.clock);
        }
    }
    // A bolt about to arrive (the game's test) is what turns the attack button into a block.
    let reach = (game.player.stats.height * 0.5).max(game.player.stats.radius);
    let middle = lsw_sim::Vec3::new(game.player.pos.x, game.player.pos.y + game.player.stats.height * 0.5, game.player.pos.z);
    input.incoming_bolt = game.combat.incoming(middle, reach);
    game.combat.events.clear();
    let world = game.data.world();
    if game.fly {
        // Free flight is this project's own: straight-line movement relative to the camera,
        // ignoring gravity and collision. The character's rules are not stepped.
        let speed = if game.fly_boost { 40.0 } else { 12.0 };
        let (x, z) = (input.stick_x, input.stick_z);
        let length = (x * x + z * z).sqrt();
        let player = &mut game.player;
        if length > 0.2 {
            let heading = lsw_sim::atan2d(x, z).wrapping_add(game.camera.yaw);
            player.pos.x += lsw_sim::ang_sin(heading) * speed * dt;
            player.pos.z += lsw_sim::ang_cos(heading) * speed * dt;
            player.facing = heading;
            player.move_facing = heading;
        }
        player.pos.y += game.fly_vertical.clamp(-1.0, 1.0) * speed * 0.6 * dt;
        player.vel = lsw_sim::Vec3::ZERO;
        player.on_ground = false;
        player.events.clear();
    } else {
        game.player.step_in(&input, game.camera.yaw, &game.data.character.clips, world, dt);
    }
    let mut goal = world.and_then(|w| w.camera_goal(game.player.pos, &mut game.sock));
    if game.chase {
        // A model level has no camera rails, so a simple chase camera stays behind the
        // character (this project's own, not the game's).
        let (p, f) = (game.player.pos, game.player.facing);
        let focus = lsw_sim::Vec3::new(p.x, p.y + game.player.stats.height * 0.5, p.z);
        let mut wanted = lsw_sim::Vec3::new(p.x - lsw_sim::ang_sin(f) * 2.4, p.y + 0.95, p.z - lsw_sim::ang_cos(f) * 2.4);
        // Keep the camera on the character's side of any wall or roof in between.
        if let Some(hit) = world.and_then(|w| w.segment_hit(focus, wanted)) {
            let keep = (hit * 0.85).max(0.08);
            wanted = lsw_sim::Vec3::new(focus.x + (wanted.x - focus.x) * keep, focus.y + (wanted.y - focus.y) * keep, focus.z + (wanted.z - focus.z) * keep);
        }
        goal = Some((wanted, 3.0));
    } else if let (None, Some(world)) = (goal, world) {
        // No rails here (the test area): the stand-in rail, lifted clear of the ground under
        // the camera by the game's height-above-terrain rule.
        goal = Some(game.camera.stand_in_goal(game.player.pos, game.player.stats.height, world));
    }
    game.camera.update_towards(game.player.pos, game.player.stats.height, goal, dt);
    if let Some(world) = world {
        let (p, stats) = (game.player.pos, &game.player.stats);
        let middle = lsw_sim::Vec3::new(p.x, p.y + stats.height * 0.5, p.z);
        game.hidden.update(game.camera.pos, middle, stats.radius, stats.height * 0.5, world, dt);
    }

    // Saber hit frames destroy droids in reach; then bolts and droids take their turn.
    if game.player.events.iter().any(|e| matches!(e, Event::ComboHit { .. })) {
        game.combat.saber_hit(&game.player);
    }
    if let Some(droid) = &game.data.droid {
        if !game.fly {
            game.combat.step(&mut game.player, &game.data.character.clips, &droid.clips, world, dt);
        }
    }
    let sound_clips = sounds.clips.clone();
    let play = |commands: &mut Commands, name: &str| {
        if let Some(handle) = sound_clips.get(name) {
            commands.spawn((AudioPlayer(handle.clone()), PlaybackSettings::DESPAWN));
        }
    };
    let events = game.player.events.clone();
    for event in &events {
        match *event {
            Event::Judder(amount) => game.camera.judder(amount, 0),
            Event::Land { hard: true } => game.camera.judder(-0.2, 0),
            Event::SaberOn => play(&mut commands, "saber_on"),
            Event::SaberOff => play(&mut commands, "saber_off"),
            Event::SaberSwing => {
                let name = ["swing1", "swing2", "swing3", "swing4"][sounds.swing % 4];
                sounds.swing += 1;
                play(&mut commands, name);
            }
            _ => {}
        }
        if let Some(file) = game.log.as_mut() {
            let _ = writeln!(file, "{:8.3} event {event:?}", game.clock);
        }
    }
    for event in game.combat.events.clone() {
        use lsw_sim::combat::CombatEvent;
        match event {
            CombatEvent::Shot(_) => play(&mut commands, "blaster"),
            CombatEvent::Deflected(_) => {
                sounds.swing += 1;
                play(&mut commands, if sounds.swing % 2 == 0 { "deflect1" } else { "deflect2" });
            }
            CombatEvent::PlayerHit => {
                play(&mut commands, "blast_hit");
                play(&mut commands, "hurt");
                game.camera.judder(-0.2, 2);
            }
            CombatEvent::EnemyDestroyed(_) => play(&mut commands, "droid_die"),
            CombatEvent::BoltGone(_) => {}
        }
        if !matches!(event, CombatEvent::BoltGone(_)) {
            if let Some(file) = game.log.as_mut() {
                let _ = writeln!(file, "{:8.3} combat {event:?} hearts {}", game.clock, game.player.hit_points);
            }
        }
    }
    // The hum plays for as long as the blade is out.
    let blade_out = game.player.blade_scale > 0.5;
    if blade_out && sounds.hum.is_none() {
        if let Some(handle) = sounds.clips.get("saber_loop") {
            sounds.hum = Some(commands.spawn((AudioPlayer(handle.clone()), PlaybackSettings::LOOP)).id());
        }
    } else if !blade_out {
        if let Some(entity) = sounds.hum.take() {
            commands.entity(entity).despawn();
        }
    }

    game.log_timer -= dt;
    if game.log_timer <= 0.0 {
        game.log_timer = 0.25;
        let (p, c) = (&game.player, &game.camera);
        if let Some(file) = game.log.as_mut() {
            let _ = writeln!(
                file,
                "{:8.3} pos ({:7.3},{:6.3},{:7.3}) vel ({:6.3},{:6.3},{:6.3}) facing {:#06x} ground {} ctx {:?} anim {:?}@{:.1} blade {:.2} cam ({:7.3},{:6.3},{:7.3}) yaw {:#06x} pitch {:#06x}",
                game.clock, p.pos.x, p.pos.y, p.pos.z, p.vel.x, p.vel.y, p.vel.z, p.facing, p.on_ground, p.context,
                p.anim.action, p.anim.time, p.blade_scale, c.pos.x, c.pos.y, c.pos.z, c.yaw, c.pitch
            );
        }
    }
}

fn sample(game: &Game, action: Action, frame: f32) -> Option<Vec<an3::NodePose>> {
    game.data.character.animations.get(&action).map(|a| a.sample_nodes(frame))
}

fn pose_character(
    game: Res<Game>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut roots: Query<&mut Transform, (With<CharacterRoot>, Without<MirrorRoot>)>,
    mut mirrors: Query<&mut Transform, (With<MirrorRoot>, Without<CharacterRoot>)>,
    mut visibility: Query<&mut Visibility>,
) {
    let player = &game.player;
    let joints = &game.data.character.model.joints;
    let anim = &player.anim;
    let mut pose = sample(&game, anim.action, anim.time);
    if let (Some(current), Some((from, frame))) = (&pose, anim.blend_from) {
        if let Some(previous) = sample(&game, from, frame) {
            pose = Some(an3::blend_poses(&previous, current, anim.blend_weight()));
        }
    }
    let world = match &pose {
        Some(pose) => an3::world_matrices(joints, pose),
        None => an3::bind_matrices(joints),
    };
    let skin_mats = skin::skin_matrices(joints, &world);
    // The game's lighting for an object standing here, turned into the character's own space.
    let lights: &[lsw_sim::lighting::Light] = game.data.level.as_ref().map_or(&game.test_lights, |l| &l.lights);
    let middle = lsw_sim::Vec3::new(player.pos.x, player.pos.y + player.stats.height * 0.5, player.pos.z);
    let lighting = lsw_sim::lighting::light_object(lights, middle, game.camera.yaw, game.camera.pitch)
        .rotated_into(player.facing.wrapping_add(0x8000));
    let mut write = |posed: &PosedMesh, positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>| {
        // Posed normals are mirrored in z for the renderer; the lighting works in model space.
        let colours: Vec<[f32; 4]> = normals
            .iter()
            .map(|n| {
                let c = lighting.shade(lsw_sim::Vec3::new(n[0], n[1], -n[2]));
                [c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2), 1.0]
            })
            .collect();
        if let Some(mut mesh) = meshes.get_mut(&posed.handle) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
    };
    for posed in &game.meshes {
        let (positions, normals) = posed.draw.pose(&world, &skin_mats, None);
        write(posed, positions, normals);
    }
    if let Some(saber) = &game.saber {
        let attach = mul(&saber.locator, &world[saber.locator_joint]);
        for posed in &saber.hilt {
            let (positions, normals) = posed.draw.pose(&world, &skin_mats, Some(&attach));
            write(posed, positions, normals);
        }
        let (axis, base, scale) = (saber.axis, saber.base, player.blade_scale);
        for posed in &saber.blade {
            let (positions, normals) = posed.draw.pose_with(&world, &skin_mats, Some(&attach), |mut p| {
                p[axis] = base + (p[axis] - base) * scale;
                p
            });
            write(posed, positions, normals);
        }
        let centre = saber.centre;
        for posed in &saber.glow {
            let (positions, normals) = posed.draw.pose_with(&world, &skin_mats, Some(&attach), |mut p| {
                for k in 0..3 {
                    if k == axis {
                        // A little longer than the core so the tip is rounded off by glow.
                        p[k] = base + (p[k] - base) * scale * 1.04;
                    } else {
                        p[k] = centre[k] + (p[k] - centre[k]) * 3.2;
                    }
                }
                p
            });
            write(posed, positions, normals);
        }
        for &entity in &saber.blade_entities {
            if let Ok(mut visible) = visibility.get_mut(entity) {
                *visible = if scale > 0.01 { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        // The blade gives off light; it is not shaded by the room.
        for posed in saber.blade.iter().chain(&saber.glow) {
            if let Some(mut mesh) = meshes.get_mut(&posed.handle) {
                let count = mesh.count_vertices();
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32, 1.0, 1.0, 1.0]; count]);
            }
        }
    }
    // Game space is left-handed; the renderer's z is the game's -z. The model faces -z in its
    // own space and heading 0 walks towards +z, hence the extra half turn.
    for mut transform in &mut roots {
        transform.translation = Vec3::new(player.pos.x, player.pos.y, -player.pos.z);
        transform.rotation = Quat::from_rotation_y(-(lsw_sim::ang_to_radians(player.facing) + std::f32::consts::PI));
    }
    // The reflection: the same pose flipped about the floor the character is over.
    for mut transform in &mut mirrors {
        transform.translation = Vec3::new(player.pos.x, 2.0 * player.floor_y - player.pos.y, -player.pos.z);
        transform.rotation = Quat::from_rotation_y(-(lsw_sim::ang_to_radians(player.facing) + std::f32::consts::PI));
        transform.scale = Vec3::new(1.0, -1.0, 1.0);
    }
}

fn place_shadow(game: Res<Game>, mut shadows: Query<&mut Transform, With<BlobShadow>>) {
    let p = &game.player;
    // Size is this project's choice; it shrinks a little as the character rises.
    let height = (p.pos.y - p.floor_y).max(0.0);
    let radius = p.stats.radius * 1.3 * (1.0 - (height / 2.0).min(0.6));
    for mut transform in &mut shadows {
        *transform = Transform::from_xyz(p.pos.x, p.floor_y + 0.012, -p.pos.z)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(radius));
    }
}

fn place_camera(game: Res<Game>, mut cameras: Query<&mut Transform, (With<Camera3d>, Without<HudCamera>)>) {
    let camera = &game.camera;
    let (yaw, pitch) = (lsw_sim::ang_to_radians(camera.yaw), lsw_sim::ang_to_radians(camera.pitch));
    let forward = Vec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), -(yaw.cos() * pitch.cos()));
    let judder = camera.judder_angle as f32 * (std::f32::consts::TAU / 65536.0);
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(camera.pos.x, camera.pos.y, -camera.pos.z).looking_to(forward, Vec3::Y);
        match camera.judder_axis() {
            0 => transform.rotate_local_x(judder),
            1 => transform.rotate_local_y(judder),
            _ => transform.rotate_local_z(judder),
        }
    }
}

fn update_hud(game: Res<Game>, keys: Res<ButtonInput<KeyCode>>, mut hud: Query<(&mut Text, &mut Visibility), With<Hud>>) {
    let p = &game.player;
    for (mut text, mut visibility) in &mut hud {
        if keys.just_pressed(KeyCode::F1) {
            *visibility = if *visibility == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
        }
        text.0 = format!(
            "Move: WASD / arrows / left stick   (Shift = walk, Ctrl = tiptoe)   F1 hides this   F = fly on/off{}\nJump: Space / A     Attack: J / left mouse / X     Put saber away: K / right mouse / B\nstate {:?}   clip {}   speed {:.2}   height {:.2}   saber {}",
            if game.fly { "  [FLYING: Space up, Ctrl/C down, Shift fast]" } else { "" },
            p.context,
            p.anim.action.name(),
            p.speed(),
            p.pos.y,
            if p.weapon_out { "out" } else { "away" }
        );
    }
}

fn run_demo(mut commands: Commands, mut game: ResMut<Game>, mut exit: MessageWriter<AppExit>) {
    let clock = game.clock;
    let Some(demo) = game.demo.as_mut() else { return };
    if let Some(&(at, name)) = demo.shots.get(demo.next_shot) {
        if clock >= at {
            demo.next_shot += 1;
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("logs/shot_{name}.png")));
        }
    }
    if clock >= demo.end {
        exit.write(AppExit::Success);
    }
}
