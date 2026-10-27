use super::*;

/// Card USKN: a part takes its class's override of the group its
/// texture is listed under; parts in no group, and classes with no
/// override of that group, keep their own texture.
#[test]
fn a_part_takes_its_classs_override_of_its_group() {
    let table: std::collections::BTreeMap<String, Vec<String>> = [
        (
            "override_texture".to_owned(),
            vec!["all_inf_soldier".to_owned()],
        ),
        (
            "override_texture2".to_owned(),
            vec!["all_face01".to_owned()],
        ),
    ]
    .into();
    let groups = part_groups(
        &[
            Some("All_Inf_Soldier"),
            Some("all_face01"),
            Some("packs"),
            None,
        ],
        &table,
    );
    assert_eq!(
        groups,
        [
            Some("override_texture".to_owned()),
            Some("override_texture2".to_owned()),
            None,
            None
        ]
    );
    let skins = crate::roster::ClassSkins {
        texture: Some("all_inf_soldier_hoth".into()),
        texture2: Some("rebel_face02".into()),
    };
    let picked: Vec<Option<&str>> = groups
        .iter()
        .map(|g| skins.of_group(g.as_deref()?))
        .collect();
    assert_eq!(
        picked,
        [
            Some("all_inf_soldier_hoth"),
            Some("rebel_face02"),
            None,
            None
        ]
    );
    let plain = crate::roster::ClassSkins::default();
    assert!(groups
        .iter()
        .all(|g| g.as_deref().and_then(|g| plain.of_group(g)).is_none()));
}

fn team(classes: &[(&str, Option<&str>, Option<&str>)]) -> TeamTemplate {
    let mut t = TeamTemplate::default_trooper();
    t.classes = classes
        .iter()
        .map(|(name, geometry, weapon)| crate::walk::ClassVisual {
            name: (*name).into(),
            label: (*name).into(),
            geometry: geometry.map(str::to_owned),
            weapon_geometry: weapon.map(str::to_owned),
            weapon_hud: Default::default(),
            held_hud: Vec::new(),
            held_models: Vec::new(),
            foley_class: 0,
            cries: Default::default(),
            anim: Default::default(),
            skins: Default::default(),
        })
        .collect();
    t
}

/// Card UWPN: BF1's Clone Wars rosters. The sharpshooter shares the
/// trooper's body and the pilot and assassin droids the assault droid's;
/// each still draws its own primary's model.
#[test]
fn classes_sharing_a_body_hold_their_own_guns() {
    let rep = team(&[
        (
            "rep_inf_clone_trooper",
            Some("rep_inf_trooper"),
            Some("rep_weap_inf_rifle"),
        ),
        (
            "rep_inf_clone_pilot",
            Some("rep_inf_pilot"),
            Some("rep_weap_inf_rifle"),
        ),
        (
            "rep_inf_clone_sharpshooter",
            Some("rep_inf_trooper"),
            Some("rep_weap_inf_sniperrifle"),
        ),
    ]);
    let cis = team(&[
        ("cis_inf_battledroid", Some("cis_inf_sbdroid"), None),
        (
            "cis_inf_assault",
            Some("cis_inf_bdroid"),
            Some("cis_weap_inf_launcher"),
        ),
        (
            "cis_inf_pilotdroid",
            Some("cis_inf_bdroid"),
            Some("com_weap_inf_torpedo"),
        ),
        (
            "cis_inf_assassindroid",
            Some("cis_inf_bdroid"),
            Some("cis_weap_inf_sniperrifle"),
        ),
    ]);
    let plan = DrawPlan::of(&[rep, cis]);
    // One upload per distinct model.
    assert_eq!(plan.bodies.len(), 4, "{:?}", plan.bodies);
    assert_eq!(plan.weapons.len(), 5, "{:?}", plan.weapons);
    let [r, c] = &plan.per_class;
    // Same body, different gun.
    assert_eq!(r[0].body, r[2].body);
    assert_ne!(r[0].weapon, r[2].weapon);
    assert_eq!(plan.weapon_of(0, 2), Some("rep_weap_inf_sniperrifle"));
    assert_eq!(c[1].body, c[2].body);
    assert_eq!(c[1].body, c[3].body);
    assert_eq!(plan.weapon_of(1, 1), Some("cis_weap_inf_launcher"));
    assert_eq!(plan.weapon_of(1, 2), Some("com_weap_inf_torpedo"));
    assert_eq!(plan.weapon_of(1, 3), Some("cis_weap_inf_sniperrifle"));
    // Different body, same gun: one weapon upload.
    assert_ne!(r[0].body, r[1].body);
    assert_eq!(r[0].weapon, r[1].weapon);
    // The super battle droid's wrist blaster has no model: none drawn.
    assert_eq!(c[0].weapon, None);
}

