//! The soldier-vs-wall queries: [`CollisionWorld::push_out`], the push out of
//! every wall a capsule penetrates, and [`CollisionWorld::wall_sweep`], the
//! first wall a step would sweep into — with the Phase A.4 penetration test
//! ([`penetration_block`]) and the parry 0.22.1 contact workaround
//! ([`mesh_contact`]) that the module header in `collision.rs` describes.

use super::shape::{isometry_of, Aabb, Body, Capsule, Layers, Shape};
use super::world::{CollisionWorld, HullFilter};
use super::{point, vector, Isometry, TriMesh, WALKABLE_MIN_NY};
use glam::Vec3;

/// PROVISIONAL (ours): capsule-vs-world push-out passes per tick.
pub const WALL_ITERATIONS: u32 = 2;
/// PROVISIONAL (ours), card WPSH: the longest XZ sub-step of
/// [`CollisionWorld::push_out_soldier_from`], in capsule radii. From a pose
/// touching a sheet, a step under one radius leaves the axis on the near
/// side; half a radius keeps a margin for the push's own slide.
pub const SUBSTEP_OF_RADIUS: f32 = 0.5;
/// PROVISIONAL (ours), card WPSH: at most this many sub-steps a tick (a
/// 2 m move at the 0.25 m sub-step). Anything longer is not a step but a
/// teleport; all sub-steps remain equal and can exceed the nominal limit.
pub const MAX_SUBSTEPS: u32 = 8;
/// PROVISIONAL (ours): the one tolerance [`CollisionWorld::wall_sweep`] has,
/// in metres. It does three jobs, all of them the same 1 mm:
///
/// 1. A cast hit NEARER than this is not a wall ahead but a body the capsule
///    already TOUCHES or is INSIDE OF, and the cast's answer for it is thrown
///    away — that body is decided by the penetration test instead
///    ([`penetration_block`]).
/// 2. It splits TOUCHING from PENETRATING in that test: deeper than this and
///    the start contact's own normal is trusted as the way out; shallower and
///    the Phase A.1 point test at the end pose decides.
/// 3. It is the tolerance of that test's depth comparison — a step may not
///    deepen a penetration by MORE than this.
///
/// The wall pass leaves a unit EXACTLY touching whatever it pushed it out of,
/// and a shape cast from a touching pose returns a hit at (float) zero
/// distance for every direction that does not strictly separate — a purely
/// TANGENTIAL one included. Measured: a capsule touching a wall at x = 1.5
/// reports a hit 6.2e-5 m along +Z, parallel to the wall, with normal
/// (-0.998, 0, -0.059) — a real wall normal, so no closing-velocity threshold
/// separates it from a genuine graze without becoming a tuning knob. The point
/// test does: "would the capsule be INSIDE this body at the end of the step"
/// is exact, and it is the rule Phase A.1 was measured against. From INSIDE a
/// body the cast is worse than uninformative — it reports zero in every
/// direction, out of the body included (measured 2026-09-03; see
/// [`CollisionWorld::wall_sweep`]) — which is why job 1 covers that case too.
///
/// It only has to be bigger than the float dust in that zero and smaller than
/// anything the steering must react to; 1 mm is four orders above the measured
/// 6.2e-5 m and three below a tick of walking (0.3 m). As job 3 it is also
/// what lets a unit walk ALONG a wall it is inside: over the AI's 1 m probe it
/// admits every direction within 0.06 deg of tangential.
pub const SWEEP_CONTACT_DIST: f32 = 1e-3;

/// The highest point of the triangle `t` whose XZ lies
/// within `r` of `c`, or `None` when the triangle's footprint misses the disc.
/// Height is linear over the triangle, so the maximum is at a vertex inside
/// the disc, where an edge crosses the circle, or (a leaning triangle) where
/// the circle is furthest up the triangle's own slope. The ledge rule's
/// "below the step where the capsule is" (card SKIRT, [`ledge_contact`]).
fn top_in_disc(t: [Vec3; 3], c: Vec3, r: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut take = |y: f32| best = Some(best.map_or(y, |b: f32| b.max(y)));
    let d2 = |p: Vec3| (p.x - c.x) * (p.x - c.x) + (p.z - c.z) * (p.z - c.z);
    for p in t {
        if d2(p) <= r * r {
            take(p.y);
        }
    }
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        // |a + s (b - a) - c|^2 = r^2 in XZ
        let (ex, ez) = (b.x - a.x, b.z - a.z);
        let (fx, fz) = (a.x - c.x, a.z - c.z);
        let qa = ex * ex + ez * ez;
        if qa < 1e-12 {
            continue;
        }
        let qb = 2.0 * (fx * ex + fz * ez);
        let qc = fx * fx + fz * fz - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            continue;
        }
        let sq = disc.sqrt();
        for s in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
            if (0.0..=1.0).contains(&s) {
                take(a.y + (b.y - a.y) * s);
            }
        }
    }
    // The circle's point furthest up the slope, when the triangle holds it.
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    if n.y.abs() > 1e-6 {
        // y = y0 - (n.x (x - x0) + n.z (z - z0)) / n.y: the gradient in XZ.
        let g = Vec3::new(-n.x / n.y, 0.0, -n.z / n.y);
        let gl = (g.x * g.x + g.z * g.z).sqrt();
        if gl > 1e-6 {
            let p = Vec3::new(c.x + g.x / gl * r, 0.0, c.z + g.z / gl * r);
            // barycentric in XZ
            let (v0x, v0z) = (t[1].x - t[0].x, t[1].z - t[0].z);
            let (v1x, v1z) = (t[2].x - t[0].x, t[2].z - t[0].z);
            let (v2x, v2z) = (p.x - t[0].x, p.z - t[0].z);
            let den = v0x * v1z - v1x * v0z;
            if den.abs() > 1e-9 {
                let u = (v2x * v1z - v1x * v2z) / den;
                let v = (v0x * v2z - v2x * v0z) / den;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
                    take(t[0].y + (t[1].y - t[0].y) * u + (t[2].y - t[0].y) * v);
                }
            }
        }
    }
    best
}

/// The world box `world` in `body`'s own space, as a parry AABB — the query
/// box for that body's mesh BVH ([`parry3d::partitioning::Bvh::intersect_aabb`]).
/// Bounds of the 8 transformed corners, so it is conservative for a rotated
/// instance, which is what a broad phase wants.
fn local_query_aabb(world: Aabb, body: &Body) -> parry3d::bounding_volume::Aabb {
    let local = Aabb::transformed(world, body.to_local);
    parry3d::bounding_volume::Aabb::new(point(local.min), point(local.max))
}

/// The deepest contact between the capsule (`cap_shape` placed by the WORLD
/// isometry `cap_iso`) and `mesh` as placed by `body`, or `None` when they do
/// not touch. Points and normals come back in WORLD space.
///
/// WORKAROUND for the parry 0.22.1 composite-contact bug — see the module
/// header. This is `CompositeShapeRef::contact_with_shape`'s own loop with the
/// pose the bug inverts: BVH candidates over the capsule's world bounds mapped
/// into body space, then parry's `contact` per TRIANGLE (a `Triangle` is
/// convex, not composite, so that call is correct), keeping the DEEPEST (most
/// negative `dist`). With `walls_first` (the wall pass) the deepest WALL-like
/// contact (|n.y| < [`WALKABLE_MIN_NY`]) wins over any deeper floor- or
/// ceiling-like one: the pass answers one contact per body, and an
/// overhang's underside deeper than the wall under it skipped the whole body
/// (card WTUN: BF1 BES1 body 26, underside 0.24 m, wall 0.07 m, walked
/// through by 95 census runs at one site).
///
/// Determinism: the same rule as everywhere else in this module — a strict `<`
/// in the BVH's TRAVERSAL ORDER, so the first candidate wins a tie, and that
/// order is fixed for a fixed mesh (the tree is built once, in [`trimesh`],
/// and never rebuilt). Nothing here is transcendental.
///
/// With `start` (the mover's axis segment at the start of its step, card
/// WPSH) a triangle whose contact would push the axis to the FAR side of
/// its face is not a candidate ([`pushes_past`]).
fn mesh_contact<S: parry3d::shape::Shape>(
    mesh: &TriMesh,
    body: &Body,
    cap_shape: &S,
    cap_iso: &Isometry,
    start: Option<(Vec3, Vec3)>,
    walls_first: bool,
) -> Option<parry3d::query::Contact> {
    let a = parry3d::shape::Shape::compute_aabb(cap_shape, cap_iso);
    let world = Aabb {
        min: Vec3::new(a.mins.x, a.mins.y, a.mins.z),
        max: Vec3::new(a.maxs.x, a.maxs.y, a.maxs.z),
    };
    let query = local_query_aabb(world, body);
    let mut best: Option<parry3d::query::Contact> = None;
    let mut best_wall: Option<parry3d::query::Contact> = None;
    for i in mesh.bvh().intersect_aabb(&query) {
        let tri = mesh.triangle(i);
        let c = parry3d::query::contact(cap_iso, cap_shape, &body.iso, &tri, 0.0)
            .ok()
            .flatten();
        let Some(c) = c else { continue };
        if start.is_some_and(|s| pushes_past(body, &tri, &c, s)) {
            continue;
        }
        if walls_first
            && c.dist < 0.0
            && c.normal2.y.abs() < WALKABLE_MIN_NY
            && best_wall.is_none_or(|b| c.dist < b.dist)
        {
            best_wall = Some(c);
        }
        if best.is_none_or(|b| c.dist < b.dist) {
            best = Some(c);
        }
    }
    best_wall.or(best)
}

/// PROVISIONAL (ours), card WPSH (`analysis/re-soldier-walls.md` §4.1): would
/// the contact `c` with the mesh triangle `tri` (in `body`'s space) push the
/// mover's axis to the FAR side of that triangle's face?
///
/// `start` is the axis segment (bottom and top sphere centres, world) at the
/// START of the step. When the whole segment lay on one side of the face's
/// plane, the soldier came from that side; a horizontal push along `c`'s
/// normal that points to the other side is not a way out of the triangle but
/// a way through it, and the pass must not take it. A segment that already
/// straddled the plane at the start says nothing, and the contact stands.
///
/// The case (the LEAK census's KAS2 control, the cp5 hut): a jump at the
/// flared, double-skinned `kas2_bldg_platform_roof` wall. The outer skin
/// leans out over the soldier (n.y −0.54, a ceiling, then skipped), so the capsule
/// rises into the flare until its axis pierces the INNER skin, triangle 5,
/// from behind. That triangle's contact is the body's deepest (0.544 m,
/// normal (0.303, 0, −0.953)), and it pushed the soldier 0.84 m into the hut.
/// A zero-thickness sheet stepped half through (M2) is the same case.
fn pushes_past(
    body: &Body,
    tri: &parry3d::shape::Triangle,
    c: &parry3d::query::Contact,
    start: (Vec3, Vec3),
) -> bool {
    let Some(face) = tri.normal() else {
        return false;
    };
    let f = body.iso.rotation * face.into_inner();
    let f = Vec3::new(f.x, f.y, f.z);
    let on = body
        .to_world
        .transform_point3(Vec3::new(tri.a.x, tri.a.y, tri.a.z));
    let (s0, s1) = ((start.0 - on).dot(f), (start.1 - on).dot(f));
    let side = if s0 > 0.0 && s1 > 0.0 {
        1.0
    } else if s0 < 0.0 && s1 < 0.0 {
        -1.0
    } else {
        return false; // already straddling the plane at the start
    };
    // Only a start IN FRONT OF (or behind) the triangle itself: the point of
    // the start segment at the triangle's mid height, projected on its plane,
    // inside it. Beside the triangle, the soldier is behind its plane's
    // EXTENSION, not the face (a neighbouring face of a corner or a crevice),
    // and its contact stands; dropping it trapped soldiers in BF2 MYG1's
    // crevices, pushed to and fro each tick (census `float` 43 -> 150).
    let w = |p: parry3d::na::Point3<f32>| body.to_world.transform_point3(Vec3::new(p.x, p.y, p.z));
    let (a, b, cc) = (on, w(tri.b), w(tri.c));
    let mid = (a.y + b.y + cc.y) / 3.0;
    let (lo, hi) = if start.0.y <= start.1.y {
        start
    } else {
        (start.1, start.0)
    };
    let p = if hi.y - lo.y > 1e-6 {
        lo.lerp(hi, ((mid - lo.y) / (hi.y - lo.y)).clamp(0.0, 1.0))
    } else {
        lo
    };
    let q = p - f * (p - a).dot(f);
    // No margin: widening the prism by 0.25 or 0.5 m saves 14-40 of the
    // census's pushes on KAS2/DAG1/MYG1 and brings MYG1's crevice trap back.
    let inside = |u: Vec3, v: Vec3| (v - u).cross(q - u).dot(f) >= 0.0;
    if !(inside(a, b) && inside(b, cc) && inside(cc, a)) {
        return false;
    }
    // The push is the contact normal's horizontal part (`push_out_with`).
    let push = Vec3::new(c.normal2.x, 0.0, c.normal2.z);
    side * push.dot(f) < 0.0
}

/// PROVISIONAL (ours): how far the ledge rule's HORIZONTAL escape
/// ([`ledge_escape`]) may push. A soldier deeper than this inside a
/// primitive was inside before the rule existed (an old replay, a spawn); he
/// is left where he is rather than teleported across a lobe.
const LEDGE_ESCAPE_MAX: f32 = 1.5;

