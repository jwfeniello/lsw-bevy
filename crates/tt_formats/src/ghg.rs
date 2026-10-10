//! Character model (`.ghg`): a relocatable memory image of the game's own scene structures.
//!
//! Layout notes are in `lego-star-wars-formats.md` section 4. Matrices are row-major with row
//! vectors (`v' = v * M`, translation in the fourth row), exactly as stored in the file.

use crate::{Error, Reader, Result};

pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// `a * b` for row-vector matrices: apply `a` first, then `b`.
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[r * 4 + c] = (0..4).map(|k| a[r * 4 + k] * b[k * 4 + c]).sum();
        }
    }
    out
}

pub fn transform_point(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
    ]
}

pub fn transform_dir(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10],
    ]
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub name: String,
    /// `None` for a root.
    pub parent: Option<usize>,
    /// The joint's bind rotation, needed by animations that set node flag 0x20.
    pub bind_rotation: Mat4,
    pub local_bind: Mat4,
    pub inverse_bind: Mat4,
    /// Rest pose in model space (`local_bind * world[parent]`).
    pub world_bind: Mat4,
}

#[derive(Clone, Debug)]
pub struct Locator {
    pub name: String,
    pub matrix: Mat4,
    pub joint: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub colour: [f32; 3],
    pub opacity: f32,
    /// Index into `Model::texture_hashes`, if the material has a diffuse texture.
    pub diffuse: Option<usize>,
    pub normal_map: Option<usize>,
    pub vertex_descriptor: u32,
    /// Environment (reflection) map texture, on glossy materials.
    pub env_map: Option<usize>,
    /// 0 = opaque; anything else blends with what is behind it.
    pub alpha_mode: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub colour: [u8; 4],
    pub uv: [f32; 2],
    /// Up to three influences as (joint index, weight). Empty for rigid geometry.
    pub weights: [(u16, f32); 3],
}

/// One draw: a triangle list sharing a material.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub material: usize,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub skinned: bool,
    /// False when the file stores no normals for this mesh (they must be computed).
    pub has_normals: bool,
}

/// One placed piece of a level scene: a mesh and where it sits in the world.
#[derive(Clone, Debug)]
pub struct StaticMesh {
    pub matrix: Mat4,
    pub mesh: Mesh,
    /// Index of the placed object this mesh belongs to, and that object's visibility flags.
    pub object: usize,
    pub flags: u8,
    /// Name of the scene object this belongs to (empty if it has none) and its object number,
    /// which is what collision groups and gizmos refer to.
    pub name: String,
    pub scene_id: u32,
}

/// A drawable piece of a layer. A rigid piece follows one joint and its vertices are stored in
/// that joint's space; a skinned piece is stored in model space with per-vertex weights.
#[derive(Clone, Debug)]
pub struct Piece {
    pub name: String,
    pub rigid_joint: Option<usize>,
    pub meshes: Vec<Mesh>,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub pieces: Vec<Piece>,
}

pub struct Model {
    pub texture_hashes: Vec<u32>,
    pub materials: Vec<Material>,
    pub joints: Vec<Joint>,
    /// In game order: `locators[n]` is what a character file's `*_locator=n` refers to.
    pub locators: Vec<Option<Locator>>,
    pub layers: Vec<Layer>,
    pub bounds: ([f32; 3], [f32; 3]),
    /// Level scenes (.gsc): every placed mesh. Empty for characters.
    pub statics: Vec<StaticMesh>,
    /// Named objects in prop scenes (pickups, HUD pieces), each in its own space.
    pub specials: Vec<Piece>,
    /// Meshes left out because they could not be read, with the reason.
    pub skipped: Vec<String>,
    /// Named point lists (camera rails, start points, door lines) in level scenes.
    pub splines: Vec<Spline>,
}

#[derive(Clone, Debug)]
pub struct Spline {
    pub name: String,
    pub points: Vec<[f32; 3]>,
}

/// The relocated image: every pointer field holds an offset into `data` (0 = null).
struct Image {
    data: Vec<u8>,
}

