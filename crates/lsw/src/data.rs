//! Everything read from the player's own game archive at start-up. Nothing is written back and
//! nothing from the archive is stored in this repository.

use lsw_sim::clips::{Action, Clips};
use lsw_sim::player::Stats;
use std::collections::HashMap;
use tt_formats::an3::Animation;
use tt_formats::atc::Texture;
use tt_formats::charcfg::CharConfig;
use tt_formats::dat::Archive;
use tt_formats::ghg::Model;
use tt_formats::{Error, Result};

pub struct CharacterData {
    pub model: Model,
    pub config: CharConfig,
    pub stats: Stats,
    pub clips: Clips,
    pub animations: HashMap<Action, Animation>,
    /// Decoded textures, indexed like `model.texture_hashes`. `None` if one failed to load.
    pub textures: Vec<Option<Texture>>,
    /// Layers drawn in normal play (`layers_high`).
    pub layers: Vec<usize>,
}

pub struct SaberData {
    pub model: Model,
    pub textures: Vec<Option<Texture>>,
    /// (layer, piece) of the hilt-and-blade piece hanging off the `|LightSabre` joint.
    pub piece: (usize, usize),
}

pub struct LevelData {
    #[allow(dead_code)]
    pub name: String,
    pub model: Model,
    pub textures: Vec<Option<Texture>>,
    pub world: lsw_sim::world::World,
    /// The level's light table, as the object-lighting rules use it.
    pub lights: Vec<lsw_sim::lighting::Light>,
    /// Force-movable objects that have a collision blocker (doors opened with the Force).
    pub force_objects: Vec<ForceObject>,
    /// The level text asks for floor reflections (`reflect_range`).
    pub reflective: bool,
}

/// HUD art that exists as pictures in the game data.
pub struct HudData {
    pub font: Option<tt_formats::fnt::Font>,
    /// The shared props scene: holds the HUD's 3D pieces (portrait ring, heart, stud).
    pub things: Option<(Model, Vec<Option<Texture>>)>,
    /// The 32 frames of the spinning stud. The game steps through consecutive textures
    /// starting at the stud model's own (script `stuff\ats\coin.ats`: texture 0 to 31, loop).
    pub coin_frames: Vec<Texture>,
    /// The character's portrait scene (named by `icon=` in the character file).
    pub portrait: Option<(Model, Vec<Option<Texture>>)>,
}

/// One "GizForce" entry of the level's gizmo file: a named set of scene objects a Jedi can
/// move with the Force. Only the names are read; the stored motion is not decoded.
pub struct ForceObject {
    pub name: String,
    /// Scene object names of the visible pieces.
    pub pieces: Vec<String>,
    /// Object numbers of invisible collision blockers that go away when the move completes.
    pub blockers: Vec<u32>,
    /// Middle of the pieces, in game space.
    pub centre: lsw_sim::Vec3,
    /// Seconds the Force has been held on it, and whether it has completed.
    pub held: f32,
    pub done: bool,
    /// 0 = untouched, 1 = fully moved aside.
    pub open: f32,
}

