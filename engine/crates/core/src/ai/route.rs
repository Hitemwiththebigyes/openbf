//! Where a unit is going and the leg it walks to get there (Phases A, A.2
//! and B): the objective rules ([`retarget`] — the commander's objective, or
//! Phase A's own rule for a unit the commander has not reached yet), the leg
//! planner [`plan_leg`] — the authored route table, the port's BFS, the
//! straight line — with its corner walk ([`fill_detour`]), the same-level
//! waypoint test ([`hub_has_floor`]), the hub a unit routes from
//! ([`start_hub`] — on the unit's own storey, [`level_hub`], card STLVL) and
//! the point a leg walks at right now ([`sub_point`]), and the point of a
//! post's capture region a last leg walks to ([`post_point`],
//! [`aim_last_leg`], card PARR — or the enemy contesting that region), and
//! the point of the next hub a leg walks at ([`hub_point`], card HUBP).
//! Card SPL11: and the route's advance ([`advance_route`]: the corners behind,
//! then the hub, `hub_reached`), the navigator's half of what `think` held.
//! The routing tests are in `route/tests.rs`.

use super::draw::{route_rand, stance_hold, unit_unit, SALT_HUB_POINT, SALT_POST_POINT};
use super::state::{AiState, NavTick, Route, RouteSource};
use super::{
    ARC_LINE_TOL, ARRIVE_MIN_RADIUS, CORNER_ARRIVE, DETOUR_MAX, DETOUR_RADIUS, DETOUR_WIDE_SLACK,
    HUB_LEVEL_TOL, LEG_LEAD, LEVEL_HUB_NEAR, LEVEL_HUB_REACH, LEVEL_HUB_TRIES,
};
use crate::nav::{self, NavGraph};
use crate::sim::Sim;
use crate::unit::UnitHandle;
use glam::{Vec2, Vec3};

/// How this unit routes on the planning graph: as a soldier on foot, as its
/// vehicle's `AISizeType` in a seat (card VAI; RE_LOG "The six type bits" —
/// the PS2's unit mask is the controlled entity's class+0x78).
pub(crate) fn mover(sim: &Sim, u: &crate::unit::Unit) -> nav::Mover {
    match u.mount.and_then(|m| sim.vehicle(m.vehicle)) {
        Some(v) => nav::Mover::new(
            sim.seat_setup
                .get(v.class as usize)
                .and_then(|s| s.as_ref())
                .map_or(nav::SOLDIER, |s| s.nav_type()),
            false,
        ),
        None => nav::Mover::new(nav::SOLDIER, u.spec.jet),
    }
}

/// A hub waypoint: its XZ with `y` from the ground under it AT THE UNIT'S OWN
/// LEVEL (`from_y`), falling back to the ground at the hub's AUTHORED height.
///
/// Hub `y` is the editor's pick height, not a floor (RE_LOG). Phase A.1
/// resolved the ground from that authored `y` alone, which is what put
/// waypoints on the wrong storey or nowhere at all: BF2 Polis Massa authors
/// all 189 of its hubs at y 0 while its floors are 13–33 m up, so
/// `ground_under` started its probe under every floor in the map, returned the
/// [`crate::collision::NO_GROUND`] sentinel, and the follower walked units at
/// a waypoint 20 m below their feet. Asking from the unit's own `y` asks the
/// question the follower means: what is under that XZ, on MY level (PLAN
/// "PHASE A.2 DESIGN" decision 3).
///
/// DEVIATION from that design, measured: the unit's level ALONE is not enough,
/// because `ground_under` calls a terrain sheet more than
/// [`crate::collision::TERRAIN_BURY`] above the feet "buried" and returns the
/// sentinel for it. That rule is right for a soldier standing under an uncut
/// heightfield and wrong for a hub UPHILL of one: on TAT1 the same waypoint
/// [-464, 17, 51] resolved to `floor 16.9` for a unit at y 18.0 and to
/// NO_GROUND for one at y 11.6, and the literal rule rejected 3,894 legs in
/// 3,000 ticks (1,682 of them with no alternative left), which stopped the
/// 6-a-side replay round fighting at all. So the authored height is the
/// FALLBACK, and a leg is only rejected when neither height finds a floor —
/// the intersection of A.1's rule and this one, which is still exactly what
/// rejects POL1's shafts (their authored probe is the one that found nothing
/// in the first place).
///
/// Card BNDS: since the hubs' floors are RESOLVED (`Sim::nav_floors`, walked
/// from the posts along the arcs), a resolved hub's waypoint is at that floor
/// whatever level the unit asks from — the hub is where the graph's own
/// walkway puts it, not on whatever lies under it at the unit's height (a
/// deck under a ramp, a platform's hidden core).
fn hub_waypoint(sim: &Sim, g: &NavGraph, hub: u16, from_y: f32) -> Vec3 {
    if let (Some(h), Some(f)) = (g.hub(hub), sim.nav_floor(hub)) {
        return Vec3::new(h.pos.x, f, h.pos.z);
    }
    match g.hub(hub) {
        Some(h) => {
            let at = |y: f32| sim.ground_under(Vec3::new(h.pos.x, y, h.pos.z), false);
            let mut y = at(from_y);
            if y <= crate::collision::NO_GROUND && h.pos.y > from_y {
                y = at(h.pos.y);
            }
            Vec3::new(h.pos.x, y, h.pos.z)
        }
        None => Vec3::ZERO,
    }
}

/// How many points of a hub [`hub_point`] draws while the draw lies in a
/// barrier: the PS2's own count (`FUN_002ed3d8`).
pub const HUB_POINT_DRAWS: u32 = 100;
/// ...and how many of them it walks the floor to before it takes the centre.
/// PROVISIONAL (ours): the PS2 has no floor test.
pub const HUB_POINT_TRIES: u32 = 8;
/// A drawn point's floor is within this of the hub's resolved floor (m):
/// half [`HUB_LEVEL_TOL`], so a unit standing on it is at the hub
/// (`hub_reached`). PROVISIONAL (ours).
pub const HUB_POINT_LEVEL: f32 = 0.5 * HUB_LEVEL_TOL;
/// ...and the floor goes on, with no wall, this far past the point (m): a
/// point on a rim or against a wall is not one to send a soldier at.
/// PROVISIONAL (ours).
pub const HUB_POINT_MARGIN: f32 = 1.0;

/// Card HUBP: the point of hub `hub` a soldier on foot walks at — the PS2's
/// waypoint (`analysis/re-soldier-ai.md` 2; B2 `FUN_002ea128`, BF1
/// `FUN_002657c8`): the hub's centre + normalize(three draws in [-1, 1)) x
/// max(0, radius - `inset`), a point of the hub's SPHERE and so, the walk
/// being XZ, of its disc (dense toward the rim). `inset` is the PS2's 4 m
/// for a droideka, else 0; the port has no droideka mover yet, so every
/// caller passes 0. Drawn per unit per leg from the unit's own stream at
/// this tick, and drawn again while it lies in a barrier that stops a
/// soldier (the PS2, up to [`HUB_POINT_DRAWS`] times).
///
/// OURS, PROVISIONAL: the point is one the hub's own floor leads to — the
/// floor walked from the centre ([`walk_floor`]) arrives within
/// [`HUB_POINT_LEVEL`] of the hub's resolved floor with no wall on the way,
/// and both hold [`HUB_POINT_MARGIN`] past it (a disc that reaches over a
/// ledge, through a wall or onto the storey below) — and one the floor
/// walked straight from the unit's feet (`pos`) arrives at too. After
/// [`HUB_POINT_TRIES`] such walks, and for a hub with no resolved floor, the
/// centre as before ([`hub_waypoint`]).
///
/// Measured and NOT kept (8 seeds x 32 maps each, card HUBP's report): a
/// wall sweep and a barrier test along the unit's line, the leg's line from
/// the unit instead of from the arc's end, and a walk by the hub's centre
/// when the next leg is the arc's own — each sent more legs at the centres
/// (a team mate within 1.5 m 7.2 % -> 8.6-9.3 % over the maps) and cured
/// nothing they were written for.
fn hub_point(sim: &Sim, g: &NavGraph, me: UnitHandle, hub: u16, pos: Vec3, inset: f32) -> Vec3 {
    let centre = hub_waypoint(sim, g, hub, pos.y);
    let (Some(h), Some(floor)) = (g.hub(hub), sim.nav_floor(hub)) else {
        return centre;
    };
    let r = (h.radius - inset).max(0.0);
    if r <= 0.0 {
        return centre;
    }
    let c = Vec2::new(centre.x, centre.z);
    let cap = crate::collision::Capsule {
        base: centre,
        height: crate::unit::capsule_height(crate::soldier::Stance::Stand),
        radius: crate::unit::SOLDIER_RADIUS,
    }
    .stepped();
    let mut walks = 0;
    for k in 0..HUB_POINT_DRAWS {
        let draw =
            |j: u32| 2.0 * unit_unit(me.index, sim.tick, me.gen, SALT_HUB_POINT + 3 * k + j) - 1.0;
        let d = Vec3::new(draw(0), draw(1), draw(2)).normalize_or_zero() * r;
        let p = c + Vec2::new(d.x, d.z);
        if g.barriers
            .iter()
            .any(|b| b.blocks(nav::SOLDIER) && b.contains(p))
        {
            continue;
        }
        let len = (p - c).length();
        if len < 1e-3 {
            return centre;
        }
        // The floor walked to the point, under it, and on past it by the
        // margin.
        let dir = (p - c) / len;
        let level = |y: &f32| (y - floor).abs() <= HUB_POINT_LEVEL;
        let stands = walk_floor(sim, centre, p).filter(level).filter(|&y| {
            // (One ray each, not the foot's ring: a foot holds an edge.)
            let under = |q: Vec2| sim.ground_point_under(Vec3::new(q.x, y, q.y), false);
            (under(p) - y).abs() <= crate::collision::STEP_HEIGHT
                && level(&under(p + dir * HUB_POINT_MARGIN))
                // ...and the floor walked from the unit's feet reaches it: a
                // straight line at a point off the centre can leave the
                // walkway the arc lies on.
                && walk_floor(sim, pos, p).is_some_and(|f| (f - y).abs() <= HUB_POINT_LEVEL)
        });
        if let Some(y) = stands {
            let dir = Vec3::new(dir.x, 0.0, dir.y);
            if sim
                .collision
                .wall_sweep(&cap, dir, len + HUB_POINT_MARGIN)
                .is_none()
            {
                return Vec3::new(p.x, y, p.y);
            }
        }
        walks += 1;
        if walks >= HUB_POINT_TRIES {
            break;
        }
    }
    centre
}

