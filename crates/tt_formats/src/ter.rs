//! Level collision (`.ter`): groups of flat shapes, each one or two triangles with a normal and
//! a surface type. Layout from reading the real file against the game's loader.
//!
//! File: a `u32` giving half the byte offset of the chunk table; chunk data from byte 4, one
//! block per chunk; the table holds 0x34-byte records (chunk count and version in the first,
//! data length in 16-bit words, group origin, chunk type). A group's data is a run of batches
//! (`i16 marker`, `i16 shape count`, four floats of bounds) each followed by 0x64-byte shapes,
//! ending at a negative marker.

use crate::{Error, Reader, Result};

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub normal: [f32; 3],
    /// Surface type index (what the floor is made of).
    pub surface: u8,
    /// Index of the group this came from.
    pub group: u16,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub origin: [f32; 3],
    /// 0 = fixed world collision, 1 = collision that belongs to a scene object (door, lift...).
    pub kind: i16,
    pub scene_index: u16,
}

pub struct Terrain {
    pub version: u16,
    pub groups: Vec<Group>,
    pub triangles: Vec<Triangle>,
    /// Wall lines: polylines of (x, z) that characters cannot cross at any height.
    pub walls: Vec<Vec<[f32; 2]>>,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Terrain {
    pub fn parse(file: &[u8]) -> Result<Self> {
        let table = Reader::new(file).u32()? as usize * 2;
        let mut head = Reader::at(file, table);
        let chunk_count = head.i16()?.max(0) as usize;
        let version = head.u16()?;
        let mut groups = Vec::new();
        let mut triangles = Vec::new();
        let mut walls = Vec::new();
        let mut data_at = 4usize;
        for chunk in 0..chunk_count {
            let mut r = Reader::at(file, table + chunk * 0x34 + 4);
            let words = r.u32()? as usize;
            let origin = [r.f32()?, r.f32()?, r.f32()?];
            let kind = r.i16()?;
            let scene_index = r.u16()?;
            if kind == 0 || kind == 1 {
                let group = groups.len() as u16;
                groups.push(Group { origin, kind, scene_index });
                let mut b = Reader::at(file, data_at);
                loop {
                    let marker = b.i16()?;
                    if marker < 0 {
                        break;
                    }
                    let shapes = b.i16()?.max(0) as usize;
                    b.pos += 16;
                    for _ in 0..shapes {
                        let mut s = Reader::at(file, b.pos + 0x18);
                        let mut v = [[0.0f32; 3]; 4];
                        for point in &mut v {
                            *point = [s.f32()? + origin[0], s.f32()? + origin[1], s.f32()? + origin[2]];
                        }
                        let normals = [[s.f32()?, s.f32()?, s.f32()?], [s.f32()?, s.f32()?, s.f32()?]];
                        let surface = s.u8()?;
                        b.pos += 0x64;
                        // First triangle is points 0,1,2.
                        let mut push = |p: [[f32; 3]; 3], normal: [f32; 3]| {
                            let face = cross(sub(p[1], p[0]), sub(p[2], p[0]));
                            if dot(face, face) < 1e-12 || dot(normal, normal) < 0.25 {
                                return;
                            }
                            let points = if dot(face, normal) < 0.0 { [p[0], p[2], p[1]] } else { p };
                            triangles.push(Triangle { points, normal, surface, group });
                        };
                        push([v[0], v[1], v[2]], normals[0]);
                        // The four points run as a strip, so a quad's second triangle is 1,2,3
                        // (a three-sided shape repeats a point there and is skipped as degenerate).
                        push([v[1], v[2], v[3]], normals[1]);
                    }
                }
            }
            // Type 2: a wall line. A marker word, a count (one more than the points that
            // matter), then xyz points whose y is not a height.
            if kind == 2 && words >= 4 {
                let mut w = Reader::at(file, data_at);
                if w.u32()? == 0x1234_5678 {
                    let count = (w.i32()? - 1).max(0) as usize;
                    let mut line = Vec::with_capacity(count);
                    for _ in 0..count {
                        let (x, _, z) = (w.f32()?, w.f32()?, w.f32()?);
                        line.push([x, z]);
                    }
                    walls.push(line);
                }
            }
            data_at += words * 2;
            if data_at > file.len() {
                return Err(Error::new("collision chunk runs past the end of the file"));
            }
        }
        Ok(Self { version, groups, triangles, walls })
    }
}
