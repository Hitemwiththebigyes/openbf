//! Playtest bug 2026-10-28: "standing in front of a healing droid, move the
//! camera to go right behind the droid, it puts the camera inside of the
//! droid" (`game::camera_collide`). On a baked BF1 Naboo battle the player
//! is put in front of each kind of blocker, his back to it, and the chase
//! camera swung behind it: the drawn camera must end outside the blocker by
//! at least the near plane, on the player's side of it, with his body hidden
//! when it comes into him; held still, it must not jitter.
//! Data-gated: SKIP without the bake.
use core::collision::{HullFilter, Layers};
use core::sim::{buttons, PadState, Sim, SpawnStage};
use game::assets::Scene;
use game::camera_collide::{closest_on_triangle, BODY_HIDE_DIST, CAMERA_RADIUS, PULL_RETURN_RATE};
use game::walk::{ClassVisual, TeamTemplate, WalkSim, CAMERA_PULL_BODY_Y};
use glam::{Mat4, Vec3};

type Tri = [Vec3; 3];

fn battle() -> Option<(WalkSim, Scene)> {
    let root = game::assets::baked_root();
    let dir = root.join("bf1_nab1");
    if !dir.join("manifest.ron").exists() {
        eprintln!("SKIP: camera clip needs baked BF1 Naboo");
        return None;
    }
    let (scene, _) = game::assets::load_battle_scene(&dir, "con").unwrap();
    let mission =
        game::mission::MissionDef::load(&root.parent().unwrap().join("missions/bf1_nab1_con.ron"))
            .unwrap();
    let factions = mission.factions();
    let sides = [factions[0].as_str(), factions[1].as_str()];
    let (rosters, _) =
        game::roster::FactionRoster::pair(&root.join("classes"), "bf1", sides, Some(&mission))
            .unwrap();
    let mut sim = Sim::empty(42);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    for (team, definition) in mission.teams.iter().enumerate() {
        sim.battle.teams[team] = core::battle::TeamState::new(definition.tickets);
    }
    let mut names = Vec::new();
    let mut team = 0;
    let templates = rosters.map(|roster| {
        let table: Vec<_> = roster
            .classes
            .iter()
            .map(|class| class.register(&mut sim, &mut names))
            .collect();
        let visuals = roster
            .classes
            .iter()
            .map(ClassVisual::from_roster)
            .collect();
        let template = TeamTemplate::from_roster(&roster, &table[0], visuals);
        sim.classes[team] = table;
        team += 1;
        template
    });
    let nav = game::layout::nav_graph(&scene.manifest, &scene.dir);
    let mut walk = WalkSim::with_teams(sim, scene.heightfield(), Vec3::ZERO, templates, nav);
    scene.install_collision(&mut walk.sim);
    walk.camera_blockers = game::camera_collide::CameraBlockers::from_scene(&scene);
    walk.fov_y_ps2 = game::walk::ps2_fov_y(&scene.manifest.source_game);
    let mut layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    walk.sim.death_regions = layout.apply_mission(&mission).death_regions;
    walk.setup_conquest(&layout, [0, 0], true);
    // The pads read the posts' owners (`main/battle.rs`'s order).
    game::vehicle_pads::install(
        &mut walk.sim,
        &scene.manifest,
        &scene.dir,
        &root.join("classes"),
        &factions,
    )
    .unwrap();
    assert!(walk.sim.open_spawn_screens(0, false));
    walk.sim.set_spawn_points(0, 12);
    for _ in 0..2 {
        walk.step_tick(&[PadState {
            buttons: buttons::SPAWN_ACCEPT,
            ..Default::default()
        }]);
        walk.step_tick(&[PadState::default()]);
    }
    for _ in 0..30 {
        walk.step_tick(&[PadState::default()]);
    }
    assert_eq!(walk.sim.spawn_stage[0], SpawnStage::Accepted);
    Some((walk, scene))
}