/// Names in the GizForce section of a `.giz` file, grouped by entry. Names are stored as a
/// length byte (counting the terminator), the text, and a zero.
fn force_entries(giz: &[u8]) -> Vec<(String, Vec<String>)> {
    let section = |tag: &[u8]| giz.windows(tag.len()).position(|w| w == tag);
    let Some(start) = section(b"GizForce") else { return Vec::new() };
    let end = ["GizmoPickup", "GizTurret", "GizPanel", "GizBuildit"]
        .iter()
        .filter_map(|t| section(t.as_bytes()))
        .filter(|&p| p > start)
        .min()
        .unwrap_or(giz.len());
    let mut entries: Vec<(String, Vec<String>)> = Vec::new();
    let mut at = start + 8;
    while at + 2 < end {
        // An entry starts with its own name ("force" and a number), stored without a length.
        if giz[at..end].starts_with(b"force") {
            let digits = giz[at + 5..end].iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && giz.get(at + 5 + digits) == Some(&0) {
                entries.push((giz[at..at + 5 + digits].iter().map(|&c| c as char).collect(), Vec::new()));
                at += 6 + digits;
                continue;
            }
        }
        let len = giz[at] as usize;
        let text = giz.get(at + 1..at + len);
        let valid = (3..=40).contains(&len)
            && giz.get(at + len) == Some(&0)
            && text.is_some_and(|t| t.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_'));
        if !valid {
            at += 1;
            continue;
        }
        let name: String = text.unwrap().iter().map(|&c| c as char).collect();
        if let Some(entry) = entries.last_mut() {
            entry.1.push(name);
        }
        at += len + 1;
    }
    entries
}

/// A level made from a plain model file instead of the game's own level data.
pub struct CustomLevel {
    pub scene: crate::obj::ObjScene,
    pub world: lsw_sim::world::World,
}

pub struct GameData {
    /// The battle droid, used as the enemy.
    pub droid: Option<CharacterData>,
    pub custom: Option<CustomLevel>,
    pub hud: HudData,
    pub level: Option<LevelData>,
    pub character: CharacterData,
    pub saber: Option<SaberData>,
    /// Sound effects by short name, as raw WAV files.
    pub sounds: HashMap<&'static str, Vec<u8>>,
}

fn load_textures(archive: &Archive, model: &Model, log: &mut Vec<String>) -> Vec<Option<Texture>> {
    let used: Vec<usize> = model.materials.iter().flat_map(|m| [m.diffuse, m.env_map]).flatten().collect();
    model
        .texture_hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| {
            if !used.contains(&i) {
                return None;
            }
            // Most textures are ATC-compressed; HUD art is stored uncompressed instead.
            let path = ["atitc", "pvrnc"]
                .map(|ext| format!("sharedtextures\\0x{hash:08x}.{ext}"))
                .into_iter()
                .find(|p| archive.contains(p))
                .unwrap_or_else(|| format!("sharedtextures\\0x{hash:08x}.atitc"));
            match archive.read(&path).and_then(|d| tt_formats::atc::decode(&d)) {
                Ok(texture) => Some(texture),
                Err(e) => {
                    log.push(format!("texture {path}: {e}"));
                    None
                }
            }
        })
        .collect()
}

/// Load one character folder: definition file, model, the clips its actions name, textures.
fn load_character(archive: &Archive, character: &str, log: &mut Vec<String>) -> Result<CharacterData> {
    let dir = format!("chars\\{character}");
    let config = CharConfig::parse(&archive.read_text(&format!("{dir}\\{character}.txt"))?);
    let model_path = [format!("{dir}\\{character}_lr_ios.ghg"), format!("{dir}\\{character}_ios.ghg")]
        .into_iter()
        .find(|p| archive.contains(p))
        .ok_or_else(|| Error::new(format!("no model found for {character}")))?;
    let model = Model::parse(&archive.read(&model_path)?)?;
    let mut parsed: HashMap<String, Animation> = HashMap::new();
    let clips = Clips::from_config(&config, |file| {
        let anim = archive.read(&format!("{dir}\\{file}.an3")).and_then(|d| Animation::parse(&d)).ok()?;
        if anim.node_count() != model.joints.len() {
            return None;
        }
        let frames = anim.frame_count;
        parsed.insert(file.to_string(), anim);
        Some(frames)
    });
    let mut animations = HashMap::new();
    for (action, clip) in clips.iter() {
        if let Some(anim) = parsed.get(&clip.file) {
            animations.insert(*action, anim.clone());
        }
    }
    let stats = Stats::from_config(&config);
    let mut layers = config.list("layers_high");
    layers.retain(|&l| l < model.layers.len());
    if layers.is_empty() {
        layers = (0..model.layers.len().min(1)).collect();
    }
    let textures = load_textures(archive, &model, log);
    log.push(format!("character {character}: {} joints, {} actions mapped", model.joints.len(), animations.len()));
    Ok(CharacterData { model, config, stats, clips, animations, textures, layers })
}

impl GameData {
    /// The collision world in use: a custom model level if one is loaded, else the game level.
    pub fn world(&self) -> Option<&lsw_sim::world::World> {
        self.custom.as_ref().map(|c| &c.world).or(self.level.as_ref().map(|l| &l.world))
    }

