//! A plain Wavefront `.obj` scene used as a level: triangles with one texture per material.
//!
//! This is not a LEGO Star Wars format. It lets any model the player has on disk stand in as a
//! level: the same triangles are drawn and collided with. Positions are scaled and converted
//! to the game's left-handed space (x, y, -z), so the model is not mirrored on screen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ObjBatch {
    pub texture: Option<PathBuf>,
    /// Flat tint (white for textured model files).
    pub colour: [f32; 3],
    /// Renderer-space positions (the file's own handedness, scaled).
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

pub struct ObjScene {
    pub batches: Vec<ObjBatch>,
    /// Collision triangles and normals in game space.
    pub triangles: Vec<([[f32; 3]; 3], [f32; 3])>,
    /// True when the file carried its own untextured collision shell and that is what
    /// `triangles` holds; otherwise collision is the drawn geometry.
    pub has_collision_shell: bool,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn materials(path: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    let folder = path.parent().unwrap_or(Path::new("."));
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("newmtl ") {
            current = name.trim().to_string();
        } else if let Some(file) = line.strip_prefix("map_Kd ") {
            out.insert(current.clone(), folder.join(file.trim()));
        }
    }
    out
}

pub fn load(path: &Path, scale: f32) -> Result<ObjScene, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut library = HashMap::new();
    let (mut points, mut coords): (Vec<[f32; 3]>, Vec<[f32; 2]>) = (Vec::new(), Vec::new());
    // One batch per material; vertices shared within a batch by (position, uv) index.
    let mut batches: Vec<ObjBatch> = Vec::new();
    let mut lookup: Vec<HashMap<(u32, u32), u32>> = Vec::new();
    let mut by_material: HashMap<String, usize> = HashMap::new();
    let mut current = usize::MAX;
    // Faces with no texture coordinates are not part of the picture. Stage rips carry the
    // game's collision shell this way: a second, untextured copy lying on the visible one.
    let mut shell: Vec<[[f32; 3]; 3]> = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("mtllib") => {
                let name = line.trim()[6..].trim();
                library = materials(&path.parent().unwrap_or(Path::new(".")).join(name));
            }
            Some("v") => {
                let v: Vec<f32> = words.take(3).filter_map(|w| w.parse().ok()).collect();
                if v.len() == 3 {
                    points.push([v[0] * scale, v[1] * scale, v[2] * scale]);
                }
            }
            Some("vt") => {
                let v: Vec<f32> = words.take(2).filter_map(|w| w.parse().ok()).collect();
                if v.len() == 2 {
                    // Image rows run downwards in the renderer.
                    coords.push([v[0], 1.0 - v[1]]);
                }
            }
            Some("usemtl") => {
                let name = words.next().unwrap_or("").to_string();
                current = *by_material.entry(name.clone()).or_insert_with(|| {
                    batches.push(ObjBatch {
                        texture: library.get(&name).cloned(),
                        colour: [1.0; 3],
                        positions: Vec::new(),
                        normals: Vec::new(),
                        uvs: Vec::new(),
                        indices: Vec::new(),
                    });
                    lookup.push(HashMap::new());
                    batches.len() - 1
                });
            }
            Some("f") => {
                if current == usize::MAX {
                    continue;
                }
                let corner = |w: &str| -> Option<(u32, u32)> {
                    let mut parts = w.split('/');
                    let v = parts.next()?.parse::<i64>().ok()?;
                    let t = parts.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
                    let resolve = |i: i64, len: usize| if i < 0 { (len as i64 + i) as u32 } else { (i - 1).max(0) as u32 };
                    Some((resolve(v, points.len()), if t == 0 { u32::MAX } else { resolve(t, coords.len()) }))
                };
                let corners: Vec<(u32, u32)> = words.filter_map(corner).collect();
                if corners.iter().any(|c| c.1 == u32::MAX) {
                    for k in 1..corners.len().saturating_sub(1) {
                        let p = [corners[0].0, corners[k].0, corners[k + 1].0].map(|i| points.get(i as usize).copied());
                        if let [Some(a), Some(b), Some(c)] = p {
                            shell.push([a, b, c]);
                        }
                    }
                    continue;
                }
                let (batch, seen) = (&mut batches[current], &mut lookup[current]);
                let mut index = |key: (u32, u32)| -> Option<u32> {
                    if let Some(&i) = seen.get(&key) {
                        return Some(i);
                    }
                    let position = *points.get(key.0 as usize)?;
                    let i = batch.positions.len() as u32;
                    batch.positions.push(position);
                    batch.uvs.push(coords.get(key.1 as usize).copied().unwrap_or([0.0, 0.0]));
                    seen.insert(key, i);
                    Some(i)
                };
                // Fan out anything with more than three corners.
                for k in 1..corners.len().saturating_sub(1) {
                    if let (Some(a), Some(b), Some(c)) = (index(corners[0]), index(corners[k]), index(corners[k + 1])) {
                        batch.indices.extend([a, b, c]);
                    }
                }
            }
            _ => {}
        }
    }

    // Decide which way faces point: on a level, near-flat ground must face up.
    let mut up = 0.0f64;
    for batch in &batches {
        for t in batch.indices.chunks_exact(3) {
            let (a, b, c) = (batch.positions[t[0] as usize], batch.positions[t[1] as usize], batch.positions[t[2] as usize]);
            let n = cross(sub(b, a), sub(c, a));
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 && (n[1] / len).abs() > 0.9 {
                up += n[1] as f64;
            }
        }
    }
    let flip = up < 0.0;
    let mut triangles = Vec::new();
    for batch in &mut batches {
        batch.normals = vec![[0.0, 1.0, 0.0]; batch.positions.len()];
        for t in batch.indices.chunks_exact_mut(3) {
            if flip {
                t.swap(1, 2);
            }
            let (a, b, c) = (batch.positions[t[0] as usize], batch.positions[t[1] as usize], batch.positions[t[2] as usize]);
            let n = cross(sub(b, a), sub(c, a));
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len < 1e-9 {
                continue;
            }
            let n = [n[0] / len, n[1] / len, n[2] / len];
            for &i in t.iter() {
                batch.normals[i as usize] = n;
            }
            // Game space is the renderer's with z negated.
            let g = |p: [f32; 3]| [p[0], p[1], -p[2]];
            triangles.push(([g(a), g(b), g(c)], [n[0], n[1], -n[2]]));
        }
    }
    // Use the collision shell when there is a real one (more than a few stray faces).
    let has_collision_shell = shell.len() * 20 > triangles.len();
    if has_collision_shell {
        let face = |t: &[[f32; 3]; 3]| cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let mut up = 0.0f64;
        for t in &shell {
            let n = face(t);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 0.0 && (n[1] / len).abs() > 0.9 {
                up += n[1] as f64;
            }
        }
        triangles.clear();
        for t in &shell {
            let t = if up < 0.0 { [t[0], t[2], t[1]] } else { *t };
            let n = face(&t);
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len < 1e-9 {
                continue;
            }
            let g = |p: [f32; 3]| [p[0], p[1], -p[2]];
            triangles.push(([g(t[0]), g(t[1]), g(t[2])], [n[0] / len, n[1] / len, -n[2] / len]));
        }
    }
    Ok(ObjScene { batches, triangles, has_collision_shell })
}
