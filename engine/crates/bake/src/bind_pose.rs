//! A placed model in its skeleton's bind pose (card TBIND): what the map bake
//! writes for a model that has a `skel`, so its moving parts sit where the PS2
//! draws them at rest.
//!
//! - A `bone_space` model (`modl INFO[0]` = 1): every vertex through its
//!   bone's bind matrix, `mungefs::model::to_model_space` — the vehicle and
//!   soldier bakes' rule (`analysis/re-softskin.md` §3, §12: flag 1 draws the
//!   pose matrices raw, and the rest pose is the bind pose).
//! - Any other model: a segment hard-bound by `BNAM` is stored in its bone's
//!   frame and goes through that bone's bind matrix; a segment with no `BNAM`
//!   stays as stored. PROVISIONAL [D]: `re-softskin.md` §3 reads flag 0 as
//!   model space (invBind × pose), but the PS2 draws BF2 HOT1's laser mortar
//!   with its head on the pillar, 3.57 m up at bone `turret_y`, where the
//!   stored segment sits at y −0.43..0.57
//!   (`lead_tmp/audit-vtur2/pose/bf2_hot1__cp1_cp6_postA_spawn__pair.png`),
//!   and the bone frame is the reading every checked BNAM segment fits
//!   (`lead_tmp/tbind/bnscan2.py`: UTA1's second barrel lands under its own
//!   `hp_fire3`, END1's treesmash trees ±11 m out, YAV1's watchtower gun on
//!   its 15 m tower, the space maps' ship turret barrels at their bones).
//!   The side vehicles fit it too, rotation included (card VBIND): BF1's
//!   Hailfire stores both hoop wheels flat at the origin, and through their
//!   bones they stand upright at x ±2.5..4.65, y 0.06..7.87 — inside the
//!   class's own model-space `coll` mesh (x ±4.72, y 0.01..7.92).
//! - A `door` class follows the same rule (card STDOR, closing BNR's
//!   question), and so do its collision prims: a prim's `XFRM` is in its
//!   `PRNT` bone's frame ([`BindPose::rest_prims`]). Both then rest in model
//!   space, where `crate::door` moves them by `B(f)·B(0)⁻¹`. [D], from the
//!   disc's three door shapes: DEA1's blast door (`bone_leftdoors` binds at
//!   x −5, its `open` clip runs −5 → −10) stores its leaves at x ±0.74..7.36
//!   — past the model's own 12 m frame with a 1.48 m slit — and through the
//!   bones they meet on the diagonal seam inside the frame and open to x
//!   ±2.64, the band the PS2's soldiers cross in (`re-round-stalls.md` §2
//!   C3); MUS1's tri and garage doors bind rotated 20.8° and their clips
//!   travel exactly along the bone's own y axis, so the panel slides in its
//!   plane only when it wears the bone's tilt; MUS1's window stores its two
//!   panes at y 0..3.24 and its shield at y 1.18..4.34, and through their
//!   bones all three lie in one slanted plane, y 1.40..4.47.
//!
//! A bone whose bind matrix is exactly the identity moves nothing, so a model
//! bound only to such bones is written byte for byte as stored.

use mungefs::entity::GeomModel;
use mungefs::model::Model;
use mungefs::skel::{xform_dir, xform_point, RowMat4, Skeleton};
use std::collections::HashSet;

const IDENTITY: RowMat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// An LVL's skeletons, geometry LOD lists and door geometries, from the map
/// bake's flattened walk (top level first; first of a name wins).
pub struct BindPose {
    skels: Vec<Skeleton>,
    gmods: Vec<GeomModel>,
    /// Lower-cased names of every `door` class's geometry and its LOD models.
    doors: HashSet<String>,
}