/// A LEDGE's answer for [`CollisionWorld::push_out_soldier`], as
/// `(unit normal out of the body, depth)`, from the FULL capsule `full` (its
/// parry shape `fshape` placed by `fiso`) against `body`.
///
/// A penetrating WALL-like contact (|n.y| < [`WALKABLE_MIN_NY`]) is the
/// answer. Otherwise the result is `None` (no contact, or only touching),
/// except for one case. A floor-like full contact means the shortest way out
/// is UP. That is a real step or landing when the ground query can stand the
/// soldier on the top. For a primitive whose top it cannot reach, it is
/// [`ledge_escape`]'s horizontal way out instead.
///
/// A primitive is convex, so parry's `contact` is the whole answer. A MESH
/// is asked per triangle, over the triangles that are WALLS by their own FACE
/// normal (rotated to world; instances are rigid) and that reach above the
/// step (a world vertex above `full.base.y + lift`) inside the capsule's
/// footprint ([`top_in_disc`]). A floor triangle under the feet, or a kerb's
/// riser below the step where the capsule is, cannot answer for the ledge.
/// Among those, the deepest wall-like contact wins, with a strict `<` in the
/// BVH's traversal order, as in [`mesh_contact`], and with its `start` rule
/// ([`pushes_past`]) on the FULL capsule's start segment.
fn ledge_contact(
    body: &Body,
    full: &Capsule,
    fshape: &parry3d::shape::Capsule,
    fiso: &Isometry,
    start: Option<(Vec3, Vec3)>,
) -> Option<(Vec3, f32)> {
    let wall = |c: &parry3d::query::Contact| c.dist < 0.0 && c.normal2.y.abs() < WALKABLE_MIN_NY;
    let answer =
        |c: parry3d::query::Contact| (Vec3::new(c.normal2.x, c.normal2.y, c.normal2.z), -c.dist);
    match &body.shape {
        Shape::Mesh(mesh) => {
            let step_top = full.base.y + super::STEP_HEIGHT.min(full.height * 0.5);
            let query = local_query_aabb(full.aabb(), body);
            let mut best: Option<parry3d::query::Contact> = None;
            for i in mesh.bvh().intersect_aabb(&query) {
                let tri = mesh.triangle(i);
                let Some(face) = tri.normal() else { continue };
                if (body.iso.rotation * face.into_inner()).y.abs() >= WALKABLE_MIN_NY {
                    continue; // a floor TRIANGLE is not the ledge's wall
                }
                let top = [tri.a, tri.b, tri.c]
                    .iter()
                    .map(|p| body.to_world.transform_point3(Vec3::new(p.x, p.y, p.z)).y)
                    .fold(f32::MIN, f32::max);
                if top <= step_top {
                    continue; // below the step: a kerb, climbed, not a wall
                }
                // ...and below the step WHERE THE CAPSULE IS — the triangle's
                // highest point inside the capsule's footprint — is a kerb there
                // too, whatever it rises to further on (card SKIRT: the riser
                // beside BF2 Kamino's pod skirt, two vertical triangles 1.9 m
                // high whose low end the soldier walks up beside, took 6 of his
                // 7 m/s in one tick; re-soldier-ground.md §6-7).
                let w = |p: parry3d::na::Point3<f32>| {
                    body.to_world.transform_point3(Vec3::new(p.x, p.y, p.z))
                };
                if top_in_disc([w(tri.a), w(tri.b), w(tri.c)], full.base, full.radius)
                    .is_none_or(|t| t <= step_top)
                {
                    continue;
                }
                let c = parry3d::query::contact(fiso, fshape, &body.iso, &tri, 0.0)
                    .ok()
                    .flatten();
                let Some(c) = c.filter(wall) else { continue };
                if start.is_some_and(|s| pushes_past(body, &tri, &c, s)) {
                    continue; // card WPSH, as in `mesh_contact`
                }
                if best.is_none_or(|b| c.dist < b.dist) {
                    best = Some(c);
                }
            }
            best.map(answer)
        }
        _ => {
            let c = body.shape.with_parry(&mut |s| {
                parry3d::query::contact(fiso, fshape, &body.iso, s, 0.0)
                    .ok()
                    .flatten()
            })?;
            if c.dist >= 0.0 {
                return None;
            }
            if wall(&c) {
                return Some(answer(c));
            }
            ledge_escape(body, full)
        }
    }
}

/// The HORIZONTAL way out of an upright primitive (a `Cylinder`, or a `Box`
/// turned only about Y) whose top is beyond the ground query's reach
/// (`top − feet > STEP_HEIGHT`), when the full capsule's own contact says
/// "up". `None` for anything else (a sphere, a tilted box such as a stair
/// ramp, a top within the step, no horizontal overlap, or deeper than
/// [`LEDGE_ESCAPE_MAX`]).
///
/// The case it serves (re-soldier-walls §2, the City Hall residue after the
/// ledge rule): a jump reaches a lobe's rim at its apex. The stepped
/// capsule's round bottom clears the rim in one tick without touching it,
/// and one tick later the full capsule is 0.3 m inside, where the shortest
/// way out of the solid is up (0.61) rather than out (0.81). The ground query
/// cannot lift him 0.61 m, so up is no way out at all. Radially out of a
/// cylinder, or out through the nearest side face of a box, is.
fn ledge_escape(body: &Body, full: &Capsule) -> Option<(Vec3, f32)> {
    let up = body.to_world.transform_vector3(Vec3::Y);
    if up.y < 0.9999 {
        return None; // not upright
    }
    let lift = super::STEP_HEIGHT.min(full.height * 0.5);
    if body.aabb.max.y - full.base.y <= lift {
        return None; // the top is within the step: the ground query's
    }
    let l = body.to_local.transform_point3(full.base);
    let r = full.radius;
    let (dir, escape) = match &body.shape {
        Shape::Cylinder { radius, .. } => {
            let d = (l.x * l.x + l.z * l.z).sqrt();
            if d < 1e-6 {
                return None;
            }
            (Vec3::new(l.x / d, 0.0, l.z / d), radius + r - d)
        }
        Shape::Box { half } => {
            let ex = half.x + r - l.x.abs();
            let ez = half.z + r - l.z.abs();
            if ex <= ez {
                (Vec3::new(if l.x < 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0), ex)
            } else {
                (Vec3::new(0.0, 0.0, if l.z < 0.0 { -1.0 } else { 1.0 }), ez)
            }
        }
        _ => return None,
    };
    if !(escape > 0.0 && escape <= LEDGE_ESCAPE_MAX) {
        return None;
    }
    Some((body.to_world.transform_vector3(dir), escape))
}

impl CollisionWorld {
    /// PROVISIONAL (ours): the horizontal push that moves `cap` out of every
    /// wall it penetrates. The PS2's soldier-vs-world response is untraced, so
    /// this is a plain iterated push-out along the contact normal, with the
    /// vertical component dropped — floors are the ground query's job and a
    /// ceiling-like contact pushes only by its normal's horizontal part (card
    /// WTUN). Returns the total push (y is exactly 0).
    ///
    /// A SOLDIER query: bodies outside [`Layers::SOLDIER`] are skipped (RE §4).
    ///
    /// Order matters here beyond the tie-breaks: pushes are ACCUMULATED in
    /// ascending `bodies` order, which is the order the broad phase offers.
    /// Contacts use one pose snapshot per iteration; the next iteration
    /// asks the poses moved by the accumulated pushes.
    ///
    /// This is the bare pass, with no LEDGE rule: the sim's soldiers go
    /// through [`Self::push_out_soldier`].
    pub fn push_out(&self, cap: &Capsule) -> Vec3 {
        self.push_out_with(cap, None, None, HullFilter::All)
    }

    /// PROVISIONAL (ours): the soldier's wall pass (card JWAL,
    /// `analysis/re-soldier-walls.md` §7). It is [`Self::push_out`] of the
    /// STEPPED capsule, `full.stepped()`, so a kerb under `STEP_HEIGHT` stays
    /// a thing to climb, plus the LEDGE rule.
    ///
    /// The bare pass skips every contact whose normal has
    /// |n.y| ≥ [`WALKABLE_MIN_NY`] as "a floor, the ground query's job". But
    /// an UPWARD contact on the stepped capsule is a surface ABOVE the step,
    /// which the ground query (its probe starts at feet + `STEP_HEIGHT`) can
    /// never stand the soldier on. Such a contact is a ledge's rim under the
    /// lifted capsule's lower sphere.
    ///
    /// Skipping it let a jump carry a BF1 soldier over the rim of a KAS2 City
    /// Hall lobe (a solid prim, 2.25 m tall), and let a walk pass through a
    /// 0.6–0.75 m box. The soldier then fell inside the solid onto the
    /// terrain (re-soldier-walls §2). The PS2 lets neither happen (§5.1,
    /// PCSX2).
    ///
    /// So a LEDGE is re-asked with the FULL capsule, `full`
    /// ([`ledge_contact`]), and its wall-like contact pushes as a wall does. A
    /// floor-like full contact is a real step or landing (the feet within the
    /// penetration of the top), which the ground query lifts him onto.
    /// Downward contacts (ceilings) push only by their horizontal part (card
    /// WTUN, in [`Self::push_out`]). A body the stepped capsule
    /// does not penetrate at all is asked the same ledge question: a riser a
    /// few cm over the step slips under the lifted capsule's round bottom.
    pub fn push_out_soldier(&self, full: &Capsule) -> Vec3 {
        self.push_out_with(&full.stepped(), Some(*full), None, HullFilter::All)
    }

    /// [`Self::push_out_soldier`] against the vehicles' HULLS alone: the AI's
    /// probe for a parked vehicle's low parts, which the steering's sweep
    /// (the stepped capsule) passes over and this pass pushes back from
    /// (card STFAL, `analysis/re-round-stalls.md` C8).
    pub fn push_out_soldier_hulls(&self, full: &Capsule) -> Vec3 {
        self.push_out_with(&full.stepped(), Some(*full), None, HullFilter::Hulls)
    }

    /// PROVISIONAL (ours): the soldier's wall pass for a tick that moved him
    /// from `from` to `full` (card WPSH, `analysis/re-soldier-walls.md` §4):
    /// [`Self::push_out_soldier`] along the move, with two rules the pose
    /// alone cannot give.
    ///
    /// - SUB-STEPS. A `coll` mesh wall is a zero-thickness sheet. Once a
    ///   tick's step carries the axis past it, the deepest contact points to
    ///   the far side and the push completes the crossing. From a pose
    ///   touching the wall that takes a step over the radius: a BF2
    ///   sprint-jump (17.55 m/s, 0.585 m a tick) passed a 4 m sheet in 46 of
    ///   51 timings. So the move is cut into equal sub-steps of at most
    ///   [`SUBSTEP_OF_RADIUS`] radii in XZ, each followed by the pass, and
    ///   the pushes carry forward. A walk (0.22 m a tick in BF1) is one step,
    ///   as before; a sprint or a jump is two or three.
    /// - NO PUSH THROUGH A FACE ([`pushes_past`]). Each (sub-)step's start
    ///   pose says which side of every mesh face the soldier came from; a
    ///   triangle contact that would push him to the other side is dropped.
    ///
    /// [H] The PS2 tests the soldier SWEPT from its previous position C+0x198
    /// to C+0x228 (§5.3: a RAM teleport that leaves C+0x198 behind falls
    /// through the world). The sub-steps are the port's stand-in for that
    /// sweep; the PS2's own test is not read.
    ///
    /// Returns the total push (y exactly 0), to be added to `full.base`.
    pub fn push_out_soldier_from(&self, from: &Capsule, full: &Capsule) -> Vec3 {
        let d = full.base - from.base;
        let dxz = (d.x * d.x + d.z * d.z).sqrt();
        let limit = full.radius * SUBSTEP_OF_RADIUS;
        let n = if limit > 0.0 {
            ((dxz / limit).ceil() as u32).clamp(1, MAX_SUBSTEPS)
        } else {
            1
        };
        let mut total = Vec3::ZERO;
        let mut at = Capsule {
            base: from.base,
            ..*full
        };
        for k in 1..=n {
            let start = at;
            let mut now = *full;
            now.base = from.base + d * (k as f32 / n as f32) + total;
            total += self.push_out_with(&now.stepped(), Some(now), Some(start), HullFilter::All);
            at = now;
            at.base = from.base + d * (k as f32 / n as f32) + total;
        }
        total
    }

