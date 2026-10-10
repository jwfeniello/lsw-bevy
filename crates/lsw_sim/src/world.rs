//! A loaded level as the rules see it: collision triangles and camera rails.
//!
//! The triangles and rails are the game's own data. The collision *method* here (probe down
//! for a floor, push out of walls) is this project's, not the game's terrain code.

use crate::Vec3;
use std::collections::HashMap;

/// Surfaces at least this upright can be stood on (the game's own "steep" threshold).
const WALKABLE_NORMAL_Y: f32 = 0.707;
const CELL: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub p: [Vec3; 3],
    pub n: Vec3,
    /// Object number of the scene object this collision belongs to, 0 for fixed scenery.
    pub tag: u32,
}

/// One camera rail set ("sock"): the play corridor runs between rails `a` and `b`, and the
/// camera rides `cam` at the same distance along.
#[derive(Clone, Debug, Default)]
pub struct Sock {
    pub index: u32,
    pub cam: Vec<Vec3>,
    pub a: Vec<Vec3>,
    pub b: Vec<Vec3>,
    /// The centre rail. With `y_limits` it is the ceiling for the camera's height.
    pub mid: Vec<Vec3>,
    /// `y_limits`: keep the camera no lower than the camera rail and no higher than `mid`.
    pub y_limits: bool,
    /// `cam_y_offset`: camera height above the player's feet when `offset_blend_y` is 1.
    pub cam_y_offset: f32,
    pub offset_blend_y: f32,
    pub position_seek: f32,
    /// `cam_height_above_terrain`: when above 0, the camera is kept at least this far above
    /// the ground underneath it.
    pub height_above_terrain: f32,
}