/// Card HUBP: a stuck unit's re-planned leg walks the arc's own line again,
/// hub centre to hub centre — the walkway — not at a second drawn point.
/// OURS, PROVISIONAL (the PS2's cure is a random walk).
pub(super) fn centre_leg(sim: &Sim, g: &NavGraph, r: &mut Route, pos: Vec3) {
    let Some(arc) = r.via_arc.and_then(|a| g.arcs.get(usize::from(a))) else {
        return;
    };
    if let Some(a) = g.hub(arc.other(r.next_hub)) {
        r.from = Vec2::new(a.pos.x, a.pos.z);
    }
    r.waypoint = hub_waypoint(sim, g, r.next_hub, pos.y);
}

/// Is there any floor under this hub's XZ that [`hub_waypoint`] can find from
/// `from_y`?
///
/// `false` means the NO_GROUND sentinel — nothing under the unit's feet there
/// and nothing under the authored height either — and the leg to that hub is
/// REJECTED from here. Note what this can and cannot see: `ground_under` looks
/// DOWN, so it catches a hub whose only floor is ABOVE the unit (a walkway
/// over an open shaft) and NOT one whose floor is far below (a unit can fall
/// to that, and on the discs often should). The design's rule is the sentinel,
/// and the sentinel is what this tests.
pub(crate) fn hub_has_floor(sim: &Sim, g: &NavGraph, hub: u16, from_y: f32) -> bool {
    hub_waypoint(sim, g, hub, from_y).y > crate::collision::NO_GROUND
}

/// Is hub `hub` on the level of a unit whose feet are at `y` — its resolved
/// floor within [`HUB_LEVEL_TOL`] of them? The test for the hub a unit ROUTES
/// FROM and for ARRIVAL (card BNDS): a unit under a walkway's hub is not at
/// that hub. An unresolved hub falls back to [`hub_has_floor`].
pub(crate) fn hub_on_level(sim: &Sim, g: &NavGraph, hub: u16, y: f32) -> bool {
    match sim.nav_floor(hub) {
        Some(f) => (f - y).abs() <= HUB_LEVEL_TOL,
        None => hub_has_floor(sim, g, hub, y),
    }
}

/// The hub a unit ROUTES FROM: [`NavGraph::unit_hub_on`] - the port's own
/// XZ reading of where a unit stands (a named deviation, card LOC3; a POST
/// is resolved by the PS2's 3-D [`NavGraph::locate`]) - with the same-level
/// preference every caller here uses. Phase B's commander needs the same
/// answer `retarget` uses, so it lives in one place.
///
/// Every objective rule starts here, [`retarget_phase_a`] too (card DAGS).
///
/// Card STLVL: a soldier on foot whose answer is a hub on ANOTHER STOREY —
/// the start is located in XZ, so a unit under a rooftop walkway or over a
/// cave locates at the hub above or below it — routes from the nearest hub
/// on its own
/// ([`level_hub`]).
pub(crate) fn start_hub(sim: &Sim, g: &NavGraph, me: UnitHandle) -> Option<u16> {
    let u = sim.unit(me)?;
    let pos = u.soldier.pos;
    let h = g.unit_hub_on(pos, mover(sim, u), &|h| hub_on_level(sim, g, h, pos.y))?;
    Some(if u.mount.is_none() {
        level_hub(sim, g, h, pos)
    } else {
        h
    })
}

/// Card SPNC's census of the spawn nodes (`game/examples/spawn_node_census.rs`):
/// the first leg unit `me`, standing where it is, plans toward hub `dest` -
/// from [`start_hub`] (`[1]`, today's rule) and from the bare XZ locate the
/// first leg after a spawn took before card DAGS (`[0]`). Each is the start
/// hub and the leg [`plan_leg`] answers from it; `None` off the graph.
/// Reads the sim only.
pub fn first_leg(sim: &Sim, me: UnitHandle, dest: u16) -> [Option<(u16, Route)>; 2] {
    let (Some(g), Some(u)) = (sim.nav.as_ref(), sim.unit(me)) else {
        return [None, None];
    };
    let pos = u.soldier.pos;
    let bare = g.unit_hub_on(pos, mover(sim, u), &|h| hub_on_level(sim, g, h, pos.y));
    let st = AiState::default();
    let mut nt = NavTick::default();
    [bare, start_hub(sim, g, me)]
        .map(|cur| cur.and_then(|c| plan_leg(sim, &st, me, c, dest, None, &mut nt).map(|r| (c, r))))
}

/// [`walk_floor`] for the census (card SPNC).
pub fn floor_walked(sim: &Sim, from: Vec3, to: Vec2) -> Option<f32> {
    walk_floor(sim, from, to)
}

/// The hub a soldier at `pos` routes from (or is hunted at), given the
/// graph's own XZ answer `h`: `h` when it is on the soldier's storey — its
/// resolved floor within [`LEVEL_HUB_NEAR`] of the feet, or reached within
/// [`LEVEL_HUB_REACH`] by the floor walked from the feet (a wide hub on a
/// slope, a ramp) — or unresolved; else the nearest of the
/// [`LEVEL_HUB_TRIES`] nearest resolved hubs ([`crate::sim::Sim::level_at`]'s
/// order: XZ to the disc plus the height between, weighted) that the floor
/// walked from the feet does reach; `h` when there is none. The tolerances
/// are under [`HUB_LEVEL_TOL`]: BF1 Geonosis's cp13 stands on a platform
/// 3.6 m over the terrain with a hub on its ramp between the two, and the
/// last clone under that platform was hunted from on top of it. OURS,
/// PROVISIONAL, part of the unit-locate deviation (card LOC3): the PS2
/// locates a unit in 3-D like a goal (BF1 a hub sphere, both games 3-D
/// corridors; RE_LOG "Arrival and locate" and its audit of 2026-10-03) and
/// has no level rule.
pub(crate) fn level_hub(sim: &Sim, g: &NavGraph, h: u16, pos: Vec3) -> u16 {
    let Some(f) = sim.nav_floor(h) else {
        return h;
    };
    if (f - pos.y).abs() <= LEVEL_HUB_NEAR || sim.floor_reaches_hub(pos, h, LEVEL_HUB_REACH) {
        return untrapped(sim, g, h, pos);
    }
    let near = sim.nearest_floor_hubs::<LEVEL_HUB_TRIES>(g, pos, |i| {
        sim.nav_floor(i as u16).filter(|_| i as u16 != h)
    });
    let h = near
        .iter()
        .filter(|c| c.1 != u16::MAX)
        .find(|c| sim.floor_reaches_hub(pos, c.1, LEVEL_HUB_REACH))
        .map_or(h, |c| c.1);
    untrapped(sim, g, h, pos)
}

