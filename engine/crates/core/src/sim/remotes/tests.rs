use super::*;
use crate::remote::tests::{Props, BF1_DROID, BF2_DROID};
use crate::remote::{Destruct, DroidClass, RemoteWeapon, Salvo};
use crate::sim::input::AimTarget;
use crate::sim::tests::flat_ground;
use crate::unit::{SlotSpec, UnitSpec};
use crate::weapon::{Dispenser, ExplosionClass, WeaponClass, DEG};
use crate::TICK_DT;

const FORWARD: PadState = PadState {
    lx: 0,
    ly: i16::MAX,
    rx: 0,
    ry: 0,
    buttons: 0,
    mx: 0,
    my: 0,
};

fn setup(bf2: bool) -> (Sim, UnitHandle, usize) {
    let mut sim = Sim::empty(42);
    sim.heightfield = Some(flat_ground());
    let mut droid = DroidClass::from_props(&Props(if bf2 { BF2_DROID } else { BF1_DROID }), bf2);
    droid.explosion = Some(ExplosionClass {
        damage: 50.0,
        damage_inner: 3.0,
        damage_outer: 5.0,
        ..Default::default()
    });
    let mut blaster = WeaponClass::bf1_rifle();
    blaster.name = "remotedroid_blaster".into();
    blaster.rounds_per_clip = 0;
    blaster.trigger_single = true;
    blaster.ordnance.max_damage = 10.0;
    blaster.ordnance.velocity = 60.0;
    blaster.ordnance.life_span = 1.0;
    let mut w = WeaponClass::bf1_rifle();
    w.name = "remotedroid".into();
    w.rounds_per_clip = 1;
    Dispenser::new(&mut w, &Props(&[]), bf2, ());
    let mut r = RemoteWeapon::new(&Props(&[("0xc43b77dd", "1")]), droid, blaster);
    let mut shell = WeaponClass::default();
    shell.ordnance.velocity = 300.0;
    shell.ordnance.life_span = 3.0;
    shell.ordnance.max_damage = 1000.0;
    shell.ordnance.explosion = Some(ExplosionClass {
        damage: 500.0,
        damage_inner: 10.5,
        damage_outer: 10.5,
        ..Default::default()
    });
    r.destruct = Some(Destruct {
        charge_time: 3.0,
        charge_sound: None,
        salvo: Some(Salvo {
            yaw: 0.0,
            pitch: 70.0 * DEG,
            distance: 600.0,
            count: 4,
            delay: 0.75,
            initial: 0.0,
            spread: 2.5,
            shell: Box::new(shell),
        }),
        burst: None,
        burst_sound: None,
    });
    w.remote = Some(Box::new(r));
    let weapon = sim.add_weapon_class(w);
    let mut spec = UnitSpec::player_in_slot(0, 0, Vec3::new(5.0, 0.0, 5.0));
    spec.slots = vec![SlotSpec {
        slot: 3,
        channel: 1,
        weapon_class: weapon,
        clips: 1,
    }]
    .into();
    let owner = sim.spawn(&spec).unwrap();
    (sim, owner, weapon)
}

fn press(sim: &mut Sim, buttons: u32) {
    sim.step(&[PadState {
        buttons,
        ..Default::default()
    }]);
}

fn run(sim: &mut Sim, pad: PadState, secs: f32) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..(secs / TICK_DT).round() as usize {
        sim.step(&[pad]);
        events.extend(sim.events.iter().cloned());
    }
    events
}

fn launch(sim: &mut Sim) {
    press(sim, buttons::SECONDARY_FIRE);
    for _ in 0..30 {
        if !sim.droids.is_empty() {
            return;
        }
        press(sim, 0);
    }
    panic!("no droid launched");
}

fn gone(events: &[Event]) -> Vec<DroidEnd> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::DroidGone { end, .. } => Some(*end),
            _ => None,
        })
        .collect()
}

#[test]
fn player_launches_steers_hands_back_and_retakes_the_droid() {
    for bf2 in [false, true] {
        let (mut sim, owner, weapon) = setup(bf2);
        launch(&mut sim);
        assert!(sim.droids[0].controlled, "bf2={bf2}");
        assert_eq!(
            sim.controlled_droid(owner).map(|d| d.weapon_class),
            Some(weapon)
        );
        let feet = sim.unit(owner).unwrap().soldier.pos;
        let from = sim.droids[0].pos;
        run(&mut sim, FORWARD, 2.0);
        let d = &sim.droids[0];
        assert!(
            d.pos.z < from.z - 6.0,
            "bf2={bf2}: {:?} -> {:?}",
            from,
            d.pos
        );
        let want = if bf2 { 2.0 } else { 1.75 };
        assert!((d.pos.y - want).abs() < 0.3, "{:?}", d.pos);
        let still = sim.unit(owner).unwrap().soldier.pos;
        assert!(still.distance(feet) < 0.05, "{feet:?} -> {still:?}");
        press(&mut sim, buttons::ACTION);
        assert!(!sim.droids[0].controlled);
        let parked = sim.droids[0].pos;
        run(&mut sim, FORWARD, 1.0);
        assert!(sim.unit(owner).unwrap().soldier.pos.z < feet.z - 1.0);
        assert!(sim.droids[0].pos.distance(parked) < 3.0, "bf2={bf2}");
        press(&mut sim, buttons::SECONDARY_FIRE);
        assert!(sim.droids[0].controlled);
        assert_eq!(sim.droids.len(), 1);
    }
}

