//! View-frustum culling: which draws can be skipped because their bounds are
//! entirely outside the camera's frustum.
//!
//! Six planes extracted from the view-projection matrix (Gribb & Hartmann),
//! tested against each draw's world-space AABB. The test is CONSERVATIVE — a
//! box that straddles a plane is kept — so culling can only remove geometry
//! that could not have contributed a pixel. Every committed golden is
//! byte-identical with it on, which is the real proof.
//!
//! What is NOT culled, and why:
//! - Sky shells: they are centred on the camera by construction.
//! - Skinned draws: an animated pose leaves the bind-pose AABB, so culling
//!   them needs a per-frame bound the animator does not yet produce. At the
//!   measured cost (about 1.7 us per skinned draw on the dGPU) they are also
//!   not where the draw-call budget goes.
//!
//! Terrain is no longer exempt — `game::viewer::scene::upload_terrain_chunks` splits
//! it into `TERRAIN_CHUNK_CELLS` draw chunks at upload (PLAN B4, 2026-08-31),
//! each with its own AABB, so it culls like any other draw.

use glam::{Mat4, Vec3, Vec4};

/// The six frustum planes in world space, each as (nx, ny, nz, d) with the
/// convention `dot(n, p) + d >= 0` inside.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// Extract from a view-projection matrix (Gribb & Hartmann 2001). glam is
    /// column-major, so a matrix ROW is `m.row(i)`.
    pub fn from_view_proj(vp: &Mat4) -> Frustum {
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);
        // wgpu clip space is x,y in [-1,1] and z in [0,1], so the near plane
        // is `+z` alone, not `w + z`.
        let mut planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2];
        for p in &mut planes {
            let n = p.truncate().length();
            if n > 1e-9 {
                *p /= n;
            }
        }
        Frustum { planes }
    }

    /// True if any part of the world-space AABB may be inside. Conservative:
    /// a box that is outside every plane individually is rejected; a box
    /// outside the frustum but inside every plane's half-space is kept.
    pub fn intersects_aabb(&self, lo: Vec3, hi: Vec3) -> bool {
        for p in &self.planes {
            let n = p.truncate();
            // The AABB corner furthest along the plane normal.
            let far = Vec3::new(
                if n.x >= 0.0 { hi.x } else { lo.x },
                if n.y >= 0.0 { hi.y } else { lo.y },
                if n.z >= 0.0 { hi.z } else { lo.z },
            );
            if n.dot(far) + p.w < 0.0 {
                return false;
            }
        }
        true
    }
}

/// The nearest and farthest distance from `eye` to any point of the world
/// box `lo..hi` (card LODP: [`crate::draw::all_discarded`]'s range).
pub fn distance_range(lo: Vec3, hi: Vec3, eye: Vec3) -> (f32, f32) {
    let near = eye.clamp(lo, hi).distance(eye);
    let far = (eye - lo).abs().max((eye - hi).abs()).length();
    (near, far)
}

/// World-space AABB of a model-space box under `model`.
pub fn transformed_aabb(model: &Mat4, lo: Vec3, hi: Vec3) -> (Vec3, Vec3) {
    // Transform the centre and extend by the absolute value of the rotated
    // half-extent — exact for any affine transform, and eight times cheaper
    // than transforming all eight corners.
    let centre = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let m = glam::Mat3::from_cols(
        model.x_axis.truncate(),
        model.y_axis.truncate(),
        model.z_axis.truncate(),
    );
    let world_centre = model.transform_point3(centre);
    let abs = glam::Mat3::from_cols(m.x_axis.abs(), m.y_axis.abs(), m.z_axis.abs());
    let world_half = abs * half;
    (world_centre - world_half, world_centre + world_half)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam_vp() -> Mat4 {
        // Looking down -Z from the origin, 60 degrees vertical, 1:1 aspect.
        let proj = Mat4::perspective_rh(60f32.to_radians(), 1.0, 0.1, 1000.0);
        proj * Mat4::IDENTITY
    }

    #[test]
    fn a_box_in_front_is_kept_and_one_behind_is_not() {
        let f = Frustum::from_view_proj(&cam_vp());
        assert!(f.intersects_aabb(Vec3::new(-1.0, -1.0, -11.0), Vec3::new(1.0, 1.0, -9.0)));
        // Same box behind the eye.
        assert!(!f.intersects_aabb(Vec3::new(-1.0, -1.0, 9.0), Vec3::new(1.0, 1.0, 11.0)));
    }

    #[test]
    fn a_box_far_off_to_the_side_is_culled() {
        let f = Frustum::from_view_proj(&cam_vp());
        assert!(!f.intersects_aabb(Vec3::new(99.0, -1.0, -11.0), Vec3::new(101.0, 1.0, -9.0)));
    }

    /// A box the camera is INSIDE must never be culled — the failure mode
    /// that would delete the terrain under your feet.
    #[test]
    fn a_box_around_the_camera_is_kept() {
        let f = Frustum::from_view_proj(&cam_vp());
        assert!(f.intersects_aabb(Vec3::splat(-500.0), Vec3::splat(500.0)));
    }

    /// Straddling the left plane is kept: the test is conservative.
    #[test]
    fn a_box_crossing_a_plane_is_kept() {
        let f = Frustum::from_view_proj(&cam_vp());
        assert!(f.intersects_aabb(Vec3::new(-20.0, -1.0, -11.0), Vec3::new(-5.0, 1.0, -9.0)));
    }

    /// Rotation, scale and translation must be carried into the world box.
    #[test]
    fn transformed_aabb_covers_a_rotated_scaled_box() {
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            Vec3::new(10.0, 0.0, -10.0),
        );
        let (lo, hi) = transformed_aabb(&m, Vec3::splat(-1.0), Vec3::splat(1.0));
        // A unit cube scaled x2 and turned 45 degrees spans 2*sqrt(2) in x/z.
        let span = 2.0 * 2.0 * std::f32::consts::SQRT_2;
        assert!((hi.x - lo.x - span).abs() < 1e-4, "{lo:?} {hi:?}");
        assert!((hi.y - lo.y - 4.0).abs() < 1e-4, "{lo:?} {hi:?}");
        assert!(((lo.x + hi.x) / 2.0 - 10.0).abs() < 1e-4, "{lo:?} {hi:?}");
        assert!(((lo.z + hi.z) / 2.0 + 10.0).abs() < 1e-4, "{lo:?} {hi:?}");
    }

    #[test]
    fn distance_range_bounds_every_point_of_the_box() {
        let (lo, hi) = (Vec3::new(10.0, -2.0, 5.0), Vec3::new(20.0, 3.0, 9.0));
        for eye in [
            Vec3::ZERO,
            Vec3::new(15.0, 0.0, 7.0),
            Vec3::new(-30.0, 40.0, 8.0),
        ] {
            let (near, far) = distance_range(lo, hi, eye);
            for k in 0..27 {
                let t = Vec3::new((k % 3) as f32, (k / 3 % 3) as f32, (k / 9) as f32) / 2.0;
                let d = (lo + (hi - lo) * t).distance(eye);
                assert!(
                    near - 1e-4 <= d && d <= far + 1e-4,
                    "{d} outside {near}..{far}"
                );
            }
        }
        // An eye inside the box is 0 from it; its far corner is the farthest point.
        let (near, far) = distance_range(lo, hi, Vec3::new(12.0, 0.0, 6.0));
        assert_eq!(near, 0.0);
        assert!((far - Vec3::new(8.0, 3.0, 3.0).length()).abs() < 1e-5);
    }
}
