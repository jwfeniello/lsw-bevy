//! A built-in test area made in code: ramps of known angles, stairs of known step heights,
//! platforms at the character's jump heights, gaps of known widths, a wall and a low ceiling.
//! It is this project's own (nothing from the game) and exists to try the movement rules.
//!
//! Everything is laid out in game space: x right, y up, z forwards from the start, in the
//! game's units (a minifig is 0.42 tall).

use crate::obj::{ObjBatch, ObjScene};
use std::collections::HashMap;

type P = [f32; 3];

struct Builder {
    /// Batches keyed by a colour (as bits), each a list of quads with their outward normal.
    quads: HashMap<[u32; 3], Vec<([P; 4], P)>>,
}

impl Builder {
    fn quad(&mut self, colour: [f32; 3], corners: [P; 4], normal: P) {
        // Sides are a little darker than tops so shapes read without any lighting.
        let shade = 0.62 + 0.38 * normal[1].max(0.0);
        let key = colour.map(|c| (c * shade).to_bits());
        self.quads.entry(key).or_default().push((corners, normal));
    }

    /// An axis-aligned box from `lo` to `hi`.
    fn solid(&mut self, colour: [f32; 3], lo: P, hi: P) {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        self.quad(colour, [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], [0.0, 1.0, 0.0]);
        self.quad(colour, [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], [0.0, -1.0, 0.0]);
        self.quad(colour, [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]], [0.0, 0.0, -1.0]);
        self.quad(colour, [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], [0.0, 0.0, 1.0]);
        self.quad(colour, [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], [-1.0, 0.0, 0.0]);
        self.quad(colour, [[x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]], [1.0, 0.0, 0.0]);
    }

    /// A ramp rising along +z from the ground at `z0` to `height` at `z1`, with side walls.
    fn ramp(&mut self, colour: [f32; 3], x0: f32, x1: f32, z0: f32, z1: f32, height: f32) {
        let run = z1 - z0;
        let len = (run * run + height * height).sqrt();
        self.quad(colour, [[x0, 0.0, z0], [x1, 0.0, z0], [x1, height, z1], [x0, height, z1]], [0.0, run / len, -height / len]);
        // Triangular sides, written as quads with a repeated corner.
        self.quad(colour, [[x0, 0.0, z0], [x0, 0.0, z1], [x0, height, z1], [x0, height, z1]], [-1.0, 0.0, 0.0]);
        self.quad(colour, [[x1, 0.0, z0], [x1, 0.0, z1], [x1, height, z1], [x1, height, z1]], [1.0, 0.0, 0.0]);
    }
}

/// Angles of the ramp row, left to right. The game treats ground steeper than 45 degrees as
/// a wall, so the last two should not be climbable.
pub const RAMP_ANGLES: [f32; 6] = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
/// Step heights of the three staircases.
pub const STEP_HEIGHTS: [f32; 3] = [0.05, 0.1, 0.15];
/// Platform heights: under a single jump (0.44), at it, between, at the double jump (0.75),
/// and above it.
pub const PLATFORM_HEIGHTS: [f32; 6] = [0.2, 0.4, 0.5, 0.7, 0.8, 1.0];
/// Widths of the gaps in the raised walkway.
pub const GAP_WIDTHS: [f32; 5] = [0.5, 1.0, 1.5, 2.0, 3.0];

pub fn build() -> ObjScene {
    let mut b = Builder { quads: HashMap::new() };
    let ground = [0.62, 0.64, 0.62];
    b.solid(ground, [-40.0, -1.0, -40.0], [40.0, 0.0, 40.0]);

    // Ramps ahead of the start, each 3 wide and 1.5 high with a landing on top.
    let ramp = [0.85, 0.6, 0.3];
    for (i, angle) in RAMP_ANGLES.iter().enumerate() {
        let x0 = -11.75 + i as f32 * 4.0;
        let run = 1.5 / angle.to_radians().tan();
        b.ramp(ramp, x0, x0 + 3.0, 6.0, 6.0 + run, 1.5);
        b.solid(ramp, [x0, 0.0, 6.0 + run], [x0 + 3.0, 1.5, 9.0 + run]);
    }

    // Stairs behind the start: ten steps each, 0.4 deep.
    let stair = [0.4, 0.6, 0.85];
    for (i, height) in STEP_HEIGHTS.iter().enumerate() {
        let x0 = -6.0 + i as f32 * 4.5;
        for step in 0..10 {
            let z0 = -6.0 - step as f32 * 0.4;
            b.solid(stair, [x0, 0.0, z0 - 0.4], [x0 + 3.0, height * (step + 1) as f32, z0]);
        }
    }

    // Jump platforms to the right, 2 by 2.
    let platform = [0.5, 0.8, 0.45];
    for (i, height) in PLATFORM_HEIGHTS.iter().enumerate() {
        let z0 = -6.0 + i as f32 * 3.0;
        b.solid(platform, [14.0, 0.0, z0], [16.0, *height, z0 + 2.0]);
    }

    // A walkway 0.4 high to the left, broken by gaps of increasing width.
    let walkway = [0.8, 0.45, 0.6];
    let mut z = -12.0;
    b.ramp(walkway, -19.0, -17.0, z - 2.0, z, 0.4);
    for gap in GAP_WIDTHS {
        b.solid(walkway, [-19.0, 0.0, z], [-17.0, 0.4, z + 3.0]);
        z += 3.0 + gap;
    }
    b.solid(walkway, [-19.0, 0.0, z], [-17.0, 0.4, z + 3.0]);

    // A wall to run into, and a slab 0.6 above the ground to jump under.
    b.solid([0.7, 0.7, 0.75], [-4.0, 0.0, -16.0], [4.0, 2.0, -15.5]);
    b.solid([0.75, 0.72, 0.5], [6.0, 0.6, -14.0], [10.0, 0.8, -10.0]);
    for (x, z) in [(6.0, -14.0), (9.8, -14.0), (6.0, -10.2), (9.8, -10.2)] {
        b.solid([0.75, 0.72, 0.5], [x, 0.0, z], [x + 0.2, 0.6, z + 0.2]);
    }

    let mut batches = Vec::new();
    let mut triangles = Vec::new();
    for (key, quads) in b.quads {
        let mut batch = ObjBatch {
            texture: None,
            colour: key.map(f32::from_bits),
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        };
        for (corners, normal) in quads {
            let base = batch.positions.len() as u32;
            for p in corners {
                // The renderer's z is the game's -z. One texture tile per game unit.
                batch.positions.push([p[0], p[1], -p[2]]);
                batch.normals.push([normal[0], normal[1], -normal[2]]);
                batch.uvs.push(if normal[1].abs() > 0.5 {
                    [p[0] * 0.5, p[2] * 0.5]
                } else if normal[0].abs() > 0.5 {
                    [p[2] * 0.5, p[1] * 0.5]
                } else {
                    [p[0] * 0.5, p[1] * 0.5]
                });
            }
            batch.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            triangles.push(([corners[0], corners[1], corners[2]], normal));
            if corners[2] != corners[3] {
                triangles.push(([corners[0], corners[2], corners[3]], normal));
            }
        }
        batches.push(batch);
    }
    ObjScene { batches, triangles, has_collision_shell: false }
}