#[test]
fn the_droid_runs_down_and_returns_control_with_no_second_launch() {
    let (mut sim, owner, _) = setup(false);
    launch(&mut sim);
    let events = run(&mut sim, PadState::default(), 41.0);
    assert_eq!(gone(&events), [DroidEnd::Expired]);
    assert!(sim.droids.is_empty());
    assert!(sim.controlled_droid(owner).is_none());
    let feet = sim.unit(owner).unwrap().soldier.pos;
    run(&mut sim, FORWARD, 1.0);
    assert!(sim.unit(owner).unwrap().soldier.pos.z < feet.z - 1.0);
    press(&mut sim, buttons::SECONDARY_FIRE);
    run(&mut sim, PadState::default(), 2.0);
    assert!(sim.droids.is_empty());
}

#[test]
fn blaster_bolts_kill_an_enemy_for_the_droids_owner() {
    let (mut sim, owner, _) = setup(false);
    launch(&mut sim);
    let enemy = sim
        .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -10.0)))
        .unwrap();
    let d = &mut sim.droids[0];
    d.pos = Vec3::new(0.0, 1.75, 0.0);
    d.vel = Vec3::ZERO;
    d.yaw = 0.0;
    d.pitch = crate::fmath::atan2(1.1 - 1.75, 10.0);
    let mut kills = Vec::new();
    for t in 0..300 {
        let buttons = if t % 2 == 0 { buttons::FIRE } else { 0 };
        press(&mut sim, buttons);
        kills.extend(sim.events.iter().filter_map(|e| match e {
            Event::Kill { shooter, victim } => Some((*shooter, *victim)),
            _ => None,
        }));
    }
    assert_eq!(kills, [(owner, enemy)]);
}

#[test]
fn blasts_and_bolts_destroy_the_droid_by_its_droid_scale() {
    let (mut sim, _, _) = setup(false);
    launch(&mut sim);
    sim.droids[0].pos = Vec3::new(0.0, 1.75, -30.0);
    let pos = sim.droids[0].pos;
    let mut vehicle_only = ExplosionClass {
        damage: 1000.0,
        damage_inner: 2.0,
        damage_outer: 2.0,
        ..Default::default()
    };
    vehicle_only.scales[crate::weapon::HealthType::Droid as usize] = 0.0;
    let health = sim.droids[0].health;
    sim.explode(&vehicle_only, pos, None, Some(1));
    assert_eq!(sim.droids[0].health, health);
    let blast = ExplosionClass {
        damage: 100.0,
        damage_inner: 2.0,
        damage_outer: 2.0,
        ..Default::default()
    };
    sim.explode(&blast, pos, None, Some(1));
    let events = run(&mut sim, PadState::default(), TICK_DT);
    assert_eq!(gone(&events), [DroidEnd::Destroyed]);
}

#[test]
fn one_press_counts_down_three_seconds_to_the_salvo() {
    let (mut sim, owner, _) = setup(false);
    launch(&mut sim);
    sim.droids[0].pos = Vec3::new(0.0, 1.75, -40.0);
    let enemy = sim
        .spawn(&UnitSpec::trooper(1, Vec3::new(1.0, 0.0, -40.0)))
        .unwrap();
    let charges = |events: &[Event]| {
        events
            .iter()
            .filter(|e| matches!(e, Event::DroidCharge { .. }))
            .count()
    };
    press(&mut sim, buttons::SECONDARY_FIRE);
    assert_eq!(charges(&sim.events), 1);
    let waiting = run(&mut sim, PadState::default(), 2.9);
    assert!(gone(&waiting).is_empty());
    let hold = PadState {
        buttons: buttons::SECONDARY_FIRE,
        ..Default::default()
    };
    let rest = run(&mut sim, hold, 0.2);
    assert_eq!(charges(&waiting) + charges(&rest), 0);
    assert_eq!(gone(&rest), [DroidEnd::Destruct]);
    assert_eq!(sim.strikes.len(), 1);
    assert!(sim.unit(enemy).is_some());
    let after = run(&mut sim, PadState::default(), 5.0);
    assert!(sim.strikes.is_empty());
    assert!(after.iter().any(
        |e| matches!(e, Event::Kill { shooter, victim } if *shooter == owner && *victim == enemy)
    ));
}