/// Card TGAP: [`level_hub`]'s answer `h`, unless a soldier would be trapped
/// there ([`crate::sim::Sim::hub_trapped`]) and he does not stand in its
/// disc: then the nearest of the [`LEVEL_HUB_TRIES`] nearest hubs he is not
/// trapped in that the floor walked from his feet reaches, `h` when none
/// does. BF1 Tatooine's spawn node (-377, 83) is 17 m off the walled
/// footpath and was started on it (hub 85) toward every post; a soldier in
/// the disc of a trapped hub (a deck no walkable arc joins) starts there.
fn untrapped(sim: &Sim, g: &NavGraph, h: u16, pos: Vec3) -> u16 {
    let p = Vec2::new(pos.x, pos.z);
    let inside = g
        .hub(h)
        .is_some_and(|hub| (hub.xz() - p).length() <= hub.radius);
    if inside || !sim.hub_trapped(h) {
        return h;
    }
    let near = sim.nearest_floor_hubs::<LEVEL_HUB_TRIES>(g, pos, |i| {
        sim.nav_floor(i as u16)
            .filter(|_| !sim.hub_trapped(i as u16))
    });
    near.iter()
        .filter(|c| c.1 != u16::MAX)
        .find(|c| sim.floor_reaches_hub(pos, c.1, LEVEL_HUB_REACH))
        .map_or(h, |c| c.1)
}

/// The hub a mover of type `ty` standing at hub `cur` routes to for `post`:
/// the post's own when its type can get there. A walker or (card VDROP) a
/// hover that cannot (`nearest`, card WAI) takes the hub it can reach that
/// is nearest the post, and its driver gets out there — on both Hoths no
/// post's hub is on a vehicle type's graph (BF2 HOT1: the nearest are
/// 29-86 m off), on BF2 GEO1 four of seven are not, and the PS2's bots ride
/// there all the same (re-walker-ai §10). OURS, PROVISIONAL: where a PS2
/// vehicle's move order comes from is not transcribed.
pub(crate) fn post_hub(
    g: &NavGraph,
    ty: usize,
    cur: u16,
    post: &crate::cp::CommandPost,
    nearest: bool,
) -> Option<u16> {
    let own = post.nav_hub?;
    if g.reachable(cur, own, ty) {
        return Some(own);
    }
    if !nearest {
        return None;
    }
    let mut best: Option<(f32, u16)> = None;
    for (i, h) in g.hubs.iter().enumerate() {
        let d = nav::xz_dist(h.pos, post.def.pos);
        if best.is_none_or(|(b, _)| d < b) && g.reachable(cur, i as u16, ty) {
            best = Some((d, i as u16));
        }
    }
    best.map(|(_, i)| i)
}

/// The world point this route is ultimately walking to: the target post, or
/// the destination hub when hunting.
fn goal_pos(sim: &Sim, g: &NavGraph, st: &AiState, dest: u16, from_y: f32) -> Vec3 {
    // (Card WAI2: a route that ends at another hub than the post's own — a
    // walker's drop hub, a claimer's vehicle — ends at that hub.)
    match st.target_cp.and_then(|i| sim.cps.get(i as usize)) {
        Some(cp) if cp.nav_hub == Some(dest) => cp.def.pos,
        _ => hub_waypoint(sim, g, dest, from_y),
    }
}

/// How many points of a post's capture region [`post_point`] draws before it
/// falls back. PROVISIONAL (ours): the PS2 draws one and paths to it.
pub const POST_POINT_TRIES: u32 = 8;
/// A last leg takes its point once the bot is within this of the capture
/// region's footprint (m, XZ). PROVISIONAL (ours): the floor is walked from
/// the bot's feet, so the draw waits until the bot is near.
pub const POST_POINT_RANGE: f32 = 10.0;
/// The drawn point's share of the region's footprint, so that a bot a
/// [`super::CORNER_ARRIVE`] short of it is inside the region. PROVISIONAL.
pub const POST_POINT_INSET: f32 = 0.9;
/// The step of [`walk_floor`] and of the fallback walk toward the bot (m).
const POST_POINT_STEP: f32 = 0.5;
/// A capsule the wall pass moves by more than this is in a body (m).
const POST_POINT_CLEAR: f32 = 0.02;

/// The floor a soldier walking the straight line from `from` (feet on a
/// floor) arrives at over `to`: the foot's own ground query a step at a time,
/// so a rise over a step is not climbed and the walk goes under it. `None`
/// over open air or a drop past [`super::LEDGE_DROP`].
pub(super) fn walk_floor(sim: &Sim, from: Vec3, to: Vec2) -> Option<f32> {
    let start = Vec2::new(from.x, from.z);
    let len = (to - start).length();
    let steps = (len / POST_POINT_STEP).ceil().max(1.0) as u32;
    let mut y = from.y;
    for i in 1..=steps {
        let p = start + (to - start) * (i as f32 / steps as f32);
        let f = sim.ground_under(Vec3::new(p.x, y, p.y), false);
        if f <= crate::collision::NO_GROUND || f < y - super::LEDGE_DROP {
            return None;
        }
        y = f;
    }
    Some(y)
}

/// One uniform draw of `region`'s volume from three numbers in [0, 1), its
/// footprint drawn in by [`POST_POINT_INSET`]. Only the XZ is used.
fn region_draw(region: &crate::region::Region, a: f32, b: f32, c: f32) -> Vec2 {
    use crate::region::Shape;
    let l = match region.shape {
        Shape::Box => Vec3::new(
            (2.0 * a - 1.0) * region.size.x * POST_POINT_INSET,
            (2.0 * c - 1.0) * region.size.y,
            (2.0 * b - 1.0) * region.size.z * POST_POINT_INSET,
        ),
        Shape::Sphere | Shape::Cylinder => {
            let r = region.radius() * a.sqrt() * POST_POINT_INSET;
            let (s, co) = crate::fmath::sin_cos(std::f32::consts::TAU * b);
            let y = match region.shape {
                Shape::Cylinder => (2.0 * c - 1.0) * region.size.y,
                _ => 0.0,
            };
            Vec3::new(r * co, y, r * s)
        }
    };
    let p = region.center + region.basis * l;
    Vec2::new(p.x, p.z)
}

/// Card PARR: the point a bot on foot walks to at a post — the PS2's capture
/// behaviour walks to the post's position + a random point of its capture
/// region and is "at the post" inside the region
/// (`analysis/re-conquest-goal.md` section 4); it never aims at the centre,
/// which on the discs is a solid plinth (BF1) or on top of a prop (BF2
/// Kamino's cp0, 3 m over the floor).
///
/// The point is drawn from the unit's own stream at this tick, and it is one
/// a soldier can stand on: the floor walked from the bot's feet reaches it
/// ([`walk_floor`]), it is inside the region there, and the wall pass leaves
/// a soldier's capsule where it is. Where the post has a floor of its own
/// under the point (card STEND's: one at the centre's height,
/// [`super::mover::SPOT_FLOOR_TOL`]) the point is ON it — a bot under a
/// post's platform, or on a roof over it, is given no point there.
///
/// Of [`POST_POINT_TRIES`] draws the best wins: on the post's own floor
/// before beside it (BF2 Kamino's cp0: the floor round the depot), with no
/// wall on the straight line from the bot before with one. Else the nearest
/// standable point from the centre toward the bot; else the centre, as
/// before. OURS, PROVISIONAL: the standing tests and the order — the PS2
/// draws once and its mover finds the way.
pub(super) fn post_point(
    sim: &Sim,
    me: UnitHandle,
    u: &crate::unit::Unit,
    cp: &crate::cp::CommandPost,
) -> Vec3 {
    let from = u.soldier.pos;
    let region = &cp.def.capture;
    let tol = super::mover::SPOT_FLOOR_TOL;
    // The point over `xz` and whether it is on the post's own floor.
    let stands = |xz: Vec2| -> Option<(Vec3, bool)> {
        let p = Vec3::new(xz.x, walk_floor(sim, from, xz)?, xz.y);
        let own = sim.ground_under(Vec3::new(xz.x, cp.def.pos.y, xz.y), false);
        let has_own = (own - cp.def.pos.y).abs() <= tol;
        if has_own && (own - p.y).abs() > crate::collision::STEP_HEIGHT {
            return None;
        }
        let cap = crate::collision::Capsule {
            base: p,
            height: crate::unit::capsule_height(crate::soldier::Stance::Stand),
            radius: crate::unit::SOLDIER_RADIUS,
        };
        (region.contains(p) && sim.collision.push_out_soldier(&cap).length() <= POST_POINT_CLEAR)
            .then_some((p, has_own))
    };
    let clear = |p: Vec3| {
        let d = Vec3::new(p.x - from.x, 0.0, p.z - from.z);
        let len = d.length();
        len < 1e-3
            || (sim
                .collision
                .wall_sweep(&u.capsule().stepped(), d / len, len)
                .is_none()
                && sim.nav.as_ref().is_none_or(|g| {
                    g.barrier_stop(Vec2::new(from.x, from.z), Vec2::new(p.x, p.z), nav::SOLDIER)
                        >= 1.0
                }))
    };
    let mut best: Option<(u8, Vec3)> = None;
    for k in 0..POST_POINT_TRIES {
        let draw = |j: u32| unit_unit(me.index, sim.tick, me.gen, SALT_POST_POINT + 3 * k + j);
        let Some((p, own)) = stands(region_draw(region, draw(0), draw(1), draw(2))) else {
            continue;
        };
        let rank = 2 * u8::from(own) + u8::from(clear(p));
        if rank == 3 {
            return p;
        }
        if best.is_none_or(|(b, _)| rank > b) {
            best = Some((rank, p));
        }
    }
    if let Some((_, p)) = best {
        return p;
    }
    let c = Vec2::new(cp.def.pos.x, cp.def.pos.z);
    let to = Vec2::new(from.x, from.z) - c;
    let reach = to.length().min(region.radius());
    let n = (reach / POST_POINT_STEP) as u32;
    (1..=n)
        .find_map(|i| stands(c + to.normalize_or_zero() * (i as f32 * POST_POINT_STEP)))
        .map_or(cp.def.pos, |(p, _)| p)
}

