//! The Jedi character controller: walking, turning, gravity, jumps and the lightsaber combo.
//!
//! Each step runs in the game's order: gravity, jump rules, weapon and combo rules, then the
//! directional movement that turns stick input into a heading and a velocity, then integration.

use crate::clips::{Action, AnimPacket, Clips};
use crate::world::World;
use crate::{ang_cos, ang_sin, atan2d, rot_diff, seek_rot, seek_val, Ang, Vec3};

/// How high a ledge can be walked up, and how far down the feet follow a slope or step
/// (estimated: not taken from the game).
const STEP_HEIGHT: f32 = 0.1;
use tt_formats::charcfg::CharConfig;

/// Per-character numbers. Defaults are the game's defaults for every character; the character
/// file overrides them.
#[derive(Clone, Debug)]
pub struct Stats {
    pub tiptoe_speed: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub gravity: f32,
    pub jump_speed: f32,
    pub jump_2_speed: f32,
    /// How quickly velocity closes on its target (`acceleration=`).
    pub acceleration: f32,
    pub turn_rate: f32,
    pub jump_move_speed_scale: f32,
    pub height: f32,
    pub radius: f32,
    /// `hit_points`: hearts shown on the HUD.
    pub hit_points: u32,
    pub slam_jump_speed: f32,
    pub slam_gravity: f32,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            tiptoe_speed: 1.0,
            walk_speed: 2.0,
            run_speed: 3.0,
            gravity: -5.0,
            jump_speed: 2.0,
            jump_2_speed: 2.0,
            acceleration: 5.0,
            turn_rate: 1.0,
            jump_move_speed_scale: 1.0,
            height: 0.42,
            radius: 0.1,
            hit_points: 4,
            slam_jump_speed: 3.0,
            slam_gravity: -15.0,
        }
    }
}

impl Stats {
    pub fn from_config(config: &CharConfig) -> Self {
        let d = Self::default();
        let get = |key: &str, default: f32| config.f32(key).unwrap_or(default);
        let scale = get("scale", 1.0);
        Self {
            tiptoe_speed: get("tiptoe_speed", d.tiptoe_speed),
            walk_speed: get("walk_speed", d.walk_speed),
            run_speed: get("run_speed", d.run_speed),
            gravity: get("air_gravity", d.gravity),
            jump_speed: get("jump_speed", d.jump_speed),
            jump_2_speed: get("jump_2_speed", d.jump_2_speed),
            acceleration: get("acceleration", d.acceleration),
            turn_rate: get("turn_rate", d.turn_rate),
            jump_move_speed_scale: get("jump_move_speed_scale", d.jump_move_speed_scale),
            height: (get("maxy", 0.42) - get("miny", 0.0)) * scale,
            radius: get("radius", d.radius) * scale,
            hit_points: get("hit_points", d.hit_points as f32) as u32,
            slam_jump_speed: get("slam_jumpspeed", d.slam_jump_speed),
            slam_gravity: get("slam_gravity", d.slam_gravity),
        }
    }

    /// Peak height of a jump launched at `speed`.
    pub fn jump_height(&self, speed: f32) -> f32 {
        if self.gravity == 0.0 {
            return 0.0;
        }
        let rise_time = -speed / self.gravity;
        speed * rise_time + self.gravity * 0.5 * rise_time * rise_time
    }
}

/// One frame of controller input. `stick` is right/forward in -1..=1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub stick_x: f32,
    pub stick_z: f32,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub action_pressed: bool,
    pub action_held: bool,
    pub special_pressed: bool,
    /// Position of a blaster bolt about to arrive, if there is one.
    pub incoming_bolt: Option<Vec3>,
}

/// What happens to a bolt that reaches the character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deflect {
    /// It hits.
    None,
    /// Guard held: knocked away at a random angle.
    Scatter,
    /// Timed block: sent straight back.
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    None,
    Jump,
    LandJump,
    LandJump2,
    LandFlip,
    LandComboJump,
    Combo,
    WeaponIn,
    WeaponOut,
    LandLunge,
    LandSlam,
    Hold,
    Block,
}

impl Context {
    /// Heading does not follow the stick.
    fn locks_heading(self) -> bool {
        use Context::*;
        matches!(self, LandJump | LandJump2 | LandFlip | LandComboJump | WeaponIn | WeaponOut | LandLunge | LandSlam)
    }

    fn is_soft_landing(self) -> bool {
        matches!(self, Context::LandJump | Context::LandJump2 | Context::LandFlip | Context::LandComboJump)
    }

    fn can_start_jump(self) -> bool {
        matches!(self, Context::None | Context::WeaponIn | Context::WeaponOut)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpState {
    Basic,
    Flip,
    ComboJump,
    Lunge,
    Slam,
}

/// Things the presentation layer reacts to (sounds, camera kick).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Jump { second: bool },
    Land { hard: bool },
    SaberOn,
    SaberOff,
    SaberSwing,
    ComboHit { strength: u8 },
    Block,
    Hurt,
    Died,
    Judder(f32),
}

const COMBO_WAITING: u8 = 0x04;
const COMBO_STAGE2_END: u8 = 0x08;
const COMBO_PAST_FRAME2: u8 = 0x10;
const COMBO_HIT_DONE: u8 = 0x40;

#[derive(Clone, Debug)]
pub struct Player {
    pub stats: Stats,
    pub pos: Vec3,
    pub vel: Vec3,
    pub target_vel: Vec3,
    /// Where the body points (what is drawn).
    pub facing: Ang,
    /// Where the body is turning to.
    pub move_facing: Ang,
    pub on_ground: bool,
    was_on_ground: bool,
    pub floor_y: f32,
    safe_pos: Vec3,

