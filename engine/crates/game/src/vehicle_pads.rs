//! A map's vehicle spawn pads, built for the sim (card VPAD,
//! analysis/re-vehicles.md §7 [O]): every `com_item_vehicle_spawn` instance
//! of the manifest, its props read as the PS2's setter reads them (B1
//! FUN_002af778), its class per team resolved against the mission's two
//! factions and the [`ClassDb`] — `core::vehicle_pad` runs the rest.
//!
//! The class table (B1 DAT_005183a0 = B2 DAT_005feeb0, 7 rows × 3 hashes, one
//! row per side): a prop whose key is a side's `Class<SideName>` goes to the
//! FIRST team of that side; `Class<Side>Atk` only to team 1, `Class<Side>Def`
//! only to team 2 (B1:239857-239875). Teams are tried in order 0 (neutral),
//! 1, 2; the port fields teams 1 and 2 (port teams 0 and 1), so the neutral
//! and locals rows spawn nothing here. A class the level does not carry is
//! null on the PS2 and spawns nothing (BF2 YAV1's `re_hover_barcspeeder`,
//! named by YAV1.LVL alone) — [`PadBuild::absent`]. A mission can load a
//! side beyond its two factions' ([`mission_side_files`]): BF1 NAB1's reads
//! SIDE\gun.lvl, whose `gun_walk_kaadu` three pads key for the defenders
//! (card KAAD; 3 kaadus live in the PS2's NAB1 RAM, one ridden).
//!
//! The building turrets (card VTURB, §8) come from the same manifest: every
//! instance whose class is an armed building with a gunner seat becomes a
//! `VehicleKind::Turret` at the instance ([`PadBuild::turrets`]) — its
//! gunner ring, guns and hull pool, never moving, no pad. The world keeps
//! drawing (and colliding with) the building's model, so its class has no
//! viewer geometry; the world's own draws of it are turned to the seat's gun
//! (card VTURD, `crate::viewer::turret_pose`, joined by the instance's
//! translation). PROVISIONAL: the instance's
//! `Team` is its `home_team` and, like any vehicle that empties to neutral,
//! it starts open to both teams (every BF1 turret instance authors Team 0;
//! how the PS2 gates a building's seat by team is untraced).

use crate::classdb::{fnv1a_lowercase, ClassDb};
use crate::vehicle_class;
use core::vehicle::gun::{Aimer, GunSetup, GunWeapon, MAX_SEAT_WEAPONS};
use core::vehicle::{BlastSetup, SeatSetup, VehicleKind, VehicleSpec};
use core::vehicle_pad::{PadClass, PadDef, DECAY_TIME, SPAWN_TIME};
use core::weapon::ExplosionClass;
use glam::Vec3;
use manifest::{Manifest, ManifestHardpoint, ManifestInstance};
use std::collections::HashMap;

/// The pad's class (BASE `vehiclespawn`).
pub const PAD_CLASS: &str = "com_item_vehicle_spawn";

/// A vehicle's radius for the decay's player-proximity test (the PS2 reads
/// the entity's collision sphere, FUN_001645a0). PROVISIONAL: one number for
/// every class until the vehicle bounds are read.
pub const VEHICLE_RADIUS: f32 = 3.0;

/// The side-table row of a port faction key: (`Class<SideName>`, the
/// `Class<Side>` stem of the Atk / Def keys).
pub fn side_row(faction: &str) -> Option<(&'static str, &'static str)> {
    Some(match faction {
        "all" => ("Alliance", "All"),
        "imp" => ("Empire", "Imp"),
        "rep" => ("Republic", "Rep"),
        "cis" => ("Cis", "Cis"),
        _ => return None,
    })
}

/// The class RONs of the sides a map's missions load beyond their two
/// factions' (`ReadDataFile("SIDE\\<x>.lvl", …)` in the mission lua), for
/// the classes its pads name. Loaded after [`crate::roster::class_files`],
/// so (first definition wins) they add classes and shadow none.
///
/// Only the sides a pad needs: BF1 NAB1's `nab1c`, `nab1i` and `nab1c_h`
/// read SIDE\gun.lvl with `gun_walk_kaadu`. The other locals sides the BF1
/// missions read (des, ewk, geo, gar, wok) carry no class a pad keys.
///
/// BF2's building turrets (card VTUR2): the conquest missions of these 11
/// maps read SIDE\tur.lvl (`cor1`, `end1`, `geo1`, `hot1`, `kam1`, `kas2`,
/// `myg1`, `nab2`, `pol1`, `tat2`, `yav1` `*_con.luac`; DAG1, DEA1, FEL1,
/// MUS1, TAN1 and UTA1's do not), and the map bake carries the geometry of
/// the TUR classes its worlds place (`bake::map`).
pub fn mission_side_files(game: &str, map: &str) -> &'static [&'static str] {
    match (game, map) {
        ("bf1", "bf1_nab1") => &["bf1_gun.ron"],
        (
            "bf2",
            "bf2_cor1" | "bf2_end1" | "bf2_geo1" | "bf2_hot1" | "bf2_kam1" | "bf2_kas2"
            | "bf2_myg1" | "bf2_nab2" | "bf2_pol1" | "tat2" | "bf2_yav1",
        ) => &["bf2_tur.ron"],
        _ => &[],
    }
}

