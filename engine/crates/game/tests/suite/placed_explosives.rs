use core::heightfield::Heightfield;
use core::mine::Detonation;
use core::sim::{buttons, Event, PadState, Sim};
use core::unit::{Controller, UnitSpec};
use game::roster::{load_classdb, ClassRoster};
use glam::Vec3;
use std::collections::BTreeSet;

fn step(sim: &mut Sim, fire: bool) {
    sim.step(&[PadState {
        buttons: if fire { buttons::SECONDARY_FIRE } else { 0 },
        ..Default::default()
    }]);
}

fn check_limit(weapon: &core::weapon::WeaponClass) {
    let d = weapon.mine.as_ref().unwrap();
    if d.item.remote {
        assert_eq!(d.max_items, 1);
        return;
    }
    let mut sim = Sim::empty(19);
    sim.heightfield = Some(Heightfield {
        size: 2,
        unit: 4000.0,
        offset_x: 2000.0,
        offset_z: 2000.0,
        heights: vec![0.0; 4],
    });
    sim.cheats.infinite_ammo = true;
    sim.cheats.no_reload = true;
    let wc = sim.add_weapon_class(weapon.clone());
    let owner = sim
        .spawn(&UnitSpec {
            slots: vec![core::unit::SlotSpec {
                slot: 2,
                channel: 1,
                weapon_class: wc,
                clips: 0,
            }]
            .into(),
            ..UnitSpec::player(0, Vec3::ZERO)
        })
        .unwrap();
    let mut first = 0;
    for n in 0..=d.max_items {
        let before = sim.next_mine_id;
        step(&mut sim, true);
        for _ in 0..180 {
            if sim.next_mine_id != before {
                break;
            }
            step(&mut sim, false);
        }
        assert_eq!(sim.next_mine_id, before + 1, "{}: place {n}", weapon.name);
        if n == 0 {
            first = sim.mines[0].id;
        }
        assert_eq!(
            sim.mines.len(),
            (n + 1).min(d.max_items) as usize,
            "{}: limit",
            weapon.name
        );
        for _ in 0..((weapon.shot_delay[0] + weapon.initial_salvo_delay + 0.1)
            * core::TICK_HZ as f32)
            .ceil() as usize
        {
            for m in &mut sim.mines {
                m.flight.life = f32::MAX;
            }
            step(&mut sim, false);
        }
        assert!(sim.unit(owner).is_some());
    }
    assert!(
        sim.mines.iter().all(|m| m.id != first),
        "{}: replaces oldest",
        weapon.name
    );
}