    pub context: Context,
    pub context_anim: Action,
    pub context_timer: f32,
    context_started: bool,

    pub jump_state: JumpState,
    /// 1 after a first jump, 2 after a double jump or a quick re-jump.
    pub jump_sequence: u8,
    jump_start_y: f32,
    jump_reentry_timer: f32,
    jump_button_latched: bool,
    second_jump_taken: bool,
    /// Seconds the fall clip has been asked for.
    fall_timer: f32,
    /// Stays at 0.2 s while standing and runs down in the air: a jump is still allowed inside it.
    ground_grace: f32,
    /// Set once a jump's clip has played out high above the floor: the fall clip takes over
    /// and air attacks are no longer available.
    pub jump_falling: bool,
    jump_clip_done_timer: f32,
    held_move_clip: Option<Action>,
    move_clip_timer: f32,

    delayed_turn_timer: f32,
    delayed_turn_target: Ang,

    pub weapon_out: bool,
    /// 0 = blade retracted, 1 = fully out.
    pub blade_scale: f32,

    pub combo_stage: u8,
    pub combo_branch: u8,
    combo_chain: u8,
    combo_alternate: bool,
    combo_flags: u8,
    combo_input_timer: f32,
    combo_latched: bool,
    combo_timing: bool,

    /// Hearts left.
    pub hit_points: u32,
    /// False while a block is still winding up (it faces the bolt during this time).
    block_latched: bool,
    block_target: Vec3,
    previous_block: u8,
    /// Block clip being shown while the guard is held.
    hold_anim: Option<Action>,
    seed: u32,

    /// Stick magnitude mapped to a speed (0, tiptoe, walk or run).
    pub input_speed: f32,
    pub input_angle: Ang,
    pub anim: AnimPacket,
    pub events: Vec<Event>,
}

impl Player {
    pub fn new(stats: Stats) -> Self {
        let hearts = stats.hit_points;
        Self {
            stats,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            target_vel: Vec3::ZERO,
            facing: 0,
            move_facing: 0,
            on_ground: true,
            was_on_ground: true,
            floor_y: 0.0,
            safe_pos: Vec3::ZERO,
            context: Context::None,
            context_anim: Action::Idle,
            context_timer: 0.0,
            context_started: false,
            jump_state: JumpState::Basic,
            jump_sequence: 0,
            jump_start_y: 0.0,
            jump_reentry_timer: 0.0,
            jump_button_latched: false,
            second_jump_taken: false,
            fall_timer: 0.0,
            ground_grace: 0.2,
            jump_falling: false,
            jump_clip_done_timer: 0.0,
            held_move_clip: None,
            move_clip_timer: 0.0,
            delayed_turn_timer: 0.0,
            delayed_turn_target: 0,
            weapon_out: false,
            blade_scale: 0.0,
            combo_stage: 0,
            combo_branch: 0,
            combo_chain: 1,
            combo_alternate: false,
            combo_flags: 0,
            combo_input_timer: 0.0,
            combo_latched: false,
            combo_timing: false,
            hit_points: hearts,
            block_latched: false,
            block_target: Vec3::ZERO,
            previous_block: 0,
            hold_anim: None,
            seed: 12345,
            input_speed: 0.0,
            input_angle: 0,
            anim: AnimPacket::new(Action::Idle),
            events: Vec::new(),
        }
    }

    fn set_context(&mut self, context: Context, anim: Action) {
        self.context = context;
        self.context_anim = anim;
        self.context_started = true;
    }

    /// Advance one frame. `camera_yaw` is the heading the camera looks along; the stick is
    /// relative to it.
    pub fn step(&mut self, input: &Input, camera_yaw: Ang, clips: &Clips, dt: f32) {
        self.step_in(input, camera_yaw, clips, None, dt);
    }

    /// As step, colliding with a level instead of the flat test floor.
    pub fn step_in(&mut self, input: &Input, camera_yaw: Ang, clips: &Clips, world: Option<&World>, dt: f32) {
        self.events.clear();
        self.read_stick(input, camera_yaw);
        if self.on_ground {
            self.ground_grace = 0.2;
        } else if self.ground_grace > 0.0 {
            self.ground_grace -= dt;
        }
        self.apply_gravity(dt);
        self.jump_code(input, clips, dt);
        self.weapon_code(clips, dt);
        self.hold_code(input);
        self.block_code(input, clips, dt);
        self.combo_code(input, clips, dt);
        self.fall_land_code(clips);
        self.directional(clips, dt);
        match world {
            Some(world) => self.integrate_in(world, dt),
            None => self.integrate(dt),
        }
        self.animate(clips, dt);
    }