impl Image {
    fn r(&self, at: usize) -> Reader<'_> {
        Reader::at(&self.data, at)
    }
    fn u8(&self, at: usize) -> Result<u8> {
        self.r(at).u8()
    }
    fn u16(&self, at: usize) -> Result<u16> {
        self.r(at).u16()
    }
    fn u32(&self, at: usize) -> Result<u32> {
        self.r(at).u32()
    }
    fn i32(&self, at: usize) -> Result<i32> {
        self.r(at).i32()
    }
    fn f32(&self, at: usize) -> Result<f32> {
        self.r(at).f32()
    }
    fn ptr(&self, at: usize) -> Result<usize> {
        Ok(self.u32(at)? as usize)
    }
    fn mat4(&self, at: usize) -> Result<Mat4> {
        self.r(at).mat4()
    }
    fn vec3(&self, at: usize) -> Result<[f32; 3]> {
        Ok([self.f32(at)?, self.f32(at + 4)?, self.f32(at + 8)?])
    }
    fn cstr(&self, at: usize) -> String {
        if at == 0 || at >= self.data.len() {
            return String::new();
        }
        let end = self.data[at..].iter().position(|&b| b == 0).map_or(self.data.len(), |p| at + p);
        self.data[at..end].iter().map(|&b| b as char).collect()
    }
}

struct Geometry {
    prim_count: usize,
    stride: usize,
    palette: Vec<u8>,
    vertex_count: usize,
    base_vertex: usize,
    first_index: usize,
    index_buffer: usize,
    vertex_buffer: usize,
}

struct Layout {
    normal: Option<usize>,
    colour: Option<usize>,
    uv: Option<usize>,
    skin: Option<usize>,
}

/// Field offsets for a vertex, from the material's 32-bit vertex descriptor. Streams appear in
/// this order when their bit is set: position, normal, tangent, binormal, colour, second
/// colour, texture coordinates, weights, joints, then two extra byte quads on level geometry.
fn layout(descriptor: u32, stride: usize) -> Result<Layout> {
    // Level files leave the binormal (and tangent) streams out even though the descriptor
    // names them.
    layout_with(descriptor, stride, true, true)
        .or_else(|_| layout_with(descriptor, stride, true, false))
        .or_else(|_| layout_with(descriptor, stride, false, false))
}

fn layout_with(descriptor: u32, stride: usize, tangent: bool, binormal: bool) -> Result<Layout> {
    let mut offset = 12;
    let mut take = |present: bool, size: usize| {
        let at = present.then_some(offset);
        if present {
            offset += size;
        }
        at
    };
    let normal = take(descriptor & 0x0088_0008 != 0, 4);
    take(tangent && descriptor & 0x0100_0020 != 0, 4);
    take(binormal && descriptor & 0x0200_0080 != 0, 4);
    let colour = take(descriptor & 0x100 != 0, 4);
    take(descriptor & 0x600 != 0, 4);
    let uv_mode = (descriptor >> 11) & 7;
    let uv = take(uv_mode != 0, [0, 8, 16, 24, 32, 16, 0, 0][uv_mode as usize]);
    let weights = take(descriptor & 0x8000 != 0, 4);
    let joints = take(descriptor & 0x2_0000 != 0, 4);
    take(descriptor & 0x0400_0000 != 0, 8);
    if offset != stride {
        return Err(Error::new(format!("vertex descriptor {descriptor:#010x} gives {offset} bytes, stride is {stride}")));
    }
    Ok(Layout { normal, colour, uv, skin: weights.and(joints).and(weights) })
}
impl Model {
    pub fn parse(file: &[u8]) -> Result<Self> {
        // --- graphics section: texture hashes, vertex buffers, index buffers ---
        let mut r = Reader::new(file);
        let section_size = r.u32()? as usize;
        let tex_word = r.u16()?;
        if tex_word & 0x8000 == 0 && tex_word != 0 {
            return Err(Error::new("inline textures are not supported"));
        }
        let mut texture_hashes = Vec::new();
        for _ in 0..(tex_word & 0x7fff) {
            texture_hashes.push(r.u32()?);
        }
        let mut vertex_buffers = Vec::new();
        for _ in 0..r.u16()? {
            let size = (r.u32()? & 0x7fff_ffff) as usize;
            vertex_buffers.push(r.bytes(size)?);
        }
        let mut index_buffers = Vec::new();
        for _ in 0..r.u16()? {
            let size = r.u32()? as usize;
            index_buffers.push(r.bytes(size)?);
        }

        // --- the image: relocate every pointer into a plain offset ---
        let base = section_size + 4;
        let mut image = Image { data: file.get(base..).ok_or(Error::new("truncated model"))?.to_vec() };
        if &image.data[0..4] != b"NU20" {
            return Err(Error::new("model image does not start with NU20"));
        }
        let mut chunks = Vec::new();
        let mut at = 16;
        while at + 8 <= image.data.len() {
            let tag = [image.data[at], image.data[at + 1], image.data[at + 2], image.data[at + 3]];
            let size = image.u32(at + 4)? as usize;
            if !tag.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) || size < 8 {
                // The one untagged block (0x20 bytes) sits between VBIB and SALI.
                at += 0x20;
                continue;
            }
            chunks.push((tag, at, size));
            at += size;
        }
        let find = |tag: &[u8; 4]| chunks.iter().find(|c| &c.0 == tag).map(|c| c.1);
        let pntr = find(b"PNTR").ok_or(Error::new("model has no PNTR chunk"))?;
        let count = image.u32(pntr + 8)? as usize;
        for i in 0..count {
            let entry = pntr + 12 + i * 4;
            let slot = (entry as i64 + image.i32(entry)? as i64) as usize;
            let value = image.i32(slot)?;
            if value != 0 {
                let target = (slot as i64 + value as i64) as u32;
                image.data[slot..slot + 4].copy_from_slice(&target.to_le_bytes());
            }
        }
        let ntbl = find(b"NTBL").ok_or(Error::new("model has no NTBL chunk"))?;
        let scene = find(b"GSNH").ok_or(Error::new("model has no GSNH chunk"))? + 8;

