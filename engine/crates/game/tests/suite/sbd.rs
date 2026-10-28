use core::sim::{buttons, Event, PadState, Sim};
use core::unit::{Controller, UnitHandle, UnitSpec};
use core::weapon::{FireInput, WeaponState, DEG};
use game::roster::{load_classdb, weapon_from_db, ClassRoster};
use glam::{Vec2, Vec3};

#[test]
fn both_discs_keep_their_sbd_source_values() {
    for (game, common, class, tri, rounds, delay, heat, clips, direct, blast) in [
        (
            "bf1",
            "COMMON/ENG/COMMON.LVL",
            "cis_inf_battledroid",
            "shrapnel",
            55,
            0.5,
            0.15,
            4,
            200.0,
            100.0,
        ),
        (
            "bf2",
            "INGAME.LVL",
            "cis_inf_rifleman",
            "trishot",
            50,
            0.66,
            0.20,
            3,
            100.0,
            250.0,
        ),
    ] {
        let side = format!("{game}/lvl/DATA/_LVL_PS2/SIDE/CIS.LVL");
        let common = format!("{game}/lvl/DATA/_LVL_PS2/{common}");
        let Some(db) = super::weapon_classdb::db(&[&side, &common], &format!("sbd-{game}")) else {
            continue;
        };
        let roster = ClassRoster::build(&db, class, 1, None).unwrap();
        assert_eq!(roster.weapon.rounds_per_clip, rounds);
        assert_eq!(roster.total_clips, 5);
        let w = weapon_from_db(&db, &format!("cis_weap_inf_wrist_{tri}")).unwrap();
        assert_eq!(w.shots_per_salvo, 3);
        assert_eq!(w.rounds_per_clip, 0);
        assert_eq!(w.shot_delay, [delay; 2]);
        assert_eq!(w.heat_per_shot, heat);
        assert_eq!(w.max_pressed_time, 0.0);
        assert_eq!(
            w.shot_tilts,
            vec![
                (-0.1 * DEG, -0.8 * DEG),
                (0.8 * DEG, 0.0),
                (-0.1 * DEG, 0.8 * DEG)
            ]
        );
        let rocket = roster
            .held
            .iter()
            .find(|s| s.channel == 1)
            .expect("wrist rocket in second channel");
        assert_eq!(rocket.clips, clips);
        assert_eq!(rocket.weapon.ordnance.max_damage, direct);
        assert_eq!(
            rocket.weapon.ordnance.explosion.as_ref().unwrap().damage,
            blast
        );
    }
}

fn battle(game: &str) -> Option<(Sim, UnitHandle, UnitHandle)> {
    let dir = game::assets::baked_root().join("classes");
    if !dir.join("bf2_cis.ron").exists() {
        eprintln!("SKIP: SBD class bakes absent");
        return None;
    }
    let db = load_classdb(&dir, game, &["cis"]).unwrap();
    let roster = ClassRoster::build(
        &db,
        if game == "bf1" {
            "cis_inf_battledroid"
        } else {
            "cis_inf_rifleman"
        },
        1,
        None,
    )
    .unwrap();
    let mut sim = Sim::empty(7);
    let class = roster.register(&mut sim, &mut Vec::new());
    let shooter = sim
        .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
        .unwrap();
    // At the body: a level shot meets the head prim, whose x3 (the PS2's
    // headshot, issue #41) is not what this suite measures.
    sim.unit_mut(shooter).unwrap().soldier.pitch = -0.2;
    let target = sim
        .spawn(&UnitSpec {
            max_health: 10_000.0,
            ..UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -8.0))
        })
        .unwrap();
    Some((sim, shooter, target))
}

fn step(sim: &mut Sim, bits: u32) -> Vec<(usize, Vec3)> {
    sim.step(&[PadState {
        buttons: bits | buttons::MOUSE_AIM,
        ..PadState::default()
    }]);
    sim.events
        .iter()
        .filter_map(|e| match e {
            Event::Shot {
                weapon_class, dir, ..
            } => Some((*weapon_class as usize, *dir)),
            _ => None,
        })
        .collect()
}