    /// [`Self::push_out`] of `cap`, with the LEDGE rule when `full` (the
    /// unstepped capsule `cap` was lifted from) is given. Contacts in each
    /// iteration use fixed capsule snapshots; pushes move both capsules for
    /// the next iteration. `start` is
    /// the full capsule at the start of the step, for [`pushes_past`].
    fn push_out_with(
        &self,
        cap: &Capsule,
        full: Option<Capsule>,
        start: Option<Capsule>,
        hulls: HullFilter,
    ) -> Vec3 {
        let mut total = Vec3::ZERO;
        let mut cap = *cap;
        let mut full = full;
        let start_seg = start.map(|s| s.stepped().segment());
        let start_full_seg = start.map(|s| s.segment());
        for _ in 0..WALL_ITERATIONS {
            let mut moved = false;
            let (shape, iso) = cap.to_parry();
            let whole = full.map(|f| (f, f.to_parry()));
            let cap_aabb = cap.aabb();
            self.visit_xz(
                cap_aabb.min.x,
                cap_aabb.max.x,
                cap_aabb.min.z,
                cap_aabb.max.z,
                hulls,
                |body| {
                    if !body.layers.contains(Layers::SOLDIER) {
                        return;
                    }
                    if !body.aabb.overlaps(&cap_aabb) {
                        return;
                    }
                    let c = match &body.shape {
                        // A MESH is composite: parry's own `contact` would test the
                        // capsule at a mirrored pose (module header), so ask the
                        // triangles ourselves.
                        Shape::Mesh(mesh) => {
                            mesh_contact(mesh, body, &shape, &iso, start_seg, true)
                        }
                        // Primitives are convex, not composite — parry's `contact`
                        // is correct for them.
                        _ => body.shape.with_parry(&mut |s| {
                            parry3d::query::contact(&iso, &shape, &body.iso, s, 0.0)
                                .ok()
                                .flatten()
                        }),
                    };
                    // Touching is not penetrating.
                    // `normal2` points out of shape 2 (the body), i.e. at the
                    // capsule — the direction the capsule has to move.
                    let stepped = c
                        .filter(|c| c.dist < 0.0)
                        .map(|c| (Vec3::new(c.normal2.x, c.normal2.y, c.normal2.z), -c.dist));
                    let (n, depth) = match stepped {
                        Some((n, depth)) if n.y.abs() < WALKABLE_MIN_NY => (n, depth),
                        // A ceiling-like contact (n.y ≤ −WALKABLE_MIN_NY): the
                        // pass has no vertical response (card VCOL), but its
                        // HORIZONTAL part still pushes, NOT renormalised
                        // (card WTUN). A slab leaning over the soldier, an
                        // overhang's lower edge at his head: skipping it let
                        // him walk through the face under it (BF2 DAG1's
                        // tilted boxes, 59 census runs at one site). A flat
                        // ceiling's contact is almost straight down and
                        // pushes almost nothing.
                        Some((n, depth)) if n.y < 0.0 => {
                            (n, depth * (n.x * n.x + n.z * n.z).sqrt())
                        }
                        // Floor-like upward: the bare pass skips it; a
                        // soldier's LEDGE is asked again with the full capsule
                        // (`push_out_soldier`). And when the stepped capsule is
                        // clear, the FULL one may still be in a wall whose top
                        // is just over the step: the lifted capsule's round
                        // bottom passes over the edge (a 0.62 m riser, BF2
                        // KAM1), the ground query cannot climb it, and a tick
                        // later the axis is past the face. The ledge rule
                        // answers for that too.
                        _ => {
                            let Some((f, (fshape, fiso))) = whole.as_ref() else {
                                return;
                            };
                            let Some(ledge) = ledge_contact(body, f, fshape, fiso, start_full_seg)
                            else {
                                return;
                            };
                            ledge
                        }
                    };
                    let n_xz = Vec3::new(n.x, 0.0, n.z);
                    let len = n_xz.length();
                    if len < 1e-6 {
                        return;
                    }
                    let push = n_xz / len * depth;
                    cap.base += push;
                    if let Some(f) = full.as_mut() {
                        f.base += push;
                    }
                    total += push;
                    moved = true;
                },
            );
            if !moved {
                break;
            }
        }
        total
    }

    /// Every body in `layers` a sphere penetrates, in ascending body order:
    /// `(depth, unit normal out of the body)` — the deepest contact per
    /// body (meshes per triangle, as [`Self::push_out`]). The hover's spring
    /// and hull spheres (`crate::vehicle::hover`) read the world through it,
    /// so it sees the map's bodies only ([`HullFilter::Static`]: no
    /// vehicle meets another's hull yet, card VHULL).
    pub fn sphere_contacts(
        &self,
        centre: Vec3,
        radius: f32,
        layers: Layers,
        out: &mut Vec<(f32, Vec3)>,
    ) {
        let ball = parry3d::shape::Ball::new(radius);
        let iso = Isometry::translation(centre.x, centre.y, centre.z);
        let aabb = Aabb {
            min: centre - Vec3::splat(radius),
            max: centre + Vec3::splat(radius),
        };
        self.collect_contacts(&ball, &iso, &aabb, layers, out);
    }

    /// [`Self::sphere_contacts`] for a box or a cylinder placed by `to_world`
    /// (rigid): a hover's hull body against the buildings (card HHUL,
    /// re-hover §13). PROVISIONAL: parry's exact contact stands in for the
    /// PS2's prim-vs-body narrowphase (FUN_0016bf10, not transcribed). A
    /// sphere or a mesh mover finds nothing here.
    pub fn shape_contacts(
        &self,
        shape: &Shape,
        to_world: glam::Mat4,
        layers: Layers,
        out: &mut Vec<(f32, Vec3)>,
    ) {
        let iso = isometry_of(to_world);
        match *shape {
            Shape::Box { half } => self.convex_contacts(
                &parry3d::shape::Cuboid::new(vector(half)),
                &iso,
                layers,
                out,
            ),
            Shape::Cylinder {
                radius,
                half_height,
            } => self.convex_contacts(
                &parry3d::shape::Cylinder::new(half_height, radius),
                &iso,
                layers,
                out,
            ),
            Shape::Sphere { .. } | Shape::Mesh(_) => {}
        }
    }

    fn convex_contacts<S: parry3d::shape::Shape>(
        &self,
        mover: &S,
        iso: &Isometry,
        layers: Layers,
        out: &mut Vec<(f32, Vec3)>,
    ) {
        let a = parry3d::shape::Shape::compute_aabb(mover, iso);
        let aabb = Aabb {
            min: Vec3::new(a.mins.x, a.mins.y, a.mins.z),
            max: Vec3::new(a.maxs.x, a.maxs.y, a.maxs.z),
        };
        self.collect_contacts(mover, iso, &aabb, layers, out);
    }

    /// Shared narrowphase; callers retain their own exact query bounds.
    fn collect_contacts<S: parry3d::shape::Shape>(
        &self,
        mover: &S,
        iso: &Isometry,
        aabb: &Aabb,
        layers: Layers,
        out: &mut Vec<(f32, Vec3)>,
    ) {
        self.visit_xz(
            aabb.min.x,
            aabb.max.x,
            aabb.min.z,
            aabb.max.z,
            HullFilter::Static,
            |body| {
                if !body.layers.contains(layers) || !body.aabb.overlaps(aabb) {
                    return;
                }
                let c = match &body.shape {
                    Shape::Mesh(mesh) => mesh_contact(mesh, body, mover, iso, None, false),
                    _ => body.shape.with_parry(&mut |s| {
                        parry3d::query::contact(iso, mover, &body.iso, s, 0.0)
                            .ok()
                            .flatten()
                    }),
                };
                let Some(c) = c.filter(|c| c.dist < 0.0) else {
                    return;
                };
                out.push((-c.dist, Vec3::new(c.normal2.x, c.normal2.y, c.normal2.z)));
            },
        );
    }

    /// PROVISIONAL (ours): the first WALL the capsule would SWEEP into moving
    /// `dist` metres along `dir_xz` — `(distance to the hit, unit XZ push
    /// normal)`, or `None` when the whole sweep is clear. The normal is the
    /// direction the capsule would have to move to get out, so it points back
    /// at the mover.
    ///
    /// TWO TESTS PER BODY, each answering what the other cannot (Phase A.4):
    ///
    /// (a) THE CAST — parry's LINEAR shape cast (`query::cast_shapes`, the
    /// capsule moving at unit speed along `dir_xz` against a static body, so
    /// the returned time of impact IS a distance in metres). It catches a wall
    /// the step passes THROUGH, which is why A.2 replaced the A.1 point probe:
    /// a wall thinner than the gap the step jumped — every BF2 partition and
    /// railing — sat between the unit and the probe and was never touched,
    /// while the wall pass pushed the unit back every tick (PLAN "PHASE A.2
    /// DESIGN"). It runs with the same AABB prefilter as [`Self::push_out`]
    /// widened to the SWEPT capsule's bounds, the same [`WALKABLE_MIN_NY`]
    /// floor filter (a hit within 60 deg of vertical is a floor, not a wall),
    /// and `stop_at_penetration`.
    ///
    /// (b) THE PENETRATION TEST — a body the capsule already TOUCHES or is
    /// INSIDE OF, which the cast cannot judge (below); it is decided by
    /// penetration DEPTH instead. A step may keep or reduce a penetration —
    /// walking ALONG a wall, or back OUT of one — but it may never deepen one,
    /// and it may not end inside a body it started clear of (that half is the
    /// Phase A.1 point test, which the whole steering layer was measured
    /// against). See [`penetration_block`] for the rule and
    /// [`deepened_by_step`] for the two exact comparisons.
    ///
    /// (b) OWNS every body whose cast reports nothing usable — no hit at all,
    /// or a hit inside [`SWEEP_CONTACT_DIST`] — and only those: a usable cast
    /// hit means the capsule is CLEAR of that body at the start of the step
    /// (a cast from an overlapping pose reports zero), so the cast's distance
    /// is the honest answer and (b) is not asked. A (b) block is reported at
    /// `t = 0` and so beats any (a) hit under the strict `<` below.
    ///
    /// WHY the cast cannot judge a body the capsule is inside (measured
    /// 2026-09-03 on the fixtures of the two A.4 tests, and read in parry3d
    /// 0.22.1): `cast_shapes` on a penetrating pose returns
    /// `time_of_impact = 0.0` for EVERY direction — including straight back
    /// out — with the SAME normal each time. `gjk::directional_distance`
    /// (`query/gjk/gjk.rs`) ray-casts the ray's origin against the Minkowski
    /// difference; with the origin INSIDE it, `project_origin_and_reduce`
    /// gives the zero vector, `Unit::try_new_and_get` fails and it returns
    /// `(ltoi = 0, ldir)` whatever the direction. `cast_shapes_support_map_-`
    /// `support_map` then takes its `time_of_impact < 1.0e-5` branch and fills
    /// the hit in from `contact_support_map_support_map`, i.e. from the
    /// direction-INDEPENDENT contact. So the cast answers "you are inside",
    /// never "which way".
    ///
    /// WHY the old rule then got it wrong. Inside [`SWEEP_CONTACT_DIST`] the
    /// A.2 code asked the A.1 point test — does the capsule PENETRATE at the
    /// END of the step? — which is exact for a body it is merely touching and
    /// wrong for one it is already inside, in both directions:
    /// - a 118 deg rotated `Box { half (4, 4, 9.8) }`, capsule (h 1.9, r 0.5)
    ///   with its feet at y 2.9 pushed 0.1 … 1.0 m into a side face, stepping
    ///   1.0 m FURTHER IN (`ai::PROBE_DIST`): the end pose is deeper than the
    ///   capsule's feet are below the box's TOP, so the deepest contact's
    ///   minimum-translation direction flips to (0, 1, 0) — a FLOOR — and the
    ///   whole body was skipped. `wall_sweep` answered `None` INTO the wall at
    ///   every depth while `push_out` reported the penetration exactly. That
    ///   is BF2 MYG1 unit 6, 53,400 ticks of `steer fwd` with `push 0.300`
    ///   0.33 m inside `myg1_bldg_large_platform_b : p_-v-rob`.
    /// - a `coll` MESH wall is a zero-thickness sheet: a capsule 0.1 … 0.45 m
    ///   inside one, stepping 1.0 m further in, ends CLEAN THROUGH it with no
    ///   contact at all, so INTO the wall answered `None` — while stepping
    ///   ALONG it, where the end pose is still inside, answered blocked. The
    ///   wrong way round in both directions.
    ///
    /// Phase A.3 — THE FIRST NON-FLOOR HIT PER MESH BODY, unchanged except
    /// that its own touching sub-branch is gone with the body one (a triangle
    /// hit inside [`SWEEP_CONTACT_DIST`] is ignored; those bodies belong to
    /// (b)). One `cast_shapes` per body answers for a body whose hit is a
    /// WALL. But a body whose whole-body hit is a FLOOR used to contribute
    /// nothing at all, so a wall standing behind a ramp, a lip or a sloped
    /// floor OF THE SAME MESH was never seen: on BF1 NAB2 ten units sat at
    /// y −49 reporting `steer fwd` while the wall pass pushed them back 0.30 m
    /// every tick (PLAN Log, "Phase A.2 landed"). So ONLY when the whole-body
    /// answer is a floor and the body is a [`Shape::Mesh`], the mesh is
    /// re-asked PER TRIANGLE: the earliest hit over its NON-FLOOR triangles,
    /// where floor-vs-wall is decided by the triangle's own FACE normal
    /// (rotated into world space — instances are rigid) and the returned push
    /// normal is still the CONTACT normal. Candidates come from the mesh's BVH
    /// over the swept AABB in body-local space.
    ///
    /// PRIMITIVES (`Sphere`, `Cylinder`, `Box`) get no per-face pass in (a):
    /// they are convex, so a sweep enters them once and the one contact is the
    /// whole answer. A graze along a primitive's TOP EDGE reports a
    /// floor-like normal. That graze is a LEDGE (card JWAL,
    /// `analysis/re-soldier-walls.md`): the swept capsule is the stepped one,
    /// so the primitive stands above the step, and it blocks like a wall. It
    /// used to skip the box, and a bot's probe called a 0.75 m crate open,
    /// which is the rule [`Self::push_out_soldier`] now shares. (b)'s
    /// primitive branch reads a floor-like upward normal the same way.
    ///
    /// `dir_xz`'s `y` is ignored; the sweep is horizontal, like the push.
    ///
    /// The AI ([`crate::ai`]) uses it to pick a direction it can actually walk
    /// in, which the PS2 does NOT do — the originals steer around authored
    /// barriers plus a contact push (RE_LOG "Barriers are the PS2's
    /// Obstacles…", local avoidance) — so this is the port's own answer for
    /// the model geometry the barriers do not cover.
    ///
    /// Determinism: bodies in ascending `bodies` order with a strict `<` on
    /// the time of impact, so the FIRST body wins a tie, as everywhere else
    /// in this module; that covers the (b) blocks too, which all tie at `t = 0`.
    /// Inside the two per-triangle passes the same strict comparison runs in
    /// the BVH's TRAVERSAL ORDER, which is fixed for a fixed mesh (the tree is
    /// built once, in `trimesh`, and never rebuilt), so the first candidate
    /// wins a tie there. Nothing on either path is transcendental: the cast
    /// reaches `cast_shapes_support_map_support_map` (GJK's
    /// `directional_distance`, and EPA only when the shapes already overlap),
    /// reached for a mesh through a BVH descent whose node cost is an AABB ray
    /// test; (b)'s `query::contact` reaches, for the four shapes this world
    /// holds, `contact_support_map_support_map` (`Cylinder`, `Cuboid` and a
    /// mesh's `Triangle` — GJK plus EPA on overlap) and
    /// `contact_convex_polyhedron_ball` (`Sphere`, which projects the ball's
    /// centre on the capsule) — all add/mul/compare plus `sqrt`. The `sin` /
    /// `cos` in parry's `Cuboid` and `Segment` are in
    /// `ConvexPolyhedron::support_feature_toward`, which is the contact-
    /// MANIFOLD path and is on neither of these (PLAN's P9 audit found
    /// `query/` clean; re-checked against parry3d 0.22.1's sources, contact
    /// paths included, 2026-09-03). The triangle face normal adds nothing
    /// transcendental either — a cross product and a normalisation.
    ///
    /// A SOLDIER query: bodies outside [`Layers::SOLDIER`] are skipped (RE §4).
    pub fn wall_sweep(&self, cap: &Capsule, dir_xz: Vec3, dist: f32) -> Option<(f32, Vec3)> {
        self.wall_sweep_body(cap, dir_xz, dist)
            .map(|(t, n, _)| (t, n))
    }