impl BindPose {
    pub fn of(chunks: &[mungefs::Chunk<'_>]) -> BindPose {
        let mut skels: Vec<Skeleton> = Vec::new();
        let mut gmods: Vec<GeomModel> = Vec::new();
        let mut door_geoms: Vec<String> = Vec::new();
        for c in chunks {
            match &c.tag {
                b"skel" => {
                    if let Ok(s) = mungefs::skel::parse_skel(c) {
                        if !skels.iter().any(|k| k.name.eq_ignore_ascii_case(&s.name)) {
                            skels.push(s);
                        }
                    }
                }
                b"gmod" => {
                    if let Ok(g) = GeomModel::parse(c) {
                        if !gmods.iter().any(|k| k.name.eq_ignore_ascii_case(&g.name)) {
                            gmods.push(g);
                        }
                    }
                }
                b"entc" => {
                    if let Ok(e) = mungefs::entity::EntityClass::parse(c) {
                        if crate::door::DoorSpec::from_entc(&e).is_some() {
                            door_geoms.extend(e.geometry_name().map(str::to_ascii_lowercase));
                        }
                    }
                }
                _ => {}
            }
        }
        let mut doors: HashSet<String> = door_geoms.iter().cloned().collect();
        for g in &gmods {
            if door_geoms.iter().any(|d| d.eq_ignore_ascii_case(&g.name)) {
                doors.extend(g.lods.iter().map(|l| l.model.to_ascii_lowercase()));
            }
        }
        BindPose {
            skels,
            gmods,
            doors,
        }
    }

    /// The skeleton a model is posed by: named as the model, else the one of
    /// a geometry that lists the model as a LOD (named as the geometry, else
    /// as its best LOD) — `crate::hardpoints::Skeletons::find`'s rule, reached
    /// from a LOD model's name as well as from the geometry's.
    fn skel_of(&self, model: &str) -> Option<&Skeleton> {
        let named = |n: &str| self.skels.iter().find(|s| s.name.eq_ignore_ascii_case(n));
        named(model).or_else(|| {
            self.gmods
                .iter()
                .find(|g| g.lods.iter().any(|l| l.model.eq_ignore_ascii_case(model)))
                .and_then(|g| named(&g.name).or_else(|| g.best_model().and_then(named)))
        })
    }

    /// The model in its bind pose, or `None` when that moves nothing (no
    /// skeleton, every bound bone at the identity).
    pub fn pose(&self, m: &Model) -> Result<Option<Model>, String> {
        let Some(skel) = self.skel_of(&m.name) else {
            return Ok(None);
        };
        if m.bone_space {
            let mut out = m.clone();
            for seg in out.segments.iter_mut() {
                let (p, n, _) = mungefs::model::to_model_space(m, seg, skel)
                    .map_err(|e| format!("{}: {e}", m.name))?;
                seg.positions = p;
                seg.normals = n;
            }
            out.bone_space = false;
            return Ok(Some(out));
        }
        pose_bnam(m, skel)
    }

    /// A door geometry's collision prims in model space at the bind pose:
    /// each prim's `XFRM` through its `PRNT` bone's bind matrix (the module
    /// doc's door rule). A prim on a bone bound at the identity, and every
    /// prim of a geometry that is not a door's, is left as stored
    /// (PROVISIONAL: `mungefs::coll` reads those as model space; a prim on a
    /// non-identity bone of a non-door geometry, read in RAM, would settle it).
    pub fn rest_prims(&self, geometry: &str, prims: &mut [manifest::ManifestPrim]) {
        if !self.doors.contains(&geometry.to_ascii_lowercase()) {
            return;
        }
        let named = |n: &str| self.skels.iter().find(|s| s.name.eq_ignore_ascii_case(n));
        let skel = named(geometry).or_else(|| {
            let g = self
                .gmods
                .iter()
                .find(|g| g.name.eq_ignore_ascii_case(geometry))?;
            g.best_model().and_then(named)
        });
        if let Some(skel) = skel {
            prims_at_bind(skel, prims);
        }
    }
}

/// Collision prims in model space at `skel`'s bind pose: each prim's `XFRM`
/// through its `PRNT` bone's bind matrix. A prim whose `PRNT` the skeleton
/// lacks, or whose bone binds at the identity, is left as stored. The door
/// rule ([`BindPose::rest_prims`]) and the vehicle bake's (card RKHUL,
/// `crate::vehicle`: every walker's legs and body, the BARC speeder's hull).
pub fn prims_at_bind(skel: &Skeleton, prims: &mut [manifest::ManifestPrim]) {
    let Ok(m0) = skel.bind_matrices() else {
        return;
    };
    for p in prims.iter_mut() {
        let Some(mat) = skel.index_of(&p.parent).map(|b| &m0[b]) else {
            continue;
        };
        if *mat == IDENTITY {
            continue;
        }
        // Row vectors: the prim's frame is `XFRM · bind`.
        for row in p.rotation.iter_mut() {
            *row = xform_dir(mat, *row);
        }
        p.translation = xform_point(mat, p.translation);
    }
}

/// The module doc's rule for a model that is not `bone_space`: each segment
/// hard-bound by `BNAM` through its bone's bind matrix. `None` when that moves
/// nothing (no `BNAM` names a bone of `skel`, or every bound bone sits at the
/// identity). The map bake's step ([`BindPose::pose`]) and the vehicle bake's
/// (`crate::vehicle`, card VBIND).
pub fn pose_bnam(m: &Model, skel: &Skeleton) -> Result<Option<Model>, String> {
    let m0 = skel
        .bind_matrices()
        .map_err(|e| format!("{}: {e}", m.name))?;
    let mut out = m.clone();
    let mut moved = false;
    for seg in out.segments.iter_mut() {
        let Some(b) = seg.bone.as_deref().and_then(|b| skel.index_of(b)) else {
            continue;
        };
        let mat = &m0[b];
        if *mat == IDENTITY {
            continue;
        }
        for p in seg.positions.iter_mut() {
            *p = xform_point(mat, *p);
        }
        for n in seg.normals.iter_mut() {
            *n = xform_dir(mat, *n);
        }
        moved = true;
    }
    Ok(moved.then_some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lvl(rel: &str) -> Option<Vec<u8>> {
        let p = format!(
            "{}/../../../extracted/bf2/lvl/DATA/_LVL_PS2/{rel}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(p).ok()
    }

    fn model(chunks: &[mungefs::Chunk<'_>], name: &str) -> Model {
        chunks
            .iter()
            .filter(|c| &c.tag == b"modl")
            .filter_map(|c| Model::parse(c).ok())
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .unwrap()
    }

    fn y_range(m: &Model, bone: &str) -> (f32, f32) {
        let s = m
            .segments
            .iter()
            .find(|s| s.bone.as_deref() == Some(bone))
            .unwrap();
        s.positions
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            })
    }

    /// Data-gated: BF2 HOT1's laser mortar (`SIDE/TUR.LVL`) wears its head
    /// on the pillar — the `turret_y` segment, stored at y −0.43..0.57,
    /// rides its bone 3.57 m up — and the base on `turret_base` (the
    /// identity) stays byte for byte as stored.
    #[test]
    fn the_hoth_mortar_head_sits_on_its_bone() {
        let Some(buf) = lvl("SIDE/TUR.LVL") else {
            eprintln!("SKIP: TUR.LVL not present");
            return;
        };
        let chunks = mungefs::content_chunks(mungefs::open(&buf).unwrap()).unwrap();
        let m = model(&chunks, "hoth_bldg_hoth_turret");
        let posed = BindPose::of(&chunks).pose(&m).unwrap().unwrap();
        let (lo, hi) = y_range(&posed, "turret_y");
        assert!(
            (lo - (3.57 - 0.43)).abs() < 0.02 && (hi - (3.57 + 0.57)).abs() < 0.02,
            "{lo}..{hi}"
        );
        let base = |m: &Model| {
            m.segments
                .iter()
                .find(|s| s.bone.as_deref() == Some("turret_base"))
                .unwrap()
                .positions
                .clone()
        };
        assert_eq!(base(&posed), base(&m));
    }

    fn range(m: &Model, bone: &str, axis: usize) -> (f32, f32) {
        m.segments
            .iter()
            .filter(|s| s.bone.as_deref() == Some(bone))
            .flat_map(|s| s.positions.iter())
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                (lo.min(p[axis]), hi.max(p[axis]))
            })
    }