/// Which port team a pad prop's key assigns its class to, for a battle of
/// `factions` (port team 0 = PS2 team 1), or `None` (not a class key, or a
/// key for a side the battle does not field).
pub fn class_key_team(key: u32, factions: &[String; 2]) -> Option<u8> {
    // PS2 team 0 is the Neutral side: `ClassNeutral` in all three columns.
    if key == fnv1a_lowercase("ClassNeutral") {
        return None;
    }
    for (t, f) in factions.iter().enumerate() {
        let (name, stem) = side_row(f)?;
        let col = if t == 0 { "Atk" } else { "Def" };
        if key == fnv1a_lowercase(&format!("Class{name}"))
            || key == fnv1a_lowercase(&format!("Class{stem}{col}"))
        {
            return Some(t as u8);
        }
    }
    None
}

/// Which integrator a vehicle base drives (re-vehicles §2-§4).
pub fn vehicle_kind(base: &str) -> Option<VehicleKind> {
    Some(match base.to_ascii_lowercase().as_str() {
        "hover" | "treaded" | "commandhover" => VehicleKind::Hover,
        "walker" | "commandwalker" => VehicleKind::Walker,
        "flyer" | "commandflyer" | "carrier" => VehicleKind::Flyer,
        "armedbuilding" | "armedbuildingdynamic" => VehicleKind::Turret,
        _ => return None,
    })
}

/// The pads of one map and the vehicle classes they name.
#[derive(Debug, Default)]
pub struct PadBuild {
    pub pads: Vec<PadDef>,
    /// The vehicle class table: `VehicleSpec::class` indexes it.
    pub classes: Vec<String>,
    /// Each class's `GeometryName` (the model the viewer draws, card
    /// VDRAW), parallel to `classes`.
    pub geometry: Vec<Option<String>>,
    /// Each class's seat ring with its cameras (card VSEAT), parallel to
    /// `classes`; filled by [`install`].
    pub seats: Vec<Vec<vehicle_class::SeatSpec>>,
    /// Each class's `EngineSound` and `TurnOffTime` (card VSND), parallel
    /// to `classes`; filled by [`install`].
    pub engines: Vec<Option<crate::engine_sound::EngineClass>>,
    /// Each class's `Label` (BF1 names a command walker's post on the spawn
    /// screen by it, re-command-walker-post §5), parallel to `classes`;
    /// filled by [`install`].
    pub labels: Vec<Option<String>>,
    /// Each class's seats' rider hangs (card VRIDE), parallel to `seats`;
    /// filled by [`install`] from the baked `skel` nodes.
    pub riders: Vec<Vec<vehicle_class::RiderHang>>,
    /// (pad, class) for every class prop of a fielded side that names no
    /// vehicle class the db carries — the PS2 spawns nothing there.
    pub absent: Vec<(String, String)>,
    /// The building turrets, in manifest order (card VTURB): spawned by
    /// [`install`] before any pad vehicle.
    pub turrets: Vec<TurretDef>,
}

/// One armed-building instance as a vehicle: its spec (at the instance,
/// `yaw` its turn) and the instance's full rotation, which the vehicle takes
/// so its fire points sit where the world draws the model.
#[derive(Clone, Debug)]
pub struct TurretDef {
    pub name: String,
    /// The instance's index in the manifest (its bodies' `Body::instance`).
    pub instance: u32,
    pub spec: VehicleSpec,
    pub rot: glam::Quat,
}

/// A building turret's acquisition sphere (card HTGT, `Vehicle::aim_sphere`):
/// the box around its building's bodies (`Body::instance`), as a centre in
/// the turret's frame (at `pos`, turned by `rot`) and half the box's
/// diagonal; `None` when the building has no body. The PS2 picks the
/// building by its bounds (`analysis/re-target-hud.md` §2).
pub fn building_sphere(
    world: &core::collision::CollisionWorld,
    instance: u32,
    pos: Vec3,
    rot: glam::Quat,
) -> Option<(Vec3, f32)> {
    let mut b = core::collision::Aabb::empty();
    for body in world
        .bodies()
        .iter()
        .filter(|b| b.instance == Some(instance))
    {
        b.add(body.aabb.min);
        b.add(body.aabb.max);
    }
    (!b.is_empty()).then(|| {
        let c = (b.min + b.max) * 0.5;
        (rot.inverse() * (c - pos), (b.max - b.min).length() * 0.5)
    })
}

/// The instance matrix (rows = the model's axes in the world, as
/// `viewer::instance_matrix` draws it) as a rotation; `None` when it is not
/// a proper rotation.
fn instance_rot(r: &[[f32; 3]; 3]) -> Option<glam::Quat> {
    let m = glam::Mat3::from_cols(r[0].into(), r[1].into(), r[2].into());
    (m.determinant() > 0.5).then(|| glam::Quat::from_mat3(&m).normalize())
}

/// A port team from an instance's `Team` (PS2 1, 2 → 0, 1; else none).
fn port_team(inst: &ManifestInstance) -> Option<u8> {
    prop(inst, "Team")
        .and_then(|s| s.trim().parse::<i32>().ok())
        .and_then(|t| match t {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        })
}

fn prop<'a>(inst: &'a ManifestInstance, key: &str) -> Option<&'a str> {
    let h = fnv1a_lowercase(key);
    inst.props
        .iter()
        .rev()
        .find(|(k, _)| *k == h)
        .map(|(_, v)| v.as_str())
}

/// The instance's turn about +Y (identity = 0, the soldier's convention:
/// positive turns left). PROVISIONAL until a vehicle is drawn at its pad.
fn yaw_of(r: &[[f32; 3]; 3]) -> f32 {
    core::fmath::atan2(r[0][2], r[0][0])
}

