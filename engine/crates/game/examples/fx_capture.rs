//! One particle effect spawned on the terrain, rendered over its life from a fixed pose (card FXP2): the
//! port's half of an effect pair against a PCSX2 recording.
//!
//!   fx_capture <map_dir> <effect> <out_prefix> x,y,z,yaw_deg,pitch_deg,dist[,fov_y_deg[,w,h]] [t0,t1,...]
//!
//! The effect is spawned with world axes on the ground `dist` m ahead of the eye along the view's
//! horizontal heading (the PS2 spawns a bolt impact that way, re-fx §7); frames are written as
//! `<out_prefix>_<ms>.png` at each time (s; default every 1/30 s over 0.3 s). The scene as `pose_render`
//! draws it (conquest layer, no soldier, no HUD); the map's two sides' effect layers are those of the
//! mission's default teams — this loads only the map and common layers, and `FX_SIDES=cis,rep` those sides'.
use anyhow::{anyhow, Result};
use game::assets::load_scene;
use game::fx::emitter::Frame;
use game::fx::Fx;
use game::viewer::upload_scene;
use glam::Vec3;
use render::{Camera, Renderer};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        return Err(anyhow!(
            "usage: fx_capture <map_dir> <effect> <out_prefix> x,y,z,yaw,pitch,dist[,fov[,w,h]] [times]"
        ));
    }
    let map = Path::new(&args[1]);
    let c = args[4]
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()?;
    if c.len() < 6 {
        return Err(anyhow!("want x,y,z,yaw,pitch,dist[,fov[,w,h]]"));
    }
    let times: Vec<f32> = match args.get(5) {
        Some(t) => t
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<Result<_, _>>()?,
        None => (0..10).map(|i| i as f32 / 30.0).collect(),
    };
    let (w, h) = if c.len() >= 9 {
        (c[7] as u32, c[8] as u32)
    } else {
        (640, 480)
    };
    let mut scene = load_scene(map)?;
    scene.apply_mode_layer("con");
    let hf = scene
        .heightfield()
        .ok_or_else(|| anyhow!("the map has no heightfield"))?;
    let cam = Camera {
        position: Vec3::new(c[0], c[1], c[2]),
        yaw: c[3].to_radians(),
        pitch: c[4].to_radians(),
        fov_y: c.get(6).copied().unwrap_or(70.0).to_radians(),
        ..Camera::default()
    };
    let f = cam.forward();
    let ahead = Vec3::new(f.x, 0.0, f.z).normalize_or_zero() * c[5];
    let (x, z) = (cam.position.x + ahead.x, cam.position.z + ahead.z);
    let at = Vec3::new(x, hf.height_at(x, z), z);
    println!("spawn at {at}");
    let assets = map.parent().unwrap_or(Path::new("."));
    // `FX_SIDES=cis,rep`: those sides' effect layers too (a weapon's effect).
    let sides = std::env::var("FX_SIDES").unwrap_or_default();
    let sides: Vec<&str> = sides.split(',').filter(|x| !x.is_empty()).collect();
    let mut fx = Fx::load(assets, map, &scene.manifest.source_game, &sides);
    if !fx.spawn(&args[2], Frame::at(at)) {
        return Err(anyhow!("no layer holds effect {}", args[2]));
    }
    let mut r = Renderer::new_headless(w, h).map_err(|e| anyhow!("renderer: {e}"))?;
    let gpu = upload_scene(&mut r, &scene)?;
    let mut now = 0.0;
    for t in times {
        fx.update(t - now);
        now = t;
        fx.submit(&mut r, &cam);
        r.render_frame(&cam, &gpu.draws, &[], gpu.clear, None)
            .map_err(|e| anyhow!("render: {e}"))?;
        let (rgba, w, h) = r.capture_rgba().map_err(|e| anyhow!("capture: {e}"))?;
        let png = render::encode_png_rgb(&rgba, w, h).map_err(|e| anyhow!("png: {e}"))?;
        let out = format!("{}_{:03}.png", args[3], (t * 1000.0).round() as u32);
        std::fs::write(&out, png)?;
        println!("{out}");
    }
    Ok(())
}