/// Card PARR: give a bot's LAST LEG at a post its point ([`post_point`]) — a
/// leg still aimed at the post's centre, of a grounded bot on foot within
/// [`POST_POINT_RANGE`] of the region. A defender's Cover / Snipe spot is not
/// the centre and is kept; a re-plan (the stuck window) aims at the centre
/// again and so draws afresh.
///
/// THE CONTESTER. A bot with no target, at a post an enemy soldier stands in
/// the region of, walks at that enemy (the nearest) instead: spread over a
/// wide region with walls in it, the two sides stand out of each other's
/// sight and the post stays contested for good (TAT1 cp1, seed 3: sixteen
/// attackers on their points and one defender in a hut 10 m off, 16 v 1 at
/// 60,000 ticks — the ring round BF1 NAB1's cp3 again). OURS, PROVISIONAL:
/// like the hunt it is not bounded by what the bot sees.
pub(super) fn aim_last_leg(sim: &Sim, me: UnitHandle, st: &mut AiState, nt: &mut NavTick) {
    let (Some(u), Some(mut r)) = (sim.unit(me), st.route) else {
        return;
    };
    let Some(cp) = st.target_cp.and_then(|i| sim.cps.get(i as usize)) else {
        return;
    };
    let pos = u.soldier.pos;
    let region = &cp.def.capture;
    if !r.is_last()
        || u.mount.is_some()
        || !u.soldier.grounded
        || nav::xz_dist(pos, region.center) > region.radius() + POST_POINT_RANGE
    {
        return;
    }
    let contester = (st.target.is_none())
        .then(|| {
            sim.alive_units()
                .filter(|(_, o)| {
                    o.team != u.team && o.mount.is_none() && region.contains(o.soldier.pos)
                })
                .map(|(_, o)| o.soldier.pos)
                .min_by(|a, b| {
                    (*a - pos)
                        .length_squared()
                        .total_cmp(&(*b - pos).length_squared())
                })
        })
        .flatten();
    let p = match contester {
        Some(e) => e,
        None if r.waypoint == cp.def.pos => post_point(sim, me, u, cp),
        None => return,
    };
    if p != r.waypoint {
        r.waypoint = p;
        // The corner walk to the new point — but not on a leg the stuck
        // window has re-planned: that one dropped its corners on purpose.
        if let (Some(g), 0) = (sim.nav.as_ref(), r.stuck_run) {
            fill_detour(g, mover(sim, u).ty, pos, &mut r, nt);
        }
        r.ref_dist = nav::xz_dist(pos, sub_point(&r, pos));
        st.route = Some(r);
    }
}

/// Fill this leg's CORNER WALK: the intermediate waypoints that get round the
/// barriers the straight leg `from → r.waypoint` crosses (PLAN "A.1 addendum",
/// [`crate::nav::NavGraph::detour`]).
///
/// An empty result — the straight leg is clear — and a FAILED search are both
/// "walk straight at the waypoint": a leg the corner search cannot resolve is
/// exactly the case the wall probe is there for, and on the discs it is
/// always a BF2 door — an engine `door` entity that opens itself, not the
/// mission Lua (`analysis/re-bf2-doors.md`) — (RE_LOG: 100 % of BF1's crossing
/// legs resolve, 1,150 of 1,262 overall). A walk of more than [`DETOUR_MAX`]
/// corners is cut to that many; [`advance_route`] asks again at the last.
fn fill_detour(g: &NavGraph, ty: usize, from: Vec3, r: &mut Route, nt: &mut NavTick) {
    r.detour = [Vec2::ZERO; DETOUR_MAX];
    r.detour_len = 0;
    r.detour_at = 0;
    if g.barriers.is_empty() {
        return;
    }
    let a = Vec2::new(from.x, from.z);
    let b = Vec2::new(r.waypoint.x, r.waypoint.z);
    let path = if ty == nav::SOLDIER {
        soldier_way(g, a, b)
    } else {
        g.detour(a, b, DETOUR_RADIUS, ty)
    };
    let Some(path) = path else {
        return;
    };
    if path.is_empty() {
        return;
    }
    for (i, &p) in path.iter().take(DETOUR_MAX).enumerate() {
        r.detour[i] = p;
        r.detour_len = (i + 1) as u8;
    }
    nt.detoured += 1;
}

/// A soldier's corner walk from `a` to `b`: the wide margin's way
/// ([`DETOUR_RADIUS`]), unless it finds none, ignores a barrier the PS2's
/// margin (the capsule radius) keeps - one an end point stands within 1.0 m
/// of, a stair's rail - or is longer than the PS2's by more than
/// [`DETOUR_WIDE_SLACK`]: then the PS2's (issue #34).
fn soldier_way(g: &NavGraph, a: Vec2, b: Vec2) -> Option<Vec<Vec2>> {
    let ty = nav::SOLDIER;
    let narrow = crate::unit::SOLDIER_RADIUS;
    let wide = g.detour(a, b, DETOUR_RADIUS, ty);
    let drops =
        g.leg_ignores(a, b, DETOUR_RADIUS, ty).len() > g.leg_ignores(a, b, narrow, ty).len();
    if !drops && wide.as_ref().is_some_and(|w| w.is_empty()) {
        return wide;
    }
    match (wide, g.detour(a, b, narrow, ty)) {
        (_, Some(p)) if drops => Some(p),
        (Some(w), Some(p)) if way_len(a, &w, b) > way_len(a, &p, b) + DETOUR_WIDE_SLACK => Some(p),
        (None, p) => p,
        (w, _) => w,
    }
}

/// The length of the walk from `a` over `corners` to `b` (XZ).
fn way_len(a: Vec2, corners: &[Vec2], b: Vec2) -> f32 {
    let mut at = a;
    let mut len = 0.0;
    for &c in corners.iter().chain(std::iter::once(&b)) {
        len += (c - at).length();
        at = c;
    }
    len
}

/// How near a detour corner counts as reached. A soldier and a walker:
/// [`CORNER_ARRIVE`]. A hover's driver: its own node radius (`min_arrive`,
/// ForwardSpeed x 0.5 — `analysis/re-hover-ai.md` 5: the PS2's hover arrives
/// at every node of its path at that distance, and a corner is a node of the
/// follower's, re-soldier-ai 2) [D], PROVISIONAL. Card HJAM: at the soldier's 1 m a BF1
/// speeder bike (40 m/s, no strafe, R 20 m) passed the corner some metres
/// wide and then turned round it at full stick for as long as its progress
/// window let it — 43 % of BF1 Endor's and 50 % of BF1 Yavin's bike-driving
/// time was within 8 m of a corner, and half of all the drivers' failed
/// windows (1,153 of 2,303 in 240 rounds) were at one.
fn corner_arrive(sim: &Sim, me: UnitHandle, min_arrive: f32) -> f32 {
    let hover = sim
        .unit(me)
        .and_then(|u| u.mount)
        .and_then(|m| sim.vehicle(m.vehicle))
        .is_some_and(|v| v.kind == crate::vehicle::VehicleKind::Hover);
    if hover {
        CORNER_ARRIVE.max(min_arrive)
    } else {
        CORNER_ARRIVE
    }
}

/// The point a leg is walking at right now: its current detour corner (which is
/// XZ, so it takes the unit's own y) or, with the corners behind it, the leg's
/// own waypoint. This is the PS2's "current path node", and it is what BOTH
/// the steering and the stuck window measure against.
pub(super) fn sub_point(r: &Route, pos: Vec3) -> Vec3 {
    match r.corner() {
        Some(c) => Vec3::new(c.x, pos.y, c.y),
        None => r.waypoint,
    }
}