        // --- materials ---
        let material_ptrs = image.ptr(scene + 0x0c)?;
        let material_count = image.u32(scene + 0x10)? as usize;
        let mut materials = Vec::new();
        for i in 0..material_count {
            let m = image.ptr(material_ptrs + i * 4)?;
            let tex = |at: usize| -> Result<Option<usize>> {
                let id = image.i32(at)?;
                Ok((id >= 0 && (id as usize) < texture_hashes.len()).then_some(id as usize))
            };
            materials.push(Material {
                colour: image.vec3(m + 0x54)?,
                opacity: image.f32(m + 0x70)?,
                diffuse: tex(m + 0xb8)?,
                normal_map: tex(m + 0x100)?,
                vertex_descriptor: image.u32(m + 0x1f0)?,
                env_map: tex(m + 0x104)?,
                alpha_mode: (image.u32(m + 0x40)? & 0xf) as u8,
            });
        }

        // --- skeleton ---
        let joint_count = image.u32(scene + 0x168)? as usize;
        let joints_at = image.ptr(scene + 0x16c)?;
        let local_at = image.ptr(scene + 0x170)?;
        let inverse_at = image.ptr(scene + 0x174)?;
        let mut joints: Vec<Joint> = Vec::with_capacity(joint_count);
        for i in 0..joint_count {
            let j = joints_at + i * 0x60;
            let parent = image.u8(j + 0x50)?;
            let parent = (parent != 0xff).then_some(parent as usize);
            let local_bind = image.mat4(local_at + i * 0x40)?;
            let world_bind = match parent {
                Some(p) => mul(&local_bind, &joints.get(p).ok_or(Error::new("joint parent out of order"))?.world_bind),
                None => local_bind,
            };
            joints.push(Joint {
                name: image.cstr(image.ptr(j + 0x4c)?),
                parent,
                bind_rotation: image.mat4(j)?,
                local_bind,
                inverse_bind: image.mat4(inverse_at + i * 0x40)?,
                world_bind,
            });
        }

        // --- locators ---
        let record_count = image.u32(scene + 0x180)? as usize;
        let records_at = image.ptr(scene + 0x184)?;
        let map_count = image.u32(scene + 0x188)? as usize;
        let map_at = image.ptr(scene + 0x18c)?;
        let mut locators = Vec::new();
        for n in 0..map_count {
            let record = image.u8(map_at + n)? as usize;
            if record >= record_count {
                locators.push(None);
                continue;
            }
            let l = records_at + record * 0x50;
            // The name field's low 16 bits are an offset into the name table (see the notes).
            let name_offset = (image.u32(l + 0x40)? & 0xffff) as usize;
            locators.push(Some(Locator {
                name: image.cstr(ntbl + 0x0c + name_offset),
                matrix: image.mat4(l)?,
                joint: image.u8(l + 0x44)? as usize,
            }));
        }

