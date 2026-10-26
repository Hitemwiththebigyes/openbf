use core::collision::{HullFilter, Layers};
use core::projectile::{Bolt, Hit, Shooter, Target};
use core::sim::{Event, PadState, Sim};
use core::unit::{UnitHandle, UnitSpec};
use core::weapon::{HealthType, OrdnanceClass};
use glam::Vec3;

fn world(map: &str) -> Option<(game::assets::Scene, Sim, UnitHandle)> {
    let root = game::assets::baked_root();
    let dir = root.join(map);
    if !dir.join("manifest.ron").exists() {
        eprintln!("SKIP: {map} is not baked");
        return None;
    }
    let mut scene = game::assets::load_scene(&dir).expect("load scene");
    scene.apply_mode_layer("con");
    scene.merge_common(
        game::assets::load_common(&root.join(format!("common_{}", scene.manifest.source_game)))
            .expect("common assets"),
    );
    let mut sim = Sim::empty(42);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    game::vehicle_pads::install(
        &mut sim,
        &scene.manifest,
        &dir,
        &root.join("classes"),
        &["all".into(), "imp".into()],
    )
    .expect("install");
    let owner = sim
        .spawn(&UnitSpec::trooper(0, Vec3::new(0.0, 1000.0, 0.0)))
        .unwrap();
    Some((scene, sim, owner))
}

#[test]
fn disc_cutters_are_available_in_both_channels_with_the_original_repair_tables() {
    let root = game::assets::baked_root().join("classes");
    for (game, shared, soldier, name, channel, amounts) in [
        (
            "bf1",
            "common",
            "rep_inf_clone_pilot",
            "rep_weap_inf_fusioncutter",
            1,
            [60.0, 20.0, 30.0],
        ),
        (
            "bf2",
            "ingame",
            "rep_inf_ep3_engineer",
            "rep_weap_inf_fusioncutter",
            0,
            [100.0, 50.0, 50.0],
        ),
    ] {
        let mut db = game::classdb::ClassDb::default();
        for side in [shared, "rep"] {
            let path = root.join(format!("{game}_{side}.ron"));
            if !path.exists() {
                return;
            }
            db.load(&path).unwrap();
        }
        let cutter = game::roster::weapon_from_db(&db, name).unwrap();
        assert_eq!(cutter.repair.unwrap().health[4..7], amounts);
        assert_eq!(cutter.shot_delay, [0.5; 2]);
        assert!(cutter.infinite_ammo());
        assert!(cutter.flash.is_none());
        let slots = db.weapon_slots(soldier);
        let primary = slots.iter().find(|s| s.channel == 0).unwrap().slot;
        let held = game::roster::held_slots(&db, soldier, primary);
        assert!(
            held.iter()
                .any(|s| s.channel == channel && s.weapon.repair.is_some()),
            "{game} {soldier}"
        );
    }
}

#[test]
fn disc_droid_rebuilding_restores_the_live_collision_sphere_and_service() {
    for map in ["bf1_bes1", "bf2_hot1"] {
        for name in ["com_item_healthrecharge", "com_item_weaponrecharge"] {
            let Some((_, mut sim, owner)) = world(map) else {
                return;
            };
            let i = sim
                .stations
                .iter()
                .position(|s| sim.station_classes[usize::from(s.class)].name == name)
                .unwrap();
            let station = sim.stations[i];
            let sphere = sim.station_classes[usize::from(station.class)]
                .sphere
                .unwrap();
            let live_count = sim.station_live_collision[i].len();
            let damage = OrdnanceClass {
                max_damage: 100_000.0,
                ..Default::default()
            }
            .damage_desc(0.0, false);
            sim.damage_station(i as u16, &damage, 1.0, Some(owner), Some(0));
            assert!(sim.stations[i].destroyed);
            sim.repair_station(i as u16, station.health.max - 1.0);
            assert!(sim.stations[i].destroyed);
            sim.repair_station(i as u16, 1.0);
            assert!(!sim.stations[i].destroyed);
            assert_eq!(
                sim.collision.station(station.instance.unwrap()),
                Some(i as u16)
            );
            assert_eq!(sim.station_bodies[i].len(), live_count);
            let rebuilt = sim.stations[i];
            assert_eq!(rebuilt.sphere_radius, sphere[3]);
            assert!(
                rebuilt.sphere_centre.distance(
                    rebuilt
                        .pose()
                        .transform_point3(Vec3::new(sphere[0], sphere[1], sphere[2]))
                ) < 0.0001
            );
            let u = sim.unit_mut(owner).unwrap();
            u.soldier.pos = station.pos + Vec3::X * 1.5;
            u.health.cur = 1.0;
            u.weapon.spare = 0;
            let mut served = false;
            for _ in 0..75 {
                sim.step(&[PadState::default()]);
                served |= sim.events.iter().any(|e| matches!(e, Event::StationServed { station, .. } if usize::from(*station) == i));
            }
            assert!(served, "{map} {name}");
        }
    }
}

