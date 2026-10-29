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

/// An enemy trooper 1 m from the droid; its health is how a test sees the
/// droid's 50-damage explosion.
fn bystander(sim: &mut Sim) -> UnitHandle {
    let at = sim.droids[0].pos.with_y(0.0) + Vec3::X;
    sim.spawn(&UnitSpec::trooper(1, at)).unwrap()
}

fn hp(sim: &Sim, h: UnitHandle) -> f32 {
    sim.unit(h).map_or(0.0, |u| u.health.cur)
}

/// Give the remote one more droid, as an ammo pickup would.
fn restock(sim: &mut Sim, owner: UnitHandle, weapon: usize) {
    let u = &mut sim.units[owner.index as usize];
    let k = u.held.iter().position(|h| h.class == weapon).unwrap();
    u.held[k].state.spare += 1;
}

#[test]
fn every_droid_times_out_quietly_at_90_s_and_hands_control_back() {
    for bf2 in [false, true] {
        let (mut sim, owner, weapon) = setup(bf2);
        launch(&mut sim);
        let launched = sim.tick;
        let max = sim.droids[0].health;
        assert_eq!(max, if bf2 { 100.0 } else { 40.0 });
        run(&mut sim, PadState::default(), 89.0);
        assert_eq!(sim.droids[0].health, max, "bf2={bf2}: no drain");
        let near = bystander(&mut sim);
        let before = hp(&sim, near);
        let mut end = None;
        for _ in 0..60 {
            sim.step(&[PadState::default()]);
            end = end.or(gone(&sim.events).first().map(|e| (*e, sim.tick)));
        }
        let (end, tick) = end.expect("timed out");
        assert_eq!(end, DroidEnd::Expired, "bf2={bf2}");
        // 2700 ticks flown, the launch tick included: exactly 90 s.
        assert_eq!(tick - launched + 1, 2700, "bf2={bf2}");
        assert_eq!(hp(&sim, near), before, "bf2={bf2}: no explosion");
        assert!(sim.droids.is_empty() && sim.strikes.is_empty());
        assert!(sim.controlled_droid(owner).is_none());
        let feet = sim.unit(owner).unwrap().soldier.pos;
        run(&mut sim, FORWARD, 1.0);
        assert!(sim.unit(owner).unwrap().soldier.pos.z < feet.z - 1.0);
        // The clip is spent as after any ending; a restock launches again.
        press(&mut sim, buttons::SECONDARY_FIRE);
        run(&mut sim, PadState::default(), 2.0);
        assert!(sim.droids.is_empty(), "bf2={bf2}: one droid per clip");
        restock(&mut sim, owner, weapon);
        let mut relaunched = false;
        for t in 0..120 {
            let b = if t % 2 == 0 {
                buttons::SECONDARY_FIRE
            } else {
                0
            };
            press(&mut sim, b);
            relaunched |= !sim.droids.is_empty();
        }
        assert!(relaunched, "bf2={bf2}: relaunch after the timeout");
    }
}