/// THE PS2'S LEG RULE (BF2 `FUN_002f1d80` -> `FUN_002a5fa8`,
/// `analysis/re-soldier-ai.md` 2; BF1 aims straight at the point, and
/// `analysis/bot-design.md` 4.5 uses BF2's rule for both games; card MOVR2):
/// where a unit on an ARC leg walks this tick is its PROJECTION on the leg's
/// own line ([`Route::from`] to its waypoint — card HUBP: from the waypoint it
/// reached in the last hub, or from that hub's centre on a leg planned
/// anywhere else, to the unit's point of the next hub; hub centre to hub
/// centre on a stuck unit's re-plan) advanced by its `speed` (m/s read as m)
/// plus [`LEG_LEAD`], clamped to the leg's end — not straight at the far hub
/// from wherever it happens to be. Card MOUTH: with a detour corner behind
/// the unit the line starts at THAT corner ([`seg_from`]) — the PS2's line is
/// its follower's segment, the node just reached to the node walked at, and a
/// corner is a node (BF2 `FUN_002f1470` -> `FUN_002f0e78`, re-soldier-ai 2).
/// Until then the line stayed the leg's own, so a unit that had just walked
/// the corner at a walkway's mouth was aimed back at the line the corner walk
/// was planned to avoid: beside the walkway, into its rail (BF2 Kamino's four
/// walkway legs 32 stalls in 8 rounds, and BF1 Bespin's bots pulled off a
/// walkway and over the rim: 46 deaths under the floor in 8 rounds against 12).
/// A unit beside its line comes back to it:
/// at a run over the next few metres, from a stand at once. (The next leg
/// can start metres off the line a bridge lies on, and the beeline from
/// there met the platform's rail beside the bridge: 20 of 32 units stood
/// stuck on BF2 KAM1 after 200 s — the maintainer, 2026-10-01.) A detour corner, and a
/// leg with no arc (the last leg, a re-plan's approach), are walked straight
/// at. Until card MOVR2 the port aimed 5 m ahead on the line, and only when
/// over 1 m off it.
pub(super) fn leg_point(r: &Route, pos: Vec3, speed: f32) -> Vec3 {
    let sp = sub_point(r, pos);
    if r.corner().is_some() {
        return sp;
    }
    let Some((a, dir, len, along, _)) = line_from(r, seg_from(r), pos) else {
        return sp;
    };
    let t = along.max(0.0) + speed + LEG_LEAD;
    if t >= len {
        return sp;
    }
    let q = a + dir * t;
    Vec3::new(q.x, pos.y, q.y)
}

/// The stuck window's re-plan (`mover::stuck_window`; card MOVR lifted it
/// here, unchanged): plan the leg again from the hub the unit stands in. Beside the arc it would leave that hub on, and not at the hub:
/// to the hub's centre first (once — an approach leg that sticks re-plans
/// the arc); card HUBP: and on the arc's own line, centre to centre, not at
/// another drawn point. The corner walk has already failed on this leg, so
/// it is DROPPED for the straight line and the wall probe: a barrier corner
/// is a point in a 2-D rectangle world and nothing says a unit can stand
/// there (BF1 NAB2 unit 25, 6.6 m from a corner for 49,440 ticks); the PS2
/// never commits to a corner either — its walk step re-aims at one per STEP
/// (`FUN_00250fc8`). Answers whether a leg was planned.
pub(super) fn replan(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    pos: Vec3,
    nt: &mut NavTick,
) -> bool {
    let Some(r) = st.route else { return false };
    let Some(g) = sim.nav.as_ref() else {
        return false;
    };
    let Some(cur) = start_hub(sim, g, me) else {
        return false;
    };
    let Some(mut next) = plan_leg(sim, st, me, cur, r.dest_hub, None, nt) else {
        return false;
    };
    centre_leg(sim, g, &mut next, pos);
    if r.via_arc.is_some() {
        let off = off_arc_line(&next, pos);
        let at = g.hub(cur).map(|h| nav::xz_dist(pos, h.pos));
        if cur != r.dest_hub
            && off.is_some_and(|d| d > ARC_LINE_TOL)
            && at.is_some_and(|d| d > ARRIVE_MIN_RADIUS)
        {
            next = approach_leg(sim, g, cur, r.dest_hub, pos);
        }
    }
    next.stuck_run = r.stuck_run;
    next.sidestep = r.sidestep;
    next.detour_len = 0;
    next.detour_at = 0;
    next.ref_dist = nav::xz_dist(pos, sub_point(&next, pos));
    st.route = Some(next);
    true
}

/// An arc leg's line and where `pos` stands beside it: the leg's start
/// ([`Route::from`]), the unit direction to its waypoint, the line's length,
/// the unit's distance along it and its distance off it. `None` on a leg
/// with no arc.
fn arc_line(r: &Route, pos: Vec3) -> Option<(Vec2, Vec2, f32, f32, f32)> {
    line_from(r, r.from, pos)
}

/// Where the stretch of the leg the unit is on starts (card MOUTH): the last
/// detour corner behind it, else the leg's own start ([`Route::from`]) — the
/// PS2 follower's "node just reached". Only [`leg_point`] asks: the stuck
/// re-plan ([`off_arc_line`]) and the arrival plane (`hub_reached`) keep the
/// leg's own line (the plane from the last corner was measured twice and not
/// kept: cards OBST2 and MOUTH).
fn seg_from(r: &Route) -> Vec2 {
    let at = usize::from(r.detour_at)
        .min(usize::from(r.detour_len))
        .min(DETOUR_MAX);
    if at > 0 {
        r.detour[at - 1]
    } else {
        r.from
    }
}

/// [`arc_line`] for the line that starts at `from`.
fn line_from(r: &Route, from: Vec2, pos: Vec3) -> Option<(Vec2, Vec2, f32, f32, f32)> {
    r.via_arc?;
    let (a, b) = (from, Vec2::new(r.waypoint.x, r.waypoint.z));
    let len = (b - a).length();
    if len < 1e-3 {
        return None;
    }
    let dir = (b - a) / len;
    let rel = Vec2::new(pos.x, pos.z) - a;
    let along = rel.dot(dir);
    Some((a, dir, len, along, (rel - dir * along).length()))
}

/// How far `pos` is off an arc leg's line; `None` on a leg with no arc.
pub(super) fn off_arc_line(r: &Route, pos: Vec3) -> Option<f32> {
    arc_line(r, pos).map(|l| l.4)
}

/// A leg TO hub `cur` itself, with no arc: the re-plan of a unit that stands
/// near `cur` but beside the line of the arc it would leave on, and has
/// already been stuck for a window walking that arc from where it stood (a
/// corner between a platform's rail and a ramp's end post, BF2 KAM1). From
/// the hub's centre the arc's line is the walkway.
pub(super) fn approach_leg(sim: &Sim, g: &NavGraph, cur: u16, dest: u16, pos: Vec3) -> Route {
    let waypoint = hub_waypoint(sim, g, cur, pos.y);
    Route {
        dest_hub: dest,
        next_hub: cur,
        via_arc: None,
        waypoint,
        ref_dist: nav::xz_dist(pos, waypoint),
        from: Vec2::new(pos.x, pos.z),
        source: RouteSource::Table,
        ..Route::default()
    }
}

