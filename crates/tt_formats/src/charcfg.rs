//! Character definition files (`chars\<name>\<name>.txt`): settings, then animation entries.
//!
//! Defaults for an animation entry match the game's parser: 30 frames per second, 0.2 s blend
//! in and out, looping on.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct AnimDef {
    /// File stem of the `.an3` in the character's folder.
    pub file: String,
    /// The game action this animation plays for, lower-cased. Empty if the entry has none.
    pub action: String,
    pub fps: f32,
    pub blend_in: f32,
    pub blend_out: f32,
    pub looping: bool,
    /// `speed=`: how fast the character travels while the animation plays (units per second).
    pub speed: f32,
    /// `frame`/`frame1` .. `frame4`: event frames (hit frame, queue frame, footsteps).
    pub frames: [f32; 4],
    pub stop_frame: f32,
    pub footsteps: bool,
    pub no_headturn: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CharConfig {
    /// Every `key=value` setting before the animation list, keys lower-cased.
    pub settings: HashMap<String, String>,
    /// Bare words such as `jedi`.
    pub flags: Vec<String>,
    pub anims: Vec<AnimDef>,
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn split(line: &str) -> (String, String) {
    let line = line.split("//").next().unwrap_or("").split(';').next().unwrap_or("").trim();
    match line.find(|c: char| c == '=' || c.is_whitespace()) {
        Some(i) => (line[..i].to_ascii_lowercase(), unquote(line[i + 1..].trim_start_matches(['=', ' ', '\t']))),
        None => (line.to_ascii_lowercase(), String::new()),
    }
}

impl CharConfig {
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut current: Option<AnimDef> = None;
        for line in text.lines() {
            let (key, value) = split(line);
            if key.is_empty() {
                continue;
            }
            let number = value.parse::<f32>().unwrap_or(0.0);
            if key == "anim_start" {
                current = Some(AnimDef {
                    file: value.to_ascii_lowercase(),
                    action: String::new(),
                    fps: 30.0,
                    blend_in: 0.2,
                    blend_out: 0.2,
                    looping: true,
                    speed: 0.0,
                    frames: [0.0; 4],
                    stop_frame: 0.0,
                    footsteps: false,
                    no_headturn: false,
                });
                continue;
            }
            let Some(anim) = current.as_mut() else {
                if value.is_empty() {
                    config.flags.push(key);
                } else {
                    config.settings.insert(key, value);
                }
                continue;
            };
            match key.as_str() {
                "anim_end" => config.anims.extend(current.take()),
                "action" => anim.action = value.to_ascii_lowercase(),
                "fpsec" => anim.fps = number,
                "blend_in" => anim.blend_in = number,
                "blend_out" => anim.blend_out = number,
                "cycle" => anim.looping = !value.eq_ignore_ascii_case("off"),
                "speed" => anim.speed = number,
                "frame" | "frame1" | "frame_hit" => anim.frames[0] = number,
                "frame2" | "frame_end" => anim.frames[1] = number,
                "frame3" => anim.frames[2] = number,
                "frame4" => anim.frames[3] = number,
                "frame_stop" => anim.stop_frame = number,
                "footsteps" => anim.footsteps = !value.eq_ignore_ascii_case("off"),
                "no_headturn" | "headturn_off" => anim.no_headturn = true,
                _ => {}
            }
        }
        config
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.settings.get(key)?.parse().ok()
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(|s| s.as_str())
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// A comma-separated list of indices, e.g. `layers_high=0,2,5`.
    pub fn list(&self, key: &str) -> Vec<usize> {
        self.str(key).map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect()).unwrap_or_default()
    }

    pub fn anim_for_action(&self, action: &str) -> Option<&AnimDef> {
        self.anims.iter().find(|a| a.action == action)
    }
}
