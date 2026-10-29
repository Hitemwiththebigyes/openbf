use core::heightfield::Heightfield;
use core::sim::{buttons, Event, PadState, Sim};
use core::unit::{Controller, UnitSpec};
use game::roster::{load_classdb, weapon_from_db, ClassRoster};
use glam::Vec3;

#[test]
fn all_officer_variants_keep_their_disc_ability_and_apply_it_through_secondary_fire() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf2_all.ron").exists() {
        eprintln!("SKIP: BF2 baked classes are required");
        return;
    }
    let db = load_classdb(&root.join("classes"), "bf2", &["all", "imp", "rep", "cis"]).unwrap();
    let mut count = 0;
    for line in include_str!("../data/weapon_slots_bf2.txt")
        .lines()
        .filter(|l| l.contains("officer [") || l.contains("officer_"))
    {
        let name = line.split_whitespace().next().unwrap();
        if !name.contains("_inf_") {
            continue;
        }
        let roster = ClassRoster::build(&db, name, 1, None).unwrap();
        let slot = roster
            .held
            .iter()
            .find(|h| h.weapon.ability.is_some())
            .unwrap();
        let ability = slot.weapon.ability.as_ref().unwrap();
        assert_eq!(ability.radius, 10.0, "{name}");
        assert_eq!(slot.channel, 1);
        assert_eq!(slot.clips, 1);
        assert_eq!(slot.weapon.rounds_per_clip, 1);
        assert_eq!(slot.weapon.initial_salvo_delay, 0.4);
        assert_eq!(slot.weapon.reload_time, 10.0);
        assert!(slot.weapon.flash.is_none());
        match &name[..3] {
            "all" => assert_eq!(ability.regeneration, (30.0, 20.0)),
            "imp" => assert_eq!(ability.offense, (15.0, 1.5)),
            "rep" => assert_eq!(ability.defense, (20.0, 0.5)),
            "cis" => assert_eq!(
                ability.poison,
                (15.0, 20.0),
                "the disc's 30/s at POISON_RATE"
            ),
            _ => unreachable!(),
        }
        assert_eq!(ability.friends, !name.starts_with("cis"));
        assert_eq!(ability.enemies, name.starts_with("cis"));
        assert_eq!(ability.self_on_hit, !name.starts_with("cis"));
        let mut sim = Sim::empty(47);
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
        let ally = sim.spawn(&UnitSpec::trooper(0, Vec3::Z * 5.0)).unwrap();
        let enemy = sim.spawn(&UnitSpec::trooper(1, Vec3::Z * -5.0)).unwrap();
        sim.unit_mut(ally).unwrap().health.cur = 100.0;
        let k = sim
            .unit(owner)
            .unwrap()
            .held
            .iter()
            .position(|w| sim.weapon_classes[w.class].ability.is_some())
            .unwrap();
        sim.unit_mut(owner).unwrap().cur1 = Some(k as u8);
        sim.unit_mut(owner).unwrap().held[k].state.active = true;
        let mut fired = 0;
        for tick in 0..30 {
            sim.step(&[PadState {
                buttons: if tick == 0 {
                    buttons::SECONDARY_FIRE
                } else {
                    0
                },
                ..Default::default()
            }]);
            fired += sim
                .events
                .iter()
                .filter(|e| matches!(e, Event::Ability { .. }))
                .count();
        }
        assert_eq!(fired, 1, "{name}");
        assert!(sim.bolts.is_empty() && sim.stickies.is_empty());
        if name.starts_with("cis") {
            assert!(sim.unit(enemy).unwrap().health.cur < 300.0);
            assert!(!sim.unit(ally).unwrap().buffs.active());
            assert!(!sim.unit(owner).unwrap().buffs.active());
        } else {
            assert!(sim.unit(owner).unwrap().buffs.active());
            assert!(sim.unit(ally).unwrap().buffs.active());
            assert!(!sim.unit(enemy).unwrap().buffs.active());
        }
        if name.starts_with("all") {
            assert!(sim.unit(ally).unwrap().health.cur > 100.0);
        }
        count += 1;
    }
    assert_eq!(count, 14);
}

#[test]
fn hero_copies_stay_deferred_but_the_invulnerability_definition_is_preserved() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf2_all.ron").exists() {
        return;
    }
    let db = load_classdb(&root.join("classes"), "bf2", &["all", "imp", "rep", "cis"]).unwrap();
    let weapon = weapon_from_db(&db, "all_weap_inf_buff_invuln").unwrap();
    let ability = weapon.ability.unwrap();
    assert_eq!(ability.defense, (10.0, 0.05));
    assert_eq!(ability.regeneration, (10.0, 20.0));
    assert_eq!(ability.radius, 10.0);
    for hero in [
        "all_hero_leia",
        "all_hero_chewbacca",
        "all_hero_hansolo_tat",
        "cis_hero_grievous",
    ] {
        assert!(game::roster::held_slots(&db, hero, 0)
            .iter()
            .all(|w| w.weapon.ability.is_none()));
    }
}