/// The spec a pad builds for `class`, or `None` if the db has no vehicle by
/// that name.
fn spec_for(db: &ClassDb, class: &str, index: u32) -> Option<VehicleSpec> {
    let kind = vehicle_kind(db.engine_base(class)?)?;
    let seats = vehicle_class::seats(db, class)
        .iter()
        .map(|s| s.kind)
        .collect();
    Some(VehicleSpec {
        kind,
        class: index,
        home_team: None,
        neutral_when_empty: vehicle_class::neutral_when_empty(db, class),
        spawn_pad: None,
        pos: Vec3::ZERO,
        yaw: 0.0,
        max_health: db.prop_f32(class, "MaxHealth").unwrap_or(0.0),
        health_type: vehicle_class::health_type(db, class),
        seats,
    })
}

/// Build every pad of `m` for a battle of `factions` (port team order).
/// `cp_index` maps a `ControlZone` name to its `Sim::cps` index. A BF2 map
/// reads `SpawnCount` (BF1's setter has no such branch).
pub fn build(
    m: &Manifest,
    db: &ClassDb,
    factions: &[String; 2],
    cp_index: impl Fn(&str) -> Option<usize>,
) -> PadBuild {
    let bf2 = m.source_game.eq_ignore_ascii_case("bf2");
    let mut out = PadBuild::default();
    for inst in m.instances.iter().filter(|i| i.class == PAD_CLASS) {
        let mut classes: [Option<PadClass>; 2] = [None, None];
        // In prop order: a later key for the same team slot wins, as the
        // setter's store does.
        for (k, v) in &inst.props {
            let Some(t) = class_key_team(*k, factions) else {
                continue;
            };
            let name = v.trim().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            let index = out
                .classes
                .iter()
                .position(|c| *c == name)
                .unwrap_or(out.classes.len());
            classes[t as usize] = match spec_for(db, &name, index as u32) {
                Some(spec) => {
                    if index == out.classes.len() {
                        out.geometry.push(
                            db.prop(&name, "GeometryName")
                                .map(|g| g.trim().to_owned())
                                .filter(|g| !g.is_empty()),
                        );
                        out.classes.push(name);
                    }
                    Some(PadClass {
                        spec,
                        radius: VEHICLE_RADIUS,
                        // Filled by `install`, which has the side bakes.
                        landed_height: None,
                    })
                }
                None => {
                    out.absent.push((inst.name.clone(), name));
                    None
                }
            };
        }
        // The load-time team (+0xfc) as a port team (PS2 1, 2 → 0, 1; 0, −1
        // and the locals are no team the port fields). `Team` (atoi) comes
        // first on every stock pad and `ControlZone` then copies its post's
        // team (B1:239915-239968) — the post's WORLD `Team`: the mission
        // re-owns posts after load, and the first tick's follow then
        // reloads the countdown (B1:240799). [H]: the copy needs the post to
        // exist when the pad's props run; 21 of 313 stock pads come before
        // their post in the manifest, and the port copies for them too.
        let zone = prop(inst, "ControlZone").filter(|z| !z.trim().is_empty());
        let post = zone.and_then(|z| {
            m.instances
                .iter()
                .find(|i| i.name.eq_ignore_ascii_case(z.trim()))
        });
        let team = port_team(post.unwrap_or(inst));
        let f = |key: &str, default: f32| {
            prop(inst, key)
                .and_then(|s| s.trim().parse::<f32>().ok())
                .unwrap_or(default)
        };
        out.pads.push(PadDef {
            name: inst.name.clone(),
            pos: Vec3::from(inst.translation),
            yaw: yaw_of(&inst.rotation),
            cp: zone.and_then(|z| cp_index(z.trim())),
            team,
            spawn_time: f("SpawnTime", SPAWN_TIME),
            decay_time: f("DecayTime", DECAY_TIME),
            spawn_count: if bf2 {
                f("SpawnCount", 1.0).max(1.0) as u32
            } else {
                1
            },
            classes,
        });
    }
    add_turrets(&mut out, &m.instances, db, bf2);
    out
}

/// The building turrets of `instances` (§8) into `out`: every instance whose
/// class (matched ignoring case, [`ClassDb::resolve`]) is an armed building
/// with at least one gunner seat — BF2 POL1's three shields are armed
/// buildings with none — at the instance, sharing `out`'s class table with
/// no viewer geometry. With `unbuilt_zones` (BF2) a buildable turret (a
/// `manifest::BUILD_ZONE_PROP`: `tur_bldg_laser` and kin) is not spawned —
/// it starts unbuilt. BF1's `com_weap_gunturret` authors the prop too, and
/// whether it starts built is open (card BZONE1), so BF1 keeps spawning it.
pub fn add_turrets(
    out: &mut PadBuild,
    instances: &[ManifestInstance],
    db: &ClassDb,
    unbuilt_zones: bool,
) {
    for (ii, inst) in instances.iter().enumerate() {
        let Some(name) = db.resolve(&inst.class).map(str::to_owned) else {
            continue;
        };
        // A buildable turret stands unbuilt at battle start: the world
        // draws its build zone (`manifest::BUILD_ZONE_PROP`, card VTUR2).
        if !vehicle_class::is_turret(db, &name)
            || (unbuilt_zones && db.prop_hash(&name, manifest::BUILD_ZONE_PROP).is_some())
        {
            continue;
        }
        let index = out
            .classes
            .iter()
            .position(|c| *c == name)
            .unwrap_or(out.classes.len());
        let Some(mut spec) = spec_for(db, &name, index as u32) else {
            continue;
        };
        if spec.seats.is_empty() {
            continue;
        }
        if index == out.classes.len() {
            out.classes.push(name);
            out.geometry.push(None);
        }
        spec.pos = Vec3::from(inst.translation);
        spec.yaw = yaw_of(&inst.rotation);
        spec.home_team = port_team(inst);
        out.turrets.push(TurretDef {
            name: inst.name.clone(),
            instance: ii as u32,
            rot: instance_rot(&inst.rotation).unwrap_or(core::vehicle::hover::quat_y(-spec.yaw)),
            spec,
        });
    }
}