    /// Stick -> speed tier and world heading. Below a third of full deflection the stick is
    /// dead; then tiptoe below 0.5, walk below 0.8, run above.
    fn read_stick(&mut self, input: &Input, camera_yaw: Ang) {
        let (mut x, mut z) = (input.stick_x.clamp(-1.0, 1.0), input.stick_z.clamp(-1.0, 1.0));
        if x * x + z * z < (42.5f32 / 127.5).powi(2) {
            x = 0.0;
            z = 0.0;
        }
        let magnitude = x.abs().max(z.abs());
        self.input_speed = if magnitude < 0.2 {
            0.0
        } else if magnitude < 0.5 {
            self.stats.tiptoe_speed
        } else if magnitude < 0.8 {
            self.stats.walk_speed
        } else {
            self.stats.run_speed
        };
        if self.input_speed > 0.0 {
            self.input_angle = atan2d(x, z).wrapping_add(camera_yaw);
        }
    }

    fn apply_gravity(&mut self, dt: f32) {
        if self.on_ground {
            self.vel.y = 0.0;
            return;
        }
        let gravity = match (self.context, self.jump_state) {
            (Context::Jump, JumpState::Lunge) => -10.0,
            (Context::Jump, JumpState::Slam) => self.stats.slam_gravity,
            _ => self.stats.gravity,
        };
        self.vel.y += gravity * dt;
    }

    /// Raise vertical speed so the jump tops out `height` above where it started.
    fn make_jump_reach(&mut self, height: f32, force: bool) {
        let remaining = height - (self.pos.y - self.jump_start_y);
        if remaining > 0.0 {
            let speed = (-2.0 * self.stats.gravity * remaining).sqrt();
            if force || speed > self.vel.y {
                self.vel.y = speed;
            }
        } else if force {
            self.vel.y = 0.0;
        }
    }

    fn takeoff(&mut self, clips: &Clips) {
        if self.context == Context::WeaponOut {
            self.weapon_out = true;
        } else if self.context == Context::WeaponIn {
            self.weapon_out = false;
        }
        if self.jump_reentry_timer <= 0.0 {
            self.jump_sequence = 0;
        }
        // Jumping while the stick has just been thrown the other way is a backflip.
        if clips.has(Action::Flip) && self.delayed_turn_timer > 0.0 {
            self.jump_state = JumpState::Flip;
            self.vel.y = self.stats.jump_2_speed;
            self.set_context(Context::Jump, Action::Flip);
        } else {
            self.jump_state = JumpState::Basic;
            self.jump_start_y = self.pos.y;
            self.jump_sequence += 1;
            if self.jump_sequence > 1 {
                // A second jump straight after landing is the taller one.
                let anim = if clips.has(Action::Jump2) { Action::Jump2 } else { Action::Jump };
                self.vel.y = self.stats.jump_2_speed;
                self.set_context(Context::Jump, anim);
            } else {
                self.vel.y = self.stats.jump_speed;
                self.set_context(Context::Jump, Action::Jump);
            }
        }
        self.second_jump_taken = false;
        self.jump_falling = false;
        self.jump_clip_done_timer = 0.0;
        self.ground_grace = 0.0;
        self.context_timer = 0.0;
        self.jump_button_latched = true;
        self.on_ground = false;
        self.delayed_turn_timer = 0.0;
        self.events.push(Event::Jump { second: self.jump_sequence > 1 });
    }

