use super::drawn_weapon;
use crate::animator::ActionKind;
use crate::soldier_vis::SoldierVisuals;
use crate::walk::{TeamTemplate, WalkSim};
use core::heightfield::Heightfield;
use core::sim::{buttons, Event, PadState};
use core::soldier::GroundRules;
use core::unit::{SlotSpec, UnitSpec};
use glam::Vec3;
use std::sync::Arc;

const THROWN: &str = "rep_weap_inf_thermaldetonator";

#[test]
fn pack_dispensers_use_the_original_drop_action_for_every_faction() {
    let root = crate::assets::baked_root();
    if !root.join("classes/bf1_all.ron").exists() || !root.join("classes/bf2_all.ron").exists() {
        return;
    }
    for (game, classes) in [
        (
            "bf1",
            [
                "all_inf_pilot",
                "imp_inf_pilottie",
                "rep_inf_clone_pilot",
                "cis_inf_pilotdroid",
            ],
        ),
        (
            "bf2",
            [
                "all_inf_engineer",
                "imp_inf_engineer",
                "rep_inf_ep3_engineer",
                "cis_inf_engineer",
            ],
        ),
    ] {
        let db =
            crate::roster::load_classdb(&root.join("classes"), game, &["all", "imp", "rep", "cis"])
                .unwrap();
        for class_name in classes {
            let roster = crate::roster::ClassRoster::build(&db, class_name, 0, None).unwrap();
            let pack = &roster
                .held
                .iter()
                .find(|h| h.weapon.dispenser.is_some())
                .unwrap()
                .weapon;
            for moving in [false, true] {
                let mut w = WalkSim::new(
                    8,
                    Vec3::ZERO,
                    Some(Heightfield {
                        size: 2,
                        unit: 4000.0,
                        offset_x: 2000.0,
                        offset_z: 2000.0,
                        heights: vec![0.0; 4],
                    }),
                );
                w.sim.ground_rules = if game == "bf1" {
                    GroundRules::Bf1
                } else {
                    GroundRules::Bf2
                };
                let primary = w.sim.add_weapon_class(roster.weapon.clone());
                let dispenser = w.sim.add_weapon_class(pack.clone());
                w.player = w
                    .sim
                    .spawn(&UnitSpec {
                        weapon_class: primary,
                        slots: Arc::new([SlotSpec {
                            slot: 3,
                            channel: 1,
                            weapon_class: dispenser,
                            clips: 5,
                        }]),
                        ..UnitSpec::player_in_slot(1, 0, Vec3::new(10.0, 0.0, 0.0))
                    })
                    .unwrap();
                let p = w.player.index as usize;
                let mut t = TeamTemplate::default_trooper();
                t.classes[0].geometry = roster.geometry.clone();
                t.classes[0].weapon_geometry = roster.weapon_geometry.clone();
                t.classes[0].anim = roster.anim.clone();
                let mut vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t]);
                vis.update(&w, core::TICK_DT);
                let mut releases = 0;
                for tick in 0..70 {
                    let pad = PadState {
                        buttons: if tick == 0 {
                            buttons::SECONDARY_FIRE
                        } else {
                            0
                        },
                        ly: if moving { 16384 } else { 0 },
                        ..Default::default()
                    };
                    w.step_tick(&[PadState::default(), pad]);
                    vis.note_shots(&w);
                    vis.update(&w, core::TICK_DT * 0.5);
                    vis.update(&w, core::TICK_DT * 0.5);
                    let a = &vis.units[p].animator;
                    if tick == 0 {
                        assert_eq!(
                            a.action(),
                            Some((
                                if game == "bf1" {
                                    ActionKind::Shoot
                                } else {
                                    ActionKind::Throw(0)
                                },
                                !moving
                            )),
                            "{game} {class_name} moving {moving}"
                        );
                        let clips = &vis.assets[vis.per_class[0][0].body.unwrap()].asset.clips;
                        let name = &clips[a.upper_clip().unwrap()].name;
                        let suffix = if game == "bf1" {
                            "grenade_stand_shoot"
                        } else {
                            "rifle_stand_shoot_secondary_full"
                        };
                        assert!(name.ends_with(suffix), "{game} {class_name}: {name}");
                        assert_eq!(
                            drawn_weapon(&w.sim.units[p], &w.sim.weapon_classes).class,
                            if game == "bf1" { dispenser } else { primary }
                        );
                    }
                    assert!(
                        !matches!(a.action(), Some((ActionKind::Throw(1 | 2), _))),
                        "{game} {class_name}: grenade throw selected at {tick}"
                    );
                    releases += w.sim.events.iter().filter(|e| matches!(e, Event::Throw { unit, weapon_class, .. } if unit.index as usize == p && *weapon_class as usize == dispenser)).count();
                }
                assert_eq!(releases, 1, "{game} {class_name}");
                assert_eq!(vis.units[p].animator.action(), None);
                assert_eq!(
                    drawn_weapon(&w.sim.units[p], &w.sim.weapon_classes).class,
                    primary
                );
            }
        }
    }
}

