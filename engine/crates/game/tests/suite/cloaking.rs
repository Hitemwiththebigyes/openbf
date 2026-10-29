use core::sim::{buttons, PadState, Sim};
use core::unit::Controller;
use game::roster::{load_classdb, ClassRoster};
use glam::Vec3;

#[test]
fn bothan_variants_keep_the_disc_cloak_and_activate_it() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf2_all.ron").exists() {
        eprintln!("SKIP: requires the local baked BF2 disc");
        return;
    }
    let db = load_classdb(&root.join("classes"), "bf2", &["all"]).unwrap();
    let mut carriers = Vec::new();
    for name in db.names() {
        if db.engine_base(name) != Some("soldier") {
            continue;
        }
        if !db
            .weapon_slots(name)
            .iter()
            .any(|s| db.engine_base(&s.weapon) == Some("invisibility"))
        {
            continue;
        }
        carriers.push(name.to_owned());
        let roster = ClassRoster::build(&db, name, 1, None).unwrap();
        let held = roster
            .held
            .iter()
            .find(|h| h.weapon.cloak.is_some())
            .unwrap();
        assert_eq!(held.channel, 0);
        assert_eq!(held.weapon.name, "all_weap_inf_invisibility");
        let cloak = held.weapon.cloak.as_ref().unwrap();
        assert_eq!(
            (cloak.energy_cost, cloak.energy_drain, cloak.delay),
            (40.0, 17.0, 0.3)
        );
        assert_eq!(
            (cloak.moving, cloak.still, cloak.fade_in, cloak.fade_out),
            (0.3, 1.0, 0.6, 0.9)
        );
        assert_eq!(
            (cloak.fast_speed, cloak.slow_speed, cloak.linger),
            (9.5, 6.5, 1.0)
        );
        assert_eq!((cloak.flicker, cloak.flicker_interval), (0.6, [5.0, 10.0]));
        assert_eq!(
            held.weapon.fire_sound.as_deref(),
            Some("all_weap_invisibility_fire")
        );
        let mut sim = Sim::empty(42);
        let class = roster.register(&mut sim, &mut Vec::new());
        let h = sim
            .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
            .unwrap();
        sim.step(&[PadState {
            buttons: buttons::SWITCH_WEAPON,
            ..Default::default()
        }]);
        sim.step(&[PadState {
            buttons: buttons::FIRE,
            ..Default::default()
        }]);
        for _ in 0..15 {
            sim.step(&[PadState::default()]);
        }
        assert!(sim.unit(h).unwrap().cloak.active, "{name}");
        assert!(!sim.unit(h).unwrap().cloak.visible(), "{name}");
        assert!(sim.bolts.is_empty());
    }
    carriers.sort();
    assert_eq!(
        carriers,
        [
            "all_inf_default_officer",
            "all_inf_officer",
            "all_inf_officer_jungle",
            "all_inf_officer_snow"
        ]
    );
    let bf1 = load_classdb(&root.join("classes"), "bf1", &["all", "imp", "rep", "cis"]).unwrap();
    assert!(!bf1
        .names()
        .any(|n| bf1.engine_base(n) == Some("invisibility")));
}