/// One hop from `cur` toward `dest`: the authored table first, the port's BFS
/// when the table is empty, a straight line at the goal when neither answers.
/// Every leg it produces carries its corner walk ([`fill_detour`]).
pub(crate) fn plan_leg(
    sim: &Sim,
    st: &AiState,
    me: UnitHandle,
    cur: u16,
    dest: u16,
    arrival_arc: Option<u16>,
    nt: &mut NavTick,
) -> Option<Route> {
    let g = sim.nav.as_ref()?;
    let u = sim.unit(me)?;
    let pos = u.soldier.pos;
    let m = mover(sim, u);
    let last = |source| Route {
        dest_hub: dest,
        next_hub: dest,
        via_arc: None,
        waypoint: goal_pos(sim, g, st, dest, pos.y),
        ref_dist: 0.0,
        from: Vec2::new(pos.x, pos.z),
        source,
        ..Route::default()
    };
    if cur == dest {
        let mut r = last(RouteSource::Table);
        fill_detour(g, m.ty, pos, &mut r, nt);
        return Some(r);
    }
    let r = route_rand(me.index, sim.tick, me.gen);
    // The SAME-LEVEL test every candidate hop has to pass: is there ground at
    // the unit's own level under that hub (PLAN "PHASE A.2 DESIGN" decision
    // 3)? It is a collision query, so it is only asked of hops the chooser
    // actually offers — never of the whole graph.
    // Card TGAP: a soldier on foot is sent into no hub he cannot walk back
    // out of (`Sim::hub_trapped`) unless his goal is in it or he is in one.
    let walker = m.ty == nav::SOLDIER && !m.jet;
    let trap = |h: u16| walker && h != dest && !sim.hub_trapped(cur) && sim.hub_trapped(h);
    let floor = |h: u16| hub_has_floor(sim, g, h, pos.y) && !trap(h);
    let table = |exclude: Option<u16>| g.next_hop(cur, dest, m, arrival_arc, exclude, r);
    let bfs = |on_floor: &dyn Fn(u16) -> bool| g.route_bfs(cur, dest, m, on_floor);
    let dead = |arc: u16| walker && sim.arc_unwalkable(arc, cur);
    // Card FLNK: a soldier on foot with the Flank role (`team::board`) takes
    // the next hub of its flank route while it stands on one of the route's
    // hubs, and off them the port's BFS toward the route's nearest hub
    // (`FlankTable::join`); the hop passes the tests below, else the
    // authored route.
    let flank = st
        .flank
        .route
        .filter(|_| walker && st.flank.to == dest)
        .and_then(|k| {
            let k = usize::from(k);
            sim.flank.next(k, cur).or_else(|| {
                let (mut hops, mut queue) = (Vec::new(), Vec::new());
                g.hop_distances_into(cur, m, &mut hops, &mut queue);
                let j = sim.flank.join(k, &hops)?;
                g.route_bfs(cur, j, m, &floor)
            })
        })
        .filter(|&(a, n)| {
            g.arcs.get(usize::from(a)).is_some_and(|x| x.allows(m)) && floor(n) && !dead(a)
        });
    let hop = match (flank, table(None)) {
        (Some(h), _) => {
            nt.flank += 1;
            Some((h, RouteSource::Flank))
        }
        (None, Some(h)) => Some((h, RouteSource::Table)),
        (None, None) => match bfs(&|_| true) {
            Some(h) => {
                nt.bfs += 1;
                Some((h, RouteSource::Bfs))
            }
            None => None,
        },
    };
    // A hop whose far hub has no floor on this level is REJECTED here, and the
    // alternatives are tried in the design's order: the chooser again with
    // that arc off the table, then the BFS restricted to hubs that DO have a
    // floor, then the PS2's own "any other arc of this hub"
    // (`NavGraph::any_other_arc`, `FUN_002659e8`). Only when all three fail
    // does the unit walk the straight line, and that is `level_unrouted`.
    // Card BNDS: an arc the floor walk found no soldier can walk from here
    // (open air on its line, a far end 8 m up, card TGAP: a wall) is
    // rejected the same way, and so is a hop into a hub a soldier is
    // trapped in (`trap`).
    let hop = match hop {
        Some(((arc, next), _)) if !floor(next) || dead(arc) => {
            nt.level_rejected += 1;
            let retry = table(Some(arc))
                .filter(|&(a, n)| floor(n) && !dead(a))
                .map(|h| (h, RouteSource::Table))
                .or_else(|| {
                    bfs(&floor).filter(|&(a, _)| !dead(a)).map(|h| {
                        nt.bfs += 1;
                        (h, RouteSource::Bfs)
                    })
                })
                .or_else(|| {
                    g.any_other_arc(cur, arrival_arc, Some(arc), m, r)
                        .filter(|&(a, n)| floor(n) && !dead(a))
                        .map(|h| (h, RouteSource::Table))
                });
            if retry.is_none() {
                nt.level_unrouted += 1;
            }
            retry
        }
        h => h,
    };
    let Some(((arc, next), source)) = hop else {
        nt.straight += 1;
        let mut r = last(RouteSource::Straight);
        fill_detour(g, m.ty, pos, &mut r, nt);
        return Some(r);
    };
    // Card HUBP: a soldier on foot walks at its own point of the next hub. A
    // vehicle keeps the centre (OURS, PROVISIONAL: the drivers' laws were
    // measured against centre legs).
    let waypoint = if u.mount.is_none() {
        hub_point(sim, g, me, next, pos, 0.0)
    } else {
        hub_waypoint(sim, g, next, pos.y)
    };
    let mut r = Route {
        dest_hub: dest,
        next_hub: next,
        via_arc: Some(arc),
        waypoint,
        ref_dist: nav::xz_dist(pos, waypoint),
        // The arc's own end: a leg planned from wherever the unit stands (a
        // spawn, a new objective, a re-plan) is walked onto the walkway
        // first ([`leg_point`]). A leg planned on arrival starts at the
        // waypoint just reached ([`advance_route`]).
        from: g.hub(cur).map_or(Vec2::new(pos.x, pos.z), |h| h.xz()),
        source,
        ..Route::default()
    };
    fill_detour(g, m.ty, pos, &mut r, nt);
    r.ref_dist = nav::xz_dist(pos, sub_point(&r, pos));
    Some(r)
}

/// (Re)plan the route toward the COMMANDER'S objective (Phase B) — or, for a
/// unit the commander has not reached yet, by Phase A's own rule.
///
/// The four cases:
///
/// - `Attack(cp)`: route to that post, exactly the way Phase A routed to the
///   nearest one. Unroutable from here (the unit moved between plans, or its
///   storey changed) leaves the ASSIGNMENT alone and drops only the route:
///   the commander sees the same unreachability next period and re-assigns.
///   [`NavTick::unroutable`] counts it.
/// - `Defend(cp)`: the same route, with the unit's own spot selected from
///   assignment time as the last leg's waypoint
///   ([`crate::commander::TeamPlan::spot`], card DEFS) — an authored
///   Cover/Snipe hint within 25 m of the post no other defender has, else a
///   point of its region as an attacker's. D13: a hint's
///   authored `PrimaryStance` becomes `AiState::want_stance`, and a Snipe hint
///   is only offered to a unit whose weapon has a scope.
/// - `Hunt`: the nearest enemy's hub, which is Phase A's fallback promoted to
///   an objective the commander hands out.
/// - `None` (a unit that spawned since the last plan): [`retarget_phase_a`],
///   unchanged, until the next plan reaches it.
pub(super) fn retarget(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    nearest_enemy: Option<Vec3>,
    nt: &mut NavTick,
) {
    let Some(u) = sim.unit(me) else { return };
    // Card WAI2: a bot on foot with a boarding claim routes to its vehicle.
    if u.mount.is_none() {
        if let Some(at) = super::ride::board_point(sim, st) {
            retarget_board(sim, me, st, at, nt);
            return;
        }
    }
    let plan = &sim.commander[usize::from(u.team).min(1)];
    match plan.of(me.index) {
        // Card PARR: a last leg at a post walks to a point of its capture
        // region, not to its centre — but for a defender with a spot.
        Some(crate::commander::Objective::Attack(cp)) => {
            retarget_to_post(sim, me, st, cp, false, nt);
            aim_last_leg(sim, me, st, nt);
        }
        Some(crate::commander::Objective::Defend(cp)) => {
            if !retarget_to_post(sim, me, st, cp, true, nt) {
                aim_last_leg(sim, me, st, nt);
            }
        }
        Some(crate::commander::Objective::Hunt) => {
            st.want_stance = 0;
            retarget_hunt(sim, me, st, nearest_enemy, nt)
        }
        None => {
            retarget_phase_a(sim, me, st, nearest_enemy, nt);
            aim_last_leg(sim, me, st, nt);
            // D13: a unit the commander has not reached yet AND that came out
            // of `retarget_phase_a` with nowhere to walk is IDLE — it takes
            // the hint rule from its own position instead of from a post, so
            // a bot standing in an authored crouch spot crouches in it.
            st.want_stance = if st.route.is_none() {
                let scoped = sim.weapon_classes[u.weapon_class].ai_distance_table();
                let mask = crate::commander::hint_stance(sim, u.soldier.pos, scoped);
                stance_hold(st.want_stance, mask, me, sim.tick)
            } else {
                0
            };
        }
    }
}