#[test]
fn sbd_fires_bolts_and_rockets_with_identical_gameplay_on_both_map_sets() {
    let mut outcomes = Vec::new();
    for game in ["bf1", "bf2"] {
        let Some((mut sim, shooter, target)) = battle(game) else {
            continue;
        };
        let shots = step(&mut sim, buttons::FIRE);
        assert_eq!(shots.len(), 1);
        assert_eq!(
            sim.unit(shooter)
                .unwrap()
                .weapon
                .rounds(&sim.weapon_classes[shots[0].0]),
            Some(249)
        );
        for _ in 0..8 {
            step(&mut sim, 0);
        }
        assert_eq!(sim.unit(target).unwrap().health.cur, 9900.0);
        step(&mut sim, buttons::SWITCH_WEAPON);
        let shots = step(&mut sim, buttons::FIRE);
        assert_eq!(shots.len(), 3);
        assert!(shots[0].1.x * shots[2].1.x < 0.0);
        assert!(shots[1].1.y < shots[0].1.y);
        let unit = sim.unit(shooter).unwrap();
        assert_eq!(
            unit.weapon.rounds(&sim.weapon_classes[unit.weapon_class]),
            None
        );
        for _ in 0..8 {
            step(&mut sim, 0);
        }
        assert_eq!(sim.unit(target).unwrap().health.cur, 9600.0);
        let mut rocket_shots = Vec::new();
        let mut rocket_damage = Vec::new();
        for t in 0..10 {
            rocket_shots.extend(step(
                &mut sim,
                if t == 0 { buttons::SECONDARY_FIRE } else { 0 },
            ));
            rocket_damage.extend(sim.events.iter().filter_map(|event| match event {
                Event::Hit { victim, damage, .. } if *victim == target => Some(*damage),
                _ => None,
            }));
        }
        assert_eq!(rocket_shots.len(), 1);
        let unit = sim.unit(shooter).unwrap();
        let rocket = &unit.held[unit.cur1.unwrap() as usize];
        assert_eq!(
            rocket.state.rounds(&sim.weapon_classes[rocket.class]),
            Some(2)
        );
        for _ in 0..20 {
            step(&mut sim, 0);
            rocket_damage.extend(sim.events.iter().filter_map(|event| match event {
                Event::Hit { victim, damage, .. } if *victim == target => Some(*damage),
                _ => None,
            }));
        }
        let hp = sim.unit(target).unwrap().health.cur;
        // The hit is a tick younger than its landing update (card SGDMG).
        assert_eq!(rocket_damage, vec![120.0, 250.0]);
        outcomes.push((hp, shots, rocket_shots));
    }
    if outcomes.len() == 2 {
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn trishot_cools_and_rocket_exhausts_its_three_rounds() {
    let Some((sim, shooter, _)) = battle("bf2") else {
        return;
    };
    let unit = sim.unit(shooter).unwrap();
    let tri = &sim.weapon_classes[unit.held.iter().find(|h| h.channel == 0).unwrap().class];
    let rocket = &sim.weapon_classes[unit.held.iter().find(|h| h.channel == 1).unwrap().class];
    let mut state = WeaponState::with_clips(tri, true, 0);
    let mut lcg = core::lcg::GameLcg::new(7);
    let mut shots = Vec::new();
    for t in 0..60 {
        state.update(
            core::TICK_DT,
            tri,
            FireInput {
                held: t % 2 == 0,
                ..Default::default()
            },
            &mut lcg,
            &mut shots,
        );
    }
    assert_eq!(shots.len(), 9);
    assert!(state.overheated);
    shots.clear();
    for _ in 0..90 {
        state.update(
            core::TICK_DT,
            tri,
            FireInput::default(),
            &mut lcg,
            &mut shots,
        );
    }
    assert!(!state.overheated);
    state.update(
        core::TICK_DT,
        tri,
        FireInput {
            held: true,
            ..Default::default()
        },
        &mut lcg,
        &mut shots,
    );
    assert_eq!(shots.len(), 3);
    let mut state = WeaponState::with_clips(rocket, true, 3);
    shots.clear();
    for t in 0..400 {
        state.update(
            core::TICK_DT,
            rocket,
            FireInput {
                held: t % 90 == 0,
                ..Default::default()
            },
            &mut lcg,
            &mut shots,
        );
    }
    assert_eq!(shots.len(), 3);
    assert_eq!(state.rounds(rocket), Some(0));
}

#[test]
fn sbd_hud_models_keep_vertex_colors_and_draw_in_both_rows() {
    let root = game::assets::baked_root();
    let lvl = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../extracted/bf2/lvl/DATA/_LVL_PS2/SIDE/CIS.LVL"
    ));
    if !lvl.exists() || !root.join("common_bf2/hud/1playerhud.ron").exists() {
        eprintln!("SKIP: SBD disc or HUD bake absent");
        return;
    }
    let out = std::env::temp_dir().join(format!("openbf-sbd-hud-{}", std::process::id()));
    for (weapon, mesh) in [
        ("cis_weap_inf_wrist_blaster", "hud_cis_wristblaster"),
        ("cis_weap_inf_wrist_trishot", "hud_cis_trishot"),
        ("cis_weap_inf_wrist_rocket", "hud_cis_wristrocket"),
    ] {
        bake::soldier::bake_weapon(lvl.to_str().unwrap(), mesh, out.to_str().unwrap(), None)
            .unwrap();
        let file = gltf::Gltf::open(out.join(format!("weapons/{mesh}.glb"))).unwrap();
        assert!(file
            .meshes()
            .flat_map(|m| m.primitives().collect::<Vec<_>>())
            .all(|p| p.get(&gltf::Semantic::Colors(0)).is_some()));
        assert!(
            root.join(format!("side_bf2/weapons/{mesh}.glb")).exists(),
            "rebake the SBD HUD models"
        );
        let row = ui::hud::HudWeapon {
            key: weapon.into(),
            class: weapon.into(),
            icon: None,
            name: String::new(),
            total: if weapon.ends_with("trishot") {
                None
            } else {
                Some(3)
            },
            clip_fraction: 1.0,
        };
        let base = ui::hud::HudModel {
            hp: 300.0,
            hp_max: 300.0,
            weapon: Some(ui::hud::HudWeapon {
                key: "cis_weap_inf_wrist_blaster".into(),
                class: "cis_weap_inf_wrist_blaster".into(),
                total: Some(250),
                ..row.clone()
            }),
            ..Default::default()
        };
        let model = if weapon.ends_with("rocket") {
            ui::hud::HudModel {
                weapon2: Some(row),
                ..base
            }
        } else {
            ui::hud::HudModel {
                weapon: Some(row),
                ..base
            }
        };
        let mut view = game::hud_feed::HudView::load(&root, render::HEADLESS_FORMAT, None).unwrap();
        view.update(&model, 10.0, [1280, 720]);
        view.update(&model, 10.0, [1280, 720]);
        let bounds = view
            .bounds(if weapon.ends_with("rocket") {
                "player1weapon2mesh"
            } else {
                "player1weapon1mesh"
            })
            .expect("SBD HUD icon drawn");
        assert!(bounds[2] > bounds[0] && bounds[3] > bounds[1]);
    }
    std::fs::remove_dir_all(out).unwrap();
}