#[test]
fn both_games_use_the_disc_health_wrecks_and_explosions_and_accept_real_collision_hits() {
    for map in ["bf1_bes1", "bf2_hot1"] {
        let mut checked = 0;
        for name in ["com_item_healthrecharge", "com_item_weaponrecharge"] {
            let Some((scene, mut sim, owner)) = world(map) else {
                return;
            };
            let models = game::stations::wreck_models(&scene).expect("wreck models");
            let index = sim
                .stations
                .iter()
                .position(|s| sim.station_classes[usize::from(s.class)].name == name)
                .expect("station");
            let s = sim.stations[index];
            let c = &sim.station_classes[usize::from(s.class)];
            let expected = if map.starts_with("bf2") && name == "com_item_healthrecharge" {
                200.0
            } else {
                80.0
            };
            assert_eq!(s.health.cur, expected);
            assert_eq!(s.health.health_type, HealthType::Building);
            assert!(c.explosion.is_some());
            assert!(models.contains_key(&(s.instance.unwrap() as usize)));
            assert!(!sim.station_wreck_collision[usize::from(s.class)].is_empty());
            assert!(s.sphere_radius > 0.0);
            let shot = [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z]
                .into_iter()
                .find_map(|d| {
                    let origin = s.sphere_centre + d * (s.sphere_radius + 0.2);
                    let b = Bolt::spawn(
                        origin,
                        -d,
                        150.0,
                        0.1,
                        Shooter {
                            owner,
                            team: 0,
                            weapon_class: 0,
                            is_player: true,
                        },
                        sim.heightfield.as_ref(),
                        sim.water_plane(),
                        &sim.collision,
                        HullFilter::All,
                        std::iter::empty(),
                    );
                    (b.hit == Hit::Target(Target::Station(index as u16))).then_some(b)
                })
                .expect("a ray hits the actual station bodies");
            sim.weapon_classes[0].ordnance = OrdnanceClass {
                max_damage: expected,
                ..Default::default()
            };
            sim.bolts.push(shot);
            let mut destroyed_event = false;
            for _ in 0..5 {
                sim.step(&[PadState::default()]);
                destroyed_event |= sim.events.iter().any(|e| matches!(e, Event::StationDestroyed { station, .. } if usize::from(*station) == index));
            }
            assert!(sim.stations[index].destroyed, "{map} {name}");
            assert!(!sim.stations[index].serving);
            assert!(destroyed_event);
            assert_eq!(sim.collision.station(s.instance.unwrap()), None);
            assert!(sim
                .collision
                .bodies()
                .iter()
                .filter(|b| b.instance == s.instance)
                .any(|b| b.layers != Layers::NONE));
            checked += 1;
        }
        assert_eq!(checked, 2);
    }
}

#[test]
fn bespin_spheres_match_pcsx2_and_collision_follows_idle_and_death_yaw() {
    let Some((_, mut sim, owner)) = world("bf1_bes1") else {
        return;
    };
    let i = sim
        .stations
        .iter()
        .position(|s| sim.station_classes[usize::from(s.class)].name == "com_item_healthrecharge")
        .unwrap();
    let c = &sim.station_classes[usize::from(sim.stations[i].class)];
    assert_eq!(
        c.sphere,
        Some([0.16547057, 1.1005828, -0.072598964, 1.3814149])
    );
    assert_eq!(
        c.wreck_sphere,
        Some([-0.22346294, 0.38546023, -0.19447792, 1.3204715])
    );
    let gonk = sim
        .station_classes
        .iter()
        .find(|c| c.name == "com_item_weaponrecharge")
        .unwrap();
    assert_eq!(gonk.sphere, Some([0.0, 0.72729754, 0.0, 0.8230426]));
    let before = sim.stations[i];
    for _ in 0..180 {
        sim.step(&[PadState::default()]);
    }
    let s = sim.stations[i];
    assert_ne!(s.motion.yaw, before.motion.yaw);
    assert!(s.sphere_centre.distance(before.sphere_centre) > 0.01);
    let sphere = sim.station_classes[usize::from(s.class)].sphere.unwrap();
    let centre = s
        .pose()
        .transform_point3(Vec3::new(sphere[0], sphere[1], sphere[2]));
    assert!(s.sphere_centre.distance(centre) < 0.00001);
    assert!(!sim.station_bodies[i].is_empty());
    for &(body, local) in &sim.station_bodies[i] {
        let expected = s.pose() * local;
        let actual = sim.collision.bodies()[body].to_world;
        assert_eq!(actual, expected);
    }
    let lethal = OrdnanceClass {
        max_damage: 10000.0,
        ..Default::default()
    }
    .damage_desc(0.0, false);
    sim.damage_station(i as u16, &lethal, 1.0, Some(owner), Some(0));
    let wreck = sim.station_wreck_collision[usize::from(s.class)].clone();
    let bodies: Vec<_> = sim
        .collision
        .bodies()
        .iter()
        .filter(|b| b.instance == s.instance && b.layers != Layers::NONE)
        .collect();
    assert_eq!(bodies.len(), wreck.len());
    for (body, part) in bodies.iter().zip(wreck) {
        assert_eq!(body.to_world, s.pose() * part.local);
    }
    let dead = sim.stations[i];
    for _ in 0..30 {
        sim.step(&[PadState::default()]);
    }
    assert_eq!(sim.stations[i].motion, dead.motion);
    assert_eq!(sim.stations[i].sphere_centre, dead.sphere_centre);
}

