//! PROVISIONAL third-person camera collision: keep the chase camera out of
//! world geometry by shortening the track when a wall sits between the player
//! and the camera.
//!
//! PROVISIONAL — no `analysis/` citation. Nothing is known here about what the
//! PS2 camera does on contact (spring rate, whether it pulls in or slides, how
//! it recovers); this is a plain pull-in so the player is not left looking at
//! the inside of a wall. The PS2 behaviour, and the engine's use of the `coll`
//! chunks, are an open RE question (a note on card CAMS).
//!
//! Geometry is exact, though: an instance AABB is only the broad phase, and
//! the track segment is then tested against the model's triangles in model
//! space. Boxes alone are not good enough here — Mos Eisley's buildings are
//! concave, and their bounding boxes swallow whole streets (measured: 2 of 6
//! sampled frames on the walk-demo path had the camera jammed against the
//! player's back in the open).

use crate::assets::Scene;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Camera sphere radius (m) — how far short of a wall the camera stops.
/// PROVISIONAL, picked to clear the near plane.
pub const CAMERA_RADIUS: f32 = 0.35;
/// Never pull closer than this fraction of the desired track distance, so a
/// player wedged into geometry still gets a third-person view.
const MIN_TRACK_FRAC: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    fn empty() -> Aabb {
        Aabb {
            min: Vec3::splat(f32::MAX),
            max: Vec3::splat(f32::MIN),
        }
    }

    fn add(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    /// World AABB of `local` transformed by `m` (bounds of the 8 corners).
    pub fn transformed(local: Aabb, m: Mat4) -> Aabb {
        let mut out = Aabb::empty();
        for i in 0..8 {
            out.add(m.transform_point3(Vec3::new(
                if i & 1 == 0 { local.min.x } else { local.max.x },
                if i & 2 == 0 { local.min.y } else { local.max.y },
                if i & 4 == 0 { local.min.z } else { local.max.z },
            )));
        }
        out
    }

    /// Does the segment `from -> from + dir*len` reach this box grown by
    /// `radius`? Broad phase only — a conservative yes/no.
    fn hits(&self, from: Vec3, dir: Vec3, len: f32, radius: f32) -> bool {
        let (lo, hi) = (
            self.min - Vec3::splat(radius),
            self.max + Vec3::splat(radius),
        );
        let (mut t0, mut t1) = (0.0f32, len);
        for a in 0..3 {
            let (o, d) = (from[a], dir[a]);
            if d.abs() < 1e-6 {
                if o < lo[a] || o > hi[a] {
                    return false;
                }
                continue;
            }
            let (mut n, mut f) = ((lo[a] - o) / d, (hi[a] - o) / d);
            if n > f {
                std::mem::swap(&mut n, &mut f);
            }
            t0 = t0.max(n);
            t1 = t1.min(f);
            if t0 > t1 {
                return false;
            }
        }
        true
    }
}

/// Model-space triangle soup, shared by every instance of one model.
#[derive(Debug)]
struct TriMesh {
    positions: Vec<Vec3>,
    indices: Vec<u32>,
    local: Aabb,
}

/// Distance along `dir` (world units — `dir` is the world direction pushed
/// through the instance's inverse, so the parameter is preserved) at which the
/// ray from `o` first crosses a triangle, within `max`. Möller-Trumbore,
/// two-sided (PS2 map models are not reliably wound outward).
fn ray_tri(o: Vec3, dir: Vec3, max: f32, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(h) * inv;
    if !(-1e-5..=1.000_01).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < -1e-5 || u + v > 1.000_01 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4 && t <= max).then_some(t)
}

#[derive(Debug)]
struct Blocker {
    world: Aabb,
    /// World -> model, so one segment test serves every instance of a model.
    inv: Mat4,
    mesh: Arc<TriMesh>,
}

/// World blockers for the chase camera. Visual only — never read by the sim,
/// so it cannot affect determinism.
#[derive(Debug, Default)]
pub struct CameraBlockers {
    blockers: Vec<Blocker>,
}