#[test]
fn sbd_shots_reach_audible_original_takes_on_both_map_sets() {
    for (game, map) in [("bf1", "bf1_geo1"), ("bf2", "bf2_geo1")] {
        let Some((sim, player, _)) = battle(game) else {
            continue;
        };
        let mut walk = game::walk::WalkSim::new(7, Vec3::ZERO, None);
        walk.sim = sim;
        walk.player = player;
        let mut lib = game::sounds::SoundLib::load(&game::assets::baked_root().join(map))
            .expect("map sounds");
        let mut fire = |walk: &mut game::walk::WalkSim, bits, expected: &str| {
            let mut heard = Vec::new();
            for t in 0..10 {
                step(&mut walk.sim, if t == 0 { bits } else { 0 });
                let ears = game::event_sounds::listener(walk).pos;
                game::event_sounds::each(walk, ears, |name, pos| {
                    if name == expected {
                        let mut starts = Vec::new();
                        lib.start(game::classdb::fnv1a_lowercase(name), pos, ears, &mut starts);
                        heard.extend(starts);
                    }
                });
            }
            assert_eq!(heard.len(), 1, "{map}: {expected} must produce one audible take per shot, including all three tri-shot bolts");
            assert!(
                heard[0].gain > 0.5,
                "{map}: {expected} own shot gain {} at {:?}, player {:?}",
                heard[0].gain,
                heard[0].pos,
                walk.sim.units[player.index as usize].soldier.pos
            );
            let decoded = audio::decode(&heard[0].path).expect("original take decodes");
            assert!(decoded.frames() > 1000);
            let ears = heard[0].ears(&game::event_sounds::listener(walk));
            assert!(
                ears.0.max(ears.1) > 0.1,
                "{map}: audible mixer gains {ears:?}"
            );
            println!(
                "{map}: {expected} gain {} {}",
                heard[0].gain,
                heard[0].path.display()
            );
        };
        let names = if game == "bf1" {
            [
                "cis_weap_inf_blaster_fire",
                "cis_weap_inf_wrist_shrapnel",
                "cis_weap_inf_rocket_fire",
            ]
        } else {
            [
                "cis_weap_inf_wrist_blaster_fire",
                "cis_weap_inf_wrist_trishot_fire",
                "cis_weap_inf_wrist_rocket_fire",
            ]
        };
        fire(&mut walk, buttons::FIRE, names[0]);
        step(&mut walk.sim, buttons::SWITCH_WEAPON);
        fire(&mut walk, buttons::FIRE, names[1]);
        fire(&mut walk, buttons::SECONDARY_FIRE, names[2]);
    }
}