#[test]
fn destroyed_droids_render_the_authored_wreck_and_freeze_their_animation() {
    let Some((scene, mut sim, owner)) = world("bf1_bes1") else {
        return;
    };
    let Ok(mut renderer) = render::Renderer::new_headless(640, 480) else {
        eprintln!("SKIP: no GPU");
        return;
    };
    let mut gpu = game::viewer::upload_scene(&mut renderer, &scene).expect("upload");
    let names = ["com_item_healthrecharge", "com_item_weaponrecharge"];
    let models = game::stations::wreck_models(&scene).unwrap();
    let lethal = OrdnanceClass {
        max_damage: 10000.0,
        ..Default::default()
    }
    .damage_desc(0.0, false);
    let out = std::env::var_os("OPENBF_STATION_CAPTURES").map(std::path::PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).unwrap();
    }
    for name in names {
        let i = sim
            .stations
            .iter()
            .position(|s| sim.station_classes[usize::from(s.class)].name == name)
            .unwrap();
        let s = sim.stations[i];
        let instance = s.instance.unwrap() as usize;
        for _ in 0..10 {
            sim.step(&[PadState::default()]);
        }
        gpu.pose_stations(&sim);
        let live = gpu.view_draws(&sim, owner);
        let d = gpu
            .stations
            .iter()
            .find(|d| d.instance == instance)
            .unwrap();
        let yaw = d.anim.yaw;
        let live_model = d.part_model(&gpu.draws, 0).unwrap();
        let live_meshes: Vec<_> = live
            .iter()
            .filter(|draw| draw.model == live_model)
            .map(|draw| draw.mesh)
            .collect();
        assert!(!live_meshes.is_empty());
        let side = if name == "com_item_healthrecharge" {
            -2.4
        } else {
            2.4
        };
        let position = s.pos + Vec3::new(side, 1.8, 3.6);
        let direction = (s.sphere_centre - position).normalize();
        let camera = render::Camera {
            position,
            yaw: (-direction.x).atan2(-direction.z),
            pitch: direction.y.asin(),
            fov_y: 50f32.to_radians(),
            ..Default::default()
        };
        renderer.render(&camera, &live, gpu.clear).unwrap();
        let (before, w, h) = renderer.capture_rgba().unwrap();
        sim.damage_station(i as u16, &lethal, 1.0, Some(owner), Some(0));
        for _ in 0..60 {
            sim.step(&[PadState::default()]);
        }
        gpu.pose_stations(&sim);
        let broken = gpu.view_draws(&sim, owner);
        let d = gpu
            .stations
            .iter()
            .find(|d| d.instance == instance)
            .unwrap();
        assert_eq!(d.anim.yaw, yaw);
        assert!(!broken
            .iter()
            .any(|draw| draw.model == live_model && live_meshes.contains(&draw.mesh)));
        assert!(models.contains_key(&instance));
        let wreck = broken
            .iter()
            .filter(|draw| draw.model.transform_point3(Vec3::ZERO).distance(s.pos) < 0.01)
            .count();
        assert!(wreck > 0, "the authored wreck is drawn");
        renderer.render(&camera, &broken, gpu.clear).unwrap();
        let (after, _, _) = renderer.capture_rgba().unwrap();
        assert_ne!(before, after);
        if let Some(out) = &out {
            std::fs::write(
                out.join(format!("{name}_intact.png")),
                render::encode_png(&before, w, h).unwrap(),
            )
            .unwrap();
            std::fs::write(
                out.join(format!("{name}_destroyed.png")),
                render::encode_png(&after, w, h).unwrap(),
            )
            .unwrap();
        }
    }
}