#[test]
fn the_fire_point_is_the_node_under_the_hand() {
    let nodes = vec![manifest::ManifestHardpoint {
        name: "hp_fire".into(),
        pos: [0.0, 0.0533, 1.097],
        axes: None,
    }];
    let hand = Mat4::from_translation(Vec3::new(10.0, 1.0, 0.0))
        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let p = fire_point(hand, &nodes, "HP_FIRE");
    assert!(p.abs_diff_eq(Vec3::new(11.097, 1.0533, 0.0), 1e-5), "{p}");
    // No such node: the hand's origin.
    assert_eq!(
        fire_point(hand, &nodes, "hp_other"),
        Vec3::new(10.0, 1.0, 0.0)
    );
}

/// Card WHELD: a class slot draws its own weapon's model - a held slot
/// its own (or none), any other slot the primary's.
#[test]
fn a_slot_in_hand_draws_its_own_model() {
    let mut t = team(&[(
        "rep_inf_ep3_rifleman",
        Some("rep_inf_ep3trooper"),
        Some("rep_weap_inf_rifle"),
    )]);
    t.classes[0].held_models = vec![
        (1, Some("rep_weap_inf_pistol".into())),
        (2, Some("com_weap_inf_grenadethermal".into())),
        (3, None),
    ];
    let plan = DrawPlan::of(&[t.clone(), t]);
    for team in 0..2 {
        assert_eq!(plan.weapon_in_slot(team, 0, 0), Some("rep_weap_inf_rifle"));
        assert_eq!(plan.weapon_in_slot(team, 0, 1), Some("rep_weap_inf_pistol"));
        assert_eq!(
            plan.weapon_in_slot(team, 0, 2),
            Some("com_weap_inf_grenadethermal")
        );
        assert_eq!(
            plan.weapon_in_slot(team, 0, 3),
            None,
            "a slot with no model"
        );
        assert_eq!(plan.weapon_in_slot(team, 0, 7), Some("rep_weap_inf_rifle"));
    }
    assert_eq!(plan.weapons.len(), 3, "each model once: {:?}", plan.weapons);
}

/// The thermal detonator as the discs author it (re-grenades §9, BF2).
fn thermal() -> core::weapon::WeaponClass {
    use core::weapon::{OrdnanceClass, Thrown, WeaponClass};
    WeaponClass {
        name: "thermal".into(),
        rounds_per_clip: 1,
        reload_time: 0.01,
        trigger_single: true,
        shot_delay: [1.3; 2],
        initial_salvo_delay: 0.51,
        thrown: Some(Thrown {
            shot_elevate: 0.21,
            hide_on_fire: true,
            hand: Some(Thrown::BF2_HAND),
        }),
        ordnance: OrdnanceClass {
            velocity: 25.0,
            life_span: 0.6,
            sticky: Some(core::sticky::StickyClass::new(true)),
            ..OrdnanceClass::default()
        },
        ..WeaponClass::default()
    }
}