        // --- display scene: items, and the geometry each piece draws ---
        let display = image.ptr(scene + 0x110)?;
        let item_count = image.u32(display + 4)? as usize;
        let items_at = image.ptr(display + 8)?;
        let read_geometry = |g: usize| -> Result<Geometry> {
            if image.u32(g)? != 6 {
                return Err(Error::new(format!("unsupported primitive type {}", image.u32(g)?)));
            }
            let mut palette = Vec::new();
            for k in 0..8 {
                match image.u8(g + 0x0a + k)? {
                    0xff => break,
                    joint => palette.push(joint),
                }
            }
            Ok(Geometry {
                prim_count: image.i32(g + 0x04)?.max(0) as usize,
                stride: image.u16(g + 0x08)? as usize,
                palette,
                vertex_count: image.i32(g + 0x18)?.max(0) as usize,
                base_vertex: image.i32(g + 0x14)?.max(0) as usize,
                first_index: image.i32(g + 0x1c)?.max(0) as usize,
                index_buffer: image.u32(g + 0x20)? as usize,
                vertex_buffer: image.u32(g + 0x24)? as usize,
            })
        };
        // The stride a geometry record states is not always what the file stores (level files
        // are 4 bytes short), so measure it: buffer size / number of vertices the indices reach.
        let mut reach = vec![0usize; vertex_buffers.len()];
        for item in 0..item_count {
            let entry = items_at + item * 16;
            if image.u8(entry)? != 0x82 {
                continue;
            }
            let Ok(g) = read_geometry(image.ptr(entry + 4)?) else { continue };
            let (Some(ib), Some(slot)) = (index_buffers.get(g.index_buffer), reach.get_mut(g.vertex_buffer)) else { continue };
            let mut ir = Reader::at(ib, g.first_index * 2);
            for _ in 0..g.prim_count + 2 {
                let Ok(index) = ir.u16() else { break };
                *slot = (*slot).max(index as usize + g.base_vertex + 1);
            }
        }
        let file_strides: Vec<usize> = vertex_buffers
            .iter()
            .zip(&reach)
            .map(|(vb, &count)| if count > 0 && vb.len() % count == 0 { vb.len() / count } else { 0 })
            .collect();
        let build_mesh = |g: &Geometry, material: usize| -> Result<Mesh> {
            let descriptor = materials.get(material).map_or(0, |m: &Material| m.vertex_descriptor);
            let stride = match file_strides.get(g.vertex_buffer) {
                // Only trust the measurement when it is the stated stride minus the streams
                // level files are known to drop; otherwise it is a coincidence of sizes.
                Some(&measured) if measured > 0 && [0, 4, 8].contains(&g.stride.wrapping_sub(measured)) => measured,
                _ => g.stride,
            };
            let lay = layout(descriptor, stride)?;
            let skinned = !g.palette.is_empty() && lay.skin.is_some();
            let ib = index_buffers.get(g.index_buffer).ok_or(Error::new("index buffer out of range"))?;
            let vb = vertex_buffers.get(g.vertex_buffer).ok_or(Error::new("vertex buffer out of range"))?;
            let mut strip = Vec::with_capacity(g.prim_count + 2);
            let mut ir = Reader::at(ib, g.first_index * 2);
            for _ in 0..g.prim_count + 2 {
                strip.push(ir.u16()? as usize + g.base_vertex);
            }
            let first = *strip.iter().min().unwrap_or(&0);
            let last = *strip.iter().max().unwrap_or(&0);
            let mut vertices = Vec::with_capacity(last - first + 1);
            for v in first..=last {
                let at = v * stride;
                let raw = vb.get(at..at + stride).ok_or(Error::new("vertex out of range"))?;
                let f = |o: usize| f32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
                let mut vertex = Vertex { position: [f(0), f(4), f(8)], colour: [255; 4], ..Default::default() };
                if let Some(o) = lay.normal {
                    vertex.normal = [0, 1, 2].map(|k| raw[o + k] as f32 / 255.0 * 2.0 - 1.0);
                }
                if let Some(o) = lay.colour {
                    vertex.colour = [raw[o], raw[o + 1], raw[o + 2], raw[o + 3]];
                }
                if let Some(o) = lay.uv {
                    vertex.uv = [f(o), f(o + 4)];
                }
                if let Some(o) = lay.skin {
                    let total: f32 = (0..3).map(|k| raw[o + k] as f32).sum();
                    for k in 0..3 {
                        let slot = raw[o + 4 + k] as usize;
                        let weight = if total > 0.0 { raw[o + k] as f32 / total } else { 0.0 };
                        let joint = g.palette.get(slot).copied().unwrap_or(0) as u16;
                        vertex.weights[k] = if weight > 0.0 { (joint, weight) } else { (0, 0.0) };
                    }
                }
                vertices.push(vertex);
            }
            // Triangle strip -> list: alternate the winding and drop degenerate triangles.
            let mut indices = Vec::with_capacity(g.prim_count * 3);
            for t in 0..g.prim_count {
                let (a, b, c) = (strip[t], strip[t + 1], strip[t + 2]);
                if a == b || b == c || a == c {
                    continue;
                }
                let tri = if t % 2 == 0 { [a, b, c] } else { [b, a, c] };
                indices.extend(tri.map(|i| (i - first) as u32));
            }
            let _ = g.vertex_count;
            Ok(Mesh { material, vertices, indices, skinned, has_normals: lay.normal.is_some() })
        };
        let layer_count = image.u32(scene + 0x190)? as usize;
        let skipped = std::cell::RefCell::new(Vec::<String>::new());
        // A "special" (0xD0 bytes) is one drawable piece; its clip object lists (material, item).
        let build_piece = |special: usize, rigid_joint: Option<usize>| -> Result<Piece> {
            let clip = image.ptr(special + 0xb0)?;
            let mut meshes = Vec::new();
            if clip != 0 {
                let draws = image.u32(clip)? as usize;
                let material_ids = image.ptr(clip + 4)?;
                let item_ids = image.ptr(clip + 8)?;
                for k in 0..draws {
                    let material = image.u32(material_ids + k * 4)? as usize;
                    let item = image.u32(item_ids + k * 4)? as usize;
                    if item >= item_count {
                        continue;
                    }
                    let entry = items_at + item * 16;
                    if image.u8(entry)? != 0x82 {
                        continue;
                    }
                    let geometry = read_geometry(image.ptr(entry + 4)?)?;
                    // In prop scenes one unreadable mesh should not lose every other object.
                    match build_mesh(&geometry, material) {
                        Ok(mesh) => meshes.push(mesh),
                        Err(e) if layer_count == 0 => skipped.borrow_mut().push(e.0),
                        Err(e) => return Err(e),
                    }
                }
            }
            Ok(Piece { name: image.cstr(image.ptr(special + 0xb4)?), rigid_joint, meshes })
        };

