//! Character animation (`.an3`, magic "ANI4"). See `lego-star-wars-formats.md` section 7.
//!
//! Only the channel kinds the notes verified are implemented: constants (type >= 16) and the
//! keyed type 6. Anything else is reported as an error instead of being guessed at.

use crate::ghg::{mul, Joint, Mat4, IDENTITY};
use crate::{Error, Reader, Result};

#[derive(Clone, Debug)]
enum Curve {
    Constant(f32),
    Keyed(Vec<f32>),
}

#[derive(Clone, Debug)]
struct Node {
    flags: u8,
    /// Translation xyz, rotation xyz, scale xyz.
    curves: [Curve; 9],
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub frame_count: u16,
    pub key_count: u16,
    pub first_frame: u16,
    nodes: Vec<Node>,
}

/// Translation, Euler rotation (radians, X then Y then Z) and scale of one node at one frame.
#[derive(Clone, Copy, Debug)]
pub struct NodePose {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub use_bind_rotation: bool,
}

impl Animation {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        if r.u32()? != 0x414e_4934 {
            return Err(Error::new("not an ANI4 animation"));
        }
        let node_count = r.u16()? as usize;
        let key_count = r.u16()?;
        let key_stride = r.u16()? as usize;
        let frame_count = r.u16()?;
        let curves_per_node = r.u16()? as usize;
        let first_frame = r.u16()?;
        if curves_per_node != 9 {
            return Err(Error::new(format!("unsupported curves per node: {curves_per_node}")));
        }
        let format_flags = Reader::at(data, 0x13).u8()?;
        if format_flags != 0 {
            return Err(Error::new(format!("unsupported animation format flags {format_flags:#x} (quaternion)")));
        }
        let mut h = Reader::at(data, 0x1c);
        let const_min = h.f32()?;
        let const_scale = h.f32()?;
        let ranges_at = h.u32()? as usize;
        let pool_at = h.u32()? as usize;
        let types_at = h.u32()? as usize;
        let keys_at = h.u32()? as usize;
        let flags_at = h.u32()? as usize;
        let name_bytes = Reader::at(data, 0x3c).bytes(32)?;
        let name = name_bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();

