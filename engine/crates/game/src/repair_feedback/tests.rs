use super::*;
use crate::classdb::ClassDb;
use crate::fx::{Fx, Library};
use crate::soldier_vis::SoldierVisuals;
use crate::walk::{TeamTemplate, WalkSim};
use core::heightfield::Heightfield;
use core::sim::{buttons, PadState, Station, StationClass};
use core::unit::{SlotSpec, UnitSpec};

fn fixture(game: &str) -> Option<(WalkSim, SoldierVisuals, Fx, PadState)> {
    let root = crate::assets::baked_root();
    let shared = if game == "bf1" { "common" } else { "ingame" };
    let geometry = if game == "bf1" {
        "rep_inf_trooper"
    } else {
        "rep_inf_ep3trooper"
    };
    let name = "rep_weap_inf_fusioncutter";
    for path in [
        format!("classes/{game}_{shared}.ron"),
        format!("classes/{game}_rep.ron"),
        format!("common_{game}/effects.ron"),
        format!("side_{game}/soldiers/{geometry}.glb"),
        format!("side_{game}/soldiers/{geometry}.anims.ron"),
        format!("side_{game}/soldiers/{geometry}.sets.ron"),
        format!("side_{game}/weapons/{name}.glb"),
        format!("side_{game}/weapons/{name}.nodes.ron"),
    ] {
        if !root.join(&path).is_file() {
            eprintln!("SKIP: {path} is not baked");
            return None;
        }
    }
    let mut db = ClassDb::default();
    for side in [shared, "rep"] {
        db.load(&root.join(format!("classes/{game}_{side}.ron")))
            .unwrap();
    }
    let cutter = crate::roster::weapon_from_db(&db, name).unwrap();
    assert_eq!(cutter.fire_sound, None);
    assert_eq!(
        cutter.fire_loop_sound.as_deref(),
        Some("com_weap_inf_fusioncutter_fire")
    );
    assert_eq!(
        cutter.muzzle_effect.as_deref(),
        Some(if game == "bf1" {
            "repairkit"
        } else {
            "com_sfx_repair"
        })
    );
    let mut walk = WalkSim::new(42, Vec3::ZERO, None);
    walk.sim = Sim::empty(42);
    walk.sim.ground_rules = core::soldier::GroundRules::for_game(game);
    walk.sim.heightfield = Some(Heightfield {
        offset_x: 16.0,
        offset_z: 16.0,
        unit: 1.0,
        size: 33,
        heights: vec![0.0; 33 * 33],
    });
    let class = walk.sim.add_weapon_class(cutter);
    let mut spec = UnitSpec::player(0, Vec3::new(0.0, 0.0, 2.5));
    if game == "bf1" {
        spec.slots = vec![SlotSpec {
            slot: 3,
            channel: 1,
            weapon_class: class,
            clips: 0,
        }]
        .into();
    } else {
        spec.weapon_class = class;
    }
    walk.player = walk.sim.spawn(&spec).unwrap();
    let mut station_class = StationClass::soldier("medical", 0.5, 15.0, 0);
    station_class.sphere = Some([0.0, 1.0, 0.0, 1.2]);
    walk.sim.add_station_class(station_class);
    let mut station = Station::new(0, Vec3::ZERO, 5.0);
    station.health.max = 10000.0;
    station.health.cur = 100.0;
    station.sphere_centre = Vec3::Y;
    station.sphere_radius = 1.2;
    walk.sim.stations.push(station);
    let mut team = TeamTemplate::default_trooper();
    team.classes[0].geometry = Some(geometry.into());
    team.classes[0].weapon_geometry = Some(name.into());
    team.classes[0].held_models = vec![(3, Some(name.into()))];
    team.classes[0]
        .anim
        .banks
        .push((name.into(), "tool".into()));
    let vis = SoldierVisuals::load_cpu(&root, game, &[team.clone(), team]);
    let lib = Library::load(&root, &root.join(format!("common_{game}")), game, &[]);
    let pad = PadState {
        buttons: if game == "bf1" {
            buttons::SECONDARY_FIRE
        } else {
            buttons::FIRE
        },
        ..Default::default()
    };
    Some((walk, vis, Fx::with_library(lib), pad))
}