    /// Load a level folder such as `levels\episode_i\negotiations\negotiations_a`: its scene,
    /// collision, camera rails and the rail settings from its text file.
    fn load_level(archive: &Archive, dir: &str, log: &mut Vec<String>) -> Result<LevelData> {
        use lsw_sim::world::{Sock, World};
        use lsw_sim::Vec3;
        let dir = dir.trim_end_matches(['\\', '/']).replace('/', "\\");
        let name = dir.rsplit('\\').next().unwrap_or("").to_string();
        let model = Model::parse(&archive.read(&format!("{dir}\\{name}_ios.gsc"))?)?;
        let terrain = tt_formats::ter::Terrain::parse(&archive.read(&format!("{dir}\\{name}.ter"))?)?;
        // Collision that belongs to a scene object carries that object's number, so it can be
        // switched off when the object moves away.
        let mut world = World::new_tagged(terrain.triangles.iter().map(|t| {
            let group = &terrain.groups[t.group as usize];
            (t.points, t.normal, if group.kind == 1 { group.scene_index as u32 } else { 0 })
        }));
        world.add_walls(&terrain.walls);
        let v3 = |p: &[f32; 3]| Vec3::new(p[0], p[1], p[2]);

        // Rails: sock_cam_NN, sock_a_NN, sock_b_NN share an index.
        let mut socks: std::collections::BTreeMap<u32, Sock> = Default::default();
        for spline in &model.splines {
            let lower = spline.name.to_ascii_lowercase();
            let Some(rest) = lower.strip_prefix("sock_") else {
                if lower == "start" {
                    world.start = spline.points.first().map(v3);
                }
                continue;
            };
            let Some((kind, index)) = rest.rsplit_once('_') else { continue };
            let Ok(index) = index.parse::<u32>() else { continue };
            let sock = socks.entry(index).or_insert_with(|| Sock { index, position_seek: 5.0, ..Default::default() });
            let points = spline.points.iter().map(v3).collect();
            match kind {
                "cam" => sock.cam = points,
                "a" => sock.a = points,
                "b" => sock.b = points,
                "mid" => sock.mid = points,
                _ => {}
            }
        }
        // Per-rail settings from the level text file.
        if let Ok(text) = archive.read_text(&format!("{dir}\\{name}.txt")) {
            let mut current: Option<u32> = None;
            for line in text.lines() {
                let mut words = line.split("//").next().unwrap_or("").split_whitespace();
                let (Some(key), value) = (words.next(), words.next()) else { continue };
                let number = value.and_then(|v| v.parse::<f32>().ok());
                match key.to_ascii_lowercase().as_str() {
                    "sock_start" => current = value.and_then(|v| v.parse().ok()),
                    "sock_end" => current = None,
                    "y_limits" => {
                        if let Some(sock) = current.and_then(|i| socks.get_mut(&i)) {
                            sock.y_limits = true;
                        }
                    }
                    key => {
                        let (Some(sock), Some(number)) = (current.and_then(|i| socks.get_mut(&i)), number) else { continue };
                        match key {
                            "cam_y_offset" => sock.cam_y_offset = number,
                            "offset_blend_y_ratio" | "offset_blend_ratio" => sock.offset_blend_y = number,
                            "campos_seek" => sock.position_seek = number,
                            "cam_height_above_terrain" => sock.height_above_terrain = number,
                            _ => {}
                        }
                    }
                }
            }
        }
        world.socks = socks.into_values().filter(|s| !s.cam.is_empty() && s.a.len() >= 2 && s.b.len() >= 2).collect();
        log.push(format!(
            "level {dir}: {} placed meshes, {} collision triangles, {} wall segments, {} camera rails, start {:?}",
            model.statics.len(),
            world.triangle_count(),
            world.wall_count(),
            world.socks.len(),
            world.start
        ));
        let lights: Vec<lsw_sim::lighting::Light> = match archive.read(&format!("{dir}\\{name}.rtl")).and_then(|d| tt_formats::rtl::parse(&d)) {
            Ok(lights) => lights
                .iter()
                .filter(|l| !l.disabled && l.chain == -1)
                .map(|l| lsw_sim::lighting::Light {
                    kind: l.kind,
                    position: v3(&l.position),
                    direction: v3(&l.direction),
                    colour: l.colour,
                    inner_radius: l.inner_radius,
                    outer_radius: l.outer_radius,
                    intensity: l.intensity,
                    pitch: l.pitch as u16,
                    yaw: l.yaw as u16,
                    group_mask: l.include_mask,
                })
                .collect(),
            Err(e) => {
                log.push(format!("level lights not loaded: {e}"));
                Vec::new()
            }
        };
        log.push(format!("level lights: {}", lights.len()));
        let textures = load_textures(archive, &model, log);
        let mut force_objects = Vec::new();
        if let Ok(giz) = archive.read(&format!("{dir}\\{name}.giz")) {
            let entries = force_entries(&giz);
            let named = model.statics.iter().filter(|s| !s.name.is_empty()).count();
            log.push(format!(
                "gizmo file: {} force entries (first: {:?} with {} names); {} of {} placed meshes are named; terr_cube placed: {}",
                entries.len(),
                entries.first().map(|e| e.0.clone()),
                entries.first().map_or(0, |e| e.1.len()),
                named,
                model.statics.len(),
                model.statics.iter().any(|s| s.name == "terr_cube")
            ));
            for (entry, names) in entries {
                let mut object = ForceObject { name: entry, pieces: Vec::new(), blockers: Vec::new(), centre: Vec3::ZERO, held: 0.0, done: false, open: 0.0 };
                let (mut sum, mut count) = ([0.0f32; 3], 0.0f32);
                for piece in names {
                    let Some(placed) = model.statics.iter().find(|s| s.name.eq_ignore_ascii_case(&piece)) else { continue };
                    // Helpers named "terr..." are the collision blockers.
                    if piece.to_ascii_lowercase().starts_with("terr") {
                        if !object.blockers.contains(&placed.scene_id) {
                            object.blockers.push(placed.scene_id);
                        }
                    } else {
                        for k in 0..3 {
                            sum[k] += placed.matrix[12 + k];
                        }
                        count += 1.0;
                        object.pieces.push(piece);
                    }
                }
                if !object.blockers.is_empty() && count > 0.0 {
                    object.centre = Vec3::new(sum[0] / count, sum[1] / count, sum[2] / count);
                    log.push(format!("force object {}: {} pieces, blockers {:?}, centre {:?}", object.name, object.pieces.len(), object.blockers, object.centre));
                    force_objects.push(object);
                }
            }
        }
        let reflective = archive
            .read_text(&format!("{dir}\\{name}.txt"))
            .is_ok_and(|t| t.lines().any(|l| l.trim_start().to_ascii_lowercase().starts_with("reflect_range")));
        Ok(LevelData { name, model, textures, world, lights, force_objects, reflective })
    }

