//! Posing the model on the CPU: joint matrices in, vertex positions and normals out.
//!
//! The game's model space is left-handed (the figure faces -z with its left side at +x). Output
//! is mirrored in z so it is right-handed for the renderer; triangle winding is fixed up once
//! at load time by comparing against the stored normals.

use tt_formats::ghg::{mul, transform_dir, transform_point, Joint, Mat4, Mesh, Piece};

pub struct DrawMesh {
    pub material: usize,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    source: Mesh,
    rigid_joint: Option<usize>,
}

fn mirror(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[1], -v[2]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-8 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// `inverse_bind * world` for every joint: takes a model-space rest vertex to its posed place.
pub fn skin_matrices(joints: &[Joint], world: &[Mat4]) -> Vec<Mat4> {
    joints.iter().zip(world).map(|(j, w)| mul(&j.inverse_bind, w)).collect()
}

impl DrawMesh {
    pub fn new(piece: &Piece, mesh: &Mesh, joints: &[Joint]) -> Self {
        let mut draw = Self {
            material: mesh.material,
            uvs: mesh.vertices.iter().map(|v| v.uv).collect(),
            indices: mesh.indices.clone(),
            source: mesh.clone(),
            rigid_joint: piece.rigid_joint,
        };
        // Make the winding agree with the normals in the mirrored space.
        let world: Vec<Mat4> = joints.iter().map(|j| j.world_bind).collect();
        let skin = skin_matrices(joints, &world);
        let (positions, normals) = draw.pose(&world, &skin, None);
        let mut agreement = 0.0f32;
        for tri in draw.indices.chunks_exact(3) {
            let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let (p, q, r) = (positions[a], positions[b], positions[c]);
            let (u, v) = ([q[0] - p[0], q[1] - p[1], q[2] - p[2]], [r[0] - p[0], r[1] - p[1], r[2] - p[2]]);
            let face = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let n = [0, 1, 2].map(|k| normals[a][k] + normals[b][k] + normals[c][k]);
            agreement += face[0] * n[0] + face[1] * n[1] + face[2] * n[2];
        }
        if agreement < 0.0 {
            for tri in draw.indices.chunks_exact_mut(3) {
                tri.swap(1, 2);
            }
        }
        draw
    }

    /// Positions and normals for a pose. `attach` is an extra matrix applied to a rigid piece
    /// instead of its own joint (used to hang the saber off a hand locator), and `stretch`
    /// reshapes the rest vertex first.
    pub fn pose(&self, world: &[Mat4], skin: &[Mat4], attach: Option<&Mat4>) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        self.pose_with(world, skin, attach, |p| p)
    }

    pub fn pose_with(
        &self,
        world: &[Mat4],
        skin: &[Mat4],
        attach: Option<&Mat4>,
        reshape: impl Fn([f32; 3]) -> [f32; 3],
    ) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        let count = self.source.vertices.len();
        let (mut positions, mut normals) = (Vec::with_capacity(count), Vec::with_capacity(count));
        let rigid = attach.or_else(|| self.rigid_joint.and_then(|j| world.get(j)));
        for vertex in &self.source.vertices {
            let rest = reshape(vertex.position);
            if let Some(matrix) = rigid {
                positions.push(mirror(transform_point(matrix, rest)));
                normals.push(mirror(normalize(transform_dir(matrix, vertex.normal))));
                continue;
            }
            let (mut p, mut n) = ([0.0f32; 3], [0.0f32; 3]);
            for (joint, weight) in vertex.weights {
                if weight <= 0.0 {
                    continue;
                }
                let Some(matrix) = skin.get(joint as usize) else { continue };
                let (tp, tn) = (transform_point(matrix, rest), transform_dir(matrix, vertex.normal));
                for k in 0..3 {
                    p[k] += tp[k] * weight;
                    n[k] += tn[k] * weight;
                }
            }
            positions.push(mirror(p));
            normals.push(mirror(normalize(n)));
        }
        (positions, normals)
    }

    /// Bounds of the rest vertices, in the piece's own space.
    pub fn rest_bounds(&self) -> ([f32; 3], [f32; 3]) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &self.source.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
        }
        (lo, hi)
    }
}