    fn jump_code(&mut self, input: &Input, clips: &Clips, dt: f32) {
        if self.jump_reentry_timer > 0.0 {
            self.jump_reentry_timer -= dt;
        }
        let landing = matches!(self.context, Context::LandLunge | Context::LandSlam) || self.context.is_soft_landing();
        if landing {
            let soft = self.context.is_soft_landing();
            if self.context == Context::LandLunge && self.combo_flags & COMBO_HIT_DONE == 0 {
                let hit_frame = clips.get(self.context_anim).map_or(0.0, |c| c.frames[0]);
                if hit_frame > 0.0 && self.anim.action == self.context_anim && self.anim.time + 1.0 >= hit_frame {
                    self.combo_flags |= COMBO_HIT_DONE;
                    self.events.push(Event::ComboHit { strength: 1 });
                }
            }
            if soft && input.jump_pressed {
                self.takeoff(clips);
                return;
            }
            if soft && self.input_speed > 0.0 {
                self.context = Context::None;
                return;
            }
            self.context_timer -= dt;
            if self.context_timer <= 0.0 {
                if !soft && input.action_held {
                    self.start_hold();
                } else {
                    self.context = Context::None;
                }
            }
            return;
        }

        if self.context != Context::Jump {
            if input.jump_pressed && (self.on_ground || self.ground_grace > 0.0) && self.context.can_start_jump() {
                self.takeoff(clips);
            }
            return;
        }

        if self.jump_button_latched && !input.jump_held {
            self.jump_button_latched = false;
        }
        self.context_timer += dt;
        let vertical_speed = self.vel.y;
        // A basic jump that has gone on for two seconds without descending is just a fall.
        if matches!(self.jump_state, JumpState::Basic | JumpState::ComboJump)
            && self.context_timer >= 2.0
            && !self.on_ground
            && self.vel.y >= 0.0
        {
            self.context = Context::None;
            self.jump_reentry_timer = 0.0;
            return;
        }
        let landed = (self.on_ground && self.context_timer >= 0.1)
            || (matches!(self.jump_state, JumpState::Lunge | JumpState::Slam) && self.context_timer >= 2.5);
        if !landed {
            // Attack in the air: a lunge from a single jump, a slam from a double jump or flip.
            if input.action_pressed && vertical_speed > -1.25 && !self.jump_falling {
                if self.jump_state == JumpState::Basic && self.jump_sequence < 2 && clips.has(Action::Lunge) {
                    self.start_lunge(clips);
                    return;
                }
                let slam_ready = match self.jump_state {
                    JumpState::Basic => self.jump_sequence == 2,
                    JumpState::Flip | JumpState::ComboJump => true,
                    _ => false,
                };
                if slam_ready && clips.has(Action::Slam) {
                    self.jump_state = JumpState::Slam;
                    self.context_timer = 0.0;
                    self.jump_sequence = 0;
                    self.vel.y = self.stats.slam_jump_speed;
                    self.weapon_out = true;
                    self.set_context(Context::Jump, Action::Slam);
                    self.events.push(Event::SaberSwing);
                    return;
                }
            }
            if input.jump_pressed && self.jump_state == JumpState::Basic && self.jump_sequence <= 1 && self.vel.y > -1.25
            {
                self.make_jump_reach(self.stats.jump_height(self.stats.jump_2_speed), false);
                self.jump_sequence += 1;
                self.second_jump_taken = true;
                self.jump_falling = false;
                self.jump_clip_done_timer = 0.0;
                let anim = if clips.has(Action::Jump2) { Action::Jump2 } else { Action::Jump };
                self.set_context(Context::Jump, anim);
                self.context_timer = 0.0;
                self.jump_button_latched = true;
                self.events.push(Event::Jump { second: true });
            }
            return;
        }

        let restart_jump = input.jump_held
            && !self.jump_button_latched
            && !matches!(self.jump_state, JumpState::Lunge | JumpState::Slam);
        let moving = self.input_speed > 0.0;
        self.jump_reentry_timer = 0.0;
        match self.jump_state {
            JumpState::Lunge if clips.has(Action::LungeLand) => {
                self.set_context(Context::LandLunge, Action::LungeLand);
                self.context_timer = clips.duration(Action::LungeLand);
                self.combo_flags &= !COMBO_HIT_DONE;
                self.events.push(Event::Land { hard: true });
            }
            JumpState::Slam if clips.has(Action::SlamLand) => {
                self.set_context(Context::LandSlam, Action::SlamLand);
                let duration = clips.duration(Action::SlamLand);
                self.context_timer = if duration <= 0.0 { 0.75 } else { duration };
                self.events.push(Event::Judder(0.5));
                self.events.push(Event::ComboHit { strength: 3 });
                self.events.push(Event::Land { hard: true });
            }
            JumpState::ComboJump if clips.has(Action::ComboLand) => {
                self.facing = self.facing.wrapping_add(0x8000);
                self.move_facing = self.move_facing.wrapping_add(0x8000);
                if moving {
                    self.context = Context::None;
                } else {
                    self.set_context(Context::LandComboJump, Action::ComboLand);
                    self.context_timer = clips.duration(Action::ComboLand);
                }
                self.events.push(Event::Land { hard: false });
            }
            JumpState::Flip if clips.has(Action::FlipLand) => {
                if moving {
                    self.context = Context::None;
                } else {
                    self.set_context(Context::LandFlip, Action::FlipLand);
                    self.context_timer = clips.duration(Action::FlipLand);
                }
                self.events.push(Event::Land { hard: false });
            }
            _ => {
                if self.jump_state == JumpState::Basic && self.jump_sequence >= 2 && clips.has(Action::Land2) {
                    if moving {
                        self.context = Context::None;
                    } else {
                        self.set_context(Context::LandJump2, Action::Land2);
                        self.context_timer = clips.duration(Action::Land2);
                    }
                } else if self.jump_state == JumpState::Basic && clips.has(Action::Land) && !moving {
                    // After a quarter-second or more of the fall clip the heavier landing plays.
                    let anim = if self.fall_timer >= 0.2 && clips.has(Action::Land2) { Action::Land2 } else { Action::Land };
                    self.set_context(Context::LandJump, anim);
                    self.context_timer = clips.duration(anim);
                    self.jump_reentry_timer = if self.jump_sequence > 1 { 0.0 } else { 0.2 };
                } else {
                    self.context = Context::None;
                    self.jump_reentry_timer = 0.2;
                }
                self.events.push(Event::Land { hard: false });
            }
        }
        if restart_jump {
            self.takeoff(clips);
        }
    }

    fn start_lunge(&mut self, clips: &Clips) {
        self.context_timer = 0.0;
        self.jump_state = JumpState::Lunge;
        self.jump_falling = false;
        self.vel.y = 0.0;
        self.jump_sequence = 0;
        self.make_jump_reach(self.stats.height, false);
        self.weapon_out = true;
        self.set_context(Context::Jump, Action::Lunge);
        let _ = clips;
        self.events.push(Event::SaberSwing);
    }

    fn start_hold(&mut self) {
        self.context = Context::Hold;
        self.context_timer = 0.0;
        self.hold_anim = None;
    }

    /// Pick one of the block clips at random, never the same one twice running.
    fn new_block_action(&mut self, clips: &Clips) -> Option<Action> {
        let options: Vec<u8> = (1..=3).filter(|&n| clips.has(Action::Block(n))).collect();
        if options.is_empty() {
            return None;
        }
        let mut pick = options[0];
        if options.len() > 1 {
            loop {
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                pick = options[(self.seed >> 16) as usize % options.len()];
                if pick != self.previous_block {
                    break;
                }
            }
        }
        self.previous_block = pick;
        Some(Action::Block(pick))
    }