/// Load the vehicle classes a battle of `factions` can spawn (the map's own
/// classes first, then the side files, then [`mission_side_files`],
/// `ClassDb::load` order), build `m`'s
/// pads with each `ControlZone` matched to a `sim.cps` post by name, and add
/// them to `sim` with each class's hover numbers (`Sim::hover_params`), then
/// spawn the map's building turrets ([`add_turrets`], card VTURB). Call AFTER
/// the posts are in (`WalkSim::add_posts`) and with the mission's conquest
/// layer merged (`Scene::apply_mode_layer`) — and BEFORE
/// `WalkSim::opening_wave`, which spawns at the command walkers these pads
/// are about to build (card CWSP).
pub fn install(
    sim: &mut core::sim::Sim,
    m: &Manifest,
    map_dir: &std::path::Path,
    classes_dir: &std::path::Path,
    factions: &[String; 2],
) -> anyhow::Result<PadBuild> {
    let mut db = ClassDb::default();
    db.load_map(map_dir)?;
    let game = m.source_game.to_ascii_lowercase();
    let f: Vec<&str> = factions.iter().map(String::as_str).collect();
    let map = map_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extra = mission_side_files(&game, &map).iter().copied();
    for file in crate::roster::class_files(&game, &f)
        .into_iter()
        .chain(extra)
    {
        let p = classes_dir.join(file);
        if pak::exists(&p) {
            db.load(&p)?;
        }
    }
    let names: Vec<String> = sim.cps.iter().map(|c| c.def.name.clone()).collect();
    let mut b = build(m, &db, factions, |zone| {
        names.iter().position(|n| n.eq_ignore_ascii_case(zone))
    });
    // Each flyer class's LandedHeight (card VREST), before any pad spawns.
    let floors = vis_floors(map_dir, &game);
    let landed: Vec<Option<f32>> = b
        .classes
        .iter()
        .map(|c| vehicle_class::landed_height(&db, c, |k| floors.get(k).copied()))
        .collect();
    for c in b
        .pads
        .iter_mut()
        .flat_map(|p| p.classes.iter_mut())
        .flatten()
    {
        c.landed_height = landed[c.spec.class as usize];
    }
    for p in &b.pads {
        sim.add_vehicle_pad(p.clone());
    }
    // The health / ammo / vehicle droids (card PWST): the same db, the same
    // mode-merged instances.
    crate::roster::add_stations(sim, &m.instances, &db, game == "bf2");
    for t in &b.turrets {
        let h = sim.spawn_vehicle(&t.spec);
        let sphere = building_sphere(&sim.collision, t.instance, t.spec.pos, t.rot);
        if let Some(v) = sim.vehicle_mut(h) {
            v.rot = t.rot;
            v.instance = Some(t.instance);
            v.aim_sphere = sphere;
        }
    }
    // The hover numbers per class (card HOVR): `Sim::tick_hovers` moves
    // every pad vehicle whose class is a hover.
    // Its hull bodies (card HHUL) are the class's named primitives, out of
    // its geometry's baked collision.
    sim.hover_params = b
        .classes
        .iter()
        .map(|c| {
            vehicle_class::HoverClass::from_db(&db, c).map(|h| core::vehicle::hover::HoverParams {
                bf1_ai: game == "bf1",
                hull: hover_hull(m, map_dir, &game, c, &h),
                ..h.params()
            })
        })
        .collect();
    // The walker numbers per class (card WALK), with each class's baked
    // anim table and prim origins from the side bakes.
    let walkers = walker_bakes(map_dir, &game);
    sim.walker_params = b
        .classes
        .iter()
        .map(|c| {
            vehicle_class::WalkerClass::from_db(&db, c)
                .map(|w| w.params(walkers.get(&c.to_ascii_lowercase())))
        })
        .collect();
    // What the use input and the exit read (card VSEAT).
    b.seats = b
        .classes
        .iter()
        .map(|c| vehicle_class::seats(&db, c))
        .collect();
    b.engines = b
        .classes
        .iter()
        .map(|c| crate::engine_sound::EngineClass::from_db(&db, c))
        .collect();
    b.labels = b
        .classes
        .iter()
        .map(|c| {
            db.prop(c, "Label")
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
        })
        .collect();
    // Each class's death explosion and blast sphere (card VDTH).
    let spheres = spheres(map_dir, &game);
    sim.blast_setup = b
        .classes
        .iter()
        .map(|c| {
            Some(blast_setup(
                &db,
                c,
                spheres.get(&c.to_ascii_lowercase()).copied(),
            ))
        })
        .collect();
    // Each class's hull (card VHULL), before anything spawns from a pad; a
    // building turret has none — its building is the world's own body.
    sim.hull_parts = hull_parts(m, map_dir, &game, &b.classes, |c| {
        vehicle_class::is_turret(&db, c)
    });
    sim.sync_hulls();
    let nodes = hardpoints(m, map_dir, &game);
    b.riders = b
        .classes
        .iter()
        .zip(&b.seats)
        .map(|(c, ring)| {
            let hp = nodes
                .get(&c.to_ascii_lowercase())
                .map_or(&[][..], Vec::as_slice);
            ring.iter()
                .map(|s| vehicle_class::RiderHang::of(s, hp))
                .collect()
        })
        .collect();
    sim.seat_setup = b
        .classes
        .iter()
        .zip(&b.seats)
        .map(|(c, ring)| {
            let mut setup = seat_setup(&db, c, ring, &game);
            let empty = Vec::new();
            let hp = nodes.get(&c.to_ascii_lowercase()).unwrap_or(&empty);
            let building = vehicle_class::is_turret(&db, c);
            setup.guns = ring
                .iter()
                .map(|s| {
                    let mut g = seat_gun(sim, &db, s, hp)?;
                    if building && !g.pilot {
                        turret_law(&mut g, s, game == "bf2");
                    }
                    Some(g)
                })
                .collect();
            Some(setup)
        })
        .collect();
    Ok(b)
}