#[derive(Default)]
pub struct World {
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Wall lines as (x, z) segments, with their own grid.
    walls: Vec<[(f32, f32); 2]>,
    wall_grid: HashMap<(i32, i32), Vec<u32>>,
    /// Object numbers whose collision is currently switched off (opened doors, moved blocks).
    removed: std::collections::HashSet<u32>,
    pub socks: Vec<Sock>,
    pub start: Option<Vec3>,
    pub lowest: f32,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn add_scaled(a: Vec3, b: Vec3, s: f32) -> Vec3 {
    Vec3::new(a.x + b.x * s, a.y + b.y * s, a.z + b.z * s)
}

/// Closest point on a triangle to `p` (Ericson, Real-Time Collision Detection).
fn closest_point(p: Vec3, t: &Tri) -> Vec3 {
    let [a, b, c] = t.p;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add_scaled(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add_scaled(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add_scaled(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    add_scaled(add_scaled(a, ab, vb * denom), ac, vc * denom)
}

impl World {
    pub fn new(triangles: impl IntoIterator<Item = ([[f32; 3]; 3], [f32; 3])>) -> Self {
        Self::new_tagged(triangles.into_iter().map(|(p, n)| (p, n, 0)))
    }

    /// Switch the collision of one scene object off.
    pub fn remove_object(&mut self, tag: u32) {
        self.removed.insert(tag);
    }

    pub fn new_tagged(triangles: impl IntoIterator<Item = ([[f32; 3]; 3], [f32; 3], u32)>) -> Self {
        let mut world = Self { lowest: f32::MAX, ..Default::default() };
        for (points, normal, tag) in triangles {
            let p = points.map(|v| Vec3::new(v[0], v[1], v[2]));
            let index = world.tris.len() as u32;
            let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
            for v in p {
                lo = (lo.0.min(v.x), lo.1.min(v.z));
                hi = (hi.0.max(v.x), hi.1.max(v.z));
                world.lowest = world.lowest.min(v.y);
            }
            for cx in (lo.0 / CELL).floor() as i32..=(hi.0 / CELL).floor() as i32 {
                for cz in (lo.1 / CELL).floor() as i32..=(hi.1 / CELL).floor() as i32 {
                    world.grid.entry((cx, cz)).or_default().push(index);
                }
            }
            world.tris.push(Tri { p, n: Vec3::new(normal[0], normal[1], normal[2]), tag });
        }
        world
    }

    /// Add wall lines: polylines of (x, z) that cannot be crossed.
    pub fn add_walls(&mut self, lines: &[Vec<[f32; 2]>]) {
        for line in lines {
            for pair in line.windows(2) {
                let (p, q) = ((pair[0][0], pair[0][1]), (pair[1][0], pair[1][1]));
                let index = self.walls.len() as u32;
                for cx in (p.0.min(q.0) / CELL).floor() as i32..=(p.0.max(q.0) / CELL).floor() as i32 {
                    for cz in (p.1.min(q.1) / CELL).floor() as i32..=(p.1.max(q.1) / CELL).floor() as i32 {
                        self.wall_grid.entry((cx, cz)).or_default().push(index);
                    }
                }
                self.walls.push([p, q]);
            }
        }
    }

    pub fn wall_count(&self) -> usize {
        self.walls.len()
    }

    /// Keep a circle of `radius` from overlapping any wall line.
    fn push_from_walls(&self, pos: &mut Vec3, radius: f32) -> bool {
        let mut touched = false;
        for _ in 0..3 {
            let mut moved = false;
            let (x0, x1) = (((pos.x - radius) / CELL).floor() as i32, ((pos.x + radius) / CELL).floor() as i32);
            let (z0, z1) = (((pos.z - radius) / CELL).floor() as i32, ((pos.z + radius) / CELL).floor() as i32);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    let Some(list) = self.wall_grid.get(&(cx, cz)) else { continue };
                    for &i in list {
                        let [p, q] = self.walls[i as usize];
                        let (ex, ez) = (q.0 - p.0, q.1 - p.1);
                        let len2 = ex * ex + ez * ez;
                        let t = if len2 > 1e-12 { (((pos.x - p.0) * ex + (pos.z - p.1) * ez) / len2).clamp(0.0, 1.0) } else { 0.0 };
                        let (dx, dz) = (pos.x - (p.0 + ex * t), pos.z - (p.1 + ez * t));
                        let dist = (dx * dx + dz * dz).sqrt();
                        if dist < radius && dist > 1e-6 {
                            pos.x += dx / dist * (radius - dist);
                            pos.z += dz / dist * (radius - dist);
                            moved = true;
                            touched = true;
                        }
                    }
                }
            }
            if !moved {
                break;
            }
        }
        touched
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    fn near(&self, x: f32, z: f32, reach: f32) -> impl Iterator<Item = &Tri> {
        let (x0, x1) = (((x - reach) / CELL).floor() as i32, ((x + reach) / CELL).floor() as i32);
        let (z0, z1) = (((z - reach) / CELL).floor() as i32, ((z + reach) / CELL).floor() as i32);
        (x0..=x1)
            .flat_map(move |cx| (z0..=z1).map(move |cz| (cx, cz)))
            .filter_map(|cell| self.grid.get(&cell))
            .flatten()
            .map(|&i| &self.tris[i as usize])
            .filter(|t| t.tag == 0 || !self.removed.contains(&t.tag))
    }

    /// Height of the surface under (x, z) on a triangle, if the point is over it.
    fn height_on(t: &Tri, x: f32, z: f32) -> Option<f32> {
        let [a, b, c] = t.p;
        let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if det.abs() < 1e-9 {
            return None;
        }
        let u = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
        let v = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
        let w = 1.0 - u - v;
        let edge = -1e-4;
        (u >= edge && v >= edge && w >= edge).then(|| u * a.y + v * b.y + w * c.y)
    }

    /// How far along the straight line from `a` to `b` (0..1) the first surface is hit, if any.
    pub fn segment_hit(&self, a: Vec3, b: Vec3) -> Option<f32> {
        let d = sub(b, a);
        let mid = add_scaled(a, d, 0.5);
        let reach = 0.5 * (d.x * d.x + d.z * d.z).sqrt();
        let mut best: Option<f32> = None;
        for t in self.near(mid.x, mid.z, reach) {
            let (e1, e2) = (sub(t.p[1], t.p[0]), sub(t.p[2], t.p[0]));
            let p = Vec3::new(d.y * e2.z - d.z * e2.y, d.z * e2.x - d.x * e2.z, d.x * e2.y - d.y * e2.x);
            let det = dot(e1, p);
            if det.abs() < 1e-9 {
                continue;
            }
            let s = sub(a, t.p[0]);
            let u = dot(s, p) / det;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = Vec3::new(s.y * e1.z - s.z * e1.y, s.z * e1.x - s.x * e1.z, s.x * e1.y - s.y * e1.x);
            let v = dot(d, q) / det;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let along = dot(e2, q) / det;
            if along > 1e-4 && along < 1.0 && best.is_none_or(|b| along < b) {
                best = Some(along);
            }
        }
        best
    }

    /// Highest walkable surface under (x, z) between `bottom` and `top`.
    pub fn floor_between(&self, x: f32, z: f32, top: f32, bottom: f32) -> Option<f32> {
        self.near(x, z, 0.0)
            .filter(|t| t.n.y > WALKABLE_NORMAL_Y)
            .filter_map(|t| Self::height_on(t, x, z))
            .filter(|&y| y <= top && y >= bottom)
            .max_by(|a, b| a.total_cmp(b))
    }

    /// The game's "camera height above terrain" rule: find the ground under the camera and, if
    /// the camera is less than `clearance` above it, raise the camera to exactly that.
    /// Returns the camera height to use.
    ///
    /// The game looks from the camera's own height down 5 units (`above` = 0, `margin` = 0).
    /// `above` also searches that far over the camera, for a camera that has ended up inside
    /// something; `margin` tests four more points that far out, so the camera rises before it
    /// touches an edge. Any upward-facing surface counts, steep or not.
    pub fn lift_above_ground(&self, cam: Vec3, clearance: f32, above: f32, margin: f32) -> f32 {
        let mut spots = vec![(0.0, 0.0)];
        if margin > 0.0 {
            spots.extend([(margin, 0.0), (-margin, 0.0), (0.0, margin), (0.0, -margin)]);
        }
        let ground = spots
            .iter()
            .flat_map(move |&(dx, dz)| {
                let (x, z) = (cam.x + dx, cam.z + dz);
                self.near(x, z, 0.0).filter(|t| t.n.y > 0.01).filter_map(move |t| Self::height_on(t, x, z))
            })
            .filter(|&y| y <= cam.y + above && y >= cam.y - 5.0)
            .max_by(|a, b| a.total_cmp(b));
        match ground {
            Some(ground) if cam.y < ground + clearance => ground + clearance,
            _ => cam.y,
        }
    }

    /// Lowest downward-facing surface over (x, z) between `bottom` and `top`.
    pub fn ceiling_between(&self, x: f32, z: f32, bottom: f32, top: f32) -> Option<f32> {
        self.near(x, z, 0.0)
            .filter(|t| t.n.y < -0.5)
            .filter_map(|t| Self::height_on(t, x, z))
            .filter(|&y| y <= top && y >= bottom)
            .min_by(|a, b| a.total_cmp(b))
    }

    /// Push an upright body out of any steep surface it overlaps. Returns true if it touched.
    /// Surfaces that end no higher than `step` above the feet are left alone: they are low
    /// enough to be stepped onto, so they must not act as walls.
    pub fn push_out(&self, pos: &mut Vec3, radius: f32, height: f32, step: f32) -> bool {
        let mut touched = self.push_from_walls(pos, radius);
        for _ in 0..3 {
            let mut moved = false;
            for sample in [0.38, 0.72] {
                let centre = Vec3::new(pos.x, pos.y + height * sample, pos.z);
                let mut push = (0.0f32, 0.0f32);
                let step_top = pos.y + step + 1e-3;
                for t in self
                    .near(pos.x, pos.z, radius)
                    .filter(|t| t.n.y.abs() <= WALKABLE_NORMAL_Y)
                    .filter(|t| t.p[0].y.max(t.p[1].y).max(t.p[2].y) > step_top)
                {
                    let q = closest_point(centre, t);
                    let d = sub(centre, q);
                    let dist = dot(d, d).sqrt();
                    if dist >= radius || dot(d, t.n) < -1e-4 {
                        continue;
                    }
                    let flat = (t.n.x * t.n.x + t.n.z * t.n.z).sqrt();
                    if flat < 1e-4 {
                        continue;
                    }
                    let depth = radius - dist;
                    push.0 += t.n.x / flat * depth;
                    push.1 += t.n.z / flat * depth;
                }
                if push.0 != 0.0 || push.1 != 0.0 {
                    pos.x += push.0;
                    pos.z += push.1;
                    moved = true;
                    touched = true;
                }
            }
            if !moved {
                break;
            }
        }
        touched
    }

    /// Where the level's rails want the camera for a player standing at `feet`. `current` is
    /// the rail set used last frame, kept while the player is still inside it.
    pub fn camera_goal(&self, feet: Vec3, current: &mut Option<usize>) -> Option<(Vec3, f32)> {
        let locate = |sock: &Sock| -> Option<Vec3> {
            let count = sock.a.len().min(sock.b.len());
            if count < 2 || sock.cam.is_empty() {
                return None;
            }
            for i in 0..count - 1 {
                let quad = [sock.a[i], sock.a[i + 1], sock.b[i + 1], sock.b[i]];
                let mut sign = 0.0f32;
                let inside = (0..4).all(|k| {
                    let (p, q) = (quad[k], quad[(k + 1) % 4]);
                    let side = (q.x - p.x) * (feet.z - p.z) - (q.z - p.z) * (feet.x - p.x);
                    if side.abs() < 1e-6 {
                        return true;
                    }
                    if sign == 0.0 {
                        sign = side.signum();
                    }
                    side.signum() == sign
                });
                if !inside {
                    continue;
                }
                // How far along the segment: distance from the entry edge against the exit edge.
                let edge_distance = |p: Vec3, q: Vec3| {
                    let (ex, ez) = (q.x - p.x, q.z - p.z);
                    let len = (ex * ex + ez * ez).sqrt().max(1e-6);
                    ((feet.x - p.x) * ez - (feet.z - p.z) * ex).abs() / len
                };
                let (near, far) = (edge_distance(sock.a[i], sock.b[i]), edge_distance(sock.a[i + 1], sock.b[i + 1]));
                let ratio = if near + far > 1e-6 { near / (near + far) } else { 0.0 };
                let last = sock.cam.len() - 1;
                let (c0, c1) = (sock.cam[i.min(last)], sock.cam[(i + 1).min(last)]);
                let mut cam = add_scaled(c0, sub(c1, c0), ratio);
                let rail_y = cam.y;
                if sock.offset_blend_y >= 1.0 {
                    cam.y = feet.y + sock.cam_y_offset;
                } else {
                    cam.y += (feet.y + sock.cam_y_offset - cam.y) * sock.offset_blend_y;
                }
                if sock.y_limits {
                    if cam.y < rail_y {
                        cam.y = rail_y;
                    } else if !sock.mid.is_empty() {
                        let last = sock.mid.len() - 1;
                        let (m0, m1) = (sock.mid[i.min(last)], sock.mid[(i + 1).min(last)]);
                        cam.y = cam.y.min(m0.y + (m1.y - m0.y) * ratio);
                    }
                }
                if sock.height_above_terrain > 0.0 {
                    cam.y = self.lift_above_ground(cam, sock.height_above_terrain, 0.0, 0.0);
                }
                return Some(cam);
            }
            None
        };
        if let Some(goal) = current.and_then(|i| self.socks.get(i)).and_then(|s| locate(s).map(|g| (g, s.position_seek))) {
            return Some(goal);
        }
        for (i, sock) in self.socks.iter().enumerate() {
            if let Some(goal) = locate(sock) {
                *current = Some(i);
                return Some((goal, sock.position_seek));
            }
        }
        None
    }
}