    fn start_block(&mut self, clips: &Clips, bolt: Vec3) {
        let Some(action) = self.new_block_action(clips) else { return };
        self.set_context(Context::Block, action);
        self.block_latched = false;
        self.block_target = bolt;
        self.context_timer = 0.3;
        self.events.push(Event::Block);
    }

    /// A block winds up for 0.3 s, then lasts while bolts keep coming and the button is held;
    /// 0.2 s after that it ends, into a held guard if the button is still down.
    fn block_code(&mut self, input: &Input, clips: &Clips, dt: f32) {
        if self.context != Context::Block {
            return;
        }
        if !self.block_latched {
            self.context_timer -= dt;
            if self.context_timer <= 0.0 {
                self.block_latched = true;
                self.context_timer = 0.2;
            }
            return;
        }
        match input.incoming_bolt {
            Some(bolt) if input.action_pressed => self.start_block(clips, bolt),
            Some(_) if input.action_held => self.context_timer = 0.2,
            _ => {
                self.context_timer -= dt;
                if self.context_timer <= 0.0 {
                    if input.action_held {
                        self.start_hold();
                    } else {
                        self.context = Context::None;
                    }
                }
            }
        }
    }

    /// What a bolt arriving now would do.
    pub fn deflect_mode(&self) -> Deflect {
        match self.context {
            Context::Block => Deflect::Back,
            Context::Hold if self.weapon_out => Deflect::Scatter,
            _ => Deflect::None,
        }
    }

    /// A bolt has just been turned away: a held guard plays a fresh block clip.
    pub fn on_deflect(&mut self, clips: &Clips) {
        if self.context == Context::Hold {
            self.hold_anim = self.new_block_action(clips);
            self.context_started = true;
        }
    }

    /// A bolt got through: lose a heart and get nudged along its path. Out of hearts, the
    /// character is restored at once (the game breaks the figure apart and rebuilds it).
    pub fn take_hit(&mut self, direction: Vec3) {
        self.vel.x += direction.x * self.stats.walk_speed;
        self.vel.z += direction.z * self.stats.walk_speed;
        self.hit_points = self.hit_points.saturating_sub(1);
        self.events.push(Event::Hurt);
        if self.hit_points == 0 {
            self.hit_points = self.stats.hit_points;
            self.events.push(Event::Died);
        }
    }

    fn hold_code(&mut self, input: &Input) {
        if self.context == Context::Hold && !input.action_held {
            self.context = Context::None;
        }
    }

    /// Drawing and putting away the saber. Standing still it is a full animation; on the move
    /// the blade simply extends or retracts.
    fn weapon_code(&mut self, clips: &Clips, dt: f32) {
        match self.context {
            Context::WeaponOut | Context::WeaponIn => {
                let out = self.context == Context::WeaponOut;
                self.context_timer -= dt;
                if let Some(clip) = clips.get(self.context_anim) {
                    let (start, end) = (clip.frames[0], clip.frames[1]);
                    if self.anim.action == self.context_anim && end > start {
                        let t = ((self.anim.time + 1.0 - start.max(1.0)) / (end - start.max(1.0))).clamp(0.0, 1.0);
                        self.blade_scale = if out { t } else { 1.0 - t };
                    }
                }
                if self.context_timer <= 0.0 {
                    self.weapon_out = out;
                    self.blade_scale = if out { 1.0 } else { 0.0 };
                    self.context = Context::None;
                }
            }
            _ => {
                // The quick version takes about a fifth of a second (not measured from the game).
                let target = if self.weapon_out { 1.0 } else { 0.0 };
                self.blade_scale = crate::seek_linear(self.blade_scale, target, dt * 5.0);
            }
        }
    }

    fn set_weapon(&mut self, out: bool, clips: &Clips) {
        let anim = if out { Action::WeaponOut } else { Action::WeaponIn };
        let calm = matches!(self.context, Context::None) || self.context.is_soft_landing();
        if self.on_ground && self.input_speed == 0.0 && clips.has(anim) && calm {
            self.set_context(if out { Context::WeaponOut } else { Context::WeaponIn }, anim);
            self.context_timer = clips.duration(anim);
        } else {
            self.weapon_out = out;
        }
        self.events.push(if out { Event::SaberOn } else { Event::SaberOff });
    }