/// Route to command post `cp` because the commander said so. `defend` swaps
/// the last leg's aim point for the defence spot from objective assignment.
/// Intermediate arc legs are kept. Answers whether the unit has such a spot.
fn retarget_to_post(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    cp: u32,
    defend: bool,
    nt: &mut NavTick,
) -> bool {
    let Some(u) = sim.unit(me) else { return false };
    let pos = u.soldier.pos;
    st.target_cp = Some(cp);
    let (Some(g), Some(post)) = (sim.nav.as_ref(), sim.cps.get(cp as usize)) else {
        // No graph: walk straight at the post, as before Phase A, and with no
        // graph there are no hints to take a stance from either (D13).
        st.route = None;
        st.want_stance = 0;
        return false;
    };
    // A defender's spot is the commander's (card DEFS: one defender a spot,
    // `commander::pick_spot`), read every think: the unit reaches the post
    // long after the leg that took it there was planned.
    //
    // DEVIATION, measured: the design takes the hint "once within its capture
    // volume". Gating it there makes the waypoint FLIP — inside the volume the
    // aim point is the hint, one step outside it is the post centre, and a hint
    // that sits outside the volume drags the unit back and forth across the
    // boundary forever (BF1 END1 unit 28: 45,587 ticks at full speed 1.5 m from
    // its own post; BF1 KAM1 and KAS2 the same shape). The spot is the
    // destination from the moment the objective is Defend, so there is one
    // aim point per objective and nothing to oscillate between.
    //
    // D13: the spot carries the hint's authored stance mask, and a Snipe hint
    // is only offered to a unit whose weapon passes the AI distance table. A
    // defender with no hint left, and a unit walking to a post it is
    // ATTACKING, has no spot and no stance wish.
    let found = defend
        .then(|| sim.commander[usize::from(u.team).min(1)].spot(me.index))
        .flatten();
    // The hint answers a MASK of allowed stances (`PrimaryStance` is a
    // bitmask); which of them this unit takes is drawn once and held
    // ([`stance_hold`]).
    st.want_stance = stance_hold(st.want_stance, found.map_or(0, |(_, m)| m), me, sim.tick);
    let spot = found.map(|(p, _)| p);
    // The leg we are already on is KEPT (Phase A's rule: re-planning every
    // think would also re-roll the chooser every ten ticks) — unless the
    // defence spot has to be written into it.
    // A driver's destination is its vehicle's ([`post_hub`], card WAI).
    let m = mover(sim, u);
    let cur = start_hub(sim, g, me);
    use crate::vehicle::VehicleKind::{Hover, Walker};
    let rides = u
        .mount
        .and_then(|m| sim.vehicle(m.vehicle))
        .is_some_and(|v| v.kind == Walker || (v.kind == Hover && super::HOVER_NEAREST));
    let want = if rides {
        cur.and_then(|c| post_hub(g, m.ty, c, post, true))
    } else {
        post.nav_hub
    };
    if let Some(r) = st.route {
        if want == Some(r.dest_hub) {
            match spot.filter(|_| r.is_last() && post.nav_hub == Some(r.dest_hub)) {
                Some(p) if r.waypoint != p => {
                    let mut r = r;
                    r.waypoint = p;
                    r.ref_dist = nav::xz_dist(pos, sub_point(&r, pos));
                    st.route = Some(r);
                }
                _ => {}
            }
            return spot.is_some();
        }
    }
    let Some(dest) = want else {
        st.route = None;
        nt.unroutable = 1;
        return false;
    };
    match cur.filter(|&c| g.reachable_by(c, dest, m)) {
        Some(c) => {
            // Card FLNK: the hub an attack on foot is planned from (a flanker
            // re-planned on its way keeps its start).
            if defend || u.mount.is_some() {
                st.flank = super::FlankState::default();
            } else if st.flank.to != dest {
                st.flank = super::FlankState {
                    from: c,
                    to: dest,
                    ..Default::default()
                };
            } else if st.flank.route.is_none() {
                st.flank.from = c;
            }
            st.route = plan_leg(sim, st, me, c, dest, None, nt);
            if let (Some(p), Some(mut r)) = (spot, st.route) {
                if r.is_last() && post.nav_hub == Some(r.dest_hub) {
                    r.waypoint = p;
                    r.ref_dist = nav::xz_dist(pos, sub_point(&r, pos));
                    st.route = Some(r);
                }
            }
        }
        None => {
            // Report it and leave the assignment for the commander to drop.
            st.route = None;
            nt.unroutable = 1;
        }
    }
    spot.is_some()
}

/// Card WAI2: a bot with a boarding claim walks the graph to the hub its
/// vehicle stands at, and from there (`think`: on the last leg) straight at
/// it — a claimer that walked straight from where it claimed slid along a
/// wall 17 m short of BF2 Hoth's snow AT-ST until its claim ran out. The
/// post is kept (`ride::board_goal` asks whether the vehicle still routes to
/// it). No graph, no hub under the vehicle or no way to it: no route, and
/// the straight walk as before. OURS, PROVISIONAL (boarding is the port's).
pub(super) fn retarget_board(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    at: Vec3,
    nt: &mut NavTick,
) {
    let (Some(g), Some(u)) = (sim.nav.as_ref(), sim.unit(me)) else {
        return;
    };
    let m = mover(sim, u);
    let dest = g
        .unit_hub_on(at, m, &|h| hub_on_level(sim, g, h, at.y))
        .map(|h| level_hub(sim, g, h, at));
    match (start_hub(sim, g, me), dest) {
        (Some(c), Some(d)) if g.reachable_by(c, d, m) => {
            if st.route.is_none_or(|r| r.dest_hub != d) {
                st.route = plan_leg(sim, st, me, c, d, None, nt);
            }
        }
        _ => st.route = None,
    }
}

/// A hunter's LAST leg walks at the enemy itself, not at the enemy's hub
/// ([`retarget_hunt`]): `false` restores the pre-STK2 rule, where a hunter
/// that reached the hub stood on it for the rest of the round with its quarry
/// out of sight — BF1 YAV1's last team-1 unit on the aqueduct deck, 36 m from
/// and 10 m above the last team-0 unit (queue STK2, NO OUTCOME at 60,000
/// ticks). PROVISIONAL: a port rule under the commander's Hunt, which is
/// itself the port's design (D6), not a PS2 behaviour in analysis/.
pub const HUNT_AT_ENEMY: bool = true;

/// HUNT: the nearest enemy's hub, resolved at the ENEMY'S level; on the last
/// leg, the enemy's own position ([`HUNT_AT_ENEMY`]).
///
/// DEVIATION (and a safety net): a Hunt that resolves to NOTHING — no enemy
/// alive, no hub under it, or no way to it — falls back to
/// [`retarget_phase_a`] rather than leaving the unit with no objective and no
/// route at all. Standing still forever is never the right answer to "there is
/// nobody to hunt", and Phase A's straight walk at the nearest post is what
/// the unit did before the commander existed.
fn retarget_hunt(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    nearest_enemy: Option<Vec3>,
    nt: &mut NavTick,
) {
    let Some(u) = sim.unit(me) else { return };
    let dest = match (sim.nav.as_ref(), nearest_enemy) {
        (Some(g), Some(e)) => {
            let m = mover(sim, u);
            g.unit_hub_on(e, m, &|h| hub_on_level(sim, g, h, e.y))
                .map(|h| level_hub(sim, g, h, e))
        }
        _ => None,
    };
    let g = sim.nav.as_ref();
    let cur = g.and_then(|g| start_hub(sim, g, me));
    match (g, cur, dest) {
        (Some(g), Some(c), Some(d)) if g.reachable_by(c, d, mover(sim, u)) => {
            st.target_cp = None;
            if st.route.is_none_or(|r| r.dest_hub != d) {
                st.route = plan_leg(sim, st, me, c, d, None, nt);
            }
            // (An armed walker stops at the end of its graph: card WAI2,
            // `ride::at_drop`.)
            let walker = u
                .mount
                .and_then(|m| sim.vehicle(m.vehicle))
                .is_some_and(|v| v.kind == crate::vehicle::VehicleKind::Walker)
                && sim.seat_look(me).is_some();
            if HUNT_AT_ENEMY && !walker {
                if let (Some(r), Some(e)) = (st.route.as_mut(), nearest_enemy) {
                    if r.is_last() {
                        r.waypoint = e;
                    }
                }
            }
        }
        _ => {
            nt.unroutable = 1;
            retarget_phase_a(sim, me, st, nearest_enemy, nt);
        }
    }
}