        let keyed_curves = key_stride / 4;
        let blocks = if key_count == 0 { 0 } else { (key_count as usize + 3) / 4 + 1 };
        let mut stream = 0usize;
        let mut nodes = Vec::with_capacity(node_count);
        for n in 0..node_count {
            let flags = Reader::at(data, flags_at + n).u8()?;
            let mut curves: [Curve; 9] = std::array::from_fn(|slot| Curve::Constant(if slot >= 6 { 1.0 } else { 0.0 }));
            for slot in 0..9 {
                let group_bit = [0x02u8, 0x01, 0x08][slot / 3];
                if flags & group_bit == 0 {
                    continue;
                }
                let kind = Reader::at(data, types_at + (n * 9 + slot) * 2).u16()? as usize;
                if kind >= 16 {
                    let raw = Reader::at(data, pool_at + (kind - 16) * 2).u16()?;
                    curves[slot] = Curve::Constant(raw as f32 * const_scale + const_min);
                } else if kind == 6 {
                    if stream >= keyed_curves {
                        return Err(Error::new("more keyed curves than the key stride allows"));
                    }
                    let mut range = Reader::at(data, ranges_at + stream * 8);
                    let scale = range.f32()?;
                    let minimum = range.f32()?;
                    let word = |block: usize| Reader::at(data, keys_at + block * key_stride + stream * 4).u32();
                    let mut keys = Vec::with_capacity(key_count as usize);
                    for k in 0..key_count as usize {
                        let (block, q) = (k / 4, k % 4);
                        debug_assert!(block + 1 < blocks);
                        let here = word(block)?;
                        let base = (here & 0xff) as f32;
                        let next = (word(block + 1)? & 0xff) as f32;
                        let fraction = ((here >> (8 + 6 * q)) & 63) as f32 / 63.0;
                        keys.push((base + (next - base) * fraction) * scale + minimum);
                    }
                    curves[slot] = Curve::Keyed(keys);
                    stream += 1;
                } else {
                    return Err(Error::new(format!("unsupported animation curve type {kind}")));
                }
            }
            nodes.push(Node { flags, curves });
        }
        Ok(Self { name, frame_count, key_count, first_frame, nodes })
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of frames one pass of the animation covers (the last frame equals the first when
    /// the animation loops).
    pub fn length(&self) -> f32 {
        (self.frame_count.max(1) - 1) as f32
    }

    fn sample(&self, curve: &Curve, frame: f32) -> f32 {
        match curve {
            Curve::Constant(value) => *value,
            Curve::Keyed(keys) => {
                let last = (keys.len() - 1) as f32;
                let span = self.length().max(1.0);
                let key = (frame * last / span).clamp(0.0, last);
                let i = key.floor() as usize;
                let j = (i + 1).min(keys.len() - 1);
                keys[i] + (keys[j] - keys[i]) * (key - i as f32)
            }
        }
    }

    /// Sample every node at `frame` (zero-based, relative to the first frame).
    pub fn sample_nodes(&self, frame: f32) -> Vec<NodePose> {
        self.nodes
            .iter()
            .map(|node| {
                let v = |slot: usize| self.sample(&node.curves[slot], frame);
                NodePose {
                    translation: [v(0), v(1), v(2)],
                    rotation: [v(3), v(4), v(5)],
                    scale: [v(6), v(7), v(8)],
                    use_bind_rotation: node.flags & 0x20 != 0,
                }
            })
            .collect()
    }

    pub fn print_summary(&self) {
        let keyed: usize = self.nodes.iter().flat_map(|n| &n.curves).filter(|c| matches!(c, Curve::Keyed(_))).count();
        println!(
            "{}: {} nodes, {} frames, {} keys, first frame {}, {keyed} keyed curves",
            self.name,
            self.nodes.len(),
            self.frame_count,
            self.key_count,
            self.first_frame
        );
    }
}

fn rotation_xyz(angles: [f32; 3]) -> Mat4 {
    // Row-vector rotations as the engine builds them: X first, then Y, then Z.
    let (sx, cx) = angles[0].sin_cos();
    let (sy, cy) = angles[1].sin_cos();
    let (sz, cz) = angles[2].sin_cos();
    let rx = [1.0, 0.0, 0.0, 0.0, 0.0, cx, sx, 0.0, 0.0, -sx, cx, 0.0, 0.0, 0.0, 0.0, 1.0];
    let ry = [cy, 0.0, -sy, 0.0, 0.0, 1.0, 0.0, 0.0, sy, 0.0, cy, 0.0, 0.0, 0.0, 0.0, 1.0];
    let rz = [cz, sz, 0.0, 0.0, -sz, cz, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    mul(&mul(&rx, &ry), &rz)
}

/// Linear blend of two sampled poses (`t` = 0 gives `a`). Angles take the short way round.
pub fn blend_poses(a: &[NodePose], b: &[NodePose], t: f32) -> Vec<NodePose> {
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    let lerp_angle = |x: f32, y: f32| {
        let mut d = (y - x) % std::f32::consts::TAU;
        if d > std::f32::consts::PI {
            d -= std::f32::consts::TAU;
        } else if d < -std::f32::consts::PI {
            d += std::f32::consts::TAU;
        }
        x + d * t
    };
    a.iter()
        .zip(b)
        .map(|(p, q)| NodePose {
            translation: std::array::from_fn(|i| lerp(p.translation[i], q.translation[i])),
            rotation: std::array::from_fn(|i| lerp_angle(p.rotation[i], q.rotation[i])),
            scale: std::array::from_fn(|i| lerp(p.scale[i], q.scale[i])),
            use_bind_rotation: if t < 0.5 { p.use_bind_rotation } else { q.use_bind_rotation },
        })
        .collect()
}

/// Model-space matrix of every joint for a sampled pose. Node i drives joint i.
pub fn world_matrices(joints: &[Joint], pose: &[NodePose]) -> Vec<Mat4> {
    let mut world: Vec<Mat4> = Vec::with_capacity(joints.len());
    for (i, joint) in joints.iter().enumerate() {
        let local = match pose.get(i) {
            Some(node) => {
                let mut m = rotation_xyz(node.rotation);
                for axis in 0..3 {
                    for c in 0..3 {
                        m[axis * 4 + c] *= node.scale[axis];
                    }
                }
                if node.use_bind_rotation {
                    m = mul(&m, &joint.bind_rotation);
                }
                m[12] = node.translation[0];
                m[13] = node.translation[1];
                m[14] = node.translation[2];
                // The animation system composes in a z-negated space: F * M * F, F = diag(1,1,-1).
                for k in [2, 6, 8, 9, 14] {
                    m[k] = -m[k];
                }
                m
            }
            None => joint.local_bind,
        };
        world.push(match joint.parent {
            Some(p) => mul(&local, &world[p]),
            None => local,
        });
    }
    world
}

/// The model's rest pose, for drawing without an animation.
pub fn bind_matrices(joints: &[Joint]) -> Vec<Mat4> {
    joints.iter().map(|j| j.world_bind).collect()
}

pub fn identity() -> Mat4 {
    IDENTITY
}