        // --- layers ---
        let layer_count = image.u32(scene + 0x190)? as usize;
        let layers_at = image.ptr(scene + 0x194)?;
        let mut layers = Vec::new();
        for i in 0..layer_count {
            let l = layers_at + i * 0x14;
            // Each layer has a main and an "alternate" set (the alternate holds the morph-target
            // face pieces). A set is a per-joint table of rigid piece handles plus one skinned
            // handle; a handle is 12 bytes with the special's address at +8.
            let mut pieces = Vec::new();
            for set in [l + 4, l + 12] {
                let rigid_table = image.ptr(set)?;
                if rigid_table != 0 {
                    for joint in 0..joint_count {
                        let handle = image.ptr(rigid_table + joint * 4)?;
                        if handle != 0 {
                            pieces.push(build_piece(image.ptr(handle + 8)?, Some(joint))?);
                        }
                    }
                }
                let handle = image.ptr(set + 4)?;
                if handle != 0 {
                    pieces.push(build_piece(image.ptr(handle + 8)?, None)?);
                }
            }
            layers.push(Layer { name: image.cstr(image.ptr(l)?), pieces });
        }

        let bounds = (image.vec3(scene + 0x1ac)?, image.vec3(scene + 0x1b8)?);
        // --- level scenes: placed objects, each a list of (material, geometry item); the item
        // just before a geometry item holds its 4x4 placement matrix ---
        let mut statics = Vec::new();
        if layer_count == 0 {
            let object_count = image.u32(display + 0x10)? as usize;
            let objects_at = image.ptr(display + 0x14)?;
            // Named objects point at their draw-list record; the object number is at +0xC0.
            let mut named: std::collections::HashMap<usize, (String, u32)> = Default::default();
            let special_count = image.u32(display + 0x6c)? as usize;
            let specials_at = image.ptr(display + 0x70)?;
            if specials_at != 0 && objects_at != 0 {
                for i in 0..special_count.min(8192) {
                    let s = specials_at + i * 0xd0;
                    let record = image.ptr(s + 0xb0)?;
                    if record >= objects_at {
                        named.insert((record - objects_at) / 12, (image.cstr(image.ptr(s + 0xb4)?), image.u32(s + 0xc0)?));
                    }
                }
            }
            let flags_at = image.ptr(display + 0x48)?;
            for object in 0..object_count {
                let record = objects_at + object * 12;
                let draws = image.u32(record)? as usize;
                let (material_ids, item_ids) = (image.ptr(record + 4)?, image.ptr(record + 8)?);
                let flags = if flags_at != 0 { image.u8(flags_at + object)? } else { 0 };
                for k in 0..draws {
                    let material = image.u32(material_ids + k * 4)? as usize;
                    let item = image.u32(item_ids + k * 4)? as usize;
                    if item == 0 || item >= item_count {
                        continue;
                    }
                    let entry = items_at + item * 16;
                    let before = entry - 16;
                    if image.u8(entry)? != 0x82 || image.u8(before)? != 0x83 {
                        continue;
                    }
                    let matrix = image.mat4(image.ptr(before + 4)?)?;
                    let geometry = read_geometry(image.ptr(entry + 4)?)?;
                    match build_mesh(&geometry, material) {
                        Ok(mesh) if !mesh.indices.is_empty() => {
                            let (name, scene_id) = named.get(&object).cloned().unwrap_or_default();
                            statics.push(StaticMesh { matrix, mesh, object, flags, name, scene_id })
                        }
                        Ok(_) => {}
                        Err(e) => skipped.borrow_mut().push(e.0),
                    }
                }
            }
        }
        // --- named objects ("specials") of prop scenes ---
        let mut specials = Vec::new();
        if layer_count == 0 {
            let special_count = image.u32(display + 0x6c)? as usize;
            let specials_at = image.ptr(display + 0x70)?;
            if specials_at != 0 {
                for i in 0..special_count.min(4096) {
                    if let Ok(piece) = build_piece(specials_at + i * 0xd0, None) {
                        specials.push(piece);
                    }
                }
            }
        }
        // --- splines: 12-byte records of point count, point size, name, points ---
        let mut splines = Vec::new();
        let spline_count = image.u32(scene + 0x30)? as usize;
        let splines_at = image.ptr(scene + 0x34)?;
        if splines_at != 0 {
            for i in 0..spline_count.min(4096) {
                let s = splines_at + i * 12;
                let (count, size) = (image.u16(s)? as usize, image.u16(s + 2)? as usize);
                let points_at = image.ptr(s + 8)?;
                let mut points = Vec::with_capacity(count);
                for p in 0..count {
                    points.push(image.vec3(points_at + p * size.max(12))?);
                }
                splines.push(Spline { name: image.cstr(image.ptr(s + 4)?), points });
            }
        }
        Ok(Self { texture_hashes, materials, joints, locators, layers, bounds, statics, specials, skipped: skipped.into_inner(), splines })
    }

    pub fn print_summary(&self) {
        println!("textures: {:x?}", self.texture_hashes);
        println!("bounds: {:?} .. {:?}", self.bounds.0, self.bounds.1);
        for (i, m) in self.materials.iter().enumerate() {
            println!(
                "material {i}: colour {:?} opacity {} diffuse {:?} normal {:?} desc {:#010x}",
                m.colour, m.opacity, m.diffuse, m.normal_map, m.vertex_descriptor
            );
        }
        for (i, j) in self.joints.iter().enumerate() {
            let w = &j.world_bind;
            println!("joint {i:2} {:18} parent {:?} at ({:.4}, {:.4}, {:.4})", j.name, j.parent, w[12], w[13], w[14]);
        }
        for (i, l) in self.locators.iter().enumerate() {
            match l {
                Some(l) => println!("locator {i}: {} on joint {}", l.name, l.joint),
                None => println!("locator {i}: absent"),
            }
        }
        for s in &self.splines {
            println!("spline {:20} {} points, first {:?}", s.name, s.points.len(), s.points.first());
        }
        if !self.statics.is_empty() {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            let (mut verts, mut tris) = (0, 0);
            for s in &self.statics {
                verts += s.mesh.vertices.len();
                tris += s.mesh.indices.len() / 3;
                for v in &s.mesh.vertices {
                    let p = transform_point(&s.matrix, v.position);
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
            println!("placed meshes: {} ({verts} vertices, {tris} triangles), world bounds {lo:?} .. {hi:?}", self.statics.len());
        }
        for (i, layer) in self.layers.iter().enumerate() {
            println!("layer {i}: {}", layer.name);
            for piece in &layer.pieces {
                let verts: usize = piece.meshes.iter().map(|m| m.vertices.len()).sum();
                let tris: usize = piece.meshes.iter().map(|m| m.indices.len() / 3).sum();
                let mats: Vec<usize> = piece.meshes.iter().map(|m| m.material).collect();
                println!("  piece {:16} joint {:?} verts {verts} tris {tris} materials {mats:?}", piece.name, piece.rigid_joint);
            }
        }
    }
}