/// The first instance of `class` that has a drawn model.
fn instance(scene: &Scene, class: &str) -> usize {
    scene
        .manifest
        .instances
        .iter()
        .position(|i| {
            i.class.eq_ignore_ascii_case(class) && scene.resolve_model(&i.class).is_some()
        })
        .unwrap_or_else(|| panic!("no drawn {class}"))
}

/// Instance `ii`'s drawn triangles at `pose` (its placed matrix when None).
fn tris(scene: &Scene, ii: usize, pose: Option<Mat4>) -> Vec<Tri> {
    let inst = &scene.manifest.instances[ii];
    let m = pose.unwrap_or_else(|| game::viewer::instance_matrix(inst.rotation, inst.translation));
    let mesh = &scene.meshes[scene.resolve_model(&inst.class).unwrap()];
    let p: Vec<Vec3> = mesh
        .positions
        .iter()
        .map(|p| m.transform_point3(Vec3::from(*p)))
        .collect();
    mesh.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t.iter().all(|&i| (i as usize) < p.len()))
        .map(|t| t.map(|i| p[i as usize]))
        .collect()
}

/// The live pose of the droid at manifest instance `ii`.
fn droid_pose(walk: &WalkSim, ii: usize) -> Mat4 {
    walk.sim
        .stations
        .iter()
        .find(|s| s.instance == Some(ii as u32))
        .expect("a sim station at the droid's instance")
        .pose()
}

fn clearance(p: Vec3, tris: &[Tri]) -> f32 {
    tris.iter()
        .map(|[a, b, c]| closest_on_triangle(p, *a, *b, *c).distance(p))
        .fold(f32::MAX, f32::min)
}