    fn combo_code(&mut self, input: &Input, clips: &Clips, dt: f32) {
        if self.context == Context::Combo {
            self.combo_step(input, clips, dt);
            return;
        }
        if input.special_pressed && self.weapon_out && self.context != Context::WeaponIn {
            let busy = matches!(self.context, Context::LandLunge | Context::LandSlam | Context::WeaponOut)
                || (self.context == Context::Jump && matches!(self.jump_state, JumpState::Lunge | JumpState::Slam));
            if !busy {
                self.set_weapon(false, clips);
            }
            return;
        }
        if !input.action_pressed {
            return;
        }
        if !self.weapon_out {
            if !matches!(self.context, Context::WeaponOut | Context::Hold) {
                self.set_weapon(true, clips);
            }
            return;
        }
        let can_start = matches!(self.context, Context::None | Context::WeaponIn | Context::WeaponOut)
            || self.context.is_soft_landing();
        if !self.on_ground || !can_start {
            return;
        }
        // With a bolt about to arrive, the attack button blocks instead of swinging.
        if let Some(bolt) = input.incoming_bolt {
            self.start_block(clips, bolt);
            return;
        }
        self.face_input();
        // Alternate between the two opening swings, starting with chain 2 when it exists.
        let chain = if clips.has(Action::Combo(2, 0)) && !self.combo_alternate { 2 } else { 1 };
        self.combo_alternate = chain == 2;
        if !clips.has(Action::Combo(chain, 0)) {
            return;
        }
        self.combo_chain = chain;
        self.combo_stage = 0;
        self.combo_branch = 0;
        self.combo_input_timer = 0.0;
        self.combo_latched = false;
        self.combo_timing = true;
        self.combo_flags = 0;
        self.blade_scale = 1.0;
        self.vel.y = 0.0;
        self.set_context(Context::Combo, Action::Combo(chain, 0));
        self.events.push(Event::SaberSwing);
    }

    fn face_input(&mut self) {
        if self.input_speed > 0.0 {
            self.move_facing = self.input_angle;
        }
    }

    fn combo_hit(&mut self, strength: u8) {
        self.combo_flags |= COMBO_HIT_DONE;
        self.events.push(Event::ComboHit { strength });
    }

    fn combo_step(&mut self, input: &Input, clips: &Clips, dt: f32) {
        if self.combo_flags & COMBO_WAITING != 0 {
            // The swing has finished: a fifth of a second to press again for the next one.
            if (!input.action_pressed && !self.combo_latched) || self.combo_stage > 1 {
                self.combo_input_timer -= dt;
                if self.combo_input_timer <= 0.0 {
                    self.context = Context::None;
                    if input.action_held {
                        self.start_hold();
                    }
                } else if self.combo_stage < 2 && input.jump_pressed && clips.has(Action::ComboJump) {
                    self.jump_state = JumpState::ComboJump;
                    self.context_timer = 0.0;
                    self.delayed_turn_timer = 0.0;
                    self.jump_button_latched = true;
                    self.vel.y = self.stats.jump_2_speed;
                    self.on_ground = false;
                    self.set_context(Context::Jump, Action::ComboJump);
                    self.events.push(Event::Jump { second: true });
                }
                return;
            }
            self.face_input();
            self.combo_flags &= !COMBO_WAITING;
            self.combo_stage = if self.combo_stage == 1 { 2 } else { 1 };
            // A well-timed press (not mashed early) takes the "b" branch when the clip exists.
            let pick = |timed: u8, plain: u8| {
                if self.combo_timing && clips.has(Action::Combo(self.combo_chain, timed)) {
                    timed
                } else {
                    plain
                }
            };
            self.combo_branch = if self.combo_stage == 1 {
                pick(2, 1)
            } else if self.combo_branch == 1 {
                pick(4, 3)
            } else {
                pick(6, 5)
            };
            self.combo_timing = true;
            self.combo_latched = false;
            self.combo_flags &= !(COMBO_HIT_DONE | COMBO_PAST_FRAME2);
            self.set_context(Context::Combo, Action::Combo(self.combo_chain, self.combo_branch));
            self.events.push(Event::SaberSwing);
            return;
        }

        let Some(clip) = clips.get(self.context_anim) else {
            self.context = Context::None;
            return;
        };
        let playing = self.anim.action == self.context_anim && !self.context_started;
        let settled = playing && !self.anim.blending();
        // Event frames in the character file count from 1.
        let time = self.anim.time + 1.0;
        let strength = if self.combo_branch == 6 { 3 } else { 1 };
        if settled && self.combo_stage == 2 {
            if self.combo_flags & COMBO_HIT_DONE == 0 && clip.frames[0] > 0.0 && time >= clip.frames[0] {
                self.combo_hit(strength);
            }
            if clip.frames[1] > 0.0 && time >= clip.frames[1] {
                self.combo_flags |= COMBO_STAGE2_END;
            }
            let ending = self.anim.finished || (clip.blend_out > 0.0 && clip.remaining(self.anim.time) < clip.blend_out);
            if ending {
                self.context = Context::None;
                if self.combo_flags & COMBO_HIT_DONE == 0 {
                    self.combo_hit(strength);
                }
                if input.action_held {
                    self.start_hold();
                }
            }
            return;
        }
        if settled && self.anim.finished {
            self.combo_input_timer = 0.2;
            self.combo_flags |= COMBO_WAITING;
            if self.combo_flags & COMBO_HIT_DONE == 0 {
                self.combo_hit(1);
            }
            return;
        }
        if playing {
            if self.combo_flags & COMBO_HIT_DONE == 0 && clip.frames[0] > 0.0 && time >= clip.frames[0] {
                self.combo_hit(if self.combo_branch == 2 { 2 } else { 1 });
            }
            if clip.frames[1] > 0.0 && time >= clip.frames[1] {
                self.combo_flags |= COMBO_PAST_FRAME2;
            }
            let can_queue = clip.remaining(self.anim.time) < 0.1;
            if input.action_pressed && !self.combo_latched {
                if can_queue {
                    self.combo_latched = true;
                } else {
                    self.combo_timing = false;
                }
            }
        }
    }

