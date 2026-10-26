use core::ai::Behaviour;
use core::sim::{Event, PadState, Pickup};
use core::unit::{Controller, UnitSpec};
use glam::Vec3;

#[test]
fn loaded_hoth_health_item_is_sought_and_collected_with_a_nearby_human() {
    let path = game::assets::baked_root().join("classes/bf2_ingame.ron");
    if !path.exists() {
        eprintln!("SKIP: {} is not baked", path.display());
        return;
    }
    let mut db = game::classdb::ClassDb::default();
    db.load(&path).unwrap();
    let item = game::roster::item_class_from_db(&db, "com_item_powerup_health25").unwrap();
    assert_eq!(item.health, 100.0);
    assert_eq!(item.life_span, 30.0);
    let Some((_, mut sim, _)) = super::station_destruction::world("bf2_hot1") else {
        return;
    };
    // Select a clear, nearly level lane from the loaded terrain and collision.
    // Avoid service spheres so only collecting the item can heal this bot.
    let lane = sim
        .stations
        .iter()
        .find_map(|station| {
            [Vec3::Z, Vec3::X, -Vec3::Z, -Vec3::X]
                .into_iter()
                .find_map(|dir| {
                    let on_floor = |p: Vec3| Vec3::new(p.x, sim.spawn_ground(p), p.z);
                    let origin = on_floor(station.pos + dir * 12.0);
                    let end = on_floor(origin + dir * 5.0);
                    let human = on_floor(origin - dir * 2.0);
                    let item_pos = on_floor(origin + dir * 4.0);
                    let level =
                        (origin.y - end.y).abs() < 0.25 && (origin.y - human.y).abs() < 0.25;
                    let away = (0..=5).all(|step| {
                        let p = origin + dir * step as f32;
                        sim.stations
                            .iter()
                            .all(|s| p.distance(s.pos) > s.radius + 1.0)
                    });
                    (level
                        && away
                        && sim.floor_line(origin, end)
                        && sim.floor_line(human, origin)
                        && core::perception::los_terrain(
                            &sim,
                            origin + Vec3::Y,
                            item_pos + Vec3::Y,
                        ))
                    .then_some((origin, item_pos, human))
                })
        })
        .expect("a clear pickup lane outside loaded station service spheres");
    let (origin, item_pos, human_pos) = lane;
    sim.spawn(&UnitSpec::player(0, human_pos)).unwrap();
    let bot = sim
        .spawn(&UnitSpec {
            controller: Controller::Ai,
            ..UnitSpec::trooper(0, origin)
        })
        .unwrap();
    sim.unit_mut(bot).unwrap().health.cur -= 100.0;
    let before = sim.unit(bot).unwrap().health.cur;
    let life = item.life_span;
    let class = sim.add_item_class(item);
    sim.pickups.push(Pickup {
        class,
        pos: item_pos,
        vel: Vec3::ZERO,
        yaw: 0.0,
        life,
        landed: true,
    });
    let mut detoured = false;
    let mut collected = false;
    for _ in 0..180 {
        sim.step(&[PadState::default()]);
        detoured |= sim.unit(bot).unwrap().ai.behaviour == Behaviour::Pickup;
        collected |= sim.events.iter().any(|e| {
            matches!(e,
                Event::ItemTaken { unit, health: true, .. } if *unit == bot
            )
        });
        assert!(
            !sim.events.iter().any(|e| matches!(e,
                Event::StationServed { unit, .. } if *unit == Some(bot)
            )),
            "station healing would confound the pickup control"
        );
        if collected {
            break;
        }
    }
    assert!(
        detoured && collected,
        "origin={origin:?}, item={item_pos:?}, detoured={detoured}, collected={collected}"
    );
    assert_eq!(sim.unit(bot).unwrap().health.cur, before + 100.0);
    assert!(sim.pickups.is_empty());
    assert!(sim.unit(bot).unwrap().soldier.pos.distance(origin) > 2.0);
}