/// Every vehicle class's baked `skel` nodes (card VTUR), by lowercase class
/// name: the map's own classes (the TAT1 skiff), then each side's
/// `side_<game>/vehicles/*/vehicles.ron` beside the map's bake — the first
/// that has the class wins.
pub fn hardpoints(
    m: &Manifest,
    map_dir: &std::path::Path,
    game: &str,
) -> HashMap<String, Vec<ManifestHardpoint>> {
    let mut out: HashMap<String, Vec<ManifestHardpoint>> = HashMap::new();
    let mut add = |k: &manifest::ManifestClass| {
        if !k.hardpoints.is_empty() {
            out.entry(k.name.to_ascii_lowercase())
                .or_insert_with(|| k.hardpoints.clone());
        }
    };
    m.classes.iter().for_each(&mut add);
    side_vehicles(map_dir, game)
        .iter()
        .for_each(|v| add(&v.class));
    out
}

/// Every vehicle class's baked `modl SPHR` sphere (card VDTH), by lowercase
/// class name, from the side bakes as [`hardpoints`] walks them. A map's own
/// class (the TAT1 skiff) has none: the map bake does not write it.
pub fn spheres(map_dir: &std::path::Path, game: &str) -> HashMap<String, [f32; 4]> {
    let mut out = HashMap::new();
    for v in side_vehicles(map_dir, game) {
        if let Some(s) = v.sphere {
            out.entry(v.class.name.to_ascii_lowercase()).or_insert(s);
        }
    }
    out
}

/// Every vehicle class's baked vis-box floor (`ManifestVehicle::vis_min_y`,
/// card VREST), by lowercase class name, from the side bakes as
/// [`hardpoints`] walks them. A map's own class has none.
pub fn vis_floors(map_dir: &std::path::Path, game: &str) -> HashMap<String, f32> {
    let mut out = HashMap::new();
    for v in side_vehicles(map_dir, game) {
        if let Some(y) = v.vis_min_y {
            out.entry(v.class.name.to_ascii_lowercase()).or_insert(y);
        }
    }
    out
}

/// Every walker class's baked anim table and prim origins (card WALK), by
/// lowercase class name, from the side bakes as [`hardpoints`] walks them.
pub fn walker_bakes(
    map_dir: &std::path::Path,
    game: &str,
) -> HashMap<String, manifest::ManifestWalker> {
    let mut out = HashMap::new();
    for v in side_vehicles(map_dir, game) {
        if let Some(w) = v.walker {
            out.entry(v.class.name.to_ascii_lowercase()).or_insert(w);
        }
    }
    out
}

/// `class`'s manifest row and its geometry's baked collision entry with the
/// bake directory it is in: the map's own bake first (the TAT1 skiff), then
/// the first side bake that has the class.
fn class_collision<'a>(
    m: &'a Manifest,
    map_dir: &'a std::path::Path,
    sides: &'a [(std::path::PathBuf, manifest::VehicleManifest)],
    class: &str,
) -> Option<(
    &'a std::path::Path,
    &'a manifest::ManifestClass,
    manifest::ManifestCollision,
)> {
    let geom_of = |k: &manifest::ManifestClass, cols: &[manifest::ManifestCollision]| {
        let g = k.geometry.as_deref()?;
        cols.iter()
            .find(|e| e.geometry.eq_ignore_ascii_case(g))
            .cloned()
    };
    let own = m
        .classes
        .iter()
        .find(|k| k.name.eq_ignore_ascii_case(class))
        .and_then(|k| Some((map_dir, k, geom_of(k, &m.collision)?)));
    own.or_else(|| {
        sides.iter().find_map(|(dir, vm)| {
            let k = vm
                .vehicles
                .iter()
                .map(|v| &v.class)
                .find(|k| k.name.eq_ignore_ascii_case(class))?;
            Some((dir.as_path(), k, geom_of(k, &vm.collision)?))
        })
    })
}

/// A hover class's hull bodies (card HHUL, `HoverClass::hull`) out of its
/// geometry's baked primitives; none for a class that names only its
/// springs — every shipped hover but the two BF2 fighter tanks — which
/// loads nothing.
pub fn hover_hull(
    m: &Manifest,
    map_dir: &std::path::Path,
    game: &str,
    class: &str,
    h: &vehicle_class::HoverClass,
) -> Vec<core::vehicle::hover::HullBody> {
    if !h.names_a_hull_body() {
        return Vec::new();
    }
    let sides = side_vehicle_files(map_dir, game);
    class_collision(m, map_dir, &sides, class)
        .map(|(_, _, entry)| h.hull(&entry.prims))
        .unwrap_or_default()
}

