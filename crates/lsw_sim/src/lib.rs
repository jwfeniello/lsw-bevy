//! Gameplay rules with no renderer attached, so they can be tested with plain numbers.
//!
//! Units and conventions are the game's own: +y is up, a minifig is about 0.42 tall, speeds are
//! units per second, and headings are 16-bit angles (0x10000 = a full turn) where 0 points along
//! +z and 0x4000 along +x. Constants and rules are documented in `docs/DESIGN.md`.

pub mod camera;
pub mod clips;
pub mod combat;
pub mod lighting;
pub mod player;
pub mod world;

/// 16-bit heading. 0 = +z, 0x4000 = +x.
pub type Ang = u16;

pub fn ang_sin(a: Ang) -> f32 {
    (a as f32 * (std::f32::consts::TAU / 65536.0)).sin()
}

pub fn ang_cos(a: Ang) -> f32 {
    (a as f32 * (std::f32::consts::TAU / 65536.0)).cos()
}

pub fn ang_to_radians(a: Ang) -> f32 {
    a as f32 * (std::f32::consts::TAU / 65536.0)
}

/// Heading of the direction (x, z).
pub fn atan2d(x: f32, z: f32) -> Ang {
    (x.atan2(z) * (65536.0 / std::f32::consts::TAU)) as i32 as u16
}

/// Signed shortest turn from `from` to `to`, in the range -0x8000..=0x7fff.
pub fn rot_diff(from: Ang, to: Ang) -> i32 {
    to.wrapping_sub(from) as i16 as i32
}

/// Ease a value towards a target: closes `rate * dt` of the gap each step (at most all of it).
pub fn seek_val(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * (rate * dt).min(1.0)
}

/// Move towards a target at a constant speed.
pub fn seek_linear(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

/// `seek_val` for headings, taking the short way round.
pub fn seek_rot(current: Ang, target: Ang, rate: f32, dt: f32) -> Ang {
    let blend = (rate * dt).min(1.0);
    (current as f32 + rot_diff(current, target) as f32 * blend) as i32 as u16
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn horizontal_length(self) -> f32 {
        (self.x * self.x + self.z * self.z).sqrt()
    }

    pub fn distance(self, other: Vec3) -> f32 {
        let (dx, dy, dz) = (self.x - other.x, self.y - other.y, self.z - other.z);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

#[cfg(test)]
mod tests;