    /// [`Self::wall_sweep`], and the body that answered: for a diagnostic
    /// that names what a bot is walking into (the playtest black box, card
    /// BLKB). The sim asks only for the time and the normal.
    pub fn wall_sweep_body(
        &self,
        cap: &Capsule,
        dir_xz: Vec3,
        dist: f32,
    ) -> Option<(f32, Vec3, &Body)> {
        self.wall_sweep_small(cap, dir_xz, dist, f32::INFINITY)
    }

    /// [`Self::wall_sweep_body`] against only the bodies whose bounds are at
    /// most `max_half` from their middle along X and along Z: what a bot
    /// that looks ahead for a SMALL body asks (`ai::avoid`), without paying
    /// for a cast against every floor and wall the look crosses.
    pub fn wall_sweep_small(
        &self,
        cap: &Capsule,
        dir_xz: Vec3,
        dist: f32,
        max_half: f32,
    ) -> Option<(f32, Vec3, &Body)> {
        if dist <= 0.0 {
            return None;
        }
        let flat = Vec3::new(dir_xz.x, 0.0, dir_xz.z);
        let len = flat.length();
        if len < 1e-6 {
            return None;
        }
        let dir = flat / len;
        let (shape, iso) = cap.to_parry();
        // The prefilter box is the union of the capsule's bounds at both ends
        // of the sweep (the motion is a pure translation, so that encloses it).
        let here = cap.aabb();
        let step = dir * dist;
        let swept = Aabb {
            min: here.min.min(here.min + step),
            max: here.max.max(here.max + step),
        };
        let vel = vector(dir);
        let still = vector(Vec3::ZERO);
        let opts = parry3d::query::ShapeCastOptions {
            max_time_of_impact: dist,
            target_distance: 0.0,
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: true,
        };
        // The pose at the END of the step, for the penetration test below.
        let mut end = *cap;
        end.base += step;
        let end_iso = end.to_parry().1;
        // (b)'s own broad phase: the capsule's bounds at the two ENDS of the
        // step, each grown by the contact tolerance. A body that lies between
        // the two poses without touching either is the CAST's business, and
        // the cast sees it correctly — the capsule is clear of it at the start.
        let grow = |a: Aabb| Aabb {
            min: a.min - Vec3::splat(SWEEP_CONTACT_DIST),
            max: a.max + Vec3::splat(SWEEP_CONTACT_DIST),
        };
        let start_box = grow(here);
        let end_box = grow(end.aabb());
        let mut best: Option<(f32, Vec3)> = None;
        // Whose answer `best` is (it changes only where `best` does).
        let mut who: Option<&Body> = None;
        self.visit_xz(
            swept.min.x,
            swept.max.x,
            swept.min.z,
            swept.max.z,
            HullFilter::All,
            |body| {
                if !body.layers.contains(Layers::SOLDIER) {
                    return;
                }
                if !body.aabb.overlaps(&swept) {
                    return;
                }
                let size = body.aabb.max - body.aabb.min;
                if 0.5 * size.x.max(size.z) > max_half {
                    return;
                }
                let hit = body.shape.with_parry(&mut |s| {
                    parry3d::query::cast_shapes(&iso, &vel, &shape, &body.iso, &still, s, opts)
                        .ok()
                        .flatten()
                });
                let usable = hit.filter(|h| h.time_of_impact >= SWEEP_CONTACT_DIST);
                let Some(hit) = usable else {
                    // (b): the cast has nothing usable for this body — the capsule
                    // is touching it, inside it, or nowhere near it. Depth decides.
                    if best.is_some_and(|(t, _)| t <= 0.0) {
                        return; // strict `<`: the first body wins a tie at t = 0
                    }
                    if !body.aabb.overlaps(&start_box) && !body.aabb.overlaps(&end_box) {
                        return;
                    }
                    let n = penetration_block(
                        body, &shape, &iso, start_box, &end_iso, end_box, swept, dir, dist,
                    );
                    let Some(n) = n else { return };
                    best = Some((0.0, n));
                    who = Some(body);
                    return;
                };
                if best.is_some_and(|(t, _)| hit.time_of_impact >= t) {
                    return; // strict `<`: the first body wins a tie
                }
                // `normal1` is the outward normal ON THE CAPSULE, in the capsule's
                // own frame — and `Capsule::to_parry` places it by a pure
                // TRANSLATION, so that frame is the world's. It points at the
                // body, so the way out is its negation (`push_out`'s `normal2`,
                // without needing the body transform).
                let n = Vec3::new(-hit.normal1.x, -hit.normal1.y, -hit.normal1.z);
                if n.y.abs() < WALKABLE_MIN_NY {
                    // A WALL: the whole-body answer, exactly as before.
                    let Some(n) = xz_unit(n) else { return };
                    best = Some((hit.time_of_impact, n));
                    who = Some(body);
                    return;
                }
                // A FLOOR. For a primitive that is the whole answer (convex: one
                // entry contact). An UPWARD one is a LEDGE (card JWAL, the rule of
                // `push_out_soldier`): the swept capsule is the stepped one, so the
                // primitive reaches above the step, where the soldier cannot stand.
                // It blocks like a wall. A downward one (an overhang) stays skipped.
                // For a MESH it only means the nearest triangle of
                // this body is walkable — a wall of the same mesh may stand behind
                // it — so re-ask the mesh per triangle. This is the only path that
                // costs more than it did, and it runs only on a floor hit.
                let Shape::Mesh(mesh) = &body.shape else {
                    if n.y > 0.0 {
                        if let Some(n) = xz_unit(n) {
                            best = Some((hit.time_of_impact, n));
                            who = Some(body);
                        }
                    }
                    return;
                };
                // The swept AABB in the mesh's own space (its BVH's space).
                let local_swept = local_query_aabb(swept, body);
                let mut tri_best: Option<(f32, Vec3)> = None;
                for i in mesh.bvh().intersect_aabb(&local_swept) {
                    let tri = mesh.triangle(i);
                    // The FACE normal decides floor-vs-wall, in WORLD space; the
                    // instance is rigid, so rotating the local normal is enough.
                    // A degenerate triangle has none and is not a wall.
                    let Some(face) = tri.normal() else { continue };
                    let face_y = (body.iso.rotation * face.into_inner()).y;
                    if face_y.abs() >= WALKABLE_MIN_NY {
                        continue; // a floor TRIANGLE is not a wall
                    }
                    // The TRIANGLE is shape 1 and the capsule shape 2, then the
                    // hit is swapped back — the argument order parry's own
                    // composite cast uses internally (it inverts the pose and
                    // negates the velocity before visiting the parts). Measured:
                    // with the capsule first instead, the same wall as ONE mesh
                    // and as TWO bodies answers 0.8000001 and 0.7998484 — the
                    // same arithmetic in a different frame, 1.5e-4 apart. This
                    // order makes the per-triangle answer BIT-IDENTICAL to the
                    // whole-body one (the test below asserts it), so how the bake
                    // grouped triangles into bodies cannot change the answer.
                    let th = parry3d::query::cast_shapes(
                        &body.iso, &still, &tri, &iso, &vel, &shape, opts,
                    )
                    .ok()
                    .flatten()
                    .map(|h| h.swapped());
                    // A hit inside the contact distance is the cast talking about
                    // a triangle the capsule already touches, which it cannot
                    // judge: (b) owns those, as for a whole body.
                    let Some(th) = th.filter(|h| h.time_of_impact >= SWEEP_CONTACT_DIST) else {
                        continue;
                    };
                    if tri_best.is_some_and(|(t, _)| th.time_of_impact >= t) {
                        continue; // strict `<`: the first candidate wins a tie
                    }
                    // The push direction is the CONTACT normal, as for a body.
                    let tn = Vec3::new(-th.normal1.x, -th.normal1.y, -th.normal1.z);
                    let Some(tn) = xz_unit(tn) else { continue };
                    tri_best = Some((th.time_of_impact, tn));
                }
                // The body's result competes with the others exactly as before.
                let Some((t, tn)) = tri_best else { return };
                if best.is_some_and(|(bt, _)| t >= bt) {
                    return;
                }
                best = Some((t, tn));
                who = Some(body);
            },
        );
        best.zip(who).map(|((t, n), b)| (t, n, b))
    }
}

/// `n` flattened to XZ and normalised, or `None` when it has no horizontal
/// part — the sweep and the push are horizontal.
fn xz_unit(n: Vec3) -> Option<Vec3> {
    let f = Vec3::new(n.x, 0.0, n.z);
    let l = f.length();
    (l >= 1e-6).then(|| f / l)
}

/// Does a step of `dist` along `dir` DEEPEN this contact pair's penetration?
/// `(penetration depth at the end of the step, the way out)` when it does.
/// The two arguments are the contacts between the same capsule and the same
/// convex piece — a primitive body, or one triangle of a mesh — at the START
/// and END poses of the step; `None` for either means "not touching there".
///
/// PROVISIONAL (ours), Phase A.4. Two regimes, split at
/// [`SWEEP_CONTACT_DIST`]:
///
/// - ALREADY PENETRATING at the start (`pen0 > SWEEP_CONTACT_DIST`): the depth
///   at the end is measured ALONG THE START CONTACT'S OWN NORMAL,
///   `pen0 − dist·(dir·n0)`, not by asking parry for a second contact. Both
///   other readings are measured to be wrong here: parry's cast reports
///   `t = 0` in every direction from inside (see [`CollisionWorld::wall_sweep`]),
///   and the end-pose contact answers about a DIFFERENT escape direction —
///   for a deep box the minimum translation flips to vertical, and for a
///   zero-thickness mesh wall a step clean through it reports no contact at
///   all, so a pure depth comparison would call both "not deepening". Along
///   `n0` the comparison is exact and needs no new constant: at 1 m (the AI's
///   `PROBE_DIST`) the 1 mm tolerance admits every direction within 0.06 deg
///   of tangential, which is what "may walk ALONG a wall it is inside" means.
/// - NOT penetrating at the start (touching, or clear): the Phase A.1 point
///   test — the end pose must not be inside the body — with the same 1 mm
///   tolerance. This is the case the whole steering layer was measured
///   against, and the case a cast from a touching pose cannot judge (it
///   reports a float-zero hit in every non-separating direction, including a
///   purely tangential one; see [`SWEEP_CONTACT_DIST`]).
///
/// The normal returned is the one the comparison was made with — the START
/// contact's in the first regime, the END contact's in the second — and it
/// points out of the body, at the mover (`push_out`'s `normal2`).
fn deepened_by_step(
    start: Option<parry3d::query::Contact>,
    end: Option<parry3d::query::Contact>,
    dir: Vec3,
    dist: f32,
) -> Option<(f32, Vec3)> {
    let depth = |c: Option<parry3d::query::Contact>| c.map_or(0.0, |c| (-c.dist).max(0.0));
    let pen0 = depth(start);
    let out = |c: parry3d::query::Contact| Vec3::new(c.normal2.x, c.normal2.y, c.normal2.z);
    let (pen1, n) = if pen0 > SWEEP_CONTACT_DIST {
        let n = out(start?);
        (pen0 - dist * dir.dot(n), n)
    } else {
        let c = end?;
        ((-c.dist).max(0.0), out(c))
    };
    (pen1 > pen0 + SWEEP_CONTACT_DIST).then_some((pen1, n))
}