    /// Landing from a fall that was not a jump (walked off an edge, or a jump that timed out).
    fn fall_land_code(&mut self, clips: &Clips) {
        if self.context == Context::None {
            if self.on_ground && !self.was_on_ground && self.fall_timer >= 0.2 && self.input_speed == 0.0 {
                let anim = if clips.has(Action::Land2) { Action::Land2 } else { Action::Land };
                if clips.has(anim) {
                    self.set_context(Context::LandJump, anim);
                    self.context_timer = clips.duration(anim);
                }
                self.events.push(Event::Land { hard: false });
            }
        }
    }

    fn directional(&mut self, clips: &Clips, dt: f32) {
        let has_input = self.input_speed > 0.0;

        // Reversing the stick does not turn the body at once: the turn is held back a fifth of
        // a second (which is the window for a backflip).
        if self.delayed_turn_timer > 0.0 {
            if has_input {
                self.delayed_turn_target = self.input_angle;
            }
            self.delayed_turn_timer -= dt;
            if self.delayed_turn_timer <= 0.0 {
                self.move_facing = self.delayed_turn_target;
            }
        } else if clips.has(Action::Flip)
            && matches!(self.context, Context::None | Context::LandJump)
            && has_input
            && rot_diff(self.move_facing, self.input_angle).abs() > 0x6aaa
        {
            self.delayed_turn_timer = 0.2;
            self.delayed_turn_target = self.input_angle;
        }

        // Heading.
        let follows_stick = match self.context {
            Context::None | Context::Hold => self.context == Context::None,
            Context::Jump => self.jump_state == JumpState::Basic || self.jump_falling,
            _ => false,
        };
        if follows_stick && has_input && self.delayed_turn_timer <= 0.0 && !self.context.locks_heading() {
            self.move_facing = self.input_angle;
        }
        // A block that is still winding up turns to face the bolt.
        if self.context == Context::Block && !self.block_latched {
            self.move_facing = atan2d(self.block_target.x - self.pos.x, self.block_target.z - self.pos.z);
        }
        self.facing = seek_rot(self.facing, self.move_facing, 8.0 * self.stats.turn_rate, dt);

        // Target velocity.
        let along = |angle: Ang, speed: f32| Vec3::new(ang_sin(angle) * speed, 0.0, ang_cos(angle) * speed);
        let mut snap = false;
        let walking = |player: &Player| {
            if !has_input {
                return Vec3::ZERO;
            }
            let mut speed = player.input_speed;
            if player.context == Context::Jump && player.jump_state == JumpState::Basic {
                speed *= player.stats.jump_move_speed_scale;
            }
            along(player.input_angle, speed)
        };
        self.target_vel = match self.context {
            Context::Combo => {
                if self.combo_flags & (COMBO_WAITING | COMBO_STAGE2_END | COMBO_PAST_FRAME2) != 0 {
                    snap = true;
                    Vec3::ZERO
                } else {
                    along(self.move_facing, clips.speed(self.context_anim))
                }
            }
            Context::Jump => match self.jump_state {
                JumpState::Lunge => {
                    let speed = clips.get(Action::Lunge).map_or(1.5, |c| c.speed);
                    along(self.move_facing, speed)
                }
                JumpState::Slam => Vec3::ZERO,
                JumpState::ComboJump => along(self.move_facing, self.stats.run_speed),
                JumpState::Basic | JumpState::Flip => walking(self),
            },
            Context::None => walking(self),
            _ => Vec3::ZERO,
        };
        if snap {
            self.vel.x = 0.0;
            self.vel.z = 0.0;
        } else {
            self.vel.x = seek_val(self.vel.x, self.target_vel.x, self.stats.acceleration, dt);
            self.vel.z = seek_val(self.vel.z, self.target_vel.z, self.stats.acceleration, dt);
        }
    }

    fn integrate(&mut self, dt: f32) {
        self.was_on_ground = self.on_ground;
        self.pos.x += self.vel.x * dt;
        self.pos.y += self.vel.y * dt;
        self.pos.z += self.vel.z * dt;
        if self.pos.y <= self.floor_y && self.vel.y <= 0.0 {
            self.pos.y = self.floor_y;
            self.vel.y = 0.0;
            self.on_ground = true;
        } else {
            self.on_ground = false;
        }
    }