/// Every class's hull parts (card VHULL, `crate::assets::hull_parts`),
/// indexed like `classes`: the class's collision lists and its geometry's
/// baked collision from the map's own bake first (the TAT1 skiff), then the
/// first side bake that has the class. `None` for a turret (`turret`), a
/// class with no baked collision, or one whose bodies are in no subset.
pub fn hull_parts(
    m: &Manifest,
    map_dir: &std::path::Path,
    game: &str,
    classes: &[String],
    turret: impl Fn(&str) -> bool,
) -> Vec<Option<std::sync::Arc<[core::collision::HullPart]>>> {
    let sides = side_vehicle_files(map_dir, game);
    classes
        .iter()
        .map(|c| {
            if turret(c) {
                return None;
            }
            let (dir, k, entry) = class_collision(m, map_dir, &sides, c)?;
            let cg = crate::assets::load_collision_geom(dir, &entry).ok()?;
            let parts = crate::assets::hull_parts(&cg, &k.collision);
            (!parts.is_empty()).then(|| parts.into())
        })
        .collect()
}

/// Each `side_<game>/vehicles/*/vehicles.ron` beside the map's bake, in path
/// order, its vehicles in file order.
fn side_vehicles(map_dir: &std::path::Path, game: &str) -> Vec<manifest::ManifestVehicle> {
    side_vehicle_files(map_dir, game)
        .into_iter()
        .flat_map(|(_, vm)| vm.vehicles)
        .collect()
}

/// Each `side_<game>/vehicles/*/vehicles.ron` beside the map's bake, in path
/// order: its directory and its manifest.
fn side_vehicle_files(
    map_dir: &std::path::Path,
    game: &str,
) -> Vec<(std::path::PathBuf, manifest::VehicleManifest)> {
    let root = map_dir
        .parent()
        .map(|p| p.join(format!("side_{game}")).join("vehicles"));
    let mut files: Vec<std::path::PathBuf> = root
        .and_then(|r| pak::read_dir(r).ok())
        .into_iter()
        .flatten()
        .map(|d| d.join("vehicles.ron"))
        .filter(|p| pak::is_file(p))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = pak::read_to_string(&f) else {
            continue;
        };
        if let Ok(vm) = ron::from_str::<manifest::VehicleManifest>(&text) {
            let dir = f
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default();
            out.push((dir, vm));
        }
    }
    out
}

/// A seat's [`GunSetup`] (card VTUR, re-vehicles §5, §9): its weapons
/// registered in `sim` (once per name), its `FirePointName` and
/// `TurretNodeName` found among the class's nodes `hp`, its aimer from the
/// PS2's defaults overridden by the seat's limits (degrees → radians) and
/// rates. `None` for a seat with no weapon the db can build.
///
/// PROVISIONAL: a seat with no `FirePointName` (BF1 YAV1's tower turret,
/// HOT1's hoth turret) fires from the class's `hp_fire` node — the name the
/// PS2 gives a weapon's own fire node by default (the weapon class setter
/// B1 FUN_002864f0, B1:210168) — else from the turret node, then the model
/// origin; a missing turret node falls back to the fire point.
pub fn seat_gun(
    sim: &mut core::sim::Sim,
    db: &ClassDb,
    s: &vehicle_class::SeatSpec,
    hp: &[ManifestHardpoint],
) -> Option<GunSetup> {
    let weapons: Vec<GunWeapon> = s
        .weapons
        .iter()
        .take(MAX_SEAT_WEAPONS)
        .filter_map(|w| {
            let class = match sim
                .weapon_classes
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&w.class))
            {
                Some(i) => i,
                None => match crate::roster::weapon_from_db(db, &w.class) {
                    Ok(wc) => sim.add_weapon_class(wc),
                    Err(e) => {
                        eprintln!("seat weapon {}: {e:#}", w.class);
                        return None;
                    }
                },
            };
            Some(GunWeapon {
                class,
                infinite: w.ammo < 1,
                spare_clips: w.ammo.max(0) as u32,
            })
        })
        .collect();
    if weapons.is_empty() {
        return None;
    }
    let node = |n: &Option<String>| {
        n.as_deref().and_then(|n| {
            hp.iter()
                .find(|h| h.name.eq_ignore_ascii_case(n.trim()))
                .map(|h| Vec3::from(h.pos))
        })
    };
    let turret = node(&s.turret_node);
    let fire_point = node(&s.fire_point)
        .or_else(|| {
            s.fire_point
                .is_none()
                .then(|| node(&Some("hp_fire".into())))
                .flatten()
        })
        .or(turret)
        .unwrap_or(Vec3::ZERO);
    let d = std::f32::consts::PI / 180.0;
    let lim = |l: [f32; 2]| [l[0].min(l[1]) * d, l[0].max(l[1]) * d];
    let mut aimer = Aimer::default();
    if let Some(l) = s.yaw_limits {
        aimer.yaw = lim(l);
    }
    if let Some(l) = s.pitch_limits {
        aimer.pitch = lim(l);
    }
    if let Some(r) = s.yaw_rate {
        aimer.yaw_rate = r;
    }
    if let Some(r) = s.pitch_rate {
        aimer.pitch_rate = r;
    }
    let pilot = s.kind == vehicle_class::SeatKind::Pilot;
    if pilot {
        // The hover pilot's aim-yaw clamp, K+0x690/+0x694 = 0/0 (re-hover §5.1 I11).
        aimer.yaw = [0.0, 0.0];
    }
    Some(GunSetup {
        weapons,
        fire_point,
        pivot: turret.unwrap_or(fire_point),
        eye: s.eye_point_offset.map_or(Vec3::ZERO, Vec3::from),
        aimer,
        pilot,
        turn: None,
        framed: false,
    })
}