impl CameraBlockers {
    /// One blocker per drawable instance in the scene (terrain excluded —
    /// `camera()` already floors against the heightfield).
    pub fn from_scene(scene: &Scene) -> Self {
        let mut meshes: HashMap<String, Arc<TriMesh>> = HashMap::new();
        let mut blockers = Vec::new();
        for inst in &scene.manifest.instances {
            let Some(model) = scene.resolve_model(&inst.class) else {
                continue;
            };
            if !meshes.contains_key(model) {
                let cpu = &scene.meshes[model];
                let positions: Vec<Vec3> = cpu.positions.iter().copied().map(Vec3::from).collect();
                let mut local = Aabb::empty();
                for p in &positions {
                    local.add(*p);
                }
                // Keep only triangles whose indices land in `positions`: the
                // GLB's indices are never range-checked on load, and
                // `pull_in` indexes with them (the collision and render
                // paths filter the same way).
                let indices: Vec<u32> = cpu
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter(|tri| tri.iter().all(|&i| (i as usize) < positions.len()))
                    .flatten()
                    .copied()
                    .collect();
                meshes.insert(
                    model.to_owned(),
                    Arc::new(TriMesh {
                        positions,
                        indices,
                        local,
                    }),
                );
            }
            let mesh = meshes[model].clone();
            if mesh.local.is_empty() || mesh.indices.len() < 3 {
                continue;
            }
            let m = crate::viewer::instance_matrix(inst.rotation, inst.translation);
            blockers.push(Blocker {
                world: Aabb::transformed(mesh.local, m),
                inv: m.inverse(),
                mesh,
            });
        }
        Self { blockers }
    }

    pub fn is_empty(&self) -> bool {
        self.blockers.is_empty()
    }