/// Card WHELD acceptance 3, through the sim's own pad path: a soldier
/// holding the rifle, a pistol (slot 1, channel 0) and a thermal (slot 2,
/// channel 1). The switch puts the pistol in hand; the throw puts the
/// thermal in hand, hidden by its `HideOnFire` for the throw and the
/// reload; then the pistol is back. On a baked BF2 clone the drawn model
/// and the fire point follow: the pistol's `hp_fire`, not the rifle's.
#[test]
fn the_weapon_in_hand_follows_the_switch_and_the_throw() {
    use core::sim::{buttons, PadState};
    let mut w = WalkSim::new(3, Vec3::ZERO, None);
    let mut pistol = core::weapon::WeaponClass::bf1_rifle();
    pistol.name = "pistol".into();
    let pistol = w.sim.add_weapon_class(pistol);
    let thermal = w.sim.add_weapon_class(thermal());
    let slot = |slot, channel, weapon_class, clips| core::unit::SlotSpec {
        slot,
        channel,
        weapon_class,
        clips,
    };
    let me = w
        .sim
        .spawn(&core::unit::UnitSpec {
            slots: std::sync::Arc::new([slot(1, 0, pistol, 0), slot(2, 1, thermal, 4)]),
            ..core::unit::UnitSpec::player_in_slot(1, 0, Vec3::new(4.0, 0.0, 0.0))
        })
        .expect("spawned")
        .index as usize;
    let pad = |b| {
        [
            PadState::default(),
            PadState {
                buttons: b,
                ..Default::default()
            },
        ]
    };
    let hand = |w: &WalkSim| weapon_in_hand(&w.sim.units[me], &w.sim.weapon_classes);
    let rifle = hand(&w);
    assert_eq!((rifle.slot, rifle.hidden), (0, false));
    w.step_tick(&pad(buttons::SWITCH_WEAPON));
    w.step_tick(&pad(0));
    let h = hand(&w);
    assert_eq!(
        (h.class, h.slot, h.hidden),
        (pistol, 1, false),
        "the pistol in hand"
    );
    let at_pistol = w.sim.tick;

    // The throw: channel 1 has him, the thermal is hidden from its fire
    // state through the reload, then the pistol is back.
    let mut seen = Vec::new();
    for n in 0..90 {
        w.step_tick(&pad(if n < 2 { buttons::SECONDARY_FIRE } else { 0 }));
        let h = hand(&w);
        if seen.last() != Some(&h) {
            seen.push(h);
        }
    }
    assert_eq!(
        seen,
        [
            InHand {
                class: thermal,
                slot: 2,
                hidden: true
            },
            InHand {
                class: pistol,
                slot: 1,
                hidden: false
            },
        ],
        "the throw's hands"
    );

    // The drawn model and its fire point, on a baked soldier.
    let root = crate::assets::baked_root();
    let ok = |p: &str| root.join(format!("side_bf2/{p}")).exists();
    if !ok("soldiers/rep_inf_ep3trooper.glb")
        || !ok("weapons/rep_weap_inf_pistol.nodes.ron")
        || !ok("weapons/rep_weap_inf_rifle.nodes.ron")
    {
        eprintln!("skip: no bf2 rep_inf_ep3trooper / pistol / rifle bake");
        return;
    }
    let mut t = team(&[(
        "rep_inf_ep3_rifleman",
        Some("rep_inf_ep3trooper"),
        Some("rep_weap_inf_rifle"),
    )]);
    t.classes[0].held_models = vec![(1, Some("rep_weap_inf_pistol".into()))];
    let mut vis = SoldierVisuals::load_cpu(&root, "bf2", &[t.clone(), t]);
    vis.update(&w, core::TICK_DT);
    let rifle = vis.weapon_for(0, 0, 0).expect("the rifle");
    let held = vis.weapon_for(0, 0, 1).expect("the pistol");
    let fire = |n: &manifest::WeaponNodes| {
        n.iter()
            .find(|n| n.name.eq_ignore_ascii_case("hp_fire"))
            .map(|n| Vec3::from(n.pos))
    };
    let (rp, pp) = (fire(&rifle.nodes).unwrap(), fire(&held.nodes).unwrap());
    assert!(rp.distance(pp) > 0.1, "two guns, two muzzles: {rp} {pp}");
    assert!(w.sim.tick > at_pistol);
    let (_, sa, model, palette) = vis
        .posed(&w)
        .into_iter()
        .find(|(i, ..)| *i == me)
        .expect("drawn");
    let hp = sa.hp_weapons.expect("hp_weapons");
    let hand_m = model * palette[hp] * sa.asset.skeleton.global_bind[hp];
    let got = vis.fire_points(&w)[me].expect("a fire point");
    assert!(got.abs_diff_eq(hand_m.transform_point3(pp), 1e-4), "{got}");
    assert!(
        !got.abs_diff_eq(hand_m.transform_point3(rp), 1e-2),
        "not the rifle's"
    );
}