/// Test (b) of [`CollisionWorld::wall_sweep`] for one body: the XZ push normal
/// when the step deepens a penetration of a WALL of it, else `None`. The
/// answer is always at `t = 0` — the capsule is touching or inside the body
/// NOW.
///
/// - Primitives (`Sphere` / `Cylinder` / `Box`) are convex, so one
///   [`parry3d::query::contact`] per pose is the whole answer (parry's own
///   `contact` is correct for a convex shape — module header), and the CONTACT
///   normal decides wall-vs-floor, as it does in [`Self::push_out`].
/// - A MESH is asked PER TRIANGLE, never deepest-of-mesh: the deepest contact
///   can be a floor triangle while a wall triangle is the one deepening (the
///   test `a_capsule_touching_a_wall_over_a_ramp_of_the_same_mesh_is_blocked`
///   is exactly that). Candidates are the mesh's BVH over the swept bounds in
///   body space; a triangle is a WALL by its own FACE normal rotated to world,
///   as in the A.3 cast pass; the contact per triangle is `query::contact` on
///   a `Triangle`, which is convex — never on the composite mesh (module
///   header). Among the triangles that block, the DEEPEST end-of-step
///   penetration wins, with a strict `>` in BVH traversal order.
///
/// Each pose's contact is skipped when the body's (or triangle's) own bounds
/// do not reach that pose's capsule box: a miss there is `None`, which is what
/// the contact would have returned anyway.
#[allow(clippy::too_many_arguments)]
fn penetration_block(
    body: &Body,
    shape: &parry3d::shape::Capsule,
    start_iso: &Isometry,
    start_box: Aabb,
    end_iso: &Isometry,
    end_box: Aabb,
    swept: Aabb,
    dir: Vec3,
    dist: f32,
) -> Option<Vec3> {
    match &body.shape {
        Shape::Mesh(mesh) => {
            use parry3d::bounding_volume::BoundingVolume;
            let local_start = local_query_aabb(start_box, body);
            let local_end = local_query_aabb(end_box, body);
            let mut best: Option<(f32, Vec3)> = None;
            for i in mesh.bvh().intersect_aabb(&local_query_aabb(swept, body)) {
                let tri = mesh.triangle(i);
                let Some(face) = tri.normal() else { continue };
                if (body.iso.rotation * face.into_inner()).y.abs() >= WALKABLE_MIN_NY {
                    continue; // a floor TRIANGLE is not a wall
                }
                let box_of = tri.local_aabb();
                let at = |iso: &Isometry| {
                    parry3d::query::contact(iso, shape, &body.iso, &tri, 0.0)
                        .ok()
                        .flatten()
                };
                let s = box_of
                    .intersects(&local_start)
                    .then(|| at(start_iso))
                    .flatten();
                let e = box_of.intersects(&local_end).then(|| at(end_iso)).flatten();
                let Some((pen1, n)) = deepened_by_step(s, e, dir, dist) else {
                    continue;
                };
                let Some(n) = xz_unit(n) else { continue };
                if best.is_some_and(|(p, _)| pen1 <= p) {
                    continue; // strict `>`: the first candidate wins a tie
                }
                best = Some((pen1, n));
            }
            best.map(|(_, n)| n)
        }
        _ => {
            let at = |iso: &Isometry| {
                body.shape.with_parry(&mut |s| {
                    parry3d::query::contact(iso, shape, &body.iso, s, 0.0)
                        .ok()
                        .flatten()
                })
            };
            let s = body
                .aabb
                .overlaps(&start_box)
                .then(|| at(start_iso))
                .flatten();
            let e = body.aabb.overlaps(&end_box).then(|| at(end_iso)).flatten();
            let (_, n) = deepened_by_step(s, e, dir, dist)?;
            if n.y <= -WALKABLE_MIN_NY {
                // An overhang. `push_out` now pushes its horizontal part (card
                // WTUN), but a probe that blocks a unit under every low deck
                // it grazes would stall the AI; the push alone answers it.
                return None;
            }
            // A floor-like UPWARD normal is a LEDGE above the step (card JWAL,
            // as in `push_out_soldier` and the cast above): a wall.
            xz_unit(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::tests::{
        floor_quad, mesh_body, mesh_body_at, ramp, world_from_triangles,
    };
    use glam::{Mat4, Quat};

    /// Contact collection appends in body order, with the exact per-body
    /// answers preserved across the broadphase and both mover entry points.
    #[test]
    fn contact_collection_preserves_per_body_bits_and_existing_output() {
        let pose =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.37), Vec3::new(4.0, 0.0, -3.0));
        let mut tris = wall_quad(0.0);
        tris.extend(wall_quad(-6.0));
        let centre = pose.transform_point3(Vec3::new(0.2, 0.0, 0.0));
        let bodies = vec![
            Body::new(
                Mat4::from_translation(centre + Vec3::new(0.3, 0.0, 0.0)),
                Shape::Sphere { radius: 0.5 },
            ),
            mesh_body_at(&tris, pose),
            Body::new(
                Mat4::from_translation(centre + Vec3::new(0.0, 0.0, 0.3)),
                Shape::Box {
                    half: Vec3::splat(0.4),
                },
            ),
        ];
        let separate: Vec<_> = bodies
            .iter()
            .map(|body| {
                CollisionWorld::new(
                    vec![Body::new(body.to_world, body.shape.clone())],
                    Vec::new(),
                )
            })
            .collect();
        let world = CollisionWorld::new(bodies, Vec::new());
        let bits = |hits: Vec<(f32, Vec3)>| -> Vec<[u32; 4]> {
            hits.into_iter()
                .map(|(d, n)| [d.to_bits(), n.x.to_bits(), n.y.to_bits(), n.z.to_bits()])
                .collect()
        };
        for shape in [
            Shape::Sphere { radius: 0.6 },
            Shape::Box {
                half: Vec3::splat(0.6),
            },
            Shape::Cylinder {
                radius: 0.6,
                half_height: 0.8,
            },
        ] {
            let query = |world: &CollisionWorld, out: &mut Vec<(f32, Vec3)>| {
                if let Shape::Sphere { radius } = shape {
                    world.sphere_contacts(centre, radius, Layers::SOLDIER, out);
                } else {
                    world.shape_contacts(
                        &shape,
                        Mat4::from_translation(centre),
                        Layers::SOLDIER,
                        out,
                    );
                }
            };
            let prefix = (17.0, Vec3::new(-0.0, 2.0, -3.0));
            let mut expected = vec![prefix];
            for body in &separate {
                let before = expected.len();
                query(body, &mut expected);
                assert!(
                    expected.len() > before,
                    "fixture body must contribute a contact"
                );
            }
            let mut actual = vec![prefix];
            query(&world, &mut actual);
            assert_eq!(bits(actual), bits(expected));
        }
    }

    /// Card OBST2: [`CollisionWorld::wall_sweep_small`] asks only the bodies
    /// no wider than its limit — a wall across the way is passed over for
    /// the crate behind it, which the whole sweep never reaches — and with
    /// the limit open it is the whole sweep.
    #[test]
    fn the_small_sweep_asks_only_small_bodies() {
        let wall = Body::new(
            Mat4::from_translation(Vec3::new(0.0, 1.0, -2.0)),
            Shape::Box {
                half: Vec3::new(5.0, 1.0, 0.25),
            },
        );
        let small = Body::new(
            Mat4::from_translation(Vec3::new(0.0, 1.0, -4.0)),
            Shape::Box {
                half: Vec3::new(0.6, 1.0, 0.6),
            },
        );
        let world = CollisionWorld::new(vec![wall, small], Vec::new());
        let cap = Capsule {
            base: Vec3::ZERO,
            height: 1.9,
            radius: 0.4,
        }
        .stepped();
        let (t, n, body) = world
            .wall_sweep_body(&cap, Vec3::NEG_Z, 5.0)
            .expect("the wall");
        assert!((t - 1.35).abs() < 1e-2 && n.z > 0.99, "the wall: {t} {n}");
        assert!(body.aabb.max.x > 4.0);
        let (t, n, body) = world
            .wall_sweep_small(&cap, Vec3::NEG_Z, 5.0, 1.5)
            .expect("the crate");
        assert!((t - 3.0).abs() < 1e-2 && n.z > 0.99, "the crate: {t} {n}");
        assert!(body.aabb.max.x < 1.0);
        // Half its width is the measure, along X and along Z.
        assert!(world
            .wall_sweep_small(&cap, Vec3::NEG_Z, 5.0, 0.59)
            .is_none());
        assert!(world
            .wall_sweep_small(&cap, Vec3::NEG_Z, 5.0, 0.61)
            .is_some());
        let whole = world.wall_sweep_small(&cap, Vec3::NEG_Z, 5.0, f32::INFINITY);
        assert!(whole.is_some_and(|(t, _, _)| (t - 1.35).abs() < 1e-2));
    }

    /// Card STFAL: the hull probe's pass sees the vehicles' hulls and
    /// nothing of the map's own — a wall of the map pushes the full pass and
    /// not the hull-only one, and a parked hull's low part pushes both.
    #[test]
    fn the_hull_only_pass_sees_hulls_and_not_the_map() {
        let wall = Body::new(
            Mat4::from_translation(Vec3::new(2.5, 0.0, 0.0)),
            Shape::Box {
                half: Vec3::new(0.5, 5.0, 5.0),
            },
        );
        let mut world = CollisionWorld::new(vec![wall], Vec::new());
        let cap = Capsule {
            base: Vec3::new(2.3, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        assert!(world.push_out_soldier(&cap).x < -0.5, "the map's wall");
        assert_eq!(world.push_out_soldier_hulls(&cap), Vec3::ZERO);
        // A low box on a hull at -3 m: its top (0.9 m) is over the step and
        // under the lifted capsule's round bottom — an AT-ST foot.
        let owner = crate::vehicle::VehicleHandle { index: 0, gen: 0 };
        let part = crate::collision::HullPart::new(
            Mat4::IDENTITY,
            Shape::Box {
                half: Vec3::new(0.5, 0.45, 2.0),
            },
            Layers::ALL,
        );
        world.set_hull(
            owner,
            &[part],
            Mat4::from_translation(Vec3::new(-3.0, 0.45, 0.0)),
        );
        let at_foot = Capsule {
            base: Vec3::new(-2.3, 0.0, 0.0),
            ..cap
        };
        let push = world.push_out_soldier_hulls(&at_foot);
        assert!(push.x > 0.1, "the hull's low box pushes: {push}");
        assert_eq!(
            push,
            world.push_out_soldier(&at_foot),
            "as the full pass does"
        );
    }

    #[test]
    fn push_out_is_horizontal_and_order_independent_for_one_wall() {
        // Slab spanning x in (2, 3).
        let wall = Body::new(
            Mat4::from_translation(Vec3::new(2.5, 0.0, 0.0)),
            Shape::Box {
                half: Vec3::new(0.5, 5.0, 5.0),
            },
        );
        let world = CollisionWorld::new(vec![wall], Vec::new());
        let cap = Capsule {
            base: Vec3::new(2.3, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        let push = world.push_out(&cap);
        assert_eq!(push.y, 0.0, "the push is horizontal: {push}");
        assert!((push.x - -(0.3 + cap.radius)).abs() < 1e-3, "{push}");
        assert!(push.z.abs() < 1e-3, "{push}");
        // A floor is the ground query's job, not the wall pass's.
        let floors = world_from_triangles(floor_quad(0.0, 20.0));
        assert_eq!(floors.push_out(&cap), Vec3::ZERO);
    }

    /// THE POINT OF THE SWEEP: a THIN wall 0.4 m ahead of a 1.0 m step.
    ///
    /// The Phase A.1 probe translated the capsule the whole 1.0 m and asked
    /// for a contact THERE, so a wall thinner than the gap it jumped — this
    /// one is 0.1 m thick and the capsule clears it by 0.2 m — was never
    /// touched, and the unit ran at it at full speed for ever while the wall
    /// pass pushed it back (PLAN "PHASE A.2 DESIGN"). The sweep sees it.
    #[test]
    fn a_thin_wall_inside_the_step_is_swept_and_the_point_probe_missed_it() {
        // Wall: a 0.1 m thick slab centred at x = 1.8 (x in [1.75, 1.85]).
        // Capsule r 0.4 at x = 1.0, so its front face is at x = 1.4 and its
        // front face after a 1.0 m step is at x = 2.4 — clear past the wall,
        // which is why the point probe found nothing.
        let world = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(1.8, 0.0, 0.0)),
                Shape::Box {
                    half: Vec3::new(0.05, 5.0, 5.0),
                },
            )],
            Vec::new(),
        );
        let cap = Capsule {
            base: Vec3::new(1.0, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        // The A.1 probe's own test: a contact at the translated pose. The
        // capsule at x = 2.0 spans x in [1.6, 2.4] — which DOES overlap this
        // slab, so make the miss explicit by translating past it instead.
        let past = Capsule {
            base: Vec3::new(3.0, 0.0, 0.0),
            ..cap
        };
        let (shape, iso) = past.to_parry();
        let point_probe = world.bodies()[0].shape.with_parry(&mut |sh| {
            parry3d::query::contact(&iso, &shape, &world.bodies()[0].iso, sh, 0.0)
                .ok()
                .flatten()
                .is_some_and(|c| c.dist < 0.0)
        });
        assert!(
            !point_probe,
            "a point probe 2 m on has walked THROUGH the wall"
        );

        let (t, n) = world
            .wall_sweep(&cap, Vec3::X, 1.0)
            .expect("the sweep sees the thin wall");
        assert!(
            (t - 0.35).abs() < 1e-3,
            "hit 0.35 m along (1.75 - 1.4): {t}"
        );
        // The normal points BACK AT THE MOVER.
        assert!((n - Vec3::NEG_X).length() < 1e-3, "points back: {n}");
        assert_eq!(n.y, 0.0, "the normal is XZ");
        assert!((n.length() - 1.0).abs() < 1e-5, "unit length: {n}");
        assert!(n.dot(Vec3::X) < 0.0, "opposes the direction of travel");

        // Clear, and nothing moved: a shorter step stops short of the wall,
        // and walking away is clear at any distance.
        assert_eq!(world.wall_sweep(&cap, Vec3::X, 0.3), None, "stops short");
        assert_eq!(world.wall_sweep(&cap, Vec3::NEG_X, 5.0), None);
        assert_eq!(cap.base, Vec3::new(1.0, 0.0, 0.0));
        // `dir_xz.y` is ignored: the sweep is horizontal.
        assert!(world
            .wall_sweep(&cap, Vec3::new(1.0, 9.0, 0.0), 1.0)
            .is_some());
        // Zero/degenerate inputs are "clear", never a panic.
        assert_eq!(world.wall_sweep(&cap, Vec3::X, 0.0), None);
        assert_eq!(world.wall_sweep(&cap, Vec3::Y, 1.0), None);
    }

    /// A WALKABLE floor is never a wall — the same `WALKABLE_MIN_NY` filter
    /// the push applies — so a unit may sweep along the ground it stands on.
    #[test]
    fn the_sweep_skips_a_walkable_floor() {
        let cap = Capsule {
            base: Vec3::new(1.0, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        let floors = world_from_triangles(floor_quad(0.0, 20.0));
        assert_eq!(floors.wall_sweep(&cap, Vec3::X, 1.0), None);
        assert_eq!(floors.push_out(&cap), Vec3::ZERO);
        // A 30 deg ramp is walkable too (|n.y| = cos 30 = 0.87): a unit
        // standing on it may sweep UP it without reading it as a wall.
        let ramps = world_from_triangles(ramp(30.0, 0.0));
        let on_ramp = Capsule {
            base: Vec3::new(1.0, 30f32.to_radians().tan(), 0.0),
            ..cap
        };
        assert_eq!(ramps.wall_sweep(&on_ramp, Vec3::X, 1.0), None);
        // A vertical face is not: the same mesh path, opposite verdict.
        let wall = world_from_triangles(vec![
            [
                Vec3::new(1.6, -5.0, -5.0),
                Vec3::new(1.6, -5.0, 5.0),
                Vec3::new(1.6, 5.0, -5.0),
            ],
            [
                Vec3::new(1.6, 5.0, -5.0),
                Vec3::new(1.6, -5.0, 5.0),
                Vec3::new(1.6, 5.0, 5.0),
            ],
        ]);
        let (t, n) = wall.wall_sweep(&cap, Vec3::X, 1.0).expect("a wall");
        assert!((t - 0.2).abs() < 1e-3, "1.6 - 1.4 = 0.2: {t}");
        assert!(n.x < -0.99, "points back at the mover: {n}");
    }

    /// A capsule the wall pass left EXACTLY TOUCHING a wall may still walk
    /// ALONG it and AWAY from it — and only walking INTO it is blocked.
    ///
    /// This is the regression that cost three maps when the sweep first
    /// landed. A shape cast from a touching pose returns a hit at (float) zero
    /// distance for every direction that does not strictly separate — 6.2e-5 m
    /// along +Z for the pose below — so the steering's slide ALONG the wall
    /// came back blocked, its whole fan came back blocked, and the unit backed
    /// off. `SWEEP_CONTACT_DIST` hands that case to the point test instead.
    #[test]
    fn a_capsule_already_touching_a_wall_may_still_walk_along_it() {
        // A DEEP block whose near face is at x = 1.5, so a step into it is
        // unambiguously into that face (a 1 m step through a 1 m slab comes
        // out the far side, and the point test rightly says so).
        let world = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(3.5, 0.0, 0.0)),
                Shape::Box {
                    half: Vec3::new(2.0, 5.0, 5.0),
                },
            )],
            Vec::new(),
        );
        // Wall face at x = 1.5; the capsule's front face is exactly on it.
        let touching = Capsule {
            base: Vec3::new(1.1, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        for d in [Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            assert_eq!(world.wall_sweep(&touching, d, 1.0), None, "dir {d}");
        }
        // Walking INTO it is still blocked, at contact distance, with the
        // wall's own normal — this is what the point test is there to keep.
        let (t, n) = world
            .wall_sweep(&touching, Vec3::X, 1.0)
            .expect("into the wall");
        assert!(t < SWEEP_CONTACT_DIST, "a contact, not a wall ahead: {t}");
        assert!(n.x < -0.99, "the wall's normal: {n}");
        // Overlapping it — what a push-out rounding error leaves — answers the
        // same way, so a unit inside geometry can always walk back out.
        let inside = Capsule {
            base: Vec3::new(1.3, 0.0, 0.0),
            ..touching
        };
        assert_eq!(world.wall_sweep(&inside, Vec3::NEG_X, 1.0), None, "out");
        assert!(
            world.wall_sweep(&inside, Vec3::X, 1.0).is_some(),
            "deeper in"
        );
        // A wall the sweep is CLOSING on from a clear pose is still reported:
        // this only silences contacts, it does not blind the probe.
        let clear = Capsule {
            base: Vec3::new(0.5, 0.0, 0.0),
            ..touching
        };
        let (t, n) = world.wall_sweep(&clear, Vec3::X, 1.0).expect("ahead");
        assert!((t - 0.6).abs() < 1e-3, "1.5 - 0.9: {t}");
        assert!(n.x < -0.99, "{n}");
        // ... and a wall it is moving ALONG is not, at any distance.
        assert_eq!(world.wall_sweep(&clear, Vec3::Z, 5.0), None);
    }

    /// Two walls in the sweep: the NEAREST decides, and an exact tie goes to
    /// the FIRST body — `bodies` order, strict `<` on the time of impact, the
    /// rule every other query in this module follows.
    #[test]
    fn the_sweep_takes_the_nearest_wall_and_the_first_body_on_a_tie() {
        let cap = Capsule {
            base: Vec3::ZERO,
            height: 1.9,
            radius: 0.4,
        };
        // Far wall listed FIRST, near wall second: the near one still wins.
        let world = CollisionWorld::new(
            vec![
                Body::new(
                    Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)),
                    Shape::Box {
                        half: Vec3::new(0.1, 5.0, 5.0),
                    },
                ),
                Body::new(
                    Mat4::from_translation(Vec3::new(1.6, 0.0, 0.0)),
                    Shape::Box {
                        half: Vec3::new(0.1, 5.0, 5.0),
                    },
                ),
            ],
            Vec::new(),
        );
        let (t, _) = world.wall_sweep(&cap, Vec3::X, 5.0).expect("blocked");
        assert!((t - 1.1).abs() < 1e-3, "the nearer wall (1.5 - 0.4): {t}");

        // An exact tie: identical slabs give identical numeric answers, so
        // also observe the identity of the FIRST body that answered.
        let slab = |m: Mat4| {
            Body::new(
                m,
                Shape::Box {
                    half: Vec3::new(0.1, 5.0, 0.1),
                },
            )
        };
        let at = Mat4::from_translation(Vec3::new(1.6, 0.0, 0.0));
        let tied = CollisionWorld::new(vec![slab(at), slab(at)], Vec::new());
        let (t0, n0) = tied.wall_sweep(&cap, Vec3::X, 5.0).expect("blocked");
        let (_, _, body) = tied.wall_sweep_body(&cap, Vec3::X, 5.0).expect("blocked");
        assert!(std::ptr::eq(body, &tied.bodies()[0]));
        let one = CollisionWorld::new(vec![slab(at)], Vec::new());
        assert_eq!(Some((t0, n0)), one.wall_sweep(&cap, Vec3::X, 5.0));
    }

    /// A 45 deg ramp quad on the plane `y = x − x0`, over `x in [lo, hi]` and
    /// `z in [−5, 5]`. Its face normal is `(∓0.707, ±0.707, 0)`: `|n.y|` is
    /// well above [`WALKABLE_MIN_NY`], so every triangle of it is a FLOOR.
    fn ramp_quad(x0: f32, lo: f32, hi: f32) -> Vec<[Vec3; 3]> {
        let p = |x: f32, z: f32| Vec3::new(x, x - x0, z);
        vec![
            [p(lo, -5.0), p(hi, -5.0), p(hi, 5.0)],
            [p(lo, -5.0), p(hi, 5.0), p(lo, 5.0)],
        ]
    }

    /// A vertical wall quad in the plane `x = at`, `y in [−1, 3]`,
    /// `z in [−5, 5]`. Face normal `(±1, 0, 0)`: a WALL.
    fn wall_quad(at: f32) -> Vec<[Vec3; 3]> {
        let q = |y: f32, z: f32| Vec3::new(at, y, z);
        vec![
            [q(-1.0, -5.0), q(-1.0, 5.0), q(3.0, -5.0)],
            [q(3.0, -5.0), q(-1.0, 5.0), q(3.0, 5.0)],
        ]
    }

    /// Phase A.3: a wall standing behind a ramp OF THE SAME MESH is seen.
    ///
    /// One body, two surfaces: a 45 deg ramp rising in front of the capsule
    /// (hit first, at 0.33 m, with a FLOOR normal) and a vertical wall 0.8 m
    /// ahead of the capsule's front face. Before A.3 the whole-body cast's
    /// floor hit skipped the whole BODY and the sweep answered `None` while
    /// the wall pass pushed the unit back every tick — the BF1 NAB2 stall
    /// (PLAN Log, "Phase A.2 landed"). The per-triangle pass finds the wall.
    #[test]
    fn the_sweep_finds_a_wall_behind_a_ramp_of_the_same_mesh() {
        let cap = Capsule {
            base: Vec3::ZERO,
            height: 1.9,
            radius: 0.4,
        };
        // Ramp `y = x − 0.5` over x in [0.5, 1.2]; wall at x = 1.2, which is
        // 0.8 m from the capsule's front face at x = 0.4.
        let ramp = ramp_quad(0.5, 0.5, 1.2);
        let wall = wall_quad(1.2);

        // The ramp ALONE is still no wall: the floor filter is intact.
        let floors = world_from_triangles(ramp.clone());
        assert_eq!(floors.wall_sweep(&cap, Vec3::X, 1.0), None, "a ramp");

        let both: Vec<[Vec3; 3]> = ramp.iter().chain(wall.iter()).copied().collect();
        let one_body = world_from_triangles(both);
        let (t, n) = one_body
            .wall_sweep(&cap, Vec3::X, 1.0)
            .expect("the wall behind the ramp");
        assert!((t - 0.8).abs() < 1e-3, "1.2 − 0.4 = 0.8: {t}");
        assert!(n.x < -0.99, "points back at the mover: {n}");
        assert_eq!(n.y, 0.0, "the normal is XZ");

        // The same geometry as TWO bodies — the ramp body contributes nothing
        // and the wall body answers through the ordinary whole-body path — is
        // the same answer, so the per-triangle path agrees with the per-body
        // one.
        let two = CollisionWorld::new(vec![mesh_body(&ramp), mesh_body(&wall)], Vec::new());
        assert_eq!(
            two.wall_sweep(&cap, Vec3::X, 1.0),
            Some((t, n)),
            "one body vs two"
        );
    }

    /// Phase A.3, the TOUCHING branch: the capsule is exactly on the wall and
    /// penetrating the ramp of the same mesh, and the ramp is the DEEPER
    /// end-pose contact — so the body branch read a floor normal and skipped
    /// the body. The per-triangle pass asks the same point-test question of
    /// each non-floor triangle and the wall answers.
    #[test]
    fn a_capsule_touching_a_wall_over_a_ramp_of_the_same_mesh_is_blocked() {
        let cap = Capsule {
            base: Vec3::new(0.8, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        // The wall is exactly on the capsule's front face (0.8 + 0.4).
        // The ramp is placed so the capsule's bottom cap already penetrates it
        // by 0.15 m, which grows to 0.36 m over the 0.3 m step while the
        // wall's penetration only reaches 0.30 m: the ramp is the deeper
        // end-pose contact, which is what the body branch reads.
        let ramp = ramp_quad(0.4 + 0.25 * std::f32::consts::SQRT_2, -2.0, 1.2);
        let wall = wall_quad(1.2);
        let both: Vec<[Vec3; 3]> = ramp.iter().chain(wall.iter()).copied().collect();
        let world = world_from_triangles(both);

        // Pin the construction: the DEEPEST end-pose contact really is a floor.
        let mut end = cap;
        end.base += Vec3::X * 0.3;
        let (end_shape, end_iso) = end.to_parry();
        let body = &world.bodies()[0];
        let c = body
            .shape
            .with_parry(&mut |s| {
                parry3d::query::contact(&end_iso, &end_shape, &body.iso, s, 0.0)
                    .ok()
                    .flatten()
            })
            .expect("an end-pose contact");
        assert!(
            c.dist < 0.0,
            "penetrating at the end of the step: {}",
            c.dist
        );
        assert!(
            c.normal2.y.abs() >= WALKABLE_MIN_NY,
            "the deepest end-pose contact is the RAMP: {:?}",
            c.normal2
        );

        let (t, n) = world
            .wall_sweep(&cap, Vec3::X, 0.3)
            .expect("blocked by the wall it is touching");
        assert!(t < SWEEP_CONTACT_DIST, "a contact, not a wall ahead: {t}");
        assert!(n.x < -0.99, "the wall's normal: {n}");
    }

    /// The `coll` MESH twin of the fixture below: a wall QUAD at x = 1.5
    /// spanning y 0..6 and z −5..5, as two triangles. A `coll` wall is a
    /// zero-thickness SHEET, which is why a step of `ai::PROBE_DIST` from
    /// inside it comes out the far side with no contact at all.
    fn wall_sheet() -> Vec<[Vec3; 3]> {
        let p = |y: f32, z: f32| Vec3::new(1.5, y, z);
        vec![
            [p(0.0, -5.0), p(0.0, 5.0), p(6.0, 5.0)],
            [p(0.0, -5.0), p(6.0, 5.0), p(6.0, -5.0)],
        ]
    }

    /// Phase A.4: a body the capsule is ALREADY INSIDE still blocks the way
    /// FURTHER IN — the cast cannot say which way, and the end-pose point test
    /// answers about the wrong escape direction.
    ///
    /// The fixture is BF2 MYG1's stall, reduced: a 118 deg rotated box, the
    /// probe capsule 0.1 … 1.0 m inside one side face with its feet at y 2.9.
    /// The capsule's feet are 1.1 m below the box's TOP, so at the END of a
    /// 1 m step INTO the face the cheapest way out of the box is UPWARD: the
    /// deepest end-pose contact's normal is (0, 1, 0), a FLOOR, and the A.2
    /// rule skipped the whole body — `wall_sweep` answered `None` in every
    /// direction while `push_out` reported the penetration exactly. Unit 6 sat
    /// there for 53,400 ticks at `steer fwd` with `push 0.300`.
    ///
    /// The raw-parry assertions come FIRST and are the DETECTOR for a future
    /// parry upgrade, as in `mesh_contact_is_not_mirrored`.
    #[test]
    fn a_penetrated_rotated_box_still_blocks_the_way_in() {
        let to_world = Mat4::from_rotation_y(118f32.to_radians());
        let nx = to_world.transform_vector3(Vec3::X).normalize();
        let nz = to_world.transform_vector3(Vec3::Z).normalize();
        let world = CollisionWorld::new(
            vec![Body::new(
                to_world,
                Shape::Box {
                    half: Vec3::new(4.0, 4.0, 9.8),
                },
            )],
            Vec::new(),
        );
        // The capsule `depth` metres inside the +X face, feet at y 2.9.
        let at = |depth: f32| Capsule {
            base: nx * (4.0 + 0.5 - depth) + Vec3::Y * 2.9,
            height: 1.9,
            radius: 0.5,
        };
        // The AI's own step (`ai::PROBE_DIST`), which is what makes the
        // end-pose escape vertical: 1.0 m in on top of 0.1 m already inside is
        // more than the 1.1 m to the box's top face.
        const STEP: f32 = 1.0;

        // RAW parry, at depth 0.3, with the arguments `wall_sweep` passes: a
        // cast from a PENETRATING pose reports t = 0 in EVERY direction — out
        // of the box included — always with the same, direction-independent
        // normal. That is why the cast cannot judge this body.
        const FIXED: &str = "parry's shape cast now answers a direction from inside a body — \
                             re-read brief_a4_sweep.md before trusting it";
        let cap = at(0.3);
        let (shape, iso) = cap.to_parry();
        let body = &world.bodies()[0];
        let raw = |d: Vec3| {
            body.shape.with_parry(&mut |s| {
                parry3d::query::cast_shapes(
                    &iso,
                    &vector(d),
                    &shape,
                    &body.iso,
                    &vector(Vec3::ZERO),
                    s,
                    parry3d::query::ShapeCastOptions {
                        max_time_of_impact: STEP,
                        target_distance: 0.0,
                        stop_at_penetration: true,
                        compute_impact_geometry_on_penetration: true,
                    },
                )
                .ok()
                .flatten()
            })
        };
        let into = raw(-nx).expect("a cast from inside reports a hit");
        for d in [nx, nz, -nz] {
            let h = raw(d).unwrap_or_else(|| panic!("{FIXED}: no hit along {d}"));
            assert_eq!(h.time_of_impact, 0.0, "{FIXED}: along {d}");
            assert_eq!(
                h.normal1, into.normal1,
                "{FIXED}: the cast's normal now depends on the direction ({d})"
            );
        }
        assert_eq!(into.time_of_impact, 0.0, "{FIXED}");

        // OURS: the penetration test answers what the cast cannot.
        for depth in [0.1f32, 0.3, 0.5, 1.0] {
            let cap = at(depth);
            let push = world.push_out(&cap);
            assert!(
                (push.length() - depth).abs() < 1e-3 && push.normalize().dot(nx) > 0.99,
                "the fixture: {depth} m inside the +X face, push {push:?}"
            );
            let (t, n) = world
                .wall_sweep(&cap, -nx, STEP)
                .unwrap_or_else(|| panic!("blocked going deeper in, depth {depth}"));
            assert!(t < SWEEP_CONTACT_DIST, "at contact, not ahead: {t}");
            assert!(n.dot(nx) > 0.99, "pushed back out of the face: {n:?}");
            // Along the face and back OUT of it are both open — a step may
            // keep or reduce a penetration, only never deepen one.
            for d in [nz, -nz, nx] {
                assert_eq!(world.wall_sweep(&cap, d, STEP), None, "depth {depth}, {d}");
            }
        }
    }

    /// Phase A.4, the MESH half: the same, for a `coll` wall — a zero-
    /// thickness sheet. A capsule already inside one and stepping
    /// `ai::PROBE_DIST` further in ends CLEAN THROUGH it, so the end-pose
    /// point test found no contact at all and answered `None` INTO the wall,
    /// while stepping ALONG it — where the end pose is still inside — answered
    /// BLOCKED. The wrong way round in both directions.
    #[test]
    fn a_penetrated_mesh_wall_still_blocks_the_way_in() {
        const STEP: f32 = 1.0;
        for deg in [0.0f32, 118.0] {
            let to_world = Mat4::from_rotation_y(deg.to_radians());
            let world =
                CollisionWorld::new(vec![mesh_body_at(&wall_sheet(), to_world)], Vec::new());
            let ax = to_world.transform_vector3(Vec3::X).normalize();
            let az = to_world.transform_vector3(Vec3::Z).normalize();
            // `depth` past the sheet, the capsule's axis still on the near
            // side of it (which is the side `push_out` pushes back to).
            for depth in [0.1f32, 0.3, 0.45] {
                let cap = Capsule {
                    base: ax * (1.0 + depth),
                    height: 1.9,
                    radius: 0.5,
                };
                let push = world.push_out(&cap);
                assert!(
                    (push.length() - depth).abs() < 1e-3 && push.normalize().dot(ax) < -0.99,
                    "the fixture: {deg} deg, {depth} m inside, push {push:?}"
                );
                let (t, n) = world
                    .wall_sweep(&cap, ax, STEP)
                    .unwrap_or_else(|| panic!("blocked going deeper in, {deg} deg, {depth}"));
                assert!(t < SWEEP_CONTACT_DIST, "at contact, not ahead: {t}");
                assert!(n.dot(ax) < -0.99, "pushed back out of the sheet: {n:?}");
                for d in [az, -az, -ax] {
                    assert_eq!(world.wall_sweep(&cap, d, STEP), None, "{depth} m, {d}");
                }
            }
        }
    }

    /// Phase A.4, the rule in one sentence: a step may KEEP a penetration
    /// (walking along the wall) or REDUCE one (walking out of it), and may
    /// never DEEPEN one. The unit-test box, the capsule 0.2 m inside its
    /// x = 1.5 face.
    #[test]
    fn a_step_may_not_deepen_a_penetration_but_may_keep_it() {
        let world = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(3.5, 0.0, 0.0)),
                Shape::Box {
                    half: Vec3::new(2.0, 5.0, 5.0),
                },
            )],
            Vec::new(),
        );
        let inside = Capsule {
            base: Vec3::new(1.3, 0.0, 0.0),
            height: 1.9,
            radius: 0.4,
        };
        assert!(
            (world.push_out(&inside) - Vec3::new(-0.2, 0.0, 0.0)).length() < 1e-3,
            "the fixture: 0.2 m inside the x = 1.5 face"
        );
        // ALONG the wall: kept, not deepened — open.
        assert_eq!(world.wall_sweep(&inside, Vec3::Z, 1.0), None, "along");
        assert_eq!(world.wall_sweep(&inside, Vec3::NEG_Z, 1.0), None, "along");
        // OUT: reduced — open.
        assert_eq!(world.wall_sweep(&inside, Vec3::NEG_X, 1.0), None, "out");
        // IN: deepened — blocked, at contact distance, with the wall's normal.
        let (t, n) = world
            .wall_sweep(&inside, Vec3::X, 1.0)
            .expect("deeper in is blocked");
        assert!(t < SWEEP_CONTACT_DIST, "at contact, not ahead: {t}");
        assert!(n.x < -0.99, "the wall's normal: {n}");
    }

    /// An ASYMMETRIC mesh body for the parry-mirror regression tests: two
    /// parallel wall quads, at local x = 5 and x = -6, on an instance
    /// translated to (100, 0, 0). The real walls are at world x = 105 and
    /// x = 94; the POINT MIRROR of the x = 105 wall through the instance's
    /// origin lands at world x = 95, where there is nothing — and it is inside
    /// the body's world AABB, so the broad phase does not hide it. This is
    /// the shape of a real building instance (BF1 BES1's walkways), and it is
    /// why the symmetric `floor_quad`/`wall_quad`-at-the-origin worlds the
    /// other tests use never caught the bug.
    fn mirror_body() -> Body {
        let mut tris = wall_quad(5.0);
        tris.extend(wall_quad(-6.0));
        mesh_body_at(&tris, Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)))
    }

    fn mirror_world() -> CollisionWorld {
        CollisionWorld::new(vec![mirror_body()], Vec::new())
    }

    fn raw_parry_contact(body: &Body, cap: &Capsule) -> Option<parry3d::query::Contact> {
        let Shape::Mesh(mesh) = &body.shape else {
            unreachable!("mesh body")
        };
        let (shape, iso) = cap.to_parry();
        parry3d::query::contact(&iso, &shape, &body.iso, mesh.as_ref(), 0.0)
            .ok()
            .flatten()
    }

    /// parry 0.22.1's composite `contact` tests the capsule at a MIRRORED pose
    /// (module header), so the wall pass was pushing units out of a phantom
    /// copy of every building. [`mesh_contact`] asks the triangles directly.
    ///
    /// The raw-parry assertions come FIRST and are the DETECTOR for a future
    /// parry upgrade: they assert that upstream is still wrong, and say what
    /// to do when it is not.
    #[test]
    fn mesh_contact_is_not_mirrored() {
        let body = mirror_body();
        // 0.2 m INTO the real wall at world x = 105.
        let real = Capsule {
            base: Vec3::new(104.7, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        // Its point mirror through the instance origin: empty air, 1.3 m from
        // the nearest real surface (the x = 94 wall).
        let phantom = Capsule {
            base: Vec3::new(95.3, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        const FIXED: &str =
            "parry fixed contact_composite_shape_shape — drop `mesh_contact`'s workaround";

        let raw_real = raw_parry_contact(&body, &real);
        assert!(
            !raw_real.is_some_and(|c| c.dist < 0.0),
            "{FIXED}: raw parry now sees the real wall the capsule is 0.2 m inside ({raw_real:?})"
        );
        let raw_phantom = raw_parry_contact(&body, &phantom);
        assert!(
            raw_phantom.is_some_and(|c| c.dist < 0.0),
            "{FIXED}: raw parry no longer reports a penetration in the empty air at the \
             mirrored pose ({raw_phantom:?})"
        );

        // OURS: the real wall pushes, the phantom does not.
        let world = mirror_world();
        let push = world.push_out(&real);
        assert!(
            push.x < -0.1 && push.y == 0.0,
            "pushed back out of the real wall, got {push:?}"
        );
        assert_eq!(
            world.push_out(&phantom),
            Vec3::ZERO,
            "nothing to push out of at the mirrored pose"
        );

        // A ROTATED instance: the mirror is no longer a reflection in x, and
        // the wall's world normal is not one of the mesh's own axes.
        let to_world = Mat4::from_rotation_translation(
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::new(-20.0, 0.0, 40.0),
        );
        let mut tris = wall_quad(5.0);
        tris.extend(wall_quad(-6.0));
        let rotated = mesh_body_at(&tris, to_world);
        // The wall's world plane and outward normal, from the instance matrix.
        let n = to_world
            .transform_vector3(Vec3::new(-1.0, 0.0, 0.0))
            .normalize();
        let on_wall = to_world.transform_point3(Vec3::new(5.0, 0.0, 0.0));
        // 0.2 m into it, from the side the wall faces.
        let cap = Capsule {
            base: on_wall + n * 0.3,
            height: 1.9,
            radius: 0.5,
        };
        let raw_rot = raw_parry_contact(&rotated, &cap);
        assert!(
            !raw_rot.is_some_and(|c| c.dist < 0.0),
            "{FIXED}: raw parry now sees the rotated instance's real wall ({raw_rot:?})"
        );
        let rot_world = CollisionWorld::new(vec![rotated], Vec::new());
        let push = rot_world.push_out(&cap);
        assert!(
            push.length() > 0.1 && push.normalize().dot(n) > 0.99,
            "pushed along the rotated wall's world normal {n:?}, got {push:?}"
        );
    }

    /// The same bug through [`CollisionWorld::wall_sweep`]'s end-pose contact
    /// (A.2's "touching branch", now the not-penetrating half of
    /// [`penetration_block`] — the test is renamed with it, its assertions
    /// unchanged): a unit standing against a wall asked parry's composite
    /// `contact` at the end pose, got the phantom's answer, and reported the
    /// way ahead CLEAR while [`CollisionWorld::push_out`] shoved it back — the
    /// stall signature. The A.3 per-triangle cast pass is not involved.
    #[test]
    fn the_wall_sweeps_penetration_test_is_not_mirrored() {
        let world = mirror_world();
        // EXACTLY touching the real wall at world x = 105, walking into it.
        let touching = Capsule {
            base: Vec3::new(104.5, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        let (t, n) = world
            .wall_sweep(&touching, Vec3::X, 0.3)
            .expect("the real wall blocks");
        assert!(t < SWEEP_CONTACT_DIST, "the touching branch, got t = {t}");
        assert!(n.x < -0.99, "pushed back off the wall, got {n:?}");

        // The mirror of that pose: nothing ahead within the step.
        let mirrored = Capsule {
            base: Vec3::new(95.5, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        assert_eq!(world.wall_sweep(&mirrored, Vec3::X, 0.3), None);
    }

    /// A KAS2 City Hall lobe (card JWAL, `analysis/re-soldier-walls.md` §2.2):
    /// a solid cylinder r 9.25 whose top is at y 17.007, at the origin.
    fn lobe() -> CollisionWorld {
        CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(0.0, 15.007, 0.0)),
                Shape::Cylinder {
                    radius: 9.25,
                    half_height: 2.0,
                },
            )],
            Vec::new(),
        )
    }

    /// The exact pose the port's soldier entered the lobe at (the walk-jump
    /// reproduction's tick 42, feet 16.0667, 0.2323 m outside the rim). The
    /// stepped capsule's lower sphere is 0.16 above the top, so its rim
    /// contact is floor-like (n.y 0.5655) and the BARE pass skips it. The
    /// soldier's pass asks the full capsule instead and pushes him out.
    #[test]
    fn a_ledges_rim_under_a_jump_is_a_wall_for_the_soldier() {
        let world = lobe();
        let full = Capsule {
            base: Vec3::new(9.4823, 16.0667, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        assert_eq!(
            world.push_out(&full.stepped()),
            Vec3::ZERO,
            "the bare pass skips the rim (the leak's mechanism)"
        );
        let push = world.push_out_soldier(&full);
        assert!(push.x > 0.25 && push.z.abs() < 1e-4, "pushed out: {push}");
        assert_eq!(push.y, 0.0);

        // One tick later in the leak's residue: 0.31 m inside the rim at feet
        // 16.39, where the full capsule's shortest way out is UP (0.61) and
        // the ground query cannot lift him. The ledge escape pushes him out
        // radially instead.
        let inside = Capsule {
            base: Vec3::new(8.942, 16.3929, 0.0),
            ..full
        };
        let push = world.push_out_soldier(&inside);
        assert!(
            (push.x - (9.25 + 0.5 - 8.942)).abs() < 1e-3,
            "radially out: {push}"
        );

        // Within the step it is a step: feet 0.5 below the top, the ground
        // query stands him on it, and the wall pass leaves him be.
        let step = Capsule {
            base: Vec3::new(9.0, 16.507, 0.0),
            ..full
        };
        assert_eq!(world.push_out_soldier(&step), Vec3::ZERO);
        // Standing on the top: nothing to push.
        let on_top = Capsule {
            base: Vec3::new(5.0, 17.007, 0.0),
            ..full
        };
        assert_eq!(world.push_out_soldier(&on_top), Vec3::ZERO);
    }

    /// The AI's probe agrees: a crate 0.75 m tall (above the step) blocks the
    /// stepped capsule's sweep, where its top-edge graze used to be skipped
    /// as a floor.
    #[test]
    fn the_probe_sees_a_crate_above_the_step() {
        let world = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(2.0, 0.375, 0.0)),
                Shape::Box {
                    half: Vec3::new(0.5, 0.375, 2.0),
                },
            )],
            Vec::new(),
        );
        let cap = Capsule {
            base: Vec3::new(0.0, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        let (t, n) = world
            .wall_sweep(&cap.stepped(), Vec3::X, 2.0)
            .expect("the crate blocks the probe");
        assert!(t < 2.0 && n.x < -0.5, "t {t} n {n}");
        // A kerb under the step (0.4 m) is still open.
        let kerb = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(2.0, 0.2, 0.0)),
                Shape::Box {
                    half: Vec3::new(0.5, 0.2, 2.0),
                },
            )],
            Vec::new(),
        );
        assert_eq!(kerb.wall_sweep(&cap.stepped(), Vec3::X, 2.0), None);
    }

    /// A riser just over the step (BF2 KAM1's 0.62 m console front, the stall
    /// card JWAL's first build hit): the stepped capsule's round bottom passes
    /// over its top edge without touching it, so no contact asks the ledge
    /// rule, yet the full capsule is 0.27 m into the face. The soldier's pass
    /// asks the full capsule anyway and pushes him back out; left alone, the
    /// next tick puts his axis past the face and the push goes to the far side.
    #[test]
    fn a_riser_just_over_the_step_blocks_before_the_axis_crosses_it() {
        let p = |x: f32, y: f32| Vec3::new(x, y, -1.0);
        let world = CollisionWorld::new(
            vec![mesh_body(&[
                [p(-5.0, 0.0), p(5.0, 0.0), p(5.0, 0.62)],
                [p(-5.0, 0.0), p(5.0, 0.62), p(-5.0, 0.62)],
            ])],
            Vec::new(),
        );
        let full = Capsule {
            base: Vec3::new(0.0, 0.0, -0.77),
            height: 1.9,
            radius: 0.5,
        };
        assert_eq!(
            world.push_out(&full.stepped()),
            Vec3::ZERO,
            "the stepped capsule is clear"
        );
        let push = world.push_out_soldier(&full);
        assert!(
            (push.z - 0.27).abs() < 1e-3 && push.x.abs() < 1e-4,
            "pushed back out: {push}"
        );
        // A riser within the step is still climbed, not pushed.
        let low = CollisionWorld::new(
            vec![mesh_body(&[
                [p(-5.0, 0.0), p(5.0, 0.0), p(5.0, 0.55)],
                [p(-5.0, 0.0), p(5.0, 0.55), p(-5.0, 0.55)],
            ])],
            Vec::new(),
        );
        assert_eq!(low.push_out_soldier(&full), Vec3::ZERO);
    }

    /// Card WPSH, M2 (`analysis/re-soldier-walls.md` §4): a BF2 sprint-jump
    /// (MaxSpeed 9 × 1.5 × 1.3 = 17.55 m/s, 0.585 m a tick) at a
    /// zero-thickness mesh sheet, from every phase of its last step before
    /// contact, on an axis-aligned and a rotated instance. The pass at the
    /// END pose alone lets the axis past the sheet, and the deepest contact
    /// then pushes to the far side; along the move it never crosses. A walk
    /// (6.5 m/s) is one sub-step, as before, and stops touching the sheet.
    #[test]
    fn a_bf2_sprint_jump_cannot_tunnel_a_mesh_sheet() {
        for deg in [0.0f32, 118.0] {
            let to_world = Mat4::from_rotation_y(deg.to_radians());
            let world =
                CollisionWorld::new(vec![mesh_body_at(&wall_sheet(), to_world)], Vec::new());
            let ax = to_world.transform_vector3(Vec3::X).normalize();
            let at = |x: f32| Capsule {
                base: ax * x + Vec3::Y * 0.3,
                height: 1.9,
                radius: 0.5,
            };
            let mut old_crossed = 0;
            for speed in [17.55f32, 6.5] {
                let step = speed / 30.0;
                for k in 0..8 {
                    // The last pose before contact: `phase` short of touching.
                    let phase = step * k as f32 / 8.0;
                    let mut x = 1.0 - phase;
                    for tick in 0..6 {
                        let from = at(x);
                        let to = at(x + step);
                        if tick == 1 && world.push_out_soldier(&to).dot(ax) > 0.0 {
                            old_crossed += 1; // the end-pose pass pushes to the far side
                        }
                        x = (to.base + world.push_out_soldier_from(&from, &to)).dot(ax);
                        assert!(
                            x <= 1.0 + 1e-3,
                            "{deg} deg, {speed} m/s, phase {phase}, tick {tick}: axis at {x}"
                        );
                    }
                    assert!((x - 1.0).abs() < 1e-3, "{speed} m/s ends touching: {x}");
                }
            }
            assert!(
                old_crossed > 0,
                "the fixture: the end-pose pass tunnels, {deg} deg"
            );
        }
    }

    /// Card WPSH, M3 (§4.1, the cp5 hut): the capsule's axis came at a face
    /// from its near side and is now past it (the hut's outer skin, a
    /// ceiling, let it in). The triangle's deepest contact points to the FAR
    /// side; the end-pose pass takes it and carries the soldier through, the
    /// pass along the move never pushes that way.
    #[test]
    fn no_push_carries_the_axis_through_a_face_it_came_at() {
        let world = CollisionWorld::new(vec![mesh_body(&wall_sheet())], Vec::new());
        let at = |x: f32| Capsule {
            base: Vec3::new(x, 0.3, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        let (from, to) = (at(1.4), at(1.6));
        assert!(
            world.push_out_soldier(&to).x > 0.3,
            "the fixture: the end-pose pass pushes through"
        );
        let push = world.push_out_soldier_from(&from, &to);
        assert!(push.x <= 0.0, "no push to the far side: {push}");
        // A start that already straddles the face says nothing: its contact
        // stands, as the end-pose pass has it.
        let tilted = CollisionWorld::new(
            vec![mesh_body(&[[
                Vec3::new(1.0, 0.0, -5.0),
                Vec3::new(1.0, 0.0, 5.0),
                Vec3::new(2.0, 4.0, 0.0),
            ]])],
            Vec::new(),
        );
        let (from, to) = (at(1.3), at(1.35));
        assert_eq!(
            tilted.push_out_soldier_from(&from, &to),
            tilted.push_out_soldier(&to),
            "straddling at the start"
        );
    }

    /// Card WTUN: a two-triangle body, an overhang's underside over the
    /// soldier's head and the wall under it. The underside's contact (0.2 m)
    /// is deeper than the wall's (0.1 m); the pass answered only the deepest,
    /// a ceiling, and skipped the whole body, so the soldier walked through
    /// the wall (BF1 BES1 body 26, 95 census runs at one site). The wall
    /// contact now wins.
    #[test]
    fn a_wall_under_a_deeper_overhang_of_the_same_mesh_still_pushes() {
        let tris = vec![
            // The wall: the plane x = 1.0, y in [−1, 3], face normal −X.
            [
                Vec3::new(1.0, -1.0, -5.0),
                Vec3::new(1.0, 3.0, 0.0),
                Vec3::new(1.0, -1.0, 5.0),
            ],
            // The underside: the plane y = 1.7 over x in [−3, 1], face down.
            [
                Vec3::new(-3.0, 1.7, -5.0),
                Vec3::new(-3.0, 1.7, 5.0),
                Vec3::new(1.0, 1.7, 0.0),
            ],
        ];
        let world = world_from_triangles(tris);
        let cap = Capsule {
            base: Vec3::new(0.6, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        let push = world.push_out(&cap);
        assert!((push - Vec3::new(-0.1, 0.0, 0.0)).length() < 1e-3, "{push}");
    }

    /// Card WTUN: a ceiling-like contact pushes by its normal's HORIZONTAL
    /// part. Under a box's lower edge (the slab leaning over the soldier, BF2
    /// DAG1) the head's contact points 45 deg down and away, and pushes; flat
    /// under the box, straight down, it pushes nothing.
    #[test]
    fn a_ceiling_contact_pushes_only_its_horizontal_part() {
        // x in (0.3, 4.3), y in (1.7, 3.7).
        let world = CollisionWorld::new(
            vec![Body::new(
                Mat4::from_translation(Vec3::new(2.3, 2.7, 0.0)),
                Shape::Box {
                    half: Vec3::new(2.0, 1.0, 5.0),
                },
            )],
            Vec::new(),
        );
        let cap = |x: f32| Capsule {
            base: Vec3::new(x, 0.0, 0.0),
            height: 1.9,
            radius: 0.5,
        };
        // The top sphere's centre (0, 1.4) is 0.424 m from the edge (0.3, 1.7):
        // 0.076 m deep along (−0.707, −0.707), so the first pass pushes
        // 0.054 m along −X and the second (`WALL_ITERATIONS`) a little more,
        // never the whole 0.108 m a renormalised wall push would take.
        let push = world.push_out(&cap(0.0));
        let first = (0.5 - 0.18f32.sqrt()) * std::f32::consts::FRAC_1_SQRT_2;
        assert!(push.y == 0.0 && push.z.abs() < 1e-4, "{push}");
        assert!(push.x < -first + 1e-4 && push.x > -2.0 * first, "{push}");
        // Flat under the box: 0.2 m deep, straight down, no push.
        assert!(world.push_out(&cap(2.0)).length() < 1e-4);
    }
}