#[test]
fn every_baked_battle_can_resolve_the_canonical_sbd_sounds() {
    let root = game::assets::baked_root();
    if !root.join("bf2_geo1/sounds/sounds.ron").is_file() {
        eprintln!("SKIP: original SBD sound bake absent");
        return;
    }
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.join("manifest.ron").exists() {
            continue;
        }
        let Some(mut lib) = game::sounds::SoundLib::load(&dir) else {
            continue;
        };
        let head: manifest::ManifestHead =
            ron::from_str(&std::fs::read_to_string(dir.join("manifest.ron")).unwrap()).unwrap();
        let names: &[&str] = if head.source_game == "bf1" {
            &[
                "cis_weap_inf_blaster_fire",
                "cis_weap_inf_wrist_shrapnel",
                "cis_weap_inf_rocket_fire",
                "cis_weap_inf_equip_med",
                "cis_weap_inf_reload_med",
                "rep_weap_inf_equip_sm",
            ]
        } else {
            &[
                "cis_weap_inf_wrist_blaster_fire",
                "cis_weap_inf_wrist_trishot_fire",
                "cis_weap_inf_wrist_rocket_fire",
                "com_weap_inf_equip_med",
                "com_weap_inf_equip_lg",
            ]
        };
        for name in names {
            let mut starts = Vec::new();
            lib.start(
                game::classdb::fnv1a_lowercase(name),
                Vec3::ZERO,
                Vec3::ZERO,
                &mut starts,
            );
            for _ in 0..15 {
                lib.tick(core::TICK_DT, Vec3::ZERO, &mut starts);
            }
            assert_eq!(starts.len(), 1, "{}: {name}", dir.display());
            assert!(starts[0].gain > 0.5);
            assert!(audio::decode(&starts[0].path).unwrap().frames() > 1000);
        }
        checked += 1;
    }
    assert!(checked >= 32, "only {checked} baked maps checked");
    println!("SBD fire and equip/reload sounds resolve and decode on {checked} maps");
}

