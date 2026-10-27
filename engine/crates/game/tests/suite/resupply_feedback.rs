use core::sim::{Event, PadState, Sim, Station};
use core::unit::Controller;
use game::fx::Fx;
use glam::Vec3;

#[test]
fn the_authored_resupply_effects_reach_the_gpu_with_their_real_textures() {
    let root = game::assets::baked_root();
    let out = std::env::var_os("OPENBF_RESUPPLY_CAPTURE").map(std::path::PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let Ok(mut renderer) = render::Renderer::new_headless(384, 384) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let camera = render::Camera {
        position: Vec3::new(0.0, 2.5, 5.0),
        pitch: -0.25,
        ..Default::default()
    };
    for (game, effect) in [("bf1", "Powerup_red"), ("bf2", "com_sfx_powerup_red")] {
        let map = root.join(format!("common_{game}"));
        if !map.join("effects.ron").exists() {
            eprintln!("SKIP: {} is not baked", map.display());
            continue;
        }
        let mut fx = Fx::load(&root, &map, game, &[]);
        assert!(fx.spawn(effect, game::fx::emitter::Frame::at(Vec3::ZERO)));
        fx.update(0.35);
        fx.submit(&mut renderer, &camera);
        renderer
            .render(&camera, &[], [0.01, 0.01, 0.01, 1.0])
            .unwrap();
        let (rgba, w, h) = renderer.capture_rgba().unwrap();
        let red = rgba
            .chunks_exact(4)
            .filter(|p| {
                i16::from(p[0]) - i16::from(p[1]) > 15 && i16::from(p[0]) - i16::from(p[2]) > 15
            })
            .count();
        assert!(red > 20, "{game}: only {red} red pixels reached the frame");
        if let Some(dir) = &out {
            std::fs::write(
                dir.join(format!("{game}-ammo-gain.png")),
                render::encode_png(&rgba, w, h).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn bespin_delivers_a_clip_every_two_seconds_with_a_red_effect_and_a_playable_sound() {
    let root = game::assets::baked_root();
    let map = root.join("bf1_bes1");
    if !map.join("manifest.ron").exists() {
        eprintln!("SKIP: BF1 BES1 is not baked");
        return;
    }
    let db = game::roster::load_classdb(&root.join("classes"), "bf1", &["all", "imp"]).unwrap();
    let roster = game::roster::ClassRoster::build(&db, "imp_inf_storm_trooper", 1, None).unwrap();
    let station = game::roster::station_class_from_db(&db, "com_item_weaponrecharge").unwrap();
    assert_eq!(station.delay, 2.0);
    let sound = station.powerup_sound.clone().unwrap();
    let mission =
        game::mission::MissionDef::load(&root.parent().unwrap().join("missions/bf1_bes1_con.ron"))
            .unwrap();
    let sounds = game::sounds::SoundLib::load_for(&map, &mission).unwrap();
    let take = sounds
        .take_path(&sound, 0)
        .expect("the ammo droid's sound must resolve");
    assert!(take.exists(), "{}", take.display());
    let flat = core::heightfield::Heightfield {
        size: 33,
        unit: 8.0,
        offset_x: 128.0,
        offset_z: 128.0,
        heights: vec![0.0; 33 * 33],
    };
    let mut walk = game::walk::WalkSim::new(1, Vec3::ZERO, None);
    walk.sim = Sim::empty(1);
    walk.sim.heightfield = Some(flat);
    let class = roster.register(&mut walk.sim, &mut Vec::new());
    walk.player = walk
        .sim
        .spawn(&class.spec(0, Vec3::ZERO, 0.0, Controller::Player { slot: 0 }, 0, 0))
        .unwrap();
    walk.sim.units[walk.player.index as usize].weapon.spare = 0;
    let index = walk.sim.add_station_class(station);
    walk.sim
        .stations
        .push(Station::new(index, Vec3::X * 2.0, 5.0));
    let mut fx = Fx::load(&root, &map, "bf1", &["all", "imp"]);
    let mut deliveries = Vec::new();
    let mut cues = 0;
    let mut visible = false;
    for tick in 1..=301 {
        walk.sim.step(&[PadState::default()]);
        let served = walk.sim.events.iter().any(|e| {
            matches!(e,
            Event::StationServed { unit: Some(u), ammo: true, .. } if *u == walk.player)
        });
        game::event_sounds::each(&walk, Vec3::ZERO, |name, _| {
            if name == sound {
                assert!(served);
                cues += 1;
            }
        });
        if served {
            deliveries.push(tick);
            assert_eq!(
                walk.sim.unit(walk.player).unwrap().weapon.spare,
                deliveries.len() as u32 * 55
            );
        }
        fx.ingest(&walk);
        fx.update(core::TICK_DT);
        let mut out = Vec::new();
        for effect in fx.live() {
            effect.draws(
                Vec3::new(0.0, 2.0, 8.0),
                Vec3::NEG_Z,
                &mut |_| None,
                &mut out,
            );
        }
        visible |= out
            .iter()
            .any(|p| p.color[0] > p.color[2] && p.color[3] > 0);
    }
    assert_eq!(deliveries.len(), 4);
    assert!((59..=61).contains(&deliveries[0]));
    assert!(deliveries
        .windows(2)
        .all(|p| (59..=61).contains(&(p[1] - p[0]))));
    assert_eq!(cues, 4);
    assert!(
        visible,
        "a successful ammo delivery must visibly draw the red effect"
    );
    assert_eq!(
        walk.sim
            .unit(walk.player)
            .unwrap()
            .weapon
            .rounds(&roster.weapon),
        Some(275)
    );
}
