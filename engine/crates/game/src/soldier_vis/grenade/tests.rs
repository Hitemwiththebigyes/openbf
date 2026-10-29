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
                            vis.hand_of(&w.sim.units[p], &w.sim.weapon_classes).class,
                            if game == "bf1" { dispenser } else { primary }
                        );
                    }
                    assert!(
                        !matches!(a.action(), Some((ActionKind::Throw(1 | 2), _))),
                        "{game} {class_name}: grenade throw selected at {tick}"
                    );
                    if game == "bf2" {
                        for event in &w.sim.events {
                            if let Event::Throw { unit, origin, .. } = event {
                                if unit.index as usize != p {
                                    continue;
                                }
                                let asset = &vis.assets[vis.per_class[0][0].body.unwrap()].asset;
                                let bone = asset.skeleton.index_of("bone_l_hand").unwrap();
                                let pose = a.pose(&asset.skeleton);
                                let hand = (super::super::model_of(&w.sim.units[p].soldier)
                                    * asset.skeleton.palette(&pose.locals())[bone]
                                    * asset.skeleton.global_bind[bone])
                                    .transform_point3(Vec3::ZERO);
                                let item = &pack.dispenser.as_ref().unwrap().item;
                                let local =
                                    glam::Quat::from_rotation_y(-w.sim.units[p].soldier.yaw)
                                        * (hand - *origin)
                                        - item.centre;
                                let gap = (local.abs() - item.half).max(Vec3::ZERO).length();
                                assert!(gap < 0.08, "{class_name} moving {moving}: pack {origin:?}, hand {hand:?}, gap {gap}");
                                assert!(
                                    (tick as f32 * core::TICK_DT - pack.initial_salvo_delay).abs()
                                        <= core::TICK_DT
                                );
                            }
                        }
                    }
                    releases += w.sim.events.iter().filter(|e| matches!(e, Event::Throw { unit, weapon_class, .. } if unit.index as usize == p && *weapon_class as usize == dispenser)).count();
                }
                assert_eq!(releases, 1, "{game} {class_name}");
                assert_eq!(vis.units[p].animator.action(), None);
                assert_eq!(
                    vis.hand_of(&w.sim.units[p], &w.sim.weapon_classes).class,
                    primary
                );
            }
        }
    }
}

/// One soldier of `game` throws a thermal detonator (standing or moving);
/// `borrow` lends a BF1 soldier BF2's throw clips first, as the bake's
/// `BORROWED_THROW` does (card GTHRW). Checks the clip, the hand, and that
/// the grenade leaves on the clip's release moment within one tick.
fn throw_run(root: &std::path::Path, game: &str, borrow: bool, moving: bool) {
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
    // A BF1 map throws as BF2 throws (`Thrown::BF1_THROWS_AS_BF2`): BF2's
    // wind-up on both games.
    assert_eq!(
        grenade.initial_salvo_delay,
        core::weapon::Thrown::BF2_WIND_UP
    );
    let geometry = grenade.ordnance.geometry.as_deref().unwrap();
    assert_eq!(geometry, "com_weap_inf_grenadethermal_ord");
    let projectile =
        crate::assets::load_glb(&root.join(format!("side_{game}/weapons/{geometry}.glb"))).unwrap();
    assert_eq!(projectile.indices.len() / 3, 170);
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
    let mut vis = SoldierVisuals::load_cpu(root, game, &[t.clone(), t]);
    let body = vis.per_class[0][0].body.unwrap();
    if borrow {
        lend_bf2_throw(
            root,
            &mut vis.assets[body],
            roster.geometry.as_deref().unwrap(),
        );
    }
    let rig = vis.assets[body].rig.as_ref().unwrap();
    let bf2_clip = game == "bf2" || rig.throws_offhand();
    // Until the soldiers are rebaked with `BF2_ANIMS` a BF1 soldier has
    // only BF1's clip; after it, the borrowed one.
    if borrow {
        assert!(bf2_clip, "{game}: no BF2 throw lent");
    }
    // The moment the clip's hand lets go: BF2's own wind-up, or BF1's clip
    // stretched to it (its 0.3 s reached at the release).
    let release_at = if bf2_clip {
        grenade.initial_salvo_delay
    } else {
        0.3
    };
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
        let clips = &vis.assets[body].asset.clips;
        let what = format!("{game} borrow {borrow} moving {moving}");
        if tick == 0 {
            assert_eq!(
                a.action(),
                Some((
                    ActionKind::Throw(if game == "bf1" { 2 } else { 1 }),
                    !moving
                )),
                "{what}"
            );
            let hand = vis.hand_of(&w.sim.units[p], &w.sim.weapon_classes);
            assert_eq!(
                (hand.class, hand.hidden),
                if bf2_clip {
                    (primary, false)
                } else {
                    (thrown, true)
                },
                "{what}"
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
            let up = a.upper_clip().unwrap();
            let name = &clips[up].name;
            let suffix = if bf2_clip {
                "rifle_stand_shoot_secondary2_full"
            } else {
                "grenade_stand_shoot3"
            };
            assert!(name.ends_with(suffix), "{what}: {name}");
            // The sim's release against the clip's: within one tick. The
            // clip's time by its own game's clock (BF1 frames / 30, BF2
            // (frames - 1) / 30: re-grenade-throw-anim §10.3's 9/51 and 16/40).
            let frames = clips[up].frames as f32 - if bf2_clip { 1.0 } else { 0.0 };
            let seconds = a.upper_phase() * frames / 30.0;
            assert!(
                (seconds - release_at).abs() <= core::TICK_DT,
                "{what}: released at clip time {seconds}, the hand lets go at {release_at}"
            );
            if bf2_clip {
                // BF2's throw lets go from its left hand: the grenade starts
                // there (`Thrown::BF2_HAND`, BF2's RAM).
                let origin = w
                    .sim
                    .events
                    .iter()
                    .find_map(|e| match e {
                        Event::Throw { unit, origin, .. } if unit.index as usize == p => {
                            Some(*origin)
                        }
                        _ => None,
                    })
                    .unwrap();
                let asset = &vis.assets[body].asset;
                let bone = asset.skeleton.index_of("bone_l_hand").unwrap();
                let pose = a.pose(&asset.skeleton);
                let hand = (super::super::model_of(&w.sim.units[p].soldier)
                    * asset.skeleton.palette(&pose.locals())[bone]
                    * asset.skeleton.global_bind[bone])
                    .transform_point3(Vec3::ZERO);
                let gap = hand.distance(origin);
                assert!(gap < 0.3, "{what}: hand {hand:?}, grenade {origin:?}");
            }
            if !moving {
                assert_eq!(a.upper_clip(), a.lower_clip(), "{what}");
            } else {
                assert_ne!(a.upper_clip(), a.lower_clip(), "{what}");
            }
        }
    }
    assert_eq!(releases, 1, "{game}");
    assert!(throw_frames > 30, "{game}: {throw_frames}");
    assert_eq!(vis.units[p].animator.action(), None);
    assert_eq!(
        vis.hand_of(&w.sim.units[p], &w.sim.weapon_classes).class,
        primary
    );
}