/// Card ROLLS: a roll to a unit's side - the player's and a bot's, through
/// the input this module fills from the sim - turns the DRAWN body so its
/// dive points along the velocity, on a real soldier of each game.
#[test]
fn a_side_roll_turns_the_drawn_body_onto_the_velocity() {
    use core::soldier::Stance;
    let root = crate::assets::baked_root();
    for game in ["bf1", "bf2"] {
        let geometry = "rep_inf_trooper";
        if !root
            .join(format!("side_{game}/soldiers/{geometry}.glb"))
            .exists()
        {
            eprintln!("skip: no {game} {geometry} bake");
            continue;
        }
        let t = team(&[("trooper", Some(geometry), None)]);
        let mut vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t]);
        let mut walk = WalkSim::new(7, Vec3::ZERO, None);
        let bot = walk.sim.spawn(&core::unit::UnitSpec {
            controller: core::unit::Controller::Ai,
            ..core::unit::UnitSpec::trooper(1, Vec3::new(6.0, 0.0, -10.0))
        });
        let player = walk.player.index as usize;
        let cases = [
            (player, 0.7f32, 1.0f32),
            (bot.expect("spawned").index as usize, -2.0, -1.0),
        ];
        for &(i, yaw, side) in &cases {
            let s = &mut walk.sim.units[i].soldier;
            s.yaw = yaw;
            s.stance = Stance::Roll;
            // His left (side +1) or right: the sim's `right` negated.
            let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
            s.vel = -right * side * 10.5;
        }
        for n in 1..=26 {
            vis.update(&walk, core::TICK_DT);
            for &(i, _, side) in &cases {
                let a = &vis.units[i].animator;
                let tw = a.twist().to_degrees();
                assert!(tw * side > 0.0, "{game} unit {i} update {n}: {tw}");
                if n >= 9 {
                    assert!(
                        (tw - 90.0 * side).abs() < 15.0,
                        "{game} unit {i} update {n}: {tw}"
                    );
                }
            }
        }
        let drawn = vis.posed(&walk);
        assert_eq!(drawn.len(), 2, "{game}: both drawn");
        for (i, sa, model, palette) in drawn {
            let skel = &sa.asset.skeleton;
            let b = skel.index_of("DummyRoot").expect("a DummyRoot");
            assert_eq!(skel.parents[b], None, "{game}: the root has no parent");
            // The drawn root (no parent: its global is its local) carries
            // the kept pose's model +Z to where the dive points.
            let kept = vis.units[i].animator.kept_pose(skel).0[b].rotation;
            let v = kept.inverse() * Vec3::Z;
            let dive = (model * palette[b] * skel.global_bind[b]).transform_vector3(v);
            let heading = |v: Vec3| v.x.atan2(v.z);
            let vel = walk.sim.units[i].soldier.vel;
            let off = (heading(dive) - heading(vel) + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            assert!(
                off.to_degrees().abs() < 2.0,
                "{game} unit {i}: {}",
                off.to_degrees()
            );
        }
    }
}