#[test]
fn rocket_reload_uses_its_own_sound_while_trishot_is_selected() {
    for game in ["bf1", "bf2"] {
        let Some((sim, player, _)) = battle(game) else {
            continue;
        };
        let mut walk = game::walk::WalkSim::new(7, Vec3::ZERO, None);
        walk.sim = sim;
        walk.player = player;
        step(&mut walk.sim, buttons::SWITCH_WEAPON);
        let rocket = walk
            .sim
            .unit(player)
            .unwrap()
            .held
            .iter()
            .find(|w| w.channel == 1)
            .unwrap()
            .class;
        let expected = if game == "bf1" {
            "cis_weap_inf_reload_med"
        } else {
            "com_weap_inf_equip_lg"
        };
        let mut reloads = 0;
        let mut heard = 0;
        for t in 0..80 {
            step(
                &mut walk.sim,
                if t == 0 { buttons::SECONDARY_FIRE } else { 0 },
            );
            for event in &walk.sim.events {
                if let Event::Reload { unit, weapon_class } = event {
                    assert_eq!(*unit, player);
                    assert_eq!(*weapon_class as usize, rocket);
                    reloads += 1;
                }
            }
            game::event_sounds::each(&walk, Vec3::ZERO, |name, _| {
                heard += usize::from(name == expected);
            });
        }
        assert_eq!(reloads, 1, "{game}");
        assert_eq!(heard, 1, "{game}: {expected}");
    }
}

#[test]
fn wrist_rocket_draws_original_trail_without_a_placeholder_box_on_both_map_sets() {
    let root = game::assets::baked_root();
    if !root.join("side_bf2/effects/cis/effects.ron").exists() {
        eprintln!("SKIP: original SBD effect bake absent");
        return;
    }
    let source: manifest::FxSet = ron::from_str(
        &std::fs::read_to_string(root.join("side_bf2/effects/cis/effects.ron")).unwrap(),
    )
    .unwrap();
    let original = game::fx::Library::from_sets(
        game::fx::emitter::Era::Bf2,
        vec![source],
        &mut game::fx::emitter::Lcg(1),
    );
    let trail = "cis_sfx_sbd_missiles_red";
    let mut captures = Vec::new();
    for (game, map) in [("bf1", "bf1_geo1"), ("bf2", "bf2_geo1")] {
        let Some((mut sim, player, target)) = battle(game) else {
            continue;
        };
        sim.unit_mut(target).unwrap().soldier.pos.z = -300.0;
        let mut fx = game::fx::Fx::load(&root, &root.join(map), game, &["cis", "rep"]);
        assert_eq!(
            fx.library().unwrap().get(trail).unwrap().emitters,
            original.get(trail).unwrap().emitters
        );
        let mut walk = game::walk::WalkSim::new(7, Vec3::ZERO, None);
        walk.sim = sim;
        walk.player = player;
        for t in 0..12 {
            step(
                &mut walk.sim,
                if t == 0 { buttons::SECONDARY_FIRE } else { 0 },
            );
            fx.ingest_sim(&walk.sim);
            fx.update(core::TICK_DT);
        }
        assert_eq!(walk.sim.bolts.len(), 1);
        assert!(
            walk.bolt_draws().is_empty(),
            "{map}: wrist rocket placeholder"
        );
        let effect = fx.live().iter().find(|e| e.name() == Some(trail)).unwrap();
        let particles = effect.particles();
        assert!(particles.iter().any(|p| !p.is_empty()));
        let mut draws = Vec::new();
        effect.draws(
            effect.frame.pos + Vec3::new(1.0, 1.0, 5.0),
            Vec3::NEG_Z,
            &mut |_| None,
            &mut draws,
        );
        assert!(
            !draws.is_empty(),
            "{map}: original trail must reach renderer"
        );
        captures.push((effect.emitted(), particles));
    }
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0], captures[1]);
}

/// The maintainer 2026-10-27: "it is possible to fire 2 wrist rockets right after each
/// other". However the trigger is worked (tapped every 2 or 5 updates, or
/// held), one rocket leaves per 2 s reload: 68-70 updates apart (the 0.21 s
/// wind-up and the 2.0 s ReloadTime), on both map sets.
#[test]
fn wrist_rockets_leave_one_per_reload_however_the_trigger_is_worked() {
    for game in ["bf2", "bf1"] {
        for pattern in [2u32, 5, 0] {
            let Some((mut sim, shooter, _)) = battle(game) else {
                return;
            };
            let mut ticks = Vec::new();
            for t in 0..240u32 {
                let press = pattern == 0 || t % pattern == 0;
                let shots = step(&mut sim, if press { buttons::SECONDARY_FIRE } else { 0 });
                let u = sim.unit(shooter).unwrap();
                let rocket = u.held[u.cur1.unwrap() as usize].class;
                if shots.iter().any(|(w, _)| *w == rocket) {
                    ticks.push(t);
                }
            }
            println!("{game} every {pattern}: rockets at {ticks:?}");
            assert!(!ticks.is_empty(), "{game} {pattern}");
            for w in ticks.windows(2) {
                assert!(w[1] - w[0] >= 66, "{game} {pattern}: rockets at {ticks:?}");
            }
        }
    }
}