/// A building turret's gun under the PS2's MountedTurret law (card VTURL,
/// `analysis/re-turret-draw.md` 2.2, 2.3 [O]; `core::vehicle::gun::Turn`):
/// the look by `YawLimits` / `PitchLimits` (turret pitch + = nose down),
/// the full-stick speeds and filters (class defaults 2.0 / 0.5 / 10, BF1
/// exponential, BF2 linear), and the aimer from the `Aimer*` keys (defaults
/// ±30° / ±45°, 2.0 / 1.0 rad/s), in the aim frame when the section names a
/// `TurretNodeName`.
pub fn turret_law(g: &mut GunSetup, s: &vehicle_class::SeatSpec, bf2: bool) {
    use core::vehicle::gun::{Slew, Turn, PITCH_SPEED, TURN_FILTER, TURN_SPEED};
    g.turn = Some(Turn::from_class(
        s.yaw_limits,
        s.pitch_limits,
        (
            s.turn_speed.unwrap_or(TURN_SPEED),
            s.pitch_speed.unwrap_or(PITCH_SPEED),
        ),
        (
            s.turn_filter.unwrap_or(TURN_FILTER),
            s.pitch_filter.unwrap_or(TURN_FILTER),
        ),
        if bf2 { Slew::Linear } else { Slew::Exp },
    ));
    let d = std::f32::consts::PI / 180.0;
    let lim = |l: [f32; 2]| [l[0].min(l[1]) * d, l[0].max(l[1]) * d];
    let mut aimer = Aimer::default();
    if let Some(l) = s.aimer_yaw_limits {
        aimer.yaw = lim(l);
    }
    if let Some(l) = s.aimer_pitch_limits {
        aimer.pitch = lim(l);
    }
    if let Some(r) = s.aimer_yaw_rate {
        aimer.yaw_rate = r;
    }
    if let Some(r) = s.aimer_pitch_rate {
        aimer.pitch_rate = r;
    }
    g.aimer = aimer;
    g.framed = s.turret_node.is_some();
}

/// A class's [`SeatSetup`]: a seat boards a soldier for PilotType `vehicle`
/// or `vehicleself` (re-vehicles §5); the exit's ground ray is BF1's 10 m or
/// BF2's 15 m; the radius (PROVISIONAL — the PS2 reads the collision sphere,
/// FUN_001645a0) bounds the hover's spring spheres about the origin — the
/// TAT1 skiff 5.0 m — else [`VEHICLE_RADIUS`].
/// A class's [`BlastSetup`] (card VDTH): its `ExplosionName` class read as
/// an [`ExplosionClass`], and [`vehicle_class::blast_sphere`] over the baked
/// SPHR `sphr`.
pub fn blast_setup(db: &ClassDb, class: &str, sphr: Option<[f32; 4]>) -> BlastSetup {
    let explosion = vehicle_class::explosion_name(db, class)
        .filter(|e| !db.chain(e).is_empty())
        .map(|e| ExplosionClass {
            look: crate::roster::explosion_look(db, &e).unwrap_or_default(),
            ..ExplosionClass::from_props(&crate::roster::ClassSrc { db, class: &e })
        });
    let (c, w) = vehicle_class::blast_sphere(db, class, sphr);
    BlastSetup {
        explosion,
        sphere_centre: Vec3::from(c),
        sphere_radius: w,
    }
}

