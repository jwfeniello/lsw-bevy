//! How the game lights a moving object: from the level's light table it keeps the three
//! strongest ambient lights and the three strongest directional/point lights at the object's
//! position, then shades each vertex as ambient + sum of (normal . direction) * colour.
//!
//! Scenery is different: its lighting is baked into vertex colours and never goes through this.

use crate::{ang_cos, ang_sin, Ang, Vec3};

#[derive(Clone, Debug)]
pub struct Light {
    /// 1 ambient, 2/3/6/8 point, 4 directional, 5 camera-relative, 7 anti-light.
    pub kind: u8,
    pub position: Vec3,
    pub direction: Vec3,
    pub colour: [f32; 3],
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub intensity: f32,
    pub pitch: Ang,
    pub yaw: Ang,
    pub group_mask: u16,
}

/// The lighting one object receives this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectLight {
    pub ambient: [f32; 3],
    /// Direction towards the light (unit length) and its colour.
    pub directional: [(Vec3, [f32; 3]); 3],
}

impl ObjectLight {
    /// Shade one vertex. The result is in the game's display (gamma) space, 0..=1 per channel.
    pub fn shade(&self, normal: Vec3) -> [f32; 3] {
        let mut out = self.ambient;
        for (direction, colour) in &self.directional {
            let facing = normal.x * direction.x + normal.y * direction.y + normal.z * direction.z;
            if facing > 0.0 {
                for k in 0..3 {
                    out[k] += colour[k] * facing;
                }
            }
        }
        out.map(|c| c.clamp(0.0, 1.0))
    }

    /// The same lighting with directions turned by a heading, for shading in an object's own
    /// space (`heading` is the object's rotation about the vertical).
    pub fn rotated_into(&self, heading: Ang) -> Self {
        let (s, c) = (ang_sin(heading), ang_cos(heading));
        let mut out = *self;
        for (direction, _) in &mut out.directional {
            // Inverse of the engine's Y rotation.
            *direction = Vec3::new(direction.x * c - direction.z * s, direction.y, direction.z * c + direction.x * s);
        }
        out
    }
}

fn normalised(v: Vec3) -> Vec3 {
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if len > 1e-6 {
        Vec3::new(v.x / len, v.y / len, v.z / len)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    }
}

/// Full strength inside the inner radius, fading in a straight line to nothing at the outer.
fn falloff(light: &Light, distance: f32) -> f32 {
    if light.inner_radius >= light.outer_radius {
        return 1.0;
    }
    1.0 - ((distance - light.inner_radius) / (light.outer_radius - light.inner_radius)).clamp(0.0, 1.0)
}

/// Light an object at `position`. `camera_yaw` / `camera_pitch` orient the camera-relative
/// lights (kind 5).
pub fn light_object(lights: &[Light], position: Vec3, camera_yaw: Ang, camera_pitch: Ang) -> ObjectLight {
    let mut ambient: Vec<(f32, &Light)> = Vec::new();
    let mut directional: Vec<(f32, &Light)> = Vec::new();
    let mut anti: Vec<(f32, &Light)> = Vec::new();
    for light in lights {
        let distance = position.distance(light.position);
        match light.kind {
            1 | 2 | 3 | 4 | 6 | 8 | 7 => {
                if distance >= light.outer_radius {
                    continue;
                }
                let strength = falloff(light, distance);
                if strength == 0.0 {
                    continue;
                }
                match light.kind {
                    1 => ambient.push((strength, light)),
                    7 => anti.push((strength, light)),
                    _ if light.colour != [0.0; 3] => directional.push((strength, light)),
                    _ => {}
                }
            }
            // Camera-relative lights always apply and outrank everything while being picked.
            5 => directional.push((2.0, light)),
            _ => {}
        }
    }
    // Only the three strongest of each kind are kept.
    ambient.sort_by(|a, b| b.0.total_cmp(&a.0));
    directional.sort_by(|a, b| b.0.total_cmp(&a.0));
    anti.truncate(3);
    let dimmed = |light: &Light, strength: f32| {
        let cut = anti
            .iter()
            .filter(|(_, a)| a.group_mask == 0 || a.group_mask & light.group_mask != 0)
            .map(|(s, _)| *s)
            .fold(0.0f32, f32::max);
        strength * (1.0 - cut)
    };

    let mut out = ObjectLight::default();
    for (strength, light) in ambient.iter().take(3) {
        let scale = dimmed(light, strength * light.intensity);
        for k in 0..3 {
            out.ambient[k] = (out.ambient[k] + light.colour[k] * scale).min(1.0);
        }
    }
    for (slot, (strength, light)) in directional.iter().take(3).enumerate() {
        let (direction, strength) = match light.kind {
            4 => (light.direction, *strength),
            5 => {
                // (0,0,1) pitched and turned by the light's own angles, then by the camera's.
                let turn = |v: Vec3, pitch: Ang, yaw: Ang| {
                    let (sp, cp) = (ang_sin(pitch), ang_cos(pitch));
                    let v = Vec3::new(v.x, v.y * cp - v.z * sp, v.y * sp + v.z * cp);
                    let (sy, cy) = (ang_sin(yaw), ang_cos(yaw));
                    Vec3::new(v.x * cy + v.z * sy, v.y, v.z * cy - v.x * sy)
                };
                let local = turn(Vec3::new(0.0, 0.0, 1.0), light.pitch, light.yaw);
                (turn(local, camera_pitch, camera_yaw), 1.0)
            }
            _ => (
                Vec3::new(light.position.x - position.x, light.position.y - position.y, light.position.z - position.z),
                *strength,
            ),
        };
        let scale = dimmed(light, strength * light.intensity);
        out.directional[slot] = (normalised(direction), light.colour.map(|c| c * scale));
    }
    out
}

/// Lighting for the flat test floor, where there is no level light table (this project's own).
pub fn test_lights() -> Vec<Light> {
    let base = Light {
        kind: 1,
        position: Vec3::ZERO,
        direction: Vec3::new(0.0, 0.0, 1.0),
        colour: [0.55, 0.55, 0.58],
        inner_radius: 1.0e9,
        outer_radius: 2.0e9,
        intensity: 1.0,
        pitch: 0,
        yaw: 0,
        group_mask: 0,
    };
    vec![base.clone(), Light { kind: 4, direction: Vec3::new(-0.35, 0.8, -0.5), colour: [0.6, 0.6, 0.56], ..base }]
}