/// Phase A's own objective rule, kept verbatim for a unit the commander has
/// not assigned yet — the nearest post not owned by my team, an unroutable one
/// skipped for the nearest routable one, and the hunt fallback when there is
/// no reachable post at all.
fn retarget_phase_a(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    nearest_enemy: Option<Vec3>,
    nt: &mut NavTick,
) {
    let Some(u) = sim.unit(me) else { return };
    let mine = |c: &crate::cp::CommandPost| c.owner == Some(u.team);
    let pos = u.soldier.pos;
    let nearest = |pred: &dyn Fn(&crate::cp::CommandPost) -> bool| {
        sim.cps
            .iter()
            .enumerate()
            .filter(|(_, c)| pred(c))
            .min_by(|(_, a), (_, b)| {
                let da = (a.def.pos - pos).length_squared();
                let db = (b.def.pos - pos).length_squared();
                da.total_cmp(&db)
            })
            .map(|(i, _)| i as u32)
    };
    let Some(g) = sim.nav.as_ref() else {
        // No graph: exactly the pre-Phase-A placeholder.
        st.target_cp = nearest(&|c| !mine(c)).or_else(|| nearest(&|_| true));
        st.route = None;
        return;
    };

    // Capturable posts, nearest first; index breaks distance ties.
    let mut cands: Vec<(f32, u32)> = sim
        .cps
        .iter()
        .enumerate()
        .filter(|(_, c)| !mine(c))
        .map(|(i, c)| ((c.def.pos - pos).length_squared(), i as u32))
        .collect();
    cands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    // The first choice has not changed and we are already walking to it: keep
    // the leg we are on (re-planning every think would also re-roll the
    // chooser every ten ticks).
    if let (Some(t), Some(r)) = (st.target_cp, st.route) {
        if cands.first().is_some_and(|&(_, i)| i == t)
            && sim.cps.get(t as usize).and_then(|c| c.nav_hub) == Some(r.dest_hub)
        {
            return;
        }
    }

    // The START hub is [`start_hub`]'s, as for every other objective (card
    // DAGS): this is the rule that plans a bot's FIRST leg after a spawn, and
    // with the bare XZ locate a bot spawned on the ground over BF2 Dagobah's
    // cave was put on the cave's arc, 8 m under its feet, and walked its roof
    // into a prop for 150 s. The enemy's hub, the HUNT destination, is
    // resolved at the ENEMY's level (PLAN "PHASE A.2 DESIGN" decision 3).
    let m = mover(sim, u);
    if let Some(cur) = start_hub(sim, g, me) {
        for &(_, i) in &cands {
            let Some(dest) = sim.cps[i as usize].nav_hub else {
                continue;
            };
            if !g.reachable_by(cur, dest, m) {
                continue;
            }
            st.target_cp = Some(i);
            st.route = plan_leg(sim, st, me, cur, dest, None, nt);
            return;
        }
        // HUNT: nothing capturable we can reach, so go to the nearest enemy's
        // hub (design decision 6 — the RHN1 "camp on the citadel" fix).
        if let Some(e) = nearest_enemy {
            if let Some(dest) = g.unit_hub_on(e, m, &|h| hub_on_level(sim, g, h, e.y)) {
                if g.reachable_by(cur, dest, m) {
                    st.target_cp = None;
                    st.route = plan_leg(sim, st, me, cur, dest, None, nt);
                    return;
                }
            }
        }
    }
    // Off the graph entirely: the placeholder's own choice, walked straight.
    st.target_cp = nearest(&|c| !mine(c)).or_else(|| nearest(&|_| true));
    st.route = None;
}

/// Route advance, now that an ENGAGED unit walks its route too (design
/// decision 4): first the detour corners behind us (XZ, CORNER_ARRIVE),
/// then the hub itself at its own radius (at least `min_arrive` — a hover
/// driver's ForwardSpeed × 0.5, card VAI), at which the chooser picks the
/// leg after it.
pub(super) fn advance_route(
    sim: &Sim,
    me: UnitHandle,
    st: &mut AiState,
    pos: Vec3,
    min_arrive: f32,
    nt: &mut NavTick,
) {
    if let (Some(g), Some(mut r)) = (sim.nav.as_ref(), st.route) {
        let mut moved_on = false;
        // Card OBSV: a corner or a waypoint a vehicle stands on is reached at
        // the vehicle's side ([`super::avoid::at_foot_of`]) — by a soldier on
        // foot; a driver's own vehicle stands on every point it comes to.
        // Card HJAM: a hover's driver reaches a corner at its node radius.
        let on_foot = sim.unit(me).is_some_and(|u| u.mount.is_none());
        let reach = corner_arrive(sim, me, min_arrive);
        while r.corner().is_some_and(|c| {
            (Vec2::new(pos.x, pos.z) - c).length() <= reach
                || (on_foot && super::avoid::at_foot_of(sim, c, pos, CORNER_ARRIVE))
        }) {
            r.detour_at = r.detour_at.saturating_add(1);
            moved_on = true;
        }
        // Card OBST2: a corner walk that filled the leg's array may have been
        // cut short (`fill_detour`): from its last corner the rest is
        // searched for. (BF2 Mustafar's leg 30 -> 3 goes round a pit by 11
        // corners; after the sixth the walk went straight at the waypoint,
        // into the pit: 23 recycles there in 8 rounds.) Not on a leg the
        // stuck window re-planned: that one dropped its corners on purpose.
        if moved_on
            && r.corner().is_none()
            && usize::from(r.detour_len) == DETOUR_MAX
            && r.stuck_run == 0
        {
            if let Some(u) = sim.unit(me) {
                fill_detour(g, mover(sim, u).ty, pos, &mut r, nt);
            }
        }
        if moved_on {
            // A new sub-waypoint opens a new progress window: the old
            // reference distance was measured to the corner just passed.
            r.ref_dist = nav::xz_dist(pos, sub_point(&r, pos));
            r.stuck_ticks = 0;
        }
        st.route = Some(r);
        let at_foot = on_foot
            && r.corner().is_none()
            && sim
                .nav_floor(r.next_hub)
                .is_none_or(|f| (f - pos.y).abs() <= HUB_LEVEL_TOL)
            && super::avoid::at_foot_of(
                sim,
                Vec2::new(r.waypoint.x, r.waypoint.z),
                pos,
                ARRIVE_MIN_RADIUS,
            );
        if !r.is_last() && (at_foot || hub_reached(sim, g, &r, pos, min_arrive)) {
            if let Some(mut next) = plan_leg(sim, st, me, r.next_hub, r.dest_hub, r.via_arc, nt) {
                // Side-step state and the stuck run survive the hop. The
                // progress window does not (card MOUTH): its reference
                // distance is the new leg's, measured this tick, so it opens
                // anew as it does past a corner. (Until then its tick count
                // was carried: a hop in the window's last ticks closed it on
                // a tick or two of the new leg, under [`STUCK_PROGRESS`] at a
                // run — a third to a half of all failed windows on BF2 Kamino
                // and Mygeeto, each a re-plan and a second's side-step for a
                // bot at full speed. At a ramp's foot that side-step put BF1
                // Kamino's bots outside the rail.)
                next.sidestep = r.sidestep;
                next.stuck_run = r.stuck_run;
                // Card HUBP: the PS2's leg — from the waypoint just reached
                // to the unit's point of the next hub.
                if next.via_arc.is_some() {
                    next.from = Vec2::new(r.waypoint.x, r.waypoint.z);
                }
                st.route = Some(next);
            }
        }
    }
}

/// Is the leg's hub reached? The PS2's rule (RE_LOG "Arrival and locate",
/// BF1 `FUN_0026a780` / `FUN_0026a878`, BF2 the same shape): within the
/// mover's node radius of the WAYPOINT in XZ, or past the waypoint's plane
/// along the leg — "the hub sphere never decides arrival". The port used the
/// hub's authored radius (12–15 m on BF2 KAM1's platforms, 2–4 m at its
/// walkway mouths), so a unit turned for the next hub metres short of this
/// one and off the next arc's line: at a ramp mouth that is the rail beside
/// the ramp (the maintainer, 2026-10-01: 20 of 32 units stuck there after 200 s).
/// The node radius is [`ARRIVE_MIN_RADIUS`] (PROVISIONAL: the PS2's is 1.0 m
/// for a soldier — measured by card MOVR2, it passes alone and with the
/// PS2's leg aim it doubles the "lost" deaths: bot-design 9, D18), or the
/// caller's `min_arrive` (a hover driver's). The leg
/// is the route's own line ([`Route::from`] to the waypoint, card HUBP), and
/// the plane is the waypoint's - also while a corner walk is on, which the
/// PS2's mover takes node by node (measured and NOT kept, card OBST2: the
/// plane of the path's last stretch, and none while a corner is ahead -
/// The maintainer's Kamino scene 7 -> 11 of about 400 trails over 6 reversals, no
/// gain over 24 seeds of four maps). OURS: the stuck unit's fallback to the
/// hub's radius.
///
/// Card BNDS: a hub is reached ON ITS LEVEL — a unit under a walkway's hub
/// (BF2 Kamino's hub 123 at 63.7 over a deck at 51.5) is not at it, and the
/// leg after it cannot be walked from there. Past a stuck window the radius
/// fallback below still lets it go.
fn hub_reached(sim: &Sim, g: &NavGraph, r: &crate::ai::Route, pos: Vec3, min_arrive: f32) -> bool {
    let d = nav::xz_dist(pos, r.waypoint);
    let level = sim
        .nav_floor(r.next_hub)
        .is_none_or(|f| (f - pos.y).abs() <= HUB_LEVEL_TOL);
    if d <= ARRIVE_MIN_RADIUS.max(min_arrive) && level {
        return true;
    }
    // A unit that has spent a stuck window on this leg and stands inside the
    // hub's authored radius has arrived as it used to: a hub whose centre
    // cannot be stood on (BF2 POL1's hub 152, 3.0 m away for 51,600 ticks)
    // must not hold it for good.
    if r.stuck_run > 0 && d <= g.radius(r.next_hub) {
        return true;
    }
    if r.via_arc.is_none() {
        return false;
    }
    let b = Vec2::new(r.waypoint.x, r.waypoint.z);
    level && (b - r.from).dot(Vec2::new(pos.x, pos.z) - b) > 0.0
}

#[cfg(test)]
mod tests;
