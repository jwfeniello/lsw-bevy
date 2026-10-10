//! Blaster bolts and simple enemies.
//!
//! From the game: the "red" blaster bolt (4 units a second, 2 second life, 1 damage), the test
//! for a bolt being "incoming" (within half a second of travel and on a line through the
//! target), and what a bolt does when it reaches a Jedi (reflected straight back from a timed
//! block, scattered when the attack button is merely held, damage otherwise).
//!
//! Not from the game: when and how often an enemy decides to shoot, and the saber's reach.
//! The game drives enemies from per-level scripts that are not implemented.

use crate::clips::{Action, AnimPacket, Clips};
use crate::player::{Deflect, Player};
use crate::world::World;
use crate::{ang_cos, ang_sin, atan2d, seek_rot, Ang, Vec3};

pub const BOLT_SPEED: f32 = 4.0;
pub const BOLT_LIFE: f32 = 2.0;
/// A bolt counts as incoming once it is this many seconds from arriving.
pub const INCOMING_SECONDS: f32 = 0.5;

/// Stand-in enemy behaviour (this project's own numbers).
const ENEMY_SIGHT: f32 = 9.0;
const ENEMY_FIRE_INTERVAL: f32 = 2.2;
const SABER_REACH: f32 = 0.75;

#[derive(Clone, Debug)]
pub struct Bolt {
    pub pos: Vec3,
    pub prev: Vec3,
    pub dir: Vec3,
    pub time: f32,
    /// True once a Jedi has sent it back: it now hurts enemies instead of the player.
    pub reflected: bool,
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub pos: Vec3,
    pub facing: Ang,
    pub alive: bool,
    /// Seconds since it was destroyed (for the falling-apart effect).
    pub dead_time: f32,
    /// Direction the killing blow came from, for throwing the pieces.
    pub blow: Vec3,
    pub anim: AnimPacket,
    fire_timer: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent {
    Shot(Vec3),
    Deflected(Vec3),
    PlayerHit,
    BoltGone(Vec3),
    EnemyDestroyed(usize),
}

#[derive(Default)]
pub struct Combat {
    pub bolts: Vec<Bolt>,
    pub enemies: Vec<Enemy>,
    pub events: Vec<CombatEvent>,
    seed: u32,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn normalised(v: Vec3) -> Vec3 {
    let len = length(v);
    if len > 1e-6 {
        Vec3::new(v.x / len, v.y / len, v.z / len)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    }
}

/// Shortest distance from `p` to the segment a-b.
fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let (ab, ap) = (sub(b, a), sub(p, a));
    let len2 = ab.x * ab.x + ab.y * ab.y + ab.z * ab.z;
    let t = if len2 > 1e-12 { ((ap.x * ab.x + ap.y * ab.y + ap.z * ab.z) / len2).clamp(0.0, 1.0) } else { 0.0 };
    length(sub(p, Vec3::new(a.x + ab.x * t, a.y + ab.y * t, a.z + ab.z * t)))
}

impl Combat {
    pub fn add_enemy(&mut self, pos: Vec3, facing: Ang) {
        let delay = 1.0 + self.random() * 1.5;
        self.enemies.push(Enemy {
            pos,
            facing,
            alive: true,
            dead_time: 0.0,
            blow: Vec3::ZERO,
            anim: AnimPacket::new(Action::Idle),
            fire_timer: delay,
        });
    }

    /// 0..1.
    fn random(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / 16_777_216.0
    }

    /// The nearest bolt about to reach a target at `middle`, by the game's test: closer than
    /// half a second of travel, and travelling along a line that passes within `radius`.
    pub fn incoming(&self, middle: Vec3, radius: f32) -> Option<Vec3> {
        self.bolts
            .iter()
            .filter(|b| !b.reflected)
            .filter(|b| middle.distance(b.pos) < INCOMING_SECONDS * BOLT_SPEED)
            .filter(|b| {
                let far = Vec3::new(b.pos.x + b.dir.x * 100.0, b.pos.y + b.dir.y * 100.0, b.pos.z + b.dir.z * 100.0);
                segment_distance(middle, b.pos, far) < radius
            })
            .min_by(|a, b| middle.distance(a.pos).total_cmp(&middle.distance(b.pos)))
            .map(|b| b.pos)
    }

    /// Kill whatever the saber reaches: called on the hit frame of a swing.
    pub fn saber_hit(&mut self, player: &Player) {
        let forward = Vec3::new(ang_sin(player.facing), 0.0, ang_cos(player.facing));
        for (i, enemy) in self.enemies.iter_mut().enumerate() {
            let to = sub(enemy.pos, player.pos);
            let flat = (to.x * to.x + to.z * to.z).sqrt();
            let ahead = flat < 0.2 || (to.x * forward.x + to.z * forward.z) / flat > 0.3;
            if enemy.alive && flat < SABER_REACH && to.y.abs() < 0.5 && ahead {
                enemy.alive = false;
                enemy.blow = normalised(Vec3::new(to.x, 0.0, to.z));
                self.events.push(CombatEvent::EnemyDestroyed(i));
            }
        }
    }