#[test]
fn the_launch_and_retake_presses_do_not_start_the_countdown() {
    for bf2 in [false, true] {
        let (mut sim, _, _) = setup(bf2);
        let hold = PadState {
            buttons: buttons::SECONDARY_FIRE,
            ..Default::default()
        };
        let launched = run(&mut sim, hold, 1.0);
        assert_eq!(sim.droids.len(), 1, "bf2={bf2}");
        press(&mut sim, buttons::ACTION);
        press(&mut sim, 0);
        let retaken = run(&mut sim, hold, 4.0);
        assert!(gone(&retaken).is_empty(), "bf2={bf2}: {:?}", gone(&retaken));
        assert!(sim.droids[0].controlled, "bf2={bf2}");
        assert!(sim.droids[0].charge.is_none(), "bf2={bf2}");
        assert!(gone(&launched).is_empty() && gone(&retaken).is_empty());
    }
}

#[test]
fn a_bot_flies_its_droid_at_the_nearest_enemy_and_fires() {
    let (mut sim, owner, weapon) = setup(false);
    sim.unit_mut(owner).unwrap().controller = Controller::Ai;
    let enemy = sim
        .spawn(&UnitSpec::trooper(1, Vec3::new(5.0, 0.0, -25.0)))
        .unwrap();
    sim.unit_mut(owner).unwrap().ai.target = Some(enemy);
    assert_eq!(
        crate::ai::ability_input(&sim, owner),
        Some(buttons::SECONDARY_FIRE)
    );
    sim.launch_droid(owner.index as usize, weapon, 0.0);
    assert_eq!(crate::ai::ability_input(&sim, owner), None);
    let blaster = sim.weapon_classes[weapon]
        .remote
        .as_ref()
        .unwrap()
        .blaster_index;
    let events = run(&mut sim, PadState::default(), 12.0);
    let shots = events
        .iter()
        .filter(|e| matches!(e, Event::Shot { unit, weapon_class, .. } if *unit == owner && *weapon_class as usize == blaster))
        .count();
    assert!(shots > 5, "{shots}");
    assert!(sim.unit(enemy).is_none_or(|u| u.health.cur < u.health.max));
}

#[test]
fn droids_are_hashed_and_replay_exactly() {
    let play = || {
        let (mut sim, _, _) = setup(false);
        launch(&mut sim);
        run(&mut sim, FORWARD, 1.0);
        sim
    };
    let (mut a, b) = (play(), play());
    assert_eq!(a.hash(), b.hash());
    a.droids[0].pos.x += 0.01;
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn the_look_preview_is_the_turn_the_next_tick_takes() {
    let (mut sim, owner, _) = setup(false);
    launch(&mut sim);
    for (x, y) in [(0.3, -0.1), (-0.2, 2.0), (0.0, 0.05)] {
        let pad = PadState {
            mx: PadState::quantize_look(x),
            my: PadState::quantize_look(y),
            buttons: buttons::MOUSE_AIM,
            ..Default::default()
        };
        let Some(crate::sim::LookPreview::Droid { droid, yaw, pitch }) =
            sim.look_preview(owner, &pad)
        else {
            panic!("no droid preview");
        };
        sim.step(&[pad]);
        let d = &sim.droids[droid];
        assert_eq!((d.yaw, d.pitch), (yaw, pitch));
    }
}

/// The reticle's pick while flying is the droid's: an enemy before the
/// droid is picked as an enemy (red), a team mate as a friend, nothing
/// when it looks away, whatever the idle body faces; the droid pulls no
/// aim-assist magnet.
#[test]
fn the_reticle_picks_what_the_droid_looks_at() {
    let (mut sim, owner, _) = setup(false);
    launch(&mut sim);
    sim.units[owner.index as usize].soldier.yaw = std::f32::consts::PI;
    let aim_at = |sim: &mut Sim, yaw: f32| {
        let d = &mut sim.droids[0];
        d.pos = Vec3::new(0.0, 1.75, 0.0);
        d.vel = Vec3::ZERO;
        d.yaw = yaw;
        d.pitch = crate::fmath::atan2(1.1 - 1.75, 10.0);
        press(sim, 0);
    };
    let enemy = sim
        .spawn(&UnitSpec::trooper(1, Vec3::new(0.0, 0.0, -10.0)))
        .unwrap();
    aim_at(&mut sim, 0.0);
    assert_eq!(sim.reticle_pick[0], Some((AimTarget::Unit(enemy), -1)));
    assert_eq!(sim.magnet[0], None, "no magnet while flying");
    aim_at(&mut sim, std::f32::consts::FRAC_PI_2);
    assert_eq!(sim.reticle_pick[0], None, "looking away: white");
    sim.units[enemy.index as usize].alive = false;
    let friend = sim
        .spawn(&UnitSpec::trooper(0, Vec3::new(0.0, 0.0, -10.0)))
        .unwrap();
    aim_at(&mut sim, 0.0);
    assert_eq!(sim.reticle_pick[0], Some((AimTarget::Unit(friend), 1)));
}