/// What the bake's `BORROWED_THROW` does to a BF1 soldier, done here on
/// the loaded asset: BF2's two throw clips (from a BF2 trooper's baked
/// GLB) on this skeleton by bone name, every bone but the root and the
/// pelvis at this model's rest offset; then the rig again.
fn lend_bf2_throw(
    root: &std::path::Path,
    sa: &mut crate::soldier_vis::SoldierAsset,
    geometry: &str,
) {
    use crate::animator::BF2_THROW_CLIPS;
    use crate::skin::{BoneXform, Clip};
    let lender =
        crate::assets::load_skinned_glb(&root.join("side_bf2/soldiers/rep_inf_ep3trooper.glb"))
            .unwrap();
    let skel = &sa.asset.skeleton;
    for name in BF2_THROW_CLIPS {
        let src = lender.clips.iter().find(|c| c.name == name).unwrap();
        let tracks = (0..skel.len())
            .map(|b| {
                let from = lender.skeleton.index_of(&skel.names[b])?;
                let keys = src.tracks.get(from)?.as_ref()?;
                let height = ["dummyroot", "bone_root", "bone_pelvis"]
                    .iter()
                    .any(|n| skel.names[b].eq_ignore_ascii_case(n));
                let rest = skel.local_bind[b].w_axis.truncate();
                Some(
                    keys.iter()
                        .map(|k| BoneXform {
                            rotation: k.rotation,
                            translation: if height { k.translation } else { rest },
                        })
                        .collect(),
                )
            })
            .collect();
        sa.asset.clips.push(Clip {
            name: name.to_owned(),
            fps: src.fps,
            frames: src.frames,
            tracks,
        });
    }
    sa.rig = SoldierVisuals::load_rig(&root.join("side_bf1"), geometry, &sa.asset);
}

#[test]
fn real_grenades_start_the_authored_throw_before_release_and_restore_the_gun() {
    let root = crate::assets::baked_root();
    if !root.join("classes/bf1_rep.ron").exists() {
        return;
    }
    for game in ["bf1", "bf2"] {
        for moving in [false, true] {
            throw_run(&root, game, false, moving);
        }
    }
}

/// BF2's throw on a BF1 map (card GTHRW, a named deviation): a BF1 soldier
/// who carries the borrowed clips plays BF2's overhand throw with the rifle
/// in his hand, and the grenade leaves on its release frame.
#[test]
fn bf1_soldier_throws_bf2s_throw_released_on_its_frame() {
    let root = crate::assets::baked_root();
    if !root.join("classes/bf1_rep.ron").exists()
        || !root
            .join("side_bf2/soldiers/rep_inf_ep3trooper.glb")
            .exists()
    {
        return;
    }
    for moving in [false, true] {
        throw_run(&root, "bf1", true, moving);
    }
}