/// Card ROLLS acceptance, the player's half: the pad's roll in each
/// direction through the real sim (BF1 and BF2 ground rules, a real
/// trooper of each game), travel less twist per update beside the PS2's
/// rows (re-roll-picture 4): within 15 degrees from the 9th update, 2 at
/// the end, a forward roll within 1 of 0; the strafe before a side roll
/// twists the legs +-71.6 (card ANIMM: BF1 on the backward clip, BF2 the
/// forward), so BF1's first update reads the PS2's -31.2 / +31.1, and
/// 0.5 s after the roll the twist is back within 10 of the strafe's.
/// `--nocapture` prints the rows.
#[test]
fn the_players_rolls_turn_the_body_as_the_ps2s() {
    use core::heightfield::Heightfield;
    use core::sim::{buttons, PadState};
    use core::soldier::{GroundRules, Stance};
    let root = crate::assets::baked_root();
    let flat = || Heightfield {
        size: 2,
        unit: 4000.0,
        offset_x: 2000.0,
        offset_z: 2000.0,
        heights: vec![0.0; 4],
    };
    let mut rolls = 0;
    for (game, rules) in [("bf1", GroundRules::Bf1), ("bf2", GroundRules::Bf2)] {
        let geometry = "rep_inf_trooper";
        if !root
            .join(format!("side_{game}/soldiers/{geometry}.glb"))
            .exists()
        {
            eprintln!("skip: no {game} {geometry} bake");
            continue;
        }
        let t = team(&[("trooper", Some(geometry), None)]);
        // (name, stick x, stick y): the pad's +y is ahead, -x his left.
        for (name, lx, ly) in [
            ("forward", 0i16, 32767i16),
            ("left", -32767, 0),
            ("right", 32767, 0),
            ("forward-left", -23170, 23170),
        ] {
            let mut vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t.clone()]);
            let mut w = WalkSim::new(1, Vec3::ZERO, Some(flat()));
            w.sim.ground_rules = rules;
            let p = w.player.index as usize;
            let stick = PadState {
                lx,
                ly,
                ..Default::default()
            };
            let press = PadState {
                buttons: buttons::ROLL_STANCE,
                ..stick
            };
            for _ in 0..19 {
                w.step_tick(&[stick]);
                vis.update(&w, core::TICK_DT);
            }
            let strafe = vis.units[p].animator.twist().to_degrees();
            let want_strafe = match (game, name) {
                ("bf1", "left") | ("bf2", "right") => -71.6,
                ("bf1", "right") | ("bf2", "left") => 71.6,
                (_, "forward") => 0.0,
                // Inside the forward window: the travel itself (the
                // walk's, under the roll's 45).
                _ => strafe.clamp(1.0, 44.0),
            };
            assert!(
                (strafe - want_strafe).abs() < 1.5,
                "{game} {name}: strafe twist {strafe}"
            );
            let mut rows = Vec::new();
            let mut after = None;
            for k in 0..90 {
                w.step_tick(&[if k == 0 { press } else { stick }]);
                vis.update(&w, core::TICK_DT);
                let a = &vis.units[p].animator;
                let r = a.roll();
                if w.sim.units[p].soldier.stance == Stance::Roll {
                    assert!(r.updates > 0);
                    rows.push((a.travel().to_degrees(), a.twist().to_degrees()));
                } else if r.after.is_some_and(|t| (t - 0.5).abs() < 1e-3) {
                    after = Some(a.twist().to_degrees());
                }
            }
            assert!(
                rows.len() >= 20,
                "{game} {name}: {} roll updates",
                rows.len()
            );
            rolls += 1;
            let line: Vec<String> = rows
                .iter()
                .map(|(t, tw)| format!("{:.1}", t - tw))
                .collect();
            println!(
                "{game} {name}: travel {:.1}, travel - twist by update {}",
                rows[0].0,
                line.join(" ")
            );
            let (travel, _) = rows[0];
            let want = match name {
                "forward" => 0.0,
                "left" => 90.0,
                "right" => -90.0,
                // The strafe is slower than the run (PS2: +37.9).
                _ => 37.9,
            };
            assert!(
                (travel - want).abs() < 1.0,
                "{game} {name}: travel {travel}"
            );
            for (n, (t, tw)) in rows.iter().enumerate() {
                if name == "forward" {
                    assert!(tw.abs() < 1.0, "{game} forward update {}: {tw}", n + 1);
                }
                if n + 1 >= 9 {
                    assert!(
                        (t - tw).abs() < 15.0,
                        "{game} {name} update {}: {}",
                        n + 1,
                        t - tw
                    );
                }
            }
            if game == "bf1" && matches!(name, "left" | "right") {
                let first = rows[0].1;
                let ps2 = if name == "left" { -31.2 } else { 31.1 };
                assert!((first - ps2).abs() < 1.0, "{game} {name}: update 1 {first}");
            }
            let (t, tw) = rows[rows.len() - 1];
            assert!((t - tw).abs() < 2.0, "{game} {name} at the end: {}", t - tw);
            let after = after.expect("0.5 s after the roll");
            println!(
                "{game} {name}: {} updates, 0.5 s after: twist {after:.2}",
                rows.len()
            );
            // The fixture's class is the default one (9 m/s ahead): BF1's
            // forward-left run is 8.06 m/s, over its chooser's 8 m/s idle
            // line, so his legs stand and turn on the spot after the roll.
            if !(game == "bf1" && name == "forward-left") {
                assert!(
                    (after - strafe).abs() < 10.0,
                    "{game} {name}: {after} after, {strafe} before"
                );
            }
        }
    }
    if rolls > 0 {
        assert_eq!(rolls, 8);
    }
}

