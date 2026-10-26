use core::sim::{Event, Sim};
use core::sticky::Sticky;
use core::unit::UnitHandle;
use game::fx::Fx;
use glam::Vec3;

fn names(fx: &Fx) -> Vec<String> {
    fx.live()
        .iter()
        .filter(|e| !e.is_dead())
        .map(|e| e.name().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn the_bf1_thermal_detonator_glows_red_in_flight_and_splashes_in_water() {
    let root = game::assets::baked_root();
    let map = root.join("common_bf1");
    if !map.join("effects.ron").exists() || !root.join("classes").exists() {
        eprintln!("SKIP: common_bf1 or the classes are not baked");
        return;
    }
    let db = game::roster::load_classdb(&root.join("classes"), "bf1", &["all", "imp"]).unwrap();
    let w = game::roster::thrown_weapon_from_db(&db, "imp_weap_inf_thermaldetonator")
        .unwrap()
        .unwrap();
    let trail = w.ordnance.impact.as_ref().and_then(|l| l.trail.clone());
    assert_eq!(trail.as_deref(), Some("com_sfx_weap_grenade_red"));
    let class = w.ordnance.sticky.clone().unwrap();
    let look = class.explosion.as_ref().unwrap().look.clone();
    assert_eq!(look.water_effect.as_deref(), Some("watersplash_lg"));
    assert!(look.radius_outer > 0.0, "{look:?}");
    let land = look.effects.clone();
    assert!(!land.is_empty());

    let mut sim = Sim::empty(1);
    sim.water_level = Some(0.0);
    let life = w.ordnance.life_span;
    let wc = sim.add_weapon_class(w);
    sim.stickies.push(Sticky::new(
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 4.0, -10.0),
        &class,
        life,
        UnitHandle { index: 0, gen: 0 },
        0,
        wc,
    ));
    let mut fx = Fx::load(&root, &map, "bf1", &["all", "imp"]);
    let mut renderer = render::Renderer::new_headless(384, 384).ok();
    if renderer.is_none() {
        eprintln!("SKIP (GPU part): no GPU adapter");
    }
    let mut blast = None;
    let mut rendered = false;
    for tick in 0..(30.0 * (life + 4.0)) as usize {
        sim.step(&[]);
        fx.ingest_sim(&sim);
        fx.update(core::TICK_DT);
        if let Some(g) = sim.stickies.first() {
            let e = fx
                .live()
                .iter()
                .find(|e| e.name() == trail.as_deref() && !e.is_dead())
                .unwrap_or_else(|| panic!("tick {tick}: no trail on the grenade"));
            assert_eq!(e.frame.pos, g.pos, "tick {tick}: the trail rides the round");
            if tick == 6 {
                if let Some(r) = renderer.as_mut() {
                    let camera = render::Camera {
                        position: g.pos + Vec3::new(0.0, 0.5, 4.0),
                        pitch: -0.1,
                        ..Default::default()
                    };
                    fx.submit(r, &camera);
                    r.render(&camera, &[], [0.01, 0.01, 0.01, 1.0]).unwrap();
                    let (rgba, w, h) = r.capture_rgba().unwrap();
                    let red = rgba
                        .chunks_exact(4)
                        .filter(|p| {
                            i16::from(p[0]) - i16::from(p[1]) > 15
                                && i16::from(p[0]) - i16::from(p[2]) > 15
                        })
                        .count();
                    assert!(
                        red > 20,
                        "only {red} red pixels of the glow reached the frame"
                    );
                    if let Some(dir) = std::env::var_os("OPENBF_WATER_CAPTURE") {
                        let dir = std::path::PathBuf::from(dir);
                        std::fs::create_dir_all(&dir).unwrap();
                        std::fs::write(
                            dir.join("bf1-thermal-glow.png"),
                            render::encode_png(&rgba, w, h).unwrap(),
                        )
                        .unwrap();
                    }
                    rendered = true;
                }
            }
        }
        if let Some(pos) = sim.events.iter().find_map(|e| match e {
            Event::StickyBlast { pos, .. } => Some(*pos),
            _ => None,
        }) {
            blast = Some(pos);
            break;
        }
    }
    let pos = blast.expect("the fuse runs out in the water");
    assert!(pos.y <= 0.3, "{pos}");
    let live = names(&fx);
    assert!(live.iter().any(|n| n == "watersplash_lg"), "{live:?}");
    assert!(!live.iter().any(|n| land.contains(n)), "{live:?}");
    let splash = fx
        .live()
        .iter()
        .find(|e| e.name() == Some("watersplash_lg"))
        .unwrap();
    assert_eq!(splash.frame.pos, Vec3::new(pos.x, 0.02, pos.z));
    assert!(fx
        .live()
        .iter()
        .filter(|e| e.name() == trail.as_deref())
        .all(|e| e.is_stopped()));
    assert!(rendered || renderer.is_none());
}

#[test]
fn kas1_carries_its_wmap_mask_and_a_ripple_reaches_the_frame() {
    let root = game::assets::baked_root();
    let dir = root.join("bf1_kas1");
    let Ok(text) = std::fs::read_to_string(dir.join("manifest.ron")) else {
        eprintln!("SKIP: bf1_kas1 is not baked");
        return;
    };
    let m: manifest::Manifest = ron::from_str(&text).unwrap();
    let water = m.water.as_ref().unwrap();
    let wet = water.wet.as_ref().expect("a bake with the WMAP mask");
    let mask =
        core::water::WetMask::new(wet.grid, wet.cell_size, wet.half_extent, &wet.cells()).unwrap();
    assert_eq!(
        (wet.grid, wet.cell_size, mask.wet_cells()),
        (64, 32.0, 4014)
    );
    let at = Vec3::new(210.79, water.height, -38.07);
    assert!(mask.is_wet(at.x, at.z));
    let mut fx = Fx::load(&root, &dir, "bf1", &["all", "imp"]);
    fx.set_water(&m);
    let look = core::impact::ExplosionLook {
        radius_outer: 6.0,
        ..Default::default()
    };
    assert!(!fx.explode(&look, at - Vec3::Y * 0.2, Some(water.height)));
    let r = fx.ripples()[0];
    assert_eq!((r.life, r.size, r.at), (3.4, 0.4, at));
    fx.update(0.5);
    let Ok(mut renderer) = render::Renderer::new_headless(256, 256) else {
        eprintln!("SKIP (GPU part): no GPU adapter");
        return;
    };
    let camera = render::Camera {
        position: at + Vec3::new(0.0, 4.0, 0.0),
        pitch: -1.55,
        ..Default::default()
    };
    let mut lit = |fx: &mut Fx| {
        fx.submit(&mut renderer, &camera);
        renderer.render(&camera, &[], [0.0, 0.0, 0.0, 1.0]).unwrap();
        let (rgba, w, h) = renderer.capture_rgba().unwrap();
        if let Some(d) = std::env::var_os("OPENBF_WATER_CAPTURE") {
            let d = std::path::PathBuf::from(d);
            std::fs::create_dir_all(&d).unwrap();
            let png = render::encode_png(&rgba, w, h).unwrap();
            std::fs::write(
                d.join(format!("kas1-ripple-{}.png", fx.ripples().len())),
                png,
            )
            .unwrap();
        }
        rgba.chunks_exact(4)
            .filter(|p| p[..3].iter().any(|&c| c > 12))
            .count()
    };
    let ring = lit(&mut fx);
    assert_eq!(fx.ripples().len(), 1);
    fx.clear();
    let bare = lit(&mut fx);
    assert_eq!(bare, 0);
    assert!(
        ring > 200,
        "only {ring} pixels of the ripple reached the frame"
    );
}