    fn near(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
    }

    fn prims_of(chunks: &[mungefs::Chunk<'_>], name: &str) -> Vec<manifest::ManifestPrim> {
        let p = chunks
            .iter()
            .filter(|c| &c.tag == b"prim")
            .filter_map(|c| mungefs::coll::Primitives::parse(c).ok())
            .find(|p| p.model.eq_ignore_ascii_case(name))
            .unwrap();
        crate::map::manifest_prims(&p).0
    }

    /// Data-gated (card STDOR): DEA1's blast door rests in its bones' frame —
    /// the leaves inside the 12 m frame and overlapping on the seam, the
    /// prims with them — and the frame's own segments stay as stored.
    #[test]
    fn the_blast_door_leaves_rest_at_their_bones() {
        let Some(buf) = lvl("DEA/DEA1.LVL") else {
            eprintln!("SKIP: DEA1.LVL not present");
            return;
        };
        let chunks = mungefs::content_chunks(mungefs::open(&buf).unwrap()).unwrap();
        let m = model(&chunks, "dea1_prop_door_blast");
        let bp = BindPose::of(&chunks);
        let posed = bp.pose(&m).unwrap().unwrap();
        assert!(near(range(&m, "bone_leftdoors", 0), (0.74, 7.365)));
        assert!(near(range(&posed, "bone_leftdoors", 0), (-4.26, 2.365)));
        assert!(near(range(&posed, "bone_rightdoors", 0), (-2.365, 4.26)));
        assert_eq!(range(&posed, "dummyroot", 0), range(&m, "dummyroot", 0));
        let stored = prims_of(&chunks, "dea1_prop_door_blast");
        let mut prims = stored.clone();
        bp.rest_prims("dea1_prop_door_blast", &mut prims);
        for (p, s) in prims.iter().zip(&stored) {
            let d = if p.parent == "bone_leftdoors" {
                -5.0
            } else {
                5.0
            };
            assert_eq!(p.translation[0], s.translation[0] + d, "{}", p.name);
            assert_eq!(p.rotation, s.rotation);
        }
        // A geometry that is not a door's keeps its prims.
        let mut other = stored.clone();
        bp.rest_prims("dea1_prop_trashcompactor", &mut other);
        assert_eq!(other, stored);
    }

    /// Data-gated (card STDOR): MUS1's tri door wears its bone's 20.8° tilt,
    /// so the `open` clip's travel (0, 3.707, 1.409) lies in the panel's own
    /// plane; its prim tilts with it.
    #[test]
    fn the_mustafar_tri_door_slides_in_its_own_plane() {
        let Some(buf) = lvl("MUS/MUS1.LVL") else {
            eprintln!("SKIP: MUS1.LVL not present");
            return;
        };
        let chunks = mungefs::content_chunks(mungefs::open(&buf).unwrap()).unwrap();
        let m = model(&chunks, "mus1_prop_door_tri");
        let bp = BindPose::of(&chunks);
        let posed = bp.pose(&m).unwrap().unwrap();
        let travel = [0.0f32, 3.707, 1.409];
        let len = travel.iter().map(|v| v * v).sum::<f32>().sqrt();
        // The panel's extent along the travel's normal in the y-z plane.
        let across = |m: &Model| {
            let n = [0.0, -travel[2] / len, travel[1] / len];
            let (lo, hi) = m.segments[0]
                .positions
                .iter()
                .map(|p| p[1] * n[1] + p[2] * n[2])
                .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)));
            hi - lo
        };
        assert!(across(&m) > 1.5, "{}", across(&m)); // stored: the travel cuts through it
        assert!(across(&posed) < 0.13, "{}", across(&posed)); // posed: the panel's thickness
        let mut prims = prims_of(&chunks, "mus1_prop_door_tri");
        bp.rest_prims("mus1_prop_door_tri", &mut prims);
        let y_axis = prims[0].rotation[1];
        assert!((y_axis[1] - 0.9346).abs() < 1e-3 && (y_axis[2] - 0.3556).abs() < 1e-3);
    }
}