/// Card ANIMM acceptance 2: the feet keep to the ground. A real trooper
/// of each game, driven at three speeds per stance and direction: the
/// ground the legs' clip covers a second (its root speed, the bake's
/// PS2 measure, over a cycle's length, times the cycles a second the
/// animator plays) against his speed, with the plain clock (main's) and
/// with the PS2's rate. The rate leaves 5 % of the gap to the clip's own
/// speed (`0.95 v / v_clip + 0.05`), so it is under 5 % of the speed
/// wherever he moves at more than half the clip's speed. `--nocapture`
/// prints the table.
#[test]
fn the_feet_keep_to_the_ground() {
    use crate::animator::{AnimInput, Animator};
    use core::soldier::Stance;
    let root = crate::assets::baked_root();
    let mut rows = 0;
    for game in ["bf1", "bf2"] {
        let geometry = "rep_inf_trooper";
        if !root
            .join(format!("side_{game}/soldiers/{geometry}.glb"))
            .exists()
        {
            eprintln!("skip: no {game} {geometry} bake");
            continue;
        }
        let t = team(&[("trooper", Some(geometry), None)]);
        let vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t]);
        let sa = &vis.assets[0];
        let (Some(rig), clips, skel) = (&sa.rig, &sa.asset.clips, &sa.asset.skeleton) else {
            panic!("{game}: no rig");
        };
        for (stance, ahead, speeds) in [
            (Stance::Stand, true, [2.5, 4.5, 6.5]),
            (Stance::Stand, false, [2.0, 3.5, 5.0]),
            (Stance::Crouch, true, [1.2, 2.0, 3.0]),
            (Stance::Prone, true, [0.6, 1.0, 1.5]),
        ] {
            // BF2's prone walk is BF1's (issue #40), with no measured root
            // speed of its own.
            if game == "bf2" && stance == Stance::Prone {
                continue;
            }
            for speed in speeds {
                let input = AnimInput {
                    stance,
                    vel: glam::Vec2::new(0.0, if ahead { speed } else { -speed }),
                    yaw: 0.0,
                    max_speed: 7.0,
                    root_scale: 1.0,
                    dead: false,
                    getup_lock: false,
                    set: 0,
                    fired: false,
                    throw: None,
                    throw_windup: 0.0,
                    reloading: false,
                    reload_time: 0.0,
                    alert: false,
                    aim: (0.0, 0.0),
                };
                let mut a = Animator::new();
                let dt = 1.0 / 30.0;
                for _ in 0..60 {
                    a.step(rig, clips, skel, &input, dt);
                }
                let c = &clips[a.lower_clip().expect("a legs clip")];
                // The clips are baked in place: the ground a cycle covers
                // is the PS2's root speed over the cycle's length.
                let denom = if game == "bf1" {
                    c.frames
                } else {
                    c.frames - 1
                } as f32;
                let v_clip = rig
                    .clip_speed(a.lower_clip().unwrap())
                    .unwrap_or_else(|| panic!("{game}: no root speed for {}", c.name))[1]
                    .abs();
                let travel = v_clip * denom / 30.0;
                // Cycles a second: 30 updates' phase steps (each under
                // one cycle).
                let mut cycles = 0.0;
                for _ in 0..30 {
                    let t0 = a.lower_phase();
                    a.step(rig, clips, skel, &input, dt);
                    cycles += (a.lower_phase() - t0).rem_euclid(1.0);
                }
                let plain = 30.0 / denom;
                let feet = cycles * travel;
                let before = plain * travel;
                let err = |f: f32| 100.0 * (f - speed).abs() / speed;
                println!(
                    "{game} {stance:?} {} {speed:.1} m/s: {} clip {:.2} m/s | plain {before:.2} ({:.1} %) | rate {feet:.2} ({:.1} %)",
                    if ahead { "ahead" } else { "back" },
                    c.name,
                    before,
                    err(before),
                    err(feet),
                );
                // The PS2's own residual, and under 5 % from half the
                // clip's speed up.
                assert!(
                    (feet - speed).abs() <= 0.05 * (before - speed).abs() + 0.01 * speed,
                    "{game} {stance:?} {speed}: {feet} against {speed}"
                );
                if speed >= before / 2.0 {
                    assert!(err(feet) < 5.0, "{game} {stance:?} {speed}: {}", err(feet));
                }
                rows += 1;
            }
        }
    }
    if rows > 0 {
        assert_eq!(rows, 21);
    }
}