    /// Move through a level: walls push back, floors are found by probing down, low steps are
    /// walked up and down.
    fn integrate_in(&mut self, world: &World, dt: f32) {
        self.was_on_ground = self.on_ground;
        let old_y = self.pos.y;
        self.pos.x += self.vel.x * dt;
        self.pos.z += self.vel.z * dt;
        // Only a character on the ground can step up; in the air every riser is a wall.
        let step = if self.on_ground { STEP_HEIGHT } else { 0.0 };
        if world.push_out(&mut self.pos, self.stats.radius, self.stats.height, step) {
            // Lose the part of the velocity that was spent going into the wall.
            let (dx, dz) = (self.pos.x - self.safe_pos.x, self.pos.z - self.safe_pos.z);
            if dt > 0.0 {
                let limit = (dx * dx + dz * dz).sqrt() / dt;
                let speed = self.vel.horizontal_length();
                if speed > limit && speed > 1e-6 {
                    self.vel.x *= limit / speed;
                    self.vel.z *= limit / speed;
                }
            }
        }
        self.pos.y += self.vel.y * dt;
        if self.vel.y > 0.0 {
            let head = self.stats.height;
            if let Some(ceiling) = world.ceiling_between(self.pos.x, self.pos.z, old_y + head * 0.5, self.pos.y + head) {
                self.pos.y = ceiling - head;
                self.vel.y = 0.0;
            }
        }
        let floor = if self.was_on_ground && self.vel.y <= 0.0 {
            world.floor_between(self.pos.x, self.pos.z, self.pos.y + STEP_HEIGHT, self.pos.y - STEP_HEIGHT)
        } else if self.vel.y <= 0.0 {
            // Falling: land only on ground the feet actually came down through.
            world.floor_between(self.pos.x, self.pos.z, old_y + 0.01, self.pos.y)
        } else {
            None
        };
        match floor {
            Some(y) => {
                self.pos.y = y;
                self.vel.y = 0.0;
                self.on_ground = true;
                self.floor_y = y;
                self.safe_pos = self.pos;
            }
            None => {
                self.on_ground = false;
                self.floor_y = world.floor_between(self.pos.x, self.pos.z, self.pos.y, self.pos.y - 50.0).unwrap_or(self.pos.y - 50.0);
                if !self.was_on_ground {
                    self.safe_pos.x = self.pos.x;
                    self.safe_pos.z = self.pos.z;
                }
            }
        }
        // Fell out of the level: back to the start.
        if self.pos.y < world.lowest - 5.0 {
            self.pos = world.start.unwrap_or(Vec3::ZERO);
            self.vel = Vec3::ZERO;
            self.safe_pos = self.pos;
        }
    }

    /// Choose the clip for the current state and advance it.
    fn animate(&mut self, clips: &Clips, dt: f32) {
        let restart = std::mem::take(&mut self.context_started);
        let height = self.pos.y - self.floor_y;
        let action = match self.context {
            Context::Jump => {
                // Once the jump clip has been finished for a tenth of a second with the floor
                // more than 0.35 below, the fall clip takes over for the rest of the jump.
                if !self.jump_falling && matches!(self.jump_state, JumpState::Basic | JumpState::Flip) {
                    let done = self.anim.action == self.context_anim && self.anim.finished && !self.anim.blending();
                    if done && !restart {
                        self.jump_clip_done_timer += dt;
                        if self.jump_clip_done_timer >= 0.1 && height > 0.35 {
                            self.jump_falling = true;
                        }
                    } else {
                        self.jump_clip_done_timer = 0.0;
                    }
                }
                if self.jump_falling && clips.has(Action::Fall) {
                    Action::Fall
                } else {
                    self.context_anim
                }
            }
            Context::Hold if self.hold_anim.is_some_and(|a| !(self.anim.action == a && self.anim.finished)) => {
                self.hold_anim.unwrap_or(Action::WeaponIdle)
            }
            Context::None | Context::Hold => {
                // In the air the fall clip plays, except just after leaving the ground or when
                // about to touch down from a short drop.
                let near_floor = self.fall_timer < 0.2 && height < 0.25 && self.vel.y < 0.0;
                if !self.on_ground && !(self.ground_grace > 0.0 || near_floor) && clips.has(Action::Fall) {
                    Action::Fall
                } else {
                    self.ground_clip(clips, dt)
                }
            }
            _ => self.context_anim,
        };
        self.fall_timer = if action == Action::Fall { self.fall_timer + dt } else { 0.0 };
        self.anim.play(clips, action, restart && self.context != Context::None);
        self.anim.update(clips, dt);
    }

    /// Idle, tiptoe, walk or run. The speed tiers split halfway between the character's
    /// speeds, and two short holds stop the clip flickering: leaving the run clip for a slower
    /// one, and leaving idle, each wait a tenth of a second.
    fn ground_clip(&mut self, clips: &Clips, dt: f32) -> Action {
        let stats = &self.stats;
        let idle = if self.weapon_out && clips.has(Action::WeaponIdle) { Action::WeaponIdle } else { Action::Idle };
        let wanted = if self.input_speed <= 0.0 || self.context == Context::Hold {
            idle
        } else if self.input_speed <= (stats.tiptoe_speed + stats.walk_speed) * 0.5 && clips.has(Action::Tiptoe) {
            Action::Tiptoe
        } else if self.input_speed <= (stats.walk_speed + stats.run_speed) * 0.5 && clips.has(Action::Walk) {
            Action::Walk
        } else {
            Action::Run
        };
        let current = self.anim.action;
        let is_idle = |a: Action| matches!(a, Action::Idle | Action::WeaponIdle);
        let is_move = |a: Action| matches!(a, Action::Tiptoe | Action::Walk | Action::Run);
        let hold = (current == Action::Run && wanted != Action::Run && (is_move(wanted) || is_idle(wanted)))
            || (is_idle(current) && is_move(wanted));
        if !hold || self.anim.blending() && self.held_move_clip.is_none() {
            self.held_move_clip = None;
            self.move_clip_timer = 0.1;
            return wanted;
        }
        if self.held_move_clip != Some(current) {
            self.held_move_clip = Some(current);
            self.move_clip_timer = 0.1;
        }
        self.move_clip_timer -= dt;
        if self.move_clip_timer <= 0.0 {
            self.held_move_clip = None;
            wanted
        } else {
            current
        }
    }
    pub fn speed(&self) -> f32 {
        self.vel.horizontal_length()
    }
}
