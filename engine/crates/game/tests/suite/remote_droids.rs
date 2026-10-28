use core::heightfield::Heightfield;
use core::remote::DroidEnd;
use core::sim::{buttons, Event, PadState, Sim};
use core::unit::Controller;
use core::weapon::{HealthType, WeaponClass, DEG};
use game::roster::{load_classdb, ClassRoster};
use glam::Vec3;

fn flat() -> Heightfield {
    Heightfield {
        size: 2,
        unit: 4000.0,
        offset_x: 2000.0,
        offset_z: 2000.0,
        heights: vec![0.0; 4],
    }
}

fn check_disc_values(game: &str, name: &str, w: &WeaponClass) {
    let r = w.remote.as_ref().unwrap();
    let c = &r.droid;
    let bf2 = game == "bf2";
    assert!(r.take_control, "{game} {name}");
    assert_eq!(r.launch_speed(0.0), 3.0, "{game} {name}");
    assert_eq!(c.geometry.as_deref(), Some("com_weap_probedroid"));
    assert_eq!(c.pitch_limits, [-50.0 * DEG, 50.0 * DEG]);
    assert_eq!((c.lift_spring, c.lift_damp), (10.0, 3.0));
    assert_eq!(
        (c.max_speed, c.max_strafe_speed, c.max_turn_speed),
        (9.0, 7.0, 2.0)
    );
    assert_eq!(c.health_type, HealthType::Droid);
    let destruct = r.destruct.as_ref().unwrap();
    assert_eq!(destruct.charge_time, 3.0);
    let blaster = &r.blaster;
    assert!(blaster.trigger_single);
    assert_eq!(blaster.ai_range[1..], [16.0, 32.0]);
    if bf2 {
        assert_eq!((c.max_health, c.add_health), (100.0, -1.0));
        assert_eq!((c.set_altitude, c.acceleration), (2.0, 25.0));
        assert_eq!(c.energy_restore, -1.7);
        assert_eq!(c.track_offset, Vec3::new(0.0, -0.25, 1.8));
        assert_eq!(c.explosion.as_ref().map(|e| e.damage), Some(0.0));
        assert_eq!(blaster.shot_delay, [0.15, 0.15]);
        assert_eq!(
            (blaster.ordnance.velocity, blaster.ordnance.max_damage),
            (160.0, 60.0)
        );
        let burst = destruct.burst.as_ref().expect("incinerator burst");
        assert_eq!((burst.damage, burst.damage_outer), (10000.0, 12.0));
        assert_eq!(burst.scales[HealthType::Person as usize], 1.0);
        assert_eq!(burst.scales[HealthType::Vehicle as usize], 0.0);
        assert!(destruct.salvo.is_none());
        assert_eq!(
            destruct.burst_sound.as_deref(),
            Some("com_sfx_weap_remotedroid_exp")
        );
    } else {
        assert_eq!((c.max_health, c.add_health), (40.0, -1.0));
        let altitude = if name.starts_with("imp_") { 2.0 } else { 1.75 };
        assert_eq!((c.set_altitude, c.acceleration), (altitude, 70.0));
        assert_eq!(c.energy_restore, 0.0);
        assert_eq!(c.track_offset, Vec3::new(0.0, -0.25, 1.5));
        let e = c.explosion.as_ref().expect("exp2");
        assert_eq!((e.damage, e.damage_inner, e.damage_outer), (50.0, 3.0, 5.0));
        assert_eq!(blaster.shot_delay, [0.2, 0.2]);
        assert_eq!(
            (blaster.ordnance.velocity, blaster.ordnance.max_damage),
            (60.0, 10.0)
        );
        let s = destruct.salvo.as_ref().expect("orbital salvo");
        assert_eq!(
            (s.count, s.delay, s.distance, s.spread),
            (4, 0.75, 600.0, 2.5)
        );
        assert!((s.pitch - 70.0 * DEG).abs() < 1e-6);
        assert_eq!(
            (s.shell.ordnance.velocity, s.shell.ordnance.max_damage),
            (300.0, 1000.0)
        );
        let x = s.shell.ordnance.explosion.as_ref().expect("salvo exp");
        assert_eq!((x.damage, x.damage_outer), (500.0, 10.5));
        assert!(destruct.burst.is_none());
    }
}