/// The maintainer 2026-10-27: a wrist rocket "square in the chest" seemed to take half
/// a clone's health. On BF2 Coruscant's own collision, a rocket from 3 m into
/// a BF2 clone's chest deals its direct hit (100: it lands in its second
/// update, so a tick old, and the age that raises it toward 250 at 0.5 s
/// counts only the updates it came through; card SGDMG) and then its
/// blast's 250 (inner radius 1 m): 350 of his 400.
#[test]
fn a_wrist_rocket_to_the_chest_on_coruscant_deals_its_hit_and_its_blast() {
    let root = game::assets::baked_root();
    let map = root.join("bf2_cor1");
    if !map.join("manifest.ron").exists() {
        eprintln!("SKIP: no baked BF2 Coruscant");
        return;
    }
    let (scene, _) = game::assets::load_battle_scene(&map, "con").unwrap();
    let layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    let db = load_classdb(&root.join("classes"), "bf2", &["rep", "cis"]).unwrap();
    let sbd = ClassRoster::build(&db, "cis_inf_rifleman", 1, None).unwrap();
    let clone = ClassRoster::build(&db, "rep_inf_ep3_rifleman", 1, None).unwrap();
    let mut checked = 0;
    for spot in layout.cps.iter().take(2).map(|c| c.pos) {
        let mut sim = Sim::empty(7);
        sim.ground_rules = core::soldier::GroundRules::Bf2;
        sim.heightfield = scene.heightfield();
        scene.install_collision(&mut sim);
        let sc = sbd.register(&mut sim, &mut Vec::new());
        let cc = clone.register(&mut sim, &mut Vec::new());
        let feet = |at: Vec3| Vec3::new(at.x, sim.ground_under(at + Vec3::Y * 2.0, false), at.z);
        let (f0, f1) = (feet(spot), feet(spot - Vec3::Z * 3.0));
        let shooter = sim
            .spawn(&sc.spec(0, f0, 0.0, Controller::Player { slot: 0 }, 0, 0))
            .unwrap();
        let target = sim
            .spawn(&cc.spec(1, f1, 0.0, Controller::Ai, 0, 0))
            .unwrap();
        for _ in 0..20 {
            step(&mut sim, 0);
        }
        let t = sim.unit(target).unwrap().soldier.pos;
        let (eye, _) = sim
            .unit(shooter)
            .unwrap()
            .aim_ray(sim.ground_rules, sim.views());
        let chest = t + Vec3::Y * 1.1;
        let flat = Vec2::new(chest.x - eye.x, chest.z - eye.z);
        let mut dmg = Vec::new();
        for k in 0..60 {
            let u = sim.unit_mut(shooter).unwrap();
            u.soldier.yaw = (-flat.x).atan2(-flat.y);
            u.soldier.pitch = (chest.y - eye.y).atan2(flat.length());
            sim.unit_mut(target).unwrap().soldier.vel = Vec3::ZERO;
            step(&mut sim, if k == 0 { buttons::SECONDARY_FIRE } else { 0 });
            dmg.extend(sim.events.iter().filter_map(|e| match e {
                Event::Hit { victim, damage, .. } if *victim == target => Some(*damage),
                _ => None,
            }));
        }
        let hp = sim.unit(target).unwrap().health.cur;
        println!("cp at {spot:.0?}: hits {dmg:?}, {hp} of 400 left");
        assert_eq!(dmg, vec![100.0, 250.0], "at {spot:?}");
        assert_eq!(hp, 50.0);
        checked += 1;
    }
    assert_eq!(checked, 2);
}