    pub fn load(archive_path: &str, character: &str, level: Option<&str>, log: &mut Vec<String>) -> Result<Self> {
        let archive = Archive::open(archive_path)?;
        let level = match level {
            Some(dir) => match Self::load_level(&archive, dir, log) {
                Ok(level) => Some(level),
                Err(e) => {
                    log.push(format!("level {dir} not loaded ({e}); using the flat test floor"));
                    None
                }
            },
            None => None,
        };
        log.push(format!("archive {archive_path}: {} files", archive.len()));
        let dir = format!("chars\\{character}");
        let config = CharConfig::parse(&archive.read_text(&format!("{dir}\\{character}.txt"))?);
        let model_path = [format!("{dir}\\{character}_lr_ios.ghg"), format!("{dir}\\{character}_ios.ghg")]
            .into_iter()
            .find(|p| archive.contains(p))
            .ok_or_else(|| Error::new(format!("no model found for {character}")))?;
        let model = Model::parse(&archive.read(&model_path)?)?;
        log.push(format!("model {model_path}: {} joints, {} layers", model.joints.len(), model.layers.len()));

        let mut animations = HashMap::new();
        let mut parsed: HashMap<String, Animation> = HashMap::new();
        let clips = Clips::from_config(&config, |file| {
            let path = format!("{dir}\\{file}.an3");
            match archive.read(&path).and_then(|d| Animation::parse(&d)) {
                Ok(anim) if anim.node_count() == model.joints.len() => {
                    let frames = anim.frame_count;
                    parsed.insert(file.to_string(), anim);
                    Some(frames)
                }
                Ok(anim) => {
                    log.push(format!("{path}: {} nodes for {} joints, skipped", anim.node_count(), model.joints.len()));
                    None
                }
                Err(e) => {
                    log.push(format!("{path}: {e}"));
                    None
                }
            }
        });
        for (action, clip) in clips.iter() {
            if let Some(anim) = parsed.get(&clip.file) {
                animations.insert(*action, anim.clone());
            }
        }
        log.push(format!("animations: {} actions mapped", animations.len()));

        let stats = Stats::from_config(&config);
        log.push(format!("stats: {stats:?}"));
        let mut layers = config.list("layers_high");
        layers.retain(|&l| l < model.layers.len());
        if layers.is_empty() {
            layers = (0..model.layers.len().min(1)).collect();
        }
        let textures = load_textures(&archive, &model, log);
        let character = CharacterData { model, config, stats, clips, animations, textures, layers };

        let saber = Self::load_saber(&archive, log);
        if saber.is_none() {
            log.push("lightsaber model not loaded".into());
        }

        let mut sounds = HashMap::new();
        for (name, path) in [
            ("saber_on", "audio\\_soundfx\\saber\\saberonj.wav"),
            ("saber_off", "audio\\_soundfx\\saber\\saberoffj.wav"),
            ("saber_loop", "audio\\_soundfx\\saber\\saberloopj.wav"),
            ("swing1", "audio\\_soundfx\\saber\\sabermove1.wav"),
            ("swing2", "audio\\_soundfx\\saber\\sabermove2.wav"),
            ("swing3", "audio\\_soundfx\\saber\\sabermove3.wav"),
            ("swing4", "audio\\_soundfx\\saber\\sabermove4.wav"),
            ("deflect1", "audio\\_soundfx\\saber\\sabdefblas1.wav"),
            ("deflect2", "audio\\_soundfx\\saber\\sabdefblas2.wav"),
            ("blaster", "audio\\_soundfx\\blaster\\blaster.wav"),
            ("blast_hit", "audio\\_soundfx\\blaster\\blasthit1.wav"),
            ("droid_die", "audio\\_soundfx\\droid\\droiddie01.wav"),
            ("hurt", "audio\\_soundfx\\quigon\\qui_hurt1.wav"),
        ] {
            match archive.read(path) {
                Ok(bytes) => {
                    sounds.insert(name, bytes);
                }
                Err(e) => log.push(format!("sound {path}: {e}")),
            }
        }
        let font = ["stuff\\text\\starwars_font_mob.android_atitc_fnt", "stuff\\text\\starwars_font_ios.fnt"]
            .into_iter()
            .find_map(|path| match archive.read(path).and_then(|d| tt_formats::fnt::parse(&d)) {
                Ok(font) => Some(font),
                Err(e) => {
                    log.push(format!("font {path}: {e}"));
                    None
                }
            });
        let mut scene = |path: String| match archive.read(&path).and_then(|d| Model::parse(&d)) {
            Ok(model) => {
                if !model.skipped.is_empty() {
                    log.push(format!("{path}: {} meshes skipped (first: {})", model.skipped.len(), model.skipped[0]));
                }
                let textures = load_textures(&archive, &model, log);
                Some((model, textures))
            }
            Err(e) => {
                log.push(format!("{path}: {e}"));
                None
            }
        };
        let things = scene("stuff\\things_ios.gsc".to_string());
        let portrait = character.config.str("icon").and_then(|icon| scene(format!("stuff\\icons\\{icon}_ios.gsc")));
        let mut coin_frames = Vec::new();
        if let Some((model, _)) = &things {
            let first = model
                .specials
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case("silver_pan_coin1"))
                .and_then(|p| p.meshes.first())
                .and_then(|m| model.materials.get(m.material))
                .and_then(|m| m.diffuse);
            if let Some(first) = first {
                for hash in model.texture_hashes.iter().skip(first).take(32) {
                    let found = ["atitc", "pvrnc"]
                        .map(|ext| format!("sharedtextures\\0x{hash:08x}.{ext}"))
                        .into_iter()
                        .find(|p| archive.contains(p));
                    match found.and_then(|p| archive.read(&p).and_then(|d| tt_formats::atc::decode(&d)).ok()) {
                        Some(texture) => coin_frames.push(texture),
                        None => break,
                    }
                }
            }
        }
        log.push(format!("hud: {} stud frames", coin_frames.len()));
        log.push(format!(
            "hud: font {} glyphs, portrait {}",
            font.as_ref().map_or(0, |f| f.glyphs.len()),
            if portrait.is_some() { "loaded" } else { "missing" }
        ));
        let droid = match load_character(&archive, "battledroid", log) {
            Ok(droid) => Some(droid),
            Err(e) => {
                log.push(format!("battle droid not loaded: {e}"));
                None
            }
        };
        Ok(Self { droid, custom: None, hud: HudData { font, things, coin_frames, portrait }, level, character, saber, sounds })
    }

    fn load_saber(archive: &Archive, log: &mut Vec<String>) -> Option<SaberData> {
        let path = "chars\\macewindu\\lightsabre_lr_ios.ghg";
        let model = match archive.read(path).and_then(|d| Model::parse(&d)) {
            Ok(model) => model,
            Err(e) => {
                log.push(format!("{path}: {e}"));
                return None;
            }
        };
        let joint = model.joints.iter().position(|j| j.name.contains("LightSabre"))?;
        let piece = model.layers.iter().enumerate().find_map(|(l, layer)| {
            layer.pieces.iter().position(|p| p.rigid_joint == Some(joint)).map(|p| (l, p))
        })?;
        let textures = load_textures(archive, &model, log);
        Some(SaberData { model, textures, piece })
    }
}