#[test]
fn original_cutters_animate_emit_and_loop_across_both_weapon_channels() {
    for game in ["bf1", "bf2"] {
        let Some((mut walk, mut vis, mut fx, pad)) = fixture(game) else {
            continue;
        };
        let mut repaired = 0;
        let mut emitted = 0;
        for tick in 0..46 {
            walk.step_tick(&[pad]);
            repaired += walk
                .sim
                .events
                .iter()
                .filter(|e| matches!(e, Event::Repair { .. }))
                .count();
            vis.note_shots(&walk);
            vis.update(&walk, core::TICK_DT);
            fx.ingest(&walk);
            if tick == 0 {
                let era = fx.library().unwrap().era;
                let before = fx.live()[0].particles(era);
                let origin = fx.live()[0].frame.pos;
                let point = vis.fire_points(&walk)[walk.player.index as usize].unwrap();
                fx.set_repair_origins(&vis.fire_points(&walk));
                let after = fx.live()[0].particles(era);
                let mut checked = 0;
                for (before, after) in before.iter().flatten().zip(after.iter().flatten()) {
                    assert!(after.pos.abs_diff_eq(before.pos + point - origin, 1e-5));
                    checked += 1;
                }
                assert!(checked > 0, "{game}: first burst");
            }
            fx.set_repair_origins(&vis.fire_points(&walk));
            fx.update(core::TICK_DT);
            assert_eq!(loops(&walk.sim).len(), 1, "{game}");
            assert!(fx.live().len() <= 2, "{game}");
            emitted += fx.live().iter().flat_map(|e| e.emitted()).sum::<u32>();
            let a = &vis.units[walk.player.index as usize].animator;
            if walk
                .sim
                .events
                .iter()
                .any(|e| matches!(e, Event::Repair { .. }))
            {
                assert!(
                    matches!(a.action(), Some((crate::animator::ActionKind::Shoot, _))),
                    "{game}: {:?}",
                    a.action()
                );
                let clip = a.upper_clip().unwrap();
                let rig = vis.assets[0].rig.as_ref().unwrap();
                let set = rig.weapon_set("human", "tool");
                let expected = rig
                    .action_slots(set, 0, crate::animator::ActionKind::Shoot)
                    .unwrap()
                    .0
                    .clip;
                assert_eq!(clip, expected, "{game}");
            }
        }
        assert!(repaired >= 3, "{game}: {repaired}");
        assert!(emitted > 10, "{game}");
        let point = vis.fire_points(&walk)[walk.player.index as usize].unwrap();
        assert!(fx.live()[0].frame.pos.abs_diff_eq(point, 1e-5));
        let hash = walk.sim.hash();
        fx.ingest(&walk);
        assert_eq!(walk.sim.hash(), hash);
        walk.step_tick(&[PadState::default()]);
        fx.ingest(&walk);
        assert!(loops(&walk.sim).is_empty());
        assert!(fx.live().iter().all(|e| e.is_stopped()));
        fx.update(2.0);
        assert!(fx.is_empty());
    }
}

#[test]
fn cutter_feedback_stops_for_lost_full_dead_and_exhausted_owners() {
    for game in ["bf1", "bf2"] {
        for case in ["range", "full", "dead", "energy", "switch"] {
            let Some((mut walk, _, mut fx, pad)) = fixture(game) else {
                continue;
            };
            walk.step_tick(&[pad]);
            fx.ingest(&walk);
            assert_eq!(loops(&walk.sim).len(), 1);
            let me = walk.player.index as usize;
            match case {
                "range" => walk.sim.units[me].soldier.pos.x += 20.0,
                "full" => walk.sim.stations[0].health.cur = walk.sim.stations[0].health.max,
                "dead" => walk.sim.units[me].alive = false,
                "energy" => walk.sim.units[me].soldier.energy = 0.0,
                "switch" => {
                    walk.sim.units[me].active_channel = 0;
                    walk.sim.units[me].weapon_class = 0;
                }
                _ => unreachable!(),
            }
            if case == "switch" {
                walk.sim.events.clear();
            } else {
                walk.step_tick(&[pad]);
            }
            fx.ingest(&walk);
            assert!(loops(&walk.sim).is_empty(), "{game}: {case}");
            assert!(fx.live().iter().all(|e| e.is_stopped()), "{game}: {case}");
        }
    }
}
