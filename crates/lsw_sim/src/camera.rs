//! The follow camera.
//!
//! The game works in two halves. A per-level "sock" (camera rail) system decides where the
//! camera would like to be and what it looks at; then a common back half eases the real camera
//! towards that, derives yaw and pitch from camera-to-target, eases those too, and adds judder.
//! The back half here follows the game. Levels with rails use them (`World::camera_goal`);
//! anywhere else the front half is a stand-in: one straight rail that keeps a fixed offset
//! from the player.
//!
//! The game never tests its camera against scenery. Rails are drawn where nothing gets in the
//! way, a rail set may ask for the camera to be kept a set height above the ground under it,
//! and when scenery does hide a player the game marks them with an arrow (`HiddenCheck`).

use crate::world::World;
use crate::{ang_cos, ang_sin, atan2d, rot_diff, seek_linear, seek_rot, Ang, Vec3};

/// Stand-in rail only: how far above the ground under it the camera is kept, how far over the
/// camera ground is looked for, and how far out to the sides. This project's own numbers; the
/// game's levels give their own height per rail set.
const STAND_IN_CLEARANCE: f32 = 0.5;
const STAND_IN_SEARCH_ABOVE: f32 = 3.0;
const STAND_IN_MARGIN: f32 = 0.3;

/// The game's test for "this player is hidden from the camera by scenery".
///
/// On every other frame it picks a random point in the upper half of the character's body and
/// casts a ray to it from the camera. When the answer has stayed the same for 0.3 seconds it
/// becomes the player's hidden state, and `fade` moves towards it at 3 a second.
#[derive(Clone, Debug, Default)]
pub struct HiddenCheck {
    blocked: bool,
    pub hidden: bool,
    steady: f32,
    pub fade: f32,
    frame: u32,
    seed: u32,
}

impl HiddenCheck {
    fn random(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / 16_777_216.0
    }

    /// `middle` is the middle of the body. `radius` and `half_height` shape the region sampled
    /// (which of the character's sizes the game uses here is not known; these are stand-ins).
    pub fn update(&mut self, camera: Vec3, middle: Vec3, radius: f32, half_height: f32, world: &World, dt: f32) {
        let was = self.blocked;
        let mut changed = false;
        self.frame = self.frame.wrapping_add(1);
        if self.frame & 1 == 0 {
            let (reach, tilt, turn) = (self.random() * radius, self.random() * std::f32::consts::FRAC_PI_2, (self.random() * 65536.0) as u32 as Ang);
            let (flat, up) = (reach * tilt.cos(), reach * tilt.sin() * (half_height / radius.max(1e-6)));
            let sample = Vec3::new(middle.x + ang_sin(turn) * flat, middle.y + up, middle.z + ang_cos(turn) * flat);
            self.blocked = world.segment_hit(camera, sample).is_some();
            if self.blocked != was {
                self.steady = 0.0;
                changed = true;
            }
        }
        if !changed && self.steady < 0.3 {
            self.steady += dt;
            if self.steady >= 0.3 {
                self.steady = 0.3;
                self.hidden = was;
            }
        }
        self.fade = seek_linear(self.fade, if self.hidden { 1.0 } else { 0.0 }, 3.0 * dt);
    }
}

#[derive(Clone, Debug)]
pub struct Rail {
    /// Camera position relative to the focus point.
    pub offset: Vec3,
    /// How quickly the position closes on its goal (levels default to 5).
    pub position_seek: f32,
    pub angle_seek: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
}

impl Default for Rail {
    fn default() -> Self {
        // Stand-in framing: about 2.4 units back, looking 24 degrees down.
        Self { offset: Vec3::new(0.0, 0.98, -2.2), position_seek: 5.0, angle_seek: 5.0, fov: 0.75 }
    }
}

#[derive(Clone, Debug)]
pub struct GameCamera {
    pub rail: Rail,
    pub pos: Vec3,
    pub target: Vec3,
    pub yaw: Ang,
    /// Positive looks down.
    pub pitch: Ang,
    pub roll: Ang,
    judder_time: f32,
    judder_duration: f32,
    judder_reverse: bool,
    judder_axis: u8,
    /// Extra rotation from judder this frame, as a 16-bit angle about `judder_axis` (0 = pitch,
    /// 1 = yaw, 2 = roll).
    pub judder_angle: i32,
    started: bool,
}