    /// Advance one frame. Events accumulate; the caller clears `events` when it has read them.
    pub fn step(&mut self, player: &mut Player, player_clips: &Clips, enemy_clips: &Clips, world: Option<&World>, dt: f32) {
        let player_middle = Vec3::new(player.pos.x, player.pos.y + player.stats.height * 0.5, player.pos.z);

        // Enemies: turn to face the player and shoot at intervals.
        let mut shots = Vec::new();
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                self.enemies[i].dead_time += dt;
                continue;
            }
            let jitter = self.random();
            let enemy = &mut self.enemies[i];
            let to = sub(player_middle, enemy.pos);
            let sees = length(to) < ENEMY_SIGHT;
            if sees {
                enemy.facing = seek_rot(enemy.facing, atan2d(to.x, to.z), 8.0, dt);
                enemy.fire_timer -= dt;
                if enemy.fire_timer <= 0.0 {
                    enemy.fire_timer = ENEMY_FIRE_INTERVAL + jitter;
                    enemy.anim.play(enemy_clips, Action::Shoot, true);
                    let muzzle = Vec3::new(enemy.pos.x + ang_sin(enemy.facing) * 0.15, enemy.pos.y + 0.28, enemy.pos.z + ang_cos(enemy.facing) * 0.15);
                    shots.push((muzzle, normalised(sub(player_middle, muzzle))));
                }
            }
            let shooting = enemy.anim.action == Action::Shoot && !enemy.anim.finished;
            if !shooting {
                enemy.anim.play(enemy_clips, Action::Idle, false);
            }
            enemy.anim.update(enemy_clips, dt);
        }
        for (pos, dir) in shots {
            self.bolts.push(Bolt { pos, prev: pos, dir, time: 0.0, reflected: false });
            self.events.push(CombatEvent::Shot(pos));
        }

        // Bolts.
        let mut added = Vec::new();
        let mut index = 0;
        while index < self.bolts.len() {
            let bolt = &mut self.bolts[index];
            bolt.prev = bolt.pos;
            bolt.pos = Vec3::new(bolt.pos.x + bolt.dir.x * BOLT_SPEED * dt, bolt.pos.y + bolt.dir.y * BOLT_SPEED * dt, bolt.pos.z + bolt.dir.z * BOLT_SPEED * dt);
            bolt.time += dt;
            let (prev, pos, dir, reflected) = (bolt.prev, bolt.pos, bolt.dir, bolt.reflected);
            let mut gone = bolt.time >= BOLT_LIFE;
            if !gone && world.is_some_and(|w| w.segment_hit(prev, pos).is_some()) {
                gone = true;
                self.events.push(CombatEvent::BoltGone(pos));
            }
            if !gone && reflected {
                for (i, enemy) in self.enemies.iter_mut().enumerate() {
                    let middle = Vec3::new(enemy.pos.x, enemy.pos.y + 0.22, enemy.pos.z);
                    if enemy.alive && segment_distance(middle, prev, pos) < 0.2 {
                        enemy.alive = false;
                        enemy.blow = Vec3::new(dir.x, 0.0, dir.z);
                        self.events.push(CombatEvent::EnemyDestroyed(i));
                        gone = true;
                        break;
                    }
                }
            } else if !gone && segment_distance(player_middle, prev, pos) < player.stats.radius + 0.06 {
                gone = true;
                match player.deflect_mode() {
                    Deflect::None => {
                        player.take_hit(dir);
                        self.events.push(CombatEvent::PlayerHit);
                    }
                    mode => {
                        // Back along the way it came; a held (untimed) guard scatters it by up to
                        // 45 degrees sideways and a little up or down.
                        let back = Vec3::new(-dir.x, -dir.y, -dir.z);
                        let mut yaw = atan2d(back.x, back.z) as i32;
                        let flat = (back.x * back.x + back.z * back.z).sqrt();
                        let mut pitch = back.y.atan2(flat);
                        if mode == Deflect::Scatter {
                            yaw += (self.random() * 16384.0 - 8192.0) as i32;
                            pitch = (pitch + (self.random() * 16384.0 - 5461.0) * (std::f32::consts::TAU / 65536.0)).clamp(-0.785, 0.785);
                        }
                        let yaw = yaw as u16;
                        let out = Vec3::new(ang_sin(yaw) * pitch.cos(), pitch.sin(), ang_cos(yaw) * pitch.cos());
                        player.on_deflect(player_clips);
                        added.push(Bolt { pos: player_middle, prev: player_middle, dir: out, time: 0.0, reflected: true });
                        self.events.push(CombatEvent::Deflected(player_middle));
                    }
                }
            }
            if gone {
                self.bolts.swap_remove(index);
            } else {
                index += 1;
            }
        }
        self.bolts.extend(added);
    }
}