    /// Pull `desired` toward `target` until the camera clears world geometry
    /// by `CAMERA_RADIUS`.
    pub fn pull_in(&self, target: Vec3, desired: Vec3) -> Vec3 {
        let delta = desired - target;
        let len = delta.length();
        if len < 1e-4 {
            return desired;
        }
        let dir = delta / len;
        let mut t = len;
        for b in &self.blockers {
            if !b.world.hits(target, dir, t, CAMERA_RADIUS) {
                continue;
            }
            let o = b.inv.transform_point3(target);
            let d = b.inv.transform_vector3(dir);
            for tri in b.mesh.indices.as_chunks::<3>().0 {
                let (a, bb, c) = (
                    b.mesh.positions[tri[0] as usize],
                    b.mesh.positions[tri[1] as usize],
                    b.mesh.positions[tri[2] as usize],
                );
                if let Some(hit) = ray_tri(o, d, t, a, bb, c) {
                    t = t.min(hit - CAMERA_RADIUS).max(0.0);
                }
            }
        }
        target + dir * t.max(len * MIN_TRACK_FRAC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10x10 m wall slab in the z = [-2.5, -2.0] plane, at identity.
    fn wall() -> CameraBlockers {
        let quads: [[Vec3; 4]; 2] = [
            [
                Vec3::new(-5.0, -5.0, -2.0),
                Vec3::new(5.0, -5.0, -2.0),
                Vec3::new(5.0, 5.0, -2.0),
                Vec3::new(-5.0, 5.0, -2.0),
            ],
            [
                Vec3::new(-5.0, -5.0, -2.5),
                Vec3::new(5.0, -5.0, -2.5),
                Vec3::new(5.0, 5.0, -2.5),
                Vec3::new(-5.0, 5.0, -2.5),
            ],
        ];
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for q in quads {
            let b = positions.len() as u32;
            positions.extend_from_slice(&q);
            indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let mut local = Aabb::empty();
        for p in &positions {
            local.add(*p);
        }
        let mesh = Arc::new(TriMesh {
            positions,
            indices,
            local,
        });
        CameraBlockers {
            blockers: vec![Blocker {
                world: Aabb::transformed(mesh.local, Mat4::IDENTITY),
                inv: Mat4::IDENTITY,
                mesh,
            }],
        }
    }

    #[test]
    fn camera_stops_short_of_a_wall() {
        let got = wall().pull_in(Vec3::ZERO, Vec3::new(0.0, 0.0, -4.0));
        assert!(
            (got.z - (-2.0 + CAMERA_RADIUS)).abs() < 1e-3,
            "stopped at {got:?}"
        );
    }

    #[test]
    fn clear_line_keeps_the_full_track_distance() {
        assert_eq!(
            wall().pull_in(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0)),
            Vec3::new(0.0, 0.0, 4.0)
        );
    }

    #[test]
    fn a_miss_to_the_side_does_not_pull_in() {
        assert_eq!(
            wall().pull_in(Vec3::new(20.0, 0.0, 0.0), Vec3::new(20.0, 0.0, -4.0)),
            Vec3::new(20.0, 0.0, -4.0)
        );
    }

    /// The reason this test file exists: a concave model's AABB covers open
    /// ground, and a box-only test collapsed the camera there. With the
    /// triangle pass, a segment through the gap keeps its full track.
    #[test]
    fn concave_model_does_not_block_through_its_opening() {
        let mut b = wall();
        // Punch the wall out of the way but keep its bounding box: move the
        // slab far to one side while its AABB still spans the origin.
        let mesh = Arc::new(TriMesh {
            positions: vec![
                Vec3::new(4.0, -5.0, -2.0),
                Vec3::new(5.0, -5.0, -2.0),
                Vec3::new(5.0, 5.0, -2.0),
            ],
            indices: vec![0, 1, 2],
            local: Aabb {
                min: Vec3::new(-5.0, -5.0, -2.5),
                max: Vec3::new(5.0, 5.0, -2.0),
            },
        });
        b.blockers[0] = Blocker {
            world: Aabb::transformed(mesh.local, Mat4::IDENTITY),
            inv: Mat4::IDENTITY,
            mesh,
        };
        assert_eq!(
            b.pull_in(Vec3::ZERO, Vec3::new(0.0, 0.0, -4.0)),
            Vec3::new(0.0, 0.0, -4.0),
            "the AABB is hit but no triangle is"
        );
    }

    #[test]
    fn target_inside_geometry_keeps_a_minimum_track() {
        // Player standing between the two slab faces: the far face is hit at
        // once, but the camera must not collapse onto the head.
        let got = wall().pull_in(Vec3::new(0.0, 0.0, -2.2), Vec3::new(0.0, 0.0, -6.2));
        assert!(
            (got.z - (-2.2 - 4.0 * MIN_TRACK_FRAC)).abs() < 1e-3,
            "{got:?}"
        );
    }

    #[test]
    fn rotated_instance_transforms_the_segment() {
        // Same slab, yawed 90 deg: it now blocks along +x instead of -z.
        let mut b = wall();
        let m = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        b.blockers[0].world = Aabb::transformed(b.blockers[0].mesh.local, m);
        b.blockers[0].inv = m.inverse();
        let got = b.pull_in(Vec3::ZERO, Vec3::new(-4.0, 0.0, 0.0));
        assert!((got.x - (-2.0 + CAMERA_RADIUS)).abs() < 1e-3, "{got:?}");
        assert_eq!(
            b.pull_in(Vec3::ZERO, Vec3::new(0.0, 0.0, -4.0)),
            Vec3::new(0.0, 0.0, -4.0),
            "no longer blocks the old direction"
        );
    }

    #[test]
    fn transformed_box_bounds_all_corners() {
        let local = Aabb {
            min: Vec3::new(-1.0, 0.0, -1.0),
            max: Vec3::new(1.0, 2.0, 1.0),
        };
        let w = Aabb::transformed(local, Mat4::from_rotation_y(std::f32::consts::FRAC_PI_4));
        assert!((w.max.x - 2f32.sqrt()).abs() < 1e-5, "{w:?}");
        assert!((w.max.y - 2.0).abs() < 1e-5, "{w:?}");
    }
}