impl GameCamera {
    pub fn new(rail: Rail) -> Self {
        Self {
            rail,
            pos: Vec3::ZERO,
            target: Vec3::ZERO,
            yaw: 0,
            pitch: 0,
            roll: 0,
            judder_time: 0.0,
            judder_duration: 0.0,
            judder_reverse: false,
            judder_axis: 0,
            judder_angle: 0,
            started: false,
        }
    }

    pub fn judder_axis(&self) -> u8 {
        self.judder_axis
    }

    /// Kick the camera. A stronger kick replaces a weaker one that is still playing.
    pub fn judder(&mut self, amount: f32, axis: u8) {
        if amount.abs() > self.judder_time {
            self.judder_duration = amount.abs();
            self.judder_time = amount.abs();
            self.judder_reverse = amount < 0.0;
            self.judder_axis = axis;
        }
    }

    /// Where the stand-in rail wants the camera in a level that has collision but no rails:
    /// the fixed offset from the player, raised clear of whatever ground is under the camera
    /// (the game's height-above-terrain rule, with this project's numbers).
    pub fn stand_in_goal(&self, feet: Vec3, player_height: f32, world: &World) -> (Vec3, f32) {
        let o = self.rail.offset;
        let mut goal = Vec3::new(feet.x + o.x, feet.y + player_height * 0.5 + o.y, feet.z + o.z);
        goal.y = world.lift_above_ground(goal, STAND_IN_CLEARANCE, STAND_IN_SEARCH_ABOVE, STAND_IN_MARGIN);
        (goal, self.rail.position_seek)
    }

    /// `feet` is the player's position; the camera looks at their middle.
    pub fn update(&mut self, feet: Vec3, player_height: f32, dt: f32) {
        self.update_towards(feet, player_height, None, dt);
    }

    /// As `update`, but `rail_goal` (a position and position-seek rate from the level's camera
    /// rails) replaces the stand-in offset when present.
    pub fn update_towards(&mut self, feet: Vec3, player_height: f32, rail_goal: Option<(Vec3, f32)>, dt: f32) {
        let focus = Vec3::new(feet.x, feet.y + player_height * 0.5, feet.z);
        let (goal, position_seek) = match rail_goal {
            Some((goal, seek)) => (goal, if seek > 0.0 { seek } else { self.rail.position_seek }),
            None => (
                Vec3::new(focus.x + self.rail.offset.x, focus.y + self.rail.offset.y, focus.z + self.rail.offset.z),
                self.rail.position_seek,
            ),
        };
        self.target = focus;
        let look = |from: Vec3, to: Vec3| {
            let (dx, dz) = (to.x - from.x, to.z - from.z);
            let yaw = atan2d(dx, dz);
            let pitch = 0u16.wrapping_sub(atan2d(to.y - from.y, (dx * dx + dz * dz).sqrt()));
            (yaw, pitch)
        };
        if !self.started {
            self.started = true;
            self.pos = goal;
            (self.yaw, self.pitch) = look(self.pos, self.target);
        } else {
            let seek = (position_seek * dt).min(1.0);
            self.pos.x += (goal.x - self.pos.x) * seek;
            self.pos.y += (goal.y - self.pos.y) * seek;
            self.pos.z += (goal.z - self.pos.z) * seek;
            let (yaw, pitch) = look(self.pos, self.target);
            self.pitch = seek_rot(self.pitch, pitch, self.rail.angle_seek, dt);
            self.yaw = seek_rot(self.yaw, yaw, self.rail.angle_seek, dt);
            self.roll = seek_rot(self.roll, 0, self.rail.angle_seek, dt);
        }

        self.judder_angle = 0;
        if self.judder_time > 0.0 {
            self.judder_time -= dt;
            if self.judder_time > 0.0 {
                // A decaying sine: one full wobble per third of a second of duration.
                let degrees = ((self.judder_duration / 0.333) as i32 + 1) * 360;
                let progress = (self.judder_duration - self.judder_time) / self.judder_duration;
                let phase = (degrees as f32 / 360.0) * progress * std::f32::consts::TAU;
                let amount = (self.judder_time / self.judder_duration) * (546.0 * self.judder_duration) * phase.sin();
                self.judder_angle = if self.judder_reverse { -amount } else { amount } as i32;
            }
        }
    }

    /// How far the camera's yaw is from looking straight at the target (for tests and logs).
    pub fn yaw_error(&self) -> i32 {
        rot_diff(self.yaw, atan2d(self.target.x - self.pos.x, self.target.z - self.pos.z))
    }
}