fn fly(game: &str, name: &str, roster: &ClassRoster, slot: u8, lifetime: f32) {
    let mut sim = Sim::empty(42);
    sim.heightfield = Some(flat());
    let class = roster.register(&mut sim, &mut Vec::new());
    let owner = sim
        .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
        .unwrap();
    let u = &mut sim.units[owner.index as usize];
    for h in &mut u.held {
        h.state.active = h.slot == slot;
    }
    u.cur1 = Some(u.held.iter().position(|h| h.slot == slot).unwrap() as u8);
    let pad = |buttons: u32, ly: i16| PadState {
        buttons,
        ly,
        ..Default::default()
    };
    sim.step(&[pad(buttons::SECONDARY_FIRE, 0)]);
    for _ in 0..30 {
        if !sim.droids.is_empty() {
            break;
        }
        sim.step(&[pad(0, 0)]);
    }
    assert_eq!(sim.droids.len(), 1, "{game} {name}: launched");
    assert!(
        sim.controlled_droid(owner).is_some(),
        "{game} {name}: control"
    );
    let launched = sim.tick;
    let feet = sim.unit(owner).unwrap().soldier.pos;
    for _ in 0..60 {
        sim.step(&[pad(0, i16::MAX)]);
    }
    let d = &sim.droids[0];
    assert!(d.pos.z < -8.0, "{game} {name}: steered to {:?}", d.pos);
    assert!(sim.unit(owner).unwrap().soldier.pos.distance(feet) < 0.05);
    sim.step(&[pad(buttons::ACTION, 0)]);
    assert!(sim.controlled_droid(owner).is_none(), "{game} {name}: back");
    let mut end = None;
    for _ in 0..((lifetime + 2.0) * core::TICK_HZ as f32) as usize {
        sim.step(&[pad(0, 0)]);
        end = end.or(sim.events.iter().find_map(|e| match e {
            Event::DroidGone { end, .. } => Some((*end, sim.tick)),
            _ => None,
        }));
        if end.is_some() {
            break;
        }
    }
    let (end, tick) = end.unwrap_or_else(|| panic!("{game} {name}: never expired"));
    assert_eq!(end, DroidEnd::Expired);
    let secs = (tick - launched) as f32 / core::TICK_HZ as f32;
    assert!((secs - lifetime).abs() < 0.2, "{game} {name}: {secs} s");
}

#[test]
fn every_remote_droid_class_flies_the_discs_droid() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf1_all.ron").exists() || !root.join("classes/bf2_all.ron").exists() {
        eprintln!("SKIP: requires both local baked discs");
        return;
    }
    for (game, want, lifetime) in [("bf1", 8, 40.0), ("bf2", 13, 100.0 / 1.7)] {
        let db = load_classdb(&root.join("classes"), game, &["all", "imp", "rep", "cis"]).unwrap();
        let model = root.join(format!("common_{game}/models/com_weap_probedroid.glb"));
        assert!(!game::assets::load_glb(&model).unwrap().indices.is_empty());
        let mut names: Vec<String> = db
            .names()
            .filter(|n| game::roster::is_soldier_chain(&db, n))
            .map(str::to_owned)
            .collect();
        names.sort();
        names.dedup();
        let mut found = 0;
        for name in &names {
            let Ok(roster) = ClassRoster::build(&db, name, 1, None) else {
                continue;
            };
            let Some(held) = roster.held.iter().find(|h| h.weapon.remote.is_some()) else {
                continue;
            };
            found += 1;
            assert_eq!((held.channel, held.clips), (1, 1), "{game} {name}");
            check_disc_values(game, name, &held.weapon);
            fly(game, name, &roster, held.slot, lifetime);
        }
        assert_eq!(found, want, "{game}: classes with a remote droid");
    }
}

#[test]
fn one_tap_calls_the_destruct_three_seconds_later() {
    let root = game::assets::baked_root();
    if !root.join("classes/bf1_all.ron").exists() || !root.join("classes/bf2_all.ron").exists() {
        eprintln!("SKIP: requires both local baked discs");
        return;
    }
    for (game, name) in [("bf1", "all_inf_marksman"), ("bf2", "rep_inf_ep3_officer")] {
        let db = load_classdb(&root.join("classes"), game, &["all", "imp", "rep", "cis"]).unwrap();
        let roster = ClassRoster::build(&db, name, 1, None).unwrap();
        let slot = roster
            .held
            .iter()
            .find(|h| h.weapon.remote.is_some())
            .unwrap()
            .slot;
        let mut sim = Sim::empty(42);
        sim.heightfield = Some(flat());
        let class = roster.register(&mut sim, &mut Vec::new());
        let owner = sim
            .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
            .unwrap();
        let u = &mut sim.units[owner.index as usize];
        for h in &mut u.held {
            h.state.active = h.slot == slot;
        }
        u.cur1 = Some(u.held.iter().position(|h| h.slot == slot).unwrap() as u8);
        let pad = |buttons: u32| PadState {
            buttons,
            ..Default::default()
        };
        for _ in 0..30 {
            sim.step(&[pad(buttons::SECONDARY_FIRE)]);
        }
        assert!(sim.controlled_droid(owner).is_some(), "{game} {name}");
        sim.step(&[pad(0)]);
        sim.step(&[pad(buttons::SECONDARY_FIRE)]);
        let tapped = sim.tick;
        let mut end = None;
        for _ in 0..120 {
            sim.step(&[pad(0)]);
            end = end.or(sim.events.iter().find_map(|e| match e {
                Event::DroidGone { end, .. } => Some((*end, sim.tick)),
                _ => None,
            }));
        }
        let (end, tick) = end.unwrap_or_else(|| panic!("{game} {name}: no destruct"));
        assert_eq!(end, DroidEnd::Destruct, "{game} {name}");
        let secs = (tick - tapped) as f32 / core::TICK_HZ as f32;
        assert!((secs - 3.0).abs() < 0.05, "{game} {name}: {secs} s");
        assert!(sim.controlled_droid(owner).is_none());
    }
}