pub fn seat_setup(
    db: &ClassDb,
    class: &str,
    ring: &[vehicle_class::SeatSpec],
    game: &str,
) -> SeatSetup {
    use vehicle_class::PilotType;
    let radius = vehicle_class::HoverClass::from_db(db, class)
        .map(|h| {
            h.springs
                .iter()
                .map(|s| Vec3::from(s.pos).length() + s.radius)
                .fold(0.0f32, f32::max)
        })
        .filter(|r| *r > 0.0)
        .unwrap_or(VEHICLE_RADIUS);
    SeatSetup {
        radius,
        boardable: ring
            .iter()
            .map(|s| matches!(s.pilot_type, PilotType::Vehicle | PilotType::VehicleSelf))
            .collect(),
        exit_drop: if game.eq_ignore_ascii_case("bf2") {
            15.0
        } else {
            10.0
        },
        ai_size: vehicle_class::ai_size(db, class),
        guns: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Card HTGT: a turret's acquisition sphere is the box around its own
    /// building's bodies only, its centre in the turret's turned frame.
    #[test]
    fn a_turrets_sphere_bounds_its_own_building() {
        use core::collision::{Body, CollisionWorld, Shape};
        let body = |at: Vec3, instance: u32| {
            let mut b = Body::new(
                glam::Mat4::from_translation(at),
                Shape::Box {
                    half: Vec3::splat(1.0),
                },
            );
            b.instance = Some(instance);
            b
        };
        let world = CollisionWorld::new(
            vec![
                body(Vec3::new(10.0, 1.0, 0.0), 5),
                body(Vec3::new(14.0, 1.0, 0.0), 5),
                body(Vec3::new(40.0, 1.0, 0.0), 6),
            ],
            Vec::new(),
        );
        let turn = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let (c, r) = building_sphere(&world, 5, Vec3::new(10.0, 0.0, 0.0), turn).unwrap();
        // The box (9..15, 0..2, -1..1): centre (12, 1, 0), half-diagonal.
        assert!((turn * c - Vec3::new(2.0, 1.0, 0.0)).length() < 1e-5, "{c}");
        assert!((r - Vec3::new(6.0, 2.0, 2.0).length() * 0.5).abs() < 1e-5);
        assert_eq!(building_sphere(&world, 7, Vec3::ZERO, turn), None);
    }

    /// Card VTURB: an armed building's `BuildingSection` lines split its
    /// ODF like a vehicle's; its ring is the gunner sections alone, its hull
    /// a building's; it stands at its instance with the instance's turn and
    /// team; a class spelled in another case still resolves; an armed
    /// building with no section (BF2 POL1's shields) is no turret.
    #[test]
    fn an_armed_building_instance_becomes_a_gunner_only_turret() {
        use crate::classdb::ClassDef;
        use core::vehicle::SeatKind;
        use core::weapon::HealthType;
        let k = fnv1a_lowercase;
        let def = |name: &str, props: &[(&str, &str)]| ClassDef {
            name: name.into(),
            kind: "entc".into(),
            base: "armedbuilding".into(),
            props: props.iter().map(|(a, b)| (k(a), b.to_string())).collect(),
        };
        let classes = vec![
            def(
                "t_GunTurret",
                &[
                    ("BuildingSection", "BODY"),
                    ("MaxHealth", "300.0"),
                    ("YawLimits", "-1 1"),
                    ("BuildingSection", "TURRET1"),
                    ("YawLimits", "-180.0 180.0"),
                    ("WeaponName", "t_weap"),
                    ("FirePointName", "hp_fire"),
                ],
            ),
            def(
                "t_shield",
                &[("BuildingSection", "BODY"), ("MaxHealth", "50")],
            ),
            {
                let mut d = def(
                    "t_buildable",
                    &[("BuildingSection", "TURRET1"), ("WeaponName", "t_weap")],
                );
                d.props
                    .push((manifest::BUILD_ZONE_PROP, "com_prop_buildzone".into()));
                d
            },
        ];
        let path = std::env::temp_dir().join(format!("openbf-vturb-{}.ron", std::process::id()));
        std::fs::write(&path, ron::to_string(&classes).unwrap()).unwrap();
        let mut db = ClassDb::default();
        db.load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let ring = vehicle_class::seats(&db, "t_GunTurret");
        assert_eq!(ring.len(), 1, "{ring:#?}");
        assert_eq!(ring[0].kind, SeatKind::Gunner(0));
        assert_eq!(ring[0].yaw_limits, Some([-180.0, 180.0]), "TURRET1's own");
        assert_eq!(ring[0].fire_point.as_deref(), Some("hp_fire"));
        assert_eq!(
            vehicle_class::health_type(&db, "t_GunTurret"),
            HealthType::Building
        );

        // Turned 90° (the model's +x along world −z), PS2 team 2.
        let inst = |class: &str, team: &str| ManifestInstance {
            class: class.into(),
            name: "gun1".into(),
            rotation: [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            translation: [10.0, 2.0, -5.0],
            props: vec![(k("Team"), team.into())],
        };
        let mut out = PadBuild::default();
        add_turrets(
            &mut out,
            &[
                inst("t_gunturret", "2"),
                inst("t_shield", "0"),
                inst("T_GUNTURRET", "0"),
            ],
            &db,
            true,
        );
        assert_eq!(out.classes, ["t_GunTurret"]);
        assert_eq!(out.geometry, [None], "the world draws the building");
        assert_eq!(out.turrets.len(), 2, "the shield has no seat");
        let t = &out.turrets[0];
        assert_eq!(t.spec.kind, VehicleKind::Turret);
        assert_eq!(t.spec.seats, [SeatKind::Gunner(0)]);
        assert_eq!(t.spec.pos, Vec3::new(10.0, 2.0, -5.0));
        assert_eq!((t.spec.max_health, t.spec.home_team), (300.0, Some(1)));
        assert_eq!(out.turrets[1].spec.home_team, None);
        // Each knows its instance in the manifest (its building's bodies).
        let of: Vec<u32> = out.turrets.iter().map(|t| t.instance).collect();
        assert_eq!(of, [0, 2]);
        assert!((t.spec.yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((t.rot * Vec3::X).distance(Vec3::new(0.0, 0.0, -1.0)) < 1e-5);
        let v = t.spec.build(1);
        assert!(v.pilot_seat().is_none());
        assert!(v.rot.angle_between(t.rot) < 1e-4, "yaw and matrix agree");

        // Card VTUR2: a buildable turret starts unbuilt on BF2 (no turret);
        // BF1 keeps spawning it.
        for (zones, n) in [(true, 0), (false, 1)] {
            let mut out = PadBuild::default();
            add_turrets(&mut out, &[inst("t_buildable", "0")], &db, zones);
            assert_eq!(out.turrets.len(), n, "unbuilt_zones {zones}");
        }
    }

    fn gcw() -> [String; 2] {
        ["all".into(), "imp".into()]
    }

    /// The side table as re_check word 0x5183a0 decodes it: Atk only for
    /// team 1, Def only for team 2, the full side name for either.
    #[test]
    fn atk_is_team_one_and_def_is_team_two() {
        let k = fnv1a_lowercase;
        let f = gcw();
        assert_eq!(class_key_team(k("ClassAllAtk"), &f), Some(0));
        assert_eq!(class_key_team(k("ClassAllDef"), &f), None);
        assert_eq!(class_key_team(k("ClassImpDef"), &f), Some(1));
        assert_eq!(class_key_team(k("ClassImpAtk"), &f), None);
        assert_eq!(class_key_team(k("ClassEmpire"), &f), Some(1));
        assert_eq!(class_key_team(k("ClassAlliance"), &f), Some(0));
        assert_eq!(class_key_team(k("ClassRepAtk"), &f), None, "CW keys");
        assert_eq!(class_key_team(k("ClassCISDef"), &f), None);
        assert_eq!(class_key_team(k("ClassLocals"), &f), None);
        let cw: [String; 2] = ["rep".into(), "cis".into()];
        assert_eq!(class_key_team(k("ClassCISDef"), &cw), Some(1));
        assert_eq!(class_key_team(k("ClassRepAtk"), &cw), Some(0));
    }
}