#[test]
fn every_disc_class_keeps_places_arms_and_triggers_its_explosives() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf1_all.ron").exists() || !root.join("classes/bf2_all.ron").exists() {
        eprintln!("SKIP: requires both local baked discs");
        return;
    }
    for (game, census, expected) in [
        ("bf1", include_str!("../data/weapon_slots_bf1.txt"), (10, 5)),
        (
            "bf2",
            include_str!("../data/weapon_slots_bf2.txt"),
            (43, 12),
        ),
    ] {
        let db = load_classdb(&root.join("classes"), game, &["all", "imp", "rep", "cis"]).unwrap();
        let mut classes = BTreeSet::new();
        let mut weapons = BTreeSet::new();
        for name in census
            .lines()
            .filter_map(|line| line.split_whitespace().next())
        {
            if db.resolve(name).is_none() {
                continue;
            }
            let slots: Vec<_> = db
                .weapon_slots(name)
                .into_iter()
                .filter(|slot| {
                    db.prop(&slot.weapon, "OrdnanceName")
                        .and_then(|o| db.engine_base(o.trim()))
                        == Some("mine")
                })
                .collect();
            if slots.is_empty() {
                continue;
            }
            let roster = ClassRoster::build(&db, name, 1, None).unwrap();
            for slot in slots {
                let held = roster
                    .held
                    .iter()
                    .find(|h| h.slot == slot.slot)
                    .unwrap_or_else(|| {
                        panic!("{game} {name}: missing explosive slot {}", slot.slot)
                    });
                let d = held.weapon.mine.as_ref().expect("mine dispenser");
                assert!(d.max_items > 0);
                for geometry in [held.geometry.as_deref(), d.item.geometry.as_deref()] {
                    let path = root.join(format!(
                        "side_{game}/weapons/{}.glb",
                        geometry.expect("explosive model")
                    ));
                    assert!(!game::assets::load_glb(&path)
                        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                        .indices
                        .is_empty());
                }
                let ord = db.prop(&slot.weapon, "OrdnanceName").unwrap().trim();
                for (reference, explosion) in [
                    (
                        db.prop(ord, "ExplosionTrigger")
                            .or_else(|| db.prop(ord, "ExplosionName"))
                            .or_else(|| db.prop_hash(ord, 0x02bb1fe0)),
                        &d.item.explosions[0],
                    ),
                    (db.prop(ord, "ExplosionExpire"), &d.item.explosions[1]),
                    (db.prop_hash(ord, 0xa2723442), &d.item.explosions[2]),
                ] {
                    assert_eq!(
                        reference.is_some(),
                        explosion.is_some(),
                        "{game} {name}: unresolved {reference:?}"
                    );
                }
                classes.insert(name.to_owned());
                weapons.insert(held.weapon.name.clone());
                check_limit(&held.weapon);
                let mut sim = Sim::empty(42);
                sim.heightfield = Some(Heightfield {
                    size: 2,
                    unit: 4000.0,
                    offset_x: 2000.0,
                    offset_z: 2000.0,
                    heights: vec![0.0; 4],
                });
                let class = roster.register(&mut sim, &mut Vec::new());
                let owner = sim
                    .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
                    .unwrap();
                let u = &mut sim.units[owner.index as usize];
                for h in &mut u.held {
                    h.state.active = h.slot == slot.slot;
                }
                u.cur1 = Some(u.held.iter().position(|h| h.slot == slot.slot).unwrap() as u8);
                for _ in 0..if d.item.remote { 16 } else { 1 } {
                    step(&mut sim, true);
                }
                for _ in 0..45 {
                    if !sim.mines.is_empty() {
                        break;
                    }
                    step(&mut sim, false);
                }
                assert_eq!(sim.mines.len(), 1, "{game} {name}: placed");
                let placed = sim.tick;
                sim.unit_mut(owner).unwrap().soldier.pos = Vec3::X * 30.0;
                for _ in 0..90 {
                    if sim.mines[0].flight.armed {
                        break;
                    }
                    step(&mut sim, false);
                }
                assert!(sim.mines[0].flight.armed, "{game} {name}: arms");
                assert!(sim.bolts.is_empty());
                if d.item.remote {
                    step(&mut sim, false);
                    step(&mut sim, true);
                    assert!(
                        sim.events.iter().any(|e| matches!(
                            e,
                            Event::MineBlast {
                                cause: Detonation::Trigger,
                                ..
                            }
                        )),
                        "{name}: remote trigger"
                    );
                } else if d.item.trigger_radius > 0.0 {
                    let pos = sim.mines[0].flight.pos;
                    sim.spawn(&UnitSpec::trooper(1, pos));
                    step(&mut sim, false);
                    assert!(
                        sim.events.iter().any(|e| matches!(
                            e,
                            Event::MineBlast {
                                cause: Detonation::Trigger,
                                ..
                            }
                        )),
                        "{name}: proximity trigger"
                    );
                } else {
                    let mut burst = None;
                    for _ in 0..(d.item.life_span * core::TICK_HZ as f32) as usize + 1 {
                        step(&mut sim, false);
                        if sim.events.iter().any(|e| {
                            matches!(
                                e,
                                Event::MineBlast {
                                    cause: Detonation::Expire,
                                    ..
                                }
                            )
                        }) {
                            burst = Some(sim.tick);
                            break;
                        }
                    }
                    let age = (burst.expect("time bomb expires") - placed) as f32 * core::TICK_DT;
                    assert!(
                        (age - d.item.life_span).abs() <= 2.0 * core::TICK_DT,
                        "{name}: {age}"
                    );
                }
                assert!(sim.mines.is_empty(), "{game} {name}: detonated");
            }
        }
        println!(
            "{game}: {} classes, {} explosive weapon kinds",
            classes.len(),
            weapons.len()
        );
        assert_eq!((classes.len(), weapons.len()), expected);
    }
}