/// Issue #40: a prone soldier plays prone clips on both games - BF2's
/// borrowed from BF1 (the disc has none; the sets' prone rows hold the stand
/// idle) - in every weapon set, lying still and crawling, and his shot and
/// reload are the prone ones.
#[test]
fn a_prone_soldier_lies_down_on_both_games() {
    use crate::animator::{ActionKind, AnimInput, Animator};
    use core::soldier::Stance;
    let root = crate::assets::baked_root();
    for (game, geometry) in [
        ("bf1", "rep_inf_trooper"),
        ("bf2", "rep_inf_trooper"),
        ("bf2", "cis_inf_sbdroid"),
        ("bf2", "all_inf_wookie"),
    ] {
        if !root
            .join(format!("side_{game}/soldiers/{geometry}.glb"))
            .exists()
        {
            eprintln!("skip: no {game} {geometry} bake");
            continue;
        }
        let t = team(&[("trooper", Some(geometry), None)]);
        let vis = SoldierVisuals::load_cpu(&root, game, &[t.clone(), t]);
        let sa = &vis.assets[0];
        let (Some(rig), clips, skel) = (&sa.rig, &sa.asset.clips, &sa.asset.skeleton) else {
            panic!("{game} {geometry}: no rig");
        };
        let mut set = 0;
        while rig.set_name(set) != "?" {
            for speed in [0.0f32, 1.0] {
                let input = AnimInput {
                    stance: Stance::Prone,
                    vel: glam::Vec2::new(0.0, speed),
                    yaw: 0.0,
                    max_speed: 7.0,
                    root_scale: 1.0,
                    dead: false,
                    getup_lock: false,
                    set,
                    fired: false,
                    throw: None,
                    throw_windup: 0.0,
                    reloading: false,
                    reload_time: 0.0,
                    alert: false,
                    aim: (0.0, 0.0),
                };
                let mut a = Animator::new();
                for _ in 0..60 {
                    a.step(rig, clips, skel, &input, 1.0 / 30.0);
                }
                let name = |c: Option<usize>| c.map_or("-", |c| clips[c].name.as_str());
                let (lo, up) = (name(a.lower_clip()), name(a.upper_clip()));
                assert!(
                    lo.contains("prone") && up.contains("prone"),
                    "{game} {geometry} {} at {speed} m/s: legs {lo}, body {up}",
                    rig.set_name(set)
                );
            }
            for kind in [ActionKind::Shoot, ActionKind::Reload] {
                if let Some((up, _)) = rig.action_slots(set, 2, kind) {
                    let n = &clips[up.clip].name;
                    assert!(n.contains("prone"), "{game} {geometry} {kind:?}: {n}");
                }
            }
            set += 1;
        }
    }
}
