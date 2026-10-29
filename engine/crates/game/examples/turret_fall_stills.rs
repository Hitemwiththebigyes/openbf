//! Card TBLD: a before / after still of one building turret, standing and
//! destroyed (its wreck or build pad, `game::turret_fall`), as a battle view
//! draws it (`SceneGpu::view_draws`), side by side.
//!
//! cargo run -p game --example turret_fall_stills -- <map> <class> <side> <side> <out.png> [k]
//! (`k` of 8: the side the camera stands on, 10 m out and 4 m up.)

use anyhow::{anyhow, Context, Result};
use glam::Vec3;
use render::{Camera, Renderer};

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [map, class, s0, s1, out] = &a[..5] else {
        return Err(anyhow!("usage: <map> <class> <side> <side> <out.png> [k]"));
    };
    let k: usize = a.get(5).map_or(Ok(0), |s| s.parse())?;
    let root = game::assets::baked_root();
    let dir = root.join(map);
    let (scene, _) = game::assets::load_battle_scene(&dir, "con").context("scene")?;
    let mut sim = core::sim::Sim::empty(1);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.heightfield = scene.heightfield();
    scene.install_collision(&mut sim);
    let f = [s0.clone(), s1.clone()];
    let build =
        game::vehicle_pads::install(&mut sim, &scene.manifest, &dir, &root.join("classes"), &f)?;
    let ci = build
        .classes
        .iter()
        .position(|c| c.eq_ignore_ascii_case(class))
        .ok_or_else(|| anyhow!("no {class} among {:?}", build.classes))?;
    let i = sim
        .building_turrets
        .iter()
        .position(|t| t.spec.class as usize == ci)
        .ok_or_else(|| anyhow!("no building turret of {class}"))?;
    let (centre, radius) = sim.building_turrets[i].sphere();
    let (w, h) = (640u32, 480u32);
    let mut r = Renderer::new_headless(w, h).map_err(|e| anyhow!("renderer: {e}"))?;
    let gpu = game::viewer::upload_scene(&mut r, &scene)?;
    let ang = k as f32 * std::f32::consts::TAU / 8.0;
    let back = (radius + 8.0).max(10.0);
    let mut eye = centre + Vec3::new(ang.sin(), 0.0, ang.cos()) * back;
    let ground = sim.spawn_ground(eye);
    eye.y = eye.y.max(ground + 1.8) + 3.0;
    let d = centre - eye;
    let cam = Camera {
        position: eye,
        yaw: (-d.x).atan2(-d.z),
        pitch: (d.y / d.length()).asin(),
        ..Camera::default()
    };
    let nobody = core::unit::UnitHandle { index: 0, gen: 0 };
    let mut panels = Vec::new();
    for down in [false, true] {
        if down {
            let h = sim.building_turrets[i].handle;
            sim.destroy_vehicle(h, None);
        }
        let draws = gpu.view_draws(&sim, nobody);
        r.render(&cam, &draws, gpu.clear)
            .map_err(|e| anyhow!("render: {e}"))?;
        let (rgba, pw, _) = r.capture_rgba().map_err(|e| anyhow!("capture: {e}"))?;
        panels.push((rgba, pw));
    }
    let mut sheet = vec![0u8; (w * 2 * h * 4) as usize];
    for (p, (rgba, pw)) in panels.iter().enumerate() {
        for y in 0..h {
            let src = &rgba[(y * pw * 4) as usize..((y * pw + w) * 4) as usize];
            let at = ((y * w * 2 + p as u32 * w) * 4) as usize;
            sheet[at..at + src.len()].copy_from_slice(src);
        }
    }
    image::save_buffer(out, &sheet, w * 2, h, image::ColorType::Rgba8)?;
    println!(
        "wrote {out}: {map} {class} at {centre:?} (r {radius:.1}), fall {:?}, camera side {k}",
        sim.building_turrets[i].fall
    );
    Ok(())
}