#[test]
fn real_grenades_start_the_authored_throw_before_release_and_restore_the_gun() {
    let root = crate::assets::baked_root();
    if !root.join("classes/bf1_rep.ron").exists() {
        return;
    }
    for game in ["bf1", "bf2"] {
        let db = crate::roster::load_classdb(&root.join("classes"), game, &["rep"]).unwrap();
        let class_name = if game == "bf1" {
            "rep_inf_clone_trooper"
        } else {
            "rep_inf_ep3_rifleman"
        };
        let roster = crate::roster::ClassRoster::build(&db, class_name, 0, None).unwrap();
        let grenade = crate::roster::thrown_weapon_from_db(&db, THROWN)
            .unwrap()
            .unwrap();
        let slot = crate::classdb::WeaponSlot {
            slot: 2,
            weapon: THROWN.into(),
            ammo: Some(4),
            channel: 1,
        };
        let thrown_bank = crate::roster::class_anim(&db, class_name, &[slot]).banks;
        assert_eq!(grenade.offhand_weapon, game == "bf2");
        let geometry = grenade.ordnance.geometry.as_deref().unwrap();
        assert_eq!(geometry, "com_weap_inf_grenadethermal_ord");
        let projectile =
            crate::assets::load_glb(&root.join(format!("side_{game}/weapons/{geometry}.glb")))
                .unwrap();
        assert_eq!(projectile.indices.len() / 3, 170);
        for moving in [false, true] {
            let mut w = WalkSim::new(
                8,
                Vec3::ZERO,
                Some(Heightfield {
                    size: 2,
                    unit: 4000.0,
                    offset_x: 2000.0,
                    offset_z: 2000.0,
                    heights: vec![0.0; 4],
                }),
            );
            w.sim.ground_rules = if game == "bf1" {
                GroundRules::Bf1
            } else {
                GroundRules::Bf2
            };
            let primary = w.sim.add_weapon_class(roster.weapon.clone());
            let thrown = w.sim.add_weapon_class(grenade.clone());
            w.player = w
                .sim
                .spawn(&UnitSpec {
                    weapon_class: primary,
                    slots: Arc::new([SlotSpec {
                        slot: 2,
                        channel: 1,
                        weapon_class: thrown,
                        clips: 4,
                    }]),
                    ..UnitSpec::player_in_slot(1, 0, Vec3::new(10.0, 0.0, 0.0))
                })
                .unwrap();
            let p = w.player.index as usize;
            let mut t = TeamTemplate::default_trooper();
            t.classes[0].geometry = roster.geometry.clone();
            t.classes[0].weapon_geometry = roster.weapon_geometry.clone();
            t.classes[0].anim = roster.anim.clone();
            t.classes[0].anim.banks.extend(thrown_bank.iter().cloned());
            t.classes[0].held_models = vec![(2, Some("com_weap_inf_grenadethermal".into()))];
            let mut vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t]);
            vis.update(&w, core::TICK_DT);
            let mut releases = 0;
            let mut throw_frames = 0;
            for tick in 0..100 {
                let pad = PadState {
                    buttons: if tick < 2 { buttons::SECONDARY_FIRE } else { 0 },
                    ly: if moving { 16384 } else { 0 },
                    ..Default::default()
                };
                w.step_tick(&[PadState::default(), pad]);
                vis.note_shots(&w);
                vis.update(&w, core::TICK_DT * 0.5);
                vis.update(&w, core::TICK_DT * 0.5);
                let a = &vis.units[p].animator;
                let clips = &vis.assets[vis.per_class[0][0].body.unwrap()].asset.clips;
                if tick == 0 {
                    assert_eq!(
                        a.action(),
                        Some((
                            ActionKind::Throw(if game == "bf1" { 2 } else { 1 }),
                            !moving
                        )),
                        "{game} {moving}"
                    );
                    let hand = drawn_weapon(&w.sim.units[p], &w.sim.weapon_classes);
                    assert_eq!(
                        (hand.class, hand.hidden),
                        if game == "bf1" {
                            (thrown, true)
                        } else {
                            (primary, false)
                        }
                    );
                }
                if matches!(a.action(), Some((ActionKind::Throw(_), _))) {
                    throw_frames += 1;
                }
                if w.sim
                    .events
                    .iter()
                    .any(|e| matches!(e, Event::Throw { unit, .. } if unit.index as usize == p))
                {
                    releases += 1;
                    let name = &clips[a.upper_clip().unwrap()].name;
                    let suffix = if game == "bf1" {
                        "grenade_stand_shoot3"
                    } else {
                        "rifle_stand_shoot_secondary2_full"
                    };
                    assert!(name.ends_with(suffix), "{game}: {name}");
                    if !moving {
                        assert_eq!(a.upper_clip(), a.lower_clip());
                        let seconds =
                            a.lower_phase() * clips[a.lower_clip().unwrap()].frames as f32 / 30.0;
                        if game == "bf1" {
                            assert!((0.29..0.34).contains(&seconds), "release at {seconds}");
                        }
                    } else {
                        assert_ne!(a.upper_clip(), a.lower_clip());
                    }
                }
            }
            assert_eq!(releases, 1, "{game}");
            assert!(throw_frames > 30, "{game}: {throw_frames}");
            assert_eq!(vis.units[p].animator.action(), None);
            assert_eq!(
                drawn_weapon(&w.sim.units[p], &w.sim.weapon_classes).class,
                primary
            );
        }
    }
}
