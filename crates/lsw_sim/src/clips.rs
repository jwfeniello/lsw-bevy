//! Actions (what the character is doing) and the animation clip that plays for each one.

use std::collections::HashMap;
use tt_formats::charcfg::CharConfig;

/// The game actions this project drives. `name()` is the `action=` word in the character file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Idle,
    WeaponIdle,
    Tiptoe,
    Walk,
    Run,
    Jump,
    Jump2,
    Flip,
    ComboJump,
    Land,
    Land2,
    FlipLand,
    ComboLand,
    Fall,
    Lunge,
    LungeLand,
    Slam,
    SlamLand,
    WeaponOut,
    WeaponIn,
    /// Enemy firing clip.
    Shoot,
    /// Saber block, 1 to 3.
    Block(u8),
    /// Chain 1 or 2, step 0..=6: `_1`, `_2a`, `_2b`, `_3a`, `_3b`, `_3c`, `_3d`.
    Combo(u8, u8),
}

const COMBO_STEPS: [&str; 7] = ["1", "2a", "2b", "3a", "3b", "3c", "3d"];

impl Action {
    pub fn name(self) -> String {
        match self {
            Action::Idle => "idle".into(),
            Action::WeaponIdle => "weaponidle".into(),
            Action::Tiptoe => "tiptoe".into(),
            Action::Walk => "walk".into(),
            Action::Run => "run".into(),
            Action::Jump => "jump".into(),
            Action::Jump2 => "jump2".into(),
            Action::Flip => "flip".into(),
            Action::ComboJump => "combojump".into(),
            Action::Land => "land".into(),
            Action::Land2 => "land2".into(),
            Action::FlipLand => "flipland".into(),
            Action::ComboLand => "comboland".into(),
            Action::Fall => "fall".into(),
            Action::Lunge => "lunge".into(),
            Action::LungeLand => "lungeland".into(),
            Action::Slam => "slam".into(),
            Action::SlamLand => "slamland".into(),
            Action::WeaponOut => "weaponout".into(),
            Action::WeaponIn => "weaponin".into(),
            Action::Shoot => "shoot".into(),
            Action::Block(n) => format!("block{n}"),
            Action::Combo(chain, step) => format!("combo{chain}_{}", COMBO_STEPS[step as usize]),
        }
    }

    pub fn all() -> Vec<Action> {
        use Action::*;
        let mut all = vec![
            Idle, WeaponIdle, Tiptoe, Walk, Run, Jump, Jump2, Flip, ComboJump, Land, Land2, FlipLand, ComboLand, Fall,
            Lunge, LungeLand, Slam, SlamLand, WeaponOut, WeaponIn, Shoot, Block(1), Block(2), Block(3),
        ];
        for chain in 1..=2 {
            for step in 0..7 {
                all.push(Combo(chain, step));
            }
        }
        all
    }
}

#[derive(Clone, Debug)]
pub struct ClipInfo {
    /// File stem of the `.an3`.
    pub file: String,
    /// Frames one pass covers (frame count - 1).
    pub length: f32,
    pub fps: f32,
    pub looping: bool,
    pub blend_in: f32,
    pub blend_out: f32,
    /// Travel speed while this clip plays (`speed=`), units per second.
    pub speed: f32,
    /// `frame1`..`frame4`.
    pub frames: [f32; 4],
}

impl ClipInfo {
    /// Seconds left when the clip is at `frame`.
    pub fn remaining(&self, frame: f32) -> f32 {
        ((self.length - frame) / self.fps).max(0.0)
    }

    pub fn duration(&self) -> f32 {
        self.remaining(0.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Clips {
    map: HashMap<Action, ClipInfo>,
}

impl Clips {
    /// Pair every action with its entry in the character file. `length_of` returns the frame
    /// count of a clip file, or `None` if the file is missing.
    pub fn from_config(config: &CharConfig, mut length_of: impl FnMut(&str) -> Option<u16>) -> Self {
        let mut map = HashMap::new();
        for action in Action::all() {
            let Some(def) = config.anim_for_action(&action.name()) else { continue };
            let Some(frames) = length_of(&def.file) else { continue };
            map.insert(
                action,
                ClipInfo {
                    file: def.file.clone(),
                    length: (frames.max(1) - 1) as f32,
                    fps: if def.fps > 0.0 { def.fps } else { 30.0 },
                    looping: def.looping,
                    blend_in: def.blend_in,
                    blend_out: def.blend_out,
                    speed: def.speed,
                    frames: def.frames,
                },
            );
        }
        Self { map }
    }

    pub fn insert(&mut self, action: Action, clip: ClipInfo) {
        self.map.insert(action, clip);
    }

    pub fn get(&self, action: Action) -> Option<&ClipInfo> {
        self.map.get(&action)
    }

    pub fn has(&self, action: Action) -> bool {
        self.map.contains_key(&action)
    }

    pub fn duration(&self, action: Action) -> f32 {
        self.get(action).map_or(0.0, |c| c.duration())
    }

    pub fn speed(&self, action: Action) -> f32 {
        self.get(action).map_or(0.0, |c| c.speed)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Action, &ClipInfo)> {
        self.map.iter()
    }
}

/// What is playing, and what it is blending in from.
#[derive(Clone, Debug)]
pub struct AnimPacket {
    pub action: Action,
    /// Current frame, zero-based.
    pub time: f32,
    pub finished: bool,
    /// The previous action frozen at the frame it was left on, while the new one blends in.
    pub blend_from: Option<(Action, f32)>,
    pub blend_time: f32,
    pub blend_duration: f32,
}

impl AnimPacket {
    pub fn new(action: Action) -> Self {
        Self { action, time: 0.0, finished: false, blend_from: None, blend_time: 0.0, blend_duration: 0.0 }
    }

    pub fn blending(&self) -> bool {
        self.blend_from.is_some()
    }

    /// 0 = entirely the previous pose, 1 = entirely the current action.
    pub fn blend_weight(&self) -> f32 {
        if self.blend_from.is_some() && self.blend_duration > 0.0 {
            (self.blend_time / self.blend_duration).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// Switch to `action`. Does nothing if it is already playing, unless `restart` is set.
    pub fn play(&mut self, clips: &Clips, action: Action, restart: bool) {
        if !clips.has(action) || (self.action == action && !restart) {
            return;
        }
        let blend_in = clips.get(action).map_or(0.0, |c| c.blend_in);
        if blend_in > 0.0 && clips.has(self.action) {
            self.blend_from = Some((self.action, self.time));
            self.blend_time = 0.0;
            self.blend_duration = blend_in;
        } else {
            self.blend_from = None;
        }
        self.action = action;
        self.time = 0.0;
        self.finished = false;
    }

    pub fn update(&mut self, clips: &Clips, dt: f32) {
        if self.blend_from.is_some() {
            self.blend_time += dt;
            if self.blend_time >= self.blend_duration {
                self.blend_from = None;
            }
        }
        let Some(clip) = clips.get(self.action) else { return };
        self.time += clip.fps * dt;
        if clip.length <= 0.0 {
            self.time = 0.0;
            self.finished = true;
        } else if clip.looping {
            self.time %= clip.length;
        } else if self.time >= clip.length {
            self.time = clip.length;
            self.finished = true;
        }
    }
}