#[test]
fn the_owners_death_removes_the_droid_with_no_explosion() {
    for bf2 in [false, true] {
        let (mut sim, owner, _) = setup(bf2);
        launch(&mut sim);
        run(&mut sim, FORWARD, 2.0);
        let near = bystander(&mut sim);
        let before = hp(&sim, near);
        sim.units[owner.index as usize].alive = false;
        let events = run(&mut sim, PadState::default(), TICK_DT);
        assert_eq!(gone(&events), [DroidEnd::OwnerDied], "bf2={bf2}");
        assert!(sim.droids.is_empty() && sim.strikes.is_empty());
        assert_eq!(hp(&sim, near), before, "bf2={bf2}: no explosion");
    }
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
    let near = bystander(&mut sim);
    let before = hp(&sim, near);
    let events = run(&mut sim, PadState::default(), TICK_DT);
    assert_eq!(gone(&events), [DroidEnd::Destroyed]);
    assert!(hp(&sim, near) < before, "shot down: it still explodes");
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
    let u = sim.unit(enemy).unwrap();
    assert!(u.health.cur < u.health.max, "the destruct still explodes");
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
    // (In the open at 30 m it would not launch: card DROIDAI's tests below.)
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

/// Sprint held while flying changes nothing: not the droid, not the idle
/// body, not its stamina.
#[test]
fn sprint_does_nothing_while_flying() {
    for bf2 in [false, true] {
        let play = |extra: u32| {
            let (mut sim, owner, _) = setup(bf2);
            launch(&mut sim);
            let pad = PadState {
                buttons: FORWARD.buttons | extra,
                ..FORWARD
            };
            run(&mut sim, pad, 3.0);
            let d = sim.droids[0].clone();
            let u = sim.unit(owner).unwrap();
            let body = (
                u.soldier.pos,
                u.soldier.vel,
                u.soldier.stance,
                u.soldier.energy,
            );
            (d.pos, d.vel, d.yaw, d.age, d.health, body)
        };
        let plain = play(0);
        assert_eq!(play(buttons::SPRINT), plain, "bf2={bf2}");
        let (.., (_, body_vel, _, _)) = plain;
        assert!(body_vel.length() < 1e-4, "bf2={bf2}: the body stands");
    }
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

// ---------------------------------------------------------------------------
// Card DROIDAI [ours]: when a bot launches, what ends its flight, the destruct.
// ---------------------------------------------------------------------------

/// A bot carrying the remote (and its rifle, weapon class 0), at (5, 0, 5).
fn bot_setup(bf2: bool) -> (Sim, UnitHandle, usize) {
    let (mut sim, owner, weapon) = setup(bf2);
    let u = sim.unit_mut(owner).unwrap();
    u.controller = Controller::Ai;
    u.is_player = false;
    (sim, owner, weapon)
}

/// An enemy trooper at `at`, known to the bot (a friend told it).
fn known_enemy(sim: &mut Sim, owner: UnitHandle, at: Vec3) -> UnitHandle {
    let e = sim.spawn(&UnitSpec::trooper(1, at)).unwrap();
    let tick = sim.tick;
    let st = &mut sim.unit_mut(owner).unwrap().ai;
    let slot = st.contacts.iter().position(Option::is_none).unwrap();
    st.contacts[slot] = Some(crate::perception::Contact {
        handle: e,
        first_seen: tick,
        last_seen: tick,
        last_pos: at,
        seen_now: false,
        heard: false,
        told: true,
    });
    e
}

/// A 6 m tall, 10 m wide wall across z = `z`.
fn wall_at(sim: &mut Sim, z: f32) {
    sim.collision.push_body(crate::collision::Body::new(
        glam::Mat4::from_translation(Vec3::new(5.0, 3.0, z)),
        crate::collision::Shape::Box {
            half: Vec3::new(5.0, 3.0, 0.5),
        },
    ));
}

/// Move the clock to the start of a launch window whose draw passes (`lucky`)
/// or fails: the chance forced on or off.
fn roll(sim: &mut Sim, owner: UnitHandle, lucky: bool) {
    let window = (crate::remote::BOT_ROLL * crate::TICK_HZ as f32) as u64;
    let w = (sim.tick / window + 1..)
        .find(|&w| {
            let d = crate::ai::unit_unit(owner.index, w, owner.gen, crate::ai::SALT_DROID);
            (d < crate::remote::BOT_CHANCE) == lucky
        })
        .unwrap();
    sim.tick = w * window;
}

fn launch_press(sim: &Sim, owner: UnitHandle) -> bool {
    matches!(
        crate::ai::ability_input(sim, owner),
        Some(buttons::SECONDARY_FIRE | buttons::SWITCH_SECONDARY)
    )
}

fn shots(events: &[Event], owner: UnitHandle, class: usize) -> usize {
    events
        .iter()
        .filter(|e| {
            matches!(e, Event::Shot { unit, weapon_class, .. }
                if *unit == owner && *weapon_class as usize == class)
        })
        .count()
}

#[test]
fn an_enemy_in_sight_near_by_means_the_bot_fights_instead() {
    for (z, why) in [
        (-5.0, "10 m"),
        (-25.0, "30 m, in sight"),
        (-50.0, "55 m, in sight"),
    ] {
        let (mut sim, owner, _) = bot_setup(false);
        roll(&mut sim, owner, true);
        known_enemy(&mut sim, owner, Vec3::new(5.0, 0.0, z));
        assert!(!launch_press(&sim, owner), "{why}");
        if z == -5.0 {
            // It fights with its rifle, and never launches.
            let events = run(&mut sim, PadState::default(), 3.0);
            assert!(sim.droids.is_empty());
            assert!(shots(&events, owner, 0) > 0, "the bot shoots its own gun");
        }
    }
    // Close behind a wall is still too close: it fights.
    let (mut sim, owner, _) = bot_setup(false);
    wall_at(&mut sim, 0.0);
    roll(&mut sim, owner, true);
    known_enemy(&mut sim, owner, Vec3::new(5.0, 0.0, -10.0));
    assert!(!launch_press(&sim, owner), "15 m behind a wall");
}

#[test]
fn a_hidden_bot_near_a_fight_launches_some_of_the_time() {
    let hidden = |lucky: bool, hit: bool| {
        let (mut sim, owner, _) = bot_setup(false);
        wall_at(&mut sim, 0.0);
        roll(&mut sim, owner, lucky);
        let e = known_enemy(&mut sim, owner, Vec3::new(5.0, 0.0, -30.0));
        if hit {
            sim.unit_mut(owner).unwrap().last_hit = Some((e, sim.tick - 30));
        }
        (sim, owner)
    };
    let (sim, owner) = hidden(true, false);
    assert!(launch_press(&sim, owner), "hidden, a fight 35 m off");
    let (sim, owner) = hidden(false, false);
    assert!(!launch_press(&sim, owner), "the draw failed");
    let (sim, owner) = hidden(true, true);
    assert!(!launch_press(&sim, owner), "hit a second ago");
    // No fight known: no launch.
    let (mut sim, owner, _) = bot_setup(false);
    roll(&mut sim, owner, true);
    assert!(!launch_press(&sim, owner), "nothing known");
    // Resting after a flight: no launch.
    let (mut sim, owner) = hidden(true, false);
    sim.unit_mut(owner).unwrap().ai.droid_rest = sim.tick + 1;
    assert!(!launch_press(&sim, owner), "resting");
    // The whole tick: the bot switches to the remote and throws it.
    let (mut sim, owner) = hidden(true, false);
    for _ in 0..30 {
        if !sim.droids.is_empty() {
            break;
        }
        sim.step(&[]);
    }
    assert_eq!(sim.droids.len(), 1, "launched");
    assert!(sim.controlled_droid(owner).is_some());
}

/// A hidden bot's droid in the air over the fight, 2 s after launch.
fn flying(bf2: bool) -> (Sim, UnitHandle, UnitHandle) {
    let (mut sim, owner, weapon) = bot_setup(bf2);
    wall_at(&mut sim, 0.0);
    let enemy = known_enemy(&mut sim, owner, Vec3::new(5.0, 0.0, -30.0));
    // (Tough: the droid's bolts do not end the fight in these tests.)
    let h = &mut sim.unit_mut(enemy).unwrap().health;
    (h.max, h.cur) = (1e6, 1e6);
    sim.launch_droid(owner.index as usize, weapon, 0.0);
    sim.droids[0].pos = Vec3::new(5.0, 2.0, -3.0);
    run(&mut sim, PadState::default(), 2.0);
    assert!(sim.controlled_droid(owner).is_some());
    (sim, owner, enemy)
}

#[test]
fn the_body_shot_or_seen_close_ends_the_flight() {
    // Shot: control comes back at once, and the bot rests from its droid.
    let (mut sim, owner, enemy) = flying(false);
    sim.unit_mut(owner).unwrap().last_hit = Some((enemy, sim.tick));
    sim.step(&[]);
    assert!(sim.controlled_droid(owner).is_none(), "shot");
    let rest = sim.unit(owner).unwrap().ai.droid_rest;
    assert!(rest >= sim.tick + 29 * u64::from(crate::TICK_HZ), "{rest}");
    assert!(!launch_press(&sim, owner));
    // Seen: an enemy walks round the wall to 10 m of the body.
    let (mut sim, owner, _) = flying(false);
    sim.spawn(&UnitSpec::trooper(1, Vec3::new(15.0, 0.0, 10.0)))
        .unwrap();
    sim.step(&[]);
    assert!(sim.controlled_droid(owner).is_none(), "seen at 10 m");
    // Neither: it flies on.
    let (mut sim, owner, _) = flying(false);
    run(&mut sim, PadState::default(), 2.0);
    assert!(sim.controlled_droid(owner).is_some());
}

#[test]
fn the_droid_among_enemies_destructs() {
    for bf2 in [false, true] {
        let (mut sim, owner, _) = flying(bf2);
        let at = sim.droids[0].pos.with_y(0.0);
        for dx in [-2.0, 2.0] {
            sim.spawn(&UnitSpec::trooper(1, at + Vec3::new(dx, 0.0, -1.0)))
                .unwrap();
        }
        let events = run(&mut sim, PadState::default(), 3.5);
        let charged = events
            .iter()
            .any(|e| matches!(e, Event::DroidCharge { .. }));
        assert!(charged, "bf2={bf2}");
        assert_eq!(gone(&events), [DroidEnd::Destruct], "bf2={bf2}");
        assert!(sim.controlled_droid(owner).is_none());
    }
    // One enemy near: no destruct while healthy, one when nearly dead.
    let (mut sim, _, _) = flying(false);
    let at = sim.droids[0].pos.with_y(0.0);
    sim.spawn(&UnitSpec::trooper(1, at + Vec3::new(2.0, 0.0, -1.0)))
        .unwrap();
    sim.step(&[]);
    assert!(sim.droids[0].charge.is_none(), "one enemy, healthy");
    sim.droids[0].health = 5.0;
    sim.step(&[]);
    assert!(sim.droids[0].charge.is_some(), "one enemy, nearly dead");
}

#[test]
fn a_flight_ends_after_30_s_and_the_bot_goes_in_itself() {
    let (mut sim, owner, _) = flying(false);
    let blaster = sim.weapon_classes[sim.droids[0].weapon_class]
        .remote
        .as_ref()
        .unwrap()
        .blaster_index;
    // The fight is still on: the droid flies at the enemy it knows of.
    let events = run(
        &mut sim,
        PadState::default(),
        crate::remote::BOT_FLIGHT - 3.0,
    );
    assert!(sim.controlled_droid(owner).is_some(), "still flying");
    assert!(shots(&events, owner, blaster) > 0, "the droid fought");
    run(&mut sim, PadState::default(), 1.5);
    assert!(sim.controlled_droid(owner).is_none(), "home at 30 s");
    // Back in its body, with the enemy now in sight, it fights with its own
    // rifle and does not fly again.
    sim.collision = Default::default();
    let events = run(&mut sim, PadState::default(), 4.0);
    assert!(shots(&events, owner, 0) > 0, "the body fires its rifle");
    assert!(sim.controlled_droid(owner).is_none());
}