/// Does the segment `a -> b` pass through a triangle?
fn crosses(a: Vec3, b: Vec3, tris: &[Tri]) -> bool {
    let (d, len) = ((b - a).normalize_or_zero(), (b - a).length());
    tris.iter().any(|[p, q, r]| {
        let (e1, e2) = (*q - *p, *r - *p);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-9 {
            return false;
        }
        let s = a - *p;
        let u = s.dot(h) / det;
        let qv = s.cross(e1);
        let v = d.dot(qv) / det;
        let t = e2.dot(qv) / det;
        (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && t > 0.0 && t < len
    })
}

/// Put the player at `feet` facing along `facing` (horizontal) with `pitch`,
/// forget the camera's stored pull, and return the drawn camera.
fn stand(walk: &mut WalkSim, feet: Vec3, facing: Vec3, pitch: f32) -> render::Camera {
    let p = walk.player;
    let u = walk.sim.unit_mut(p).unwrap();
    u.soldier.pos = feet;
    // forward = (-sin yaw, 0, -cos yaw).
    u.soldier.yaw = (-facing.x).atan2(-facing.z);
    u.soldier.pitch = pitch;
    walk.cam_pull = 1.0;
    walk.camera()
}

/// The player's ideal (unpulled) camera now.
fn ideal(walk: &WalkSim) -> Vec3 {
    let me = walk.sim.unit(walk.player).unwrap();
    me.ideal_camera(walk.sim.ground_rules, walk.sim.views())
        .position
}

/// Ring the solid `tris` (centre `c`, standing floor `floor_y`) with the
/// player, his back to it, and check every drawn camera. Returns how many
/// placements had the ideal camera inside or behind the solid.
fn swing_behind(
    walk: &mut WalkSim,
    what: &str,
    c: Vec3,
    floor_y: f32,
    solid: impl Fn(&WalkSim) -> Vec<Tri>,
) -> usize {
    let mut blocked = 0;
    let mut checked = 0;
    for k in 0..12 {
        let a = std::f32::consts::TAU * k as f32 / 12.0;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let t0 = solid(walk);
        let reach = t0
            .iter()
            .flatten()
            .map(|p| (*p - c).dot(out))
            .fold(0.0f32, f32::max);
        for d in [0.35f32, 0.6, 1.0, 1.6, 2.4] {
            for turn in [-0.5f32, 0.0, 0.5] {
                for pitch in [-0.6f32, -0.2, 0.0, 0.35] {
                    let feet = Vec3::new(c.x, floor_y, c.z) + out * (reach + d);
                    let facing = Vec3::new(
                        out.x * turn.cos() - out.z * turn.sin(),
                        0.0,
                        out.x * turn.sin() + out.z * turn.cos(),
                    );
                    let cam = stand(walk, feet, facing, pitch);
                    let tris = solid(walk);
                    let body = feet + Vec3::Y * CAMERA_PULL_BODY_Y;
                    if clearance(body, &tris) < CAMERA_RADIUS {
                        continue; // the soldier himself would stand in it
                    }
                    checked += 1;
                    let want = ideal(walk);
                    if clearance(want, &tris) < CAMERA_RADIUS || crosses(body, want, &tris) {
                        blocked += 1;
                    }
                    let e = cam.position;
                    let gap = clearance(e, &tris);
                    let at = format!(
                        "{what}: bearing {k}, {d} m out, turn {turn}, pitch {pitch}: \
                         feet {feet:.2?} camera {e:.2?} ideal {want:.2?}"
                    );
                    assert!(gap >= cam.near, "{at}: {gap:.3} m from the {what}");
                    assert!(!crosses(body, e, &tris), "{at}: behind the {what}");
                    if e.distance(body) < BODY_HIDE_DIST {
                        assert!(walk.player_body_hidden(), "{at}: the camera is in his body");
                    }
                }
            }
        }
    }
    eprintln!("{what}: {checked} placements, {blocked} with the ideal camera in or behind it");
    assert!(checked > 100, "{what}: only {checked} placements");
    blocked
}

/// The reported case: a health droid (it turns: checked at its live pose).
#[test]
fn the_camera_stays_out_of_a_health_droid() {
    let Some((mut walk, scene)) = battle() else {
        return;
    };
    let ii = instance(&scene, "com_item_healthrecharge");
    let c = Vec3::from(scene.manifest.instances[ii].translation);
    let s = walk
        .sim
        .stations
        .iter()
        .position(|s| s.instance == Some(ii as u32))
        .expect("the droid's station");
    // Where it was placed, then turned a way along its idle spin.
    for yaw in [0.0f32, 0.9, 2.3] {
        walk.sim.stations[s].motion.yaw = yaw;
        let what = format!("health droid at yaw {yaw}");
        let blocked = swing_behind(&mut walk, &what, c, c.y, |w| {
            tris(&scene, ii, Some(droid_pose(w, ii)))
        });
        assert!(
            blocked > 50,
            "{what}: the swing must put the ideal camera into the droid"
        );
    }
}

/// Walls and props: a ruin wall, a column (thin) and a Gungan head bunker.
#[test]
fn the_camera_stays_out_of_walls_and_props() {
    let Some((mut walk, scene)) = battle() else {
        return;
    };
    for class in [
        "nab1_prop_ruins1",
        "nab1_prop_ruins_column1",
        "nab1_prop_gunganhead_bunker",
    ] {
        let ii = instance(&scene, class);
        let c = Vec3::from(scene.manifest.instances[ii].translation);
        let floor = walk
            .sim
            .heightfield
            .as_ref()
            .map_or(c.y, |h| h.height_at(c.x, c.z));
        let solid = tris(&scene, ii, None);
        // The player stands at the terrain's height under its origin.
        let blocked = swing_behind(&mut walk, class, c, floor, |_| solid.clone());
        assert!(
            blocked > 20,
            "{class}: the swing must put the ideal camera into it"
        );
    }
}

/// A building turret's gun emplacement, walked up to on foot.
#[test]
fn the_camera_stays_out_of_a_building_turret() {
    let Some((mut walk, scene)) = battle() else {
        return;
    };
    let ii = instance(&scene, "com_weap_gunturret");
    assert!(
        walk.sim
            .vehicles
            .iter()
            .any(|v| v.instance == Some(ii as u32)),
        "the gun turret is a building turret"
    );
    let c = Vec3::from(scene.manifest.instances[ii].translation);
    let solid = tris(&scene, ii, None);
    let blocked = swing_behind(&mut walk, "gun turret", c, c.y, |_| solid.clone());
    assert!(blocked > 20, "the swing must put the ideal camera into it");
}

/// A parked vehicle: its collision hull blocks the camera (ours; the PS2's
/// camera mask has no vehicle layer).
#[test]
fn the_camera_stays_out_of_a_vehicle() {
    let Some((mut walk, _scene)) = battle() else {
        return;
    };
    // A pad's tank or speeder, spawned on its pad.
    let has_hull = |class: u32| {
        walk.sim
            .hull_parts
            .get(class as usize)
            .is_some_and(|h| h.as_ref().is_some_and(|h| !h.is_empty()))
    };
    let (pad, spec) = walk
        .sim
        .vehicle_pads
        .iter()
        .find_map(|p| {
            let c = p
                .def
                .classes
                .iter()
                .flatten()
                .find(|c| has_hull(c.spec.class))?;
            Some((p.def.clone(), c.spec.clone()))
        })
        .expect("a pad class with a hull on Naboo");
    let mut spec = spec;
    spec.pos = pad.pos;
    spec.yaw = pad.yaw;
    let vh = walk.sim.spawn_vehicle(&spec);
    walk.step_tick(&[PadState::default()]);
    let c = walk.sim.vehicle(vh).expect("spawned").pos;
    let hull_hit = |w: &WalkSim, o: Vec3, d: Vec3, len: f32| {
        w.sim
            .collision
            .ray_with(o, d, len, Layers::ORDNANCE, HullFilter::Hulls)
            .filter(|h| h.owner == Some(vh))
            .map(|h| h.t)
    };
    // 26 directions: how near the hull is to a point.
    let dirs: Vec<Vec3> = (0..27)
        .filter(|&i| i != 13)
        .map(|i| {
            Vec3::new(
                (i % 3) as f32 - 1.0,
                ((i / 3) % 3) as f32 - 1.0,
                (i / 9) as f32 - 1.0,
            )
            .normalize()
        })
        .collect();
    let near =
        |w: &WalkSim, p: Vec3, len: f32| dirs.iter().any(|d| hull_hit(w, p, *d, len).is_some());
    let (mut checked, mut blocked) = (0, 0);
    for k in 0..12 {
        let a = std::f32::consts::TAU * k as f32 / 12.0;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        // The hull's reach along this bearing at the pull point's height,
        // from outside in.
        let from = c + Vec3::Y * CAMERA_PULL_BODY_Y + out * 20.0;
        let reach = hull_hit(&walk, from, -out, 20.0).map_or(0.0, |t| 20.0 - t);
        for d in [0.4f32, 0.8, 1.4, 2.2] {
            for pitch in [-0.6f32, -0.2, 0.0, 0.35] {
                let feet = c + out * (reach + d);
                let cam = stand(&mut walk, feet, out, pitch);
                let body = feet + Vec3::Y * CAMERA_PULL_BODY_Y;
                if near(&walk, body, CAMERA_RADIUS) {
                    continue; // the soldier would stand in the hull
                }
                checked += 1;
                let at = format!(
                    "bearing {k}, {d} m out, pitch {pitch}: feet {feet:.2?} camera {:.2?}",
                    cam.position
                );
                let want = ideal(&walk);
                let to = want - body;
                if hull_hit(&walk, body, to.normalize(), to.length() + CAMERA_RADIUS).is_some()
                    || near(&walk, want, CAMERA_RADIUS)
                {
                    blocked += 1;
                }
                let e = cam.position - body;
                assert!(
                    e.length() < 1e-3 || hull_hit(&walk, body, e.normalize(), e.length()).is_none(),
                    "{at}: behind the hull"
                );
                assert!(
                    !near(&walk, cam.position, cam.near),
                    "{at}: the hull within {} m",
                    cam.near
                );
                if e.length() < BODY_HIDE_DIST {
                    assert!(walk.player_body_hidden(), "{at}: the camera is in his body");
                }
            }
        }
    }
    eprintln!("vehicle: {checked} placements, {blocked} with the ideal camera in or behind it");
    assert!(
        checked > 50 && blocked > 20,
        "{checked} placements, {blocked} blocked"
    );
}

/// Held still in front of a turning droid for 3 s: the camera never pops
/// back out faster than the PS2's return rate, stays out of the droid every
/// tick, and in front of a still wall it does not move at all.
#[test]
fn the_pulled_in_camera_does_not_jitter() {
    let Some((mut walk, scene)) = battle() else {
        return;
    };
    let ii = instance(&scene, "com_item_healthrecharge");
    let c = Vec3::from(scene.manifest.instances[ii].translation);
    let out = Vec3::X;
    let reach = tris(&scene, ii, Some(droid_pose(&walk, ii)))
        .iter()
        .flatten()
        .map(|p| (*p - c).length())
        .fold(0.0f32, f32::max);
    stand(&mut walk, c + out * (reach + 0.4), out, -0.2);
    let full = (ideal(&walk) - (walk.soldier().pos + Vec3::Y * CAMERA_PULL_BODY_Y)).length();
    let mut last: Option<f32> = None;
    let mut pulled = 0;
    for tick in 0..90 {
        walk.step_tick(&[PadState::default()]);
        let body = walk.soldier().pos + Vec3::Y * CAMERA_PULL_BODY_Y;
        let cam = walk.camera();
        let len = cam.position.distance(body);
        let gap = clearance(cam.position, &tris(&scene, ii, Some(droid_pose(&walk, ii))));
        assert!(gap >= cam.near, "tick {tick}: {gap:.3} m from the droid");
        if let Some(l) = last {
            assert!(
                len - l <= PULL_RETURN_RATE * core::TICK_DT * full + 1e-3,
                "tick {tick}: the camera jumped out {l:.3} -> {len:.3}"
            );
        }
        if len < full - 0.05 {
            pulled += 1;
        }
        last = Some(len);
    }
    assert!(
        pulled > 30,
        "the droid must pull the camera in ({pulled} ticks)"
    );

    let wall = instance(&scene, "nab1_prop_ruins1");
    let w = Vec3::from(scene.manifest.instances[wall].translation);
    let solid = tris(&scene, wall, None);
    // The first bearing whose ideal camera the wall blocks.
    let floor = walk
        .sim
        .heightfield
        .as_ref()
        .map_or(w.y, |h| h.height_at(w.x, w.z));
    let mut seen = Vec::new();
    for k in 0..12 {
        let a = std::f32::consts::TAU * k as f32 / 12.0;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let reach = solid
            .iter()
            .flatten()
            .map(|p| (*p - w).dot(out))
            .fold(0.0f32, f32::max);
        stand(
            &mut walk,
            Vec3::new(w.x, floor, w.z) + out * (reach + 0.5),
            out,
            -0.2,
        );
        let body = walk.soldier().pos + Vec3::Y * CAMERA_PULL_BODY_Y;
        if clearance(body, &solid) > CAMERA_RADIUS && crosses(body, ideal(&walk), &solid) {
            for _ in 0..60 {
                walk.step_tick(&[PadState::default()]);
                seen.push(walk.camera().position);
            }
            break;
        }
    }
    assert!(!seen.is_empty(), "no bearing of the wall blocks the camera");
    let settled = &seen[10..];
    assert!(
        settled.windows(2).all(|p| p[0].distance(p[1]) < 1e-4),
        "a still camera at a still wall moves: {:?}",
        &settled[..4]
    );
}
