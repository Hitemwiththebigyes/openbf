//! Card PWST slice 2 (`analysis/re-powerup-station.md` §6): the droids of
//! a baked battle drawn moving, read back from the scene's draws.

use glam::{Mat4, Vec3};

fn tick_motion(sim: &mut core::sim::Sim) {
    sim.tick += 1;
    for s in &mut sim.stations {
        s.motion.step(
            &sim.station_classes[usize::from(s.class)].anim,
            s.serving,
            core::TICK_DT,
        );
    }
}

/// Data-gated (the bake with the droid rigs, card PWST, and a GPU): on BF1
/// Naboo every placed droid is drawn with its sim station's animation.
/// Idle for 6 s, each droid has turned about its +Y (the FX-7 and the
/// vehicle droid at 1.5 rad/s, the gonk at 0.5); one FX-7 and one gonk
/// told they are serving stop turning, the FX-7's arms and socket move off
/// their bind pose and the gonk's body sits lower.
#[test]
fn the_droids_turn_idle_and_move_their_nodes_while_serving() {
    let root = game::assets::baked_root();
    let dir = root.join("bf1_nab1");
    let Ok(mut scene) = game::assets::load_scene(&dir) else {
        eprintln!("SKIP: bf1_nab1 not baked");
        return;
    };
    scene.apply_mode_layer("con");
    let common = root.join("common_bf1");
    scene.merge_common(game::assets::load_common(&common).expect("common_bf1"));
    if !scene
        .manifest
        .classes
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case("com_item_healthrecharge") && c.turret_rig.is_some())
    {
        eprintln!("SKIP: the bake has no droid rigs (older than card PWST slice 2)");
        return;
    }
    let mut sim = core::sim::Sim::empty(1);
    sim.heightfield = scene.heightfield();
    let f = ["rep".to_owned(), "cis".to_owned()];
    game::vehicle_pads::install(&mut sim, &scene.manifest, &dir, &root.join("classes"), &f)
        .expect("install");
    let Ok(mut r) = render::Renderer::new_headless(64, 64) else {
        eprintln!("SKIP: no GPU");
        return;
    };
    let mut gpu = game::viewer::upload_scene(&mut r, &scene).expect("upload");
    assert_eq!(
        sim.stations.len(),
        15,
        "{:?}",
        sim.station_classes
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
    );
    assert_eq!(gpu.stations.len(), 15, "every droid is drawn as one");
    for st in &sim.stations {
        assert!(gpu.stations.iter().any(|d| d.pos == st.pos), "{st:?}");
    }
    let class_of = |sim: &core::sim::Sim, pos: Vec3| {
        let st = sim.stations.iter().find(|s| s.pos == pos).unwrap();
        sim.station_classes[st.class as usize].name.clone()
    };
    let pick = |name: &str| {
        gpu.stations
            .iter()
            .position(|d| class_of(&sim, d.pos) == name)
            .unwrap()
    };
    let (fx7, gonk) = (
        pick("com_item_healthrecharge"),
        pick("com_item_weaponrecharge"),
    );
    let (fx7_pos, gonk_pos) = (gpu.stations[fx7].pos, gpu.stations[gonk].pos);
    // A part's node matrix now, from its draw: world . R_y(yaw) . node . bind^-1.
    let node_of = |gpu: &game::viewer::SceneGpu, i: usize, name: &str| -> Mat4 {
        let d = &gpu.stations[i];
        let rig = d.rig().expect("a rig");
        let n = rig
            .nodes
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(name))
            .unwrap();
        let k = rig.parts.iter().position(|p| *p == Some(n as u32)).unwrap();
        let inst = &scene.manifest.instances[d.instance];
        let world = game::viewer::instance_matrix(inst.rotation, inst.translation);
        let body = world * Mat4::from_rotation_y(d.anim.yaw);
        body.inverse()
            * d.part_model(&gpu.draws, k).unwrap()
            * game::viewer::turret_pose::bind_of(rig, n)
    };
    let bind = |gpu: &game::viewer::SceneGpu, i: usize, name: &str| {
        let rig = gpu.stations[i].rig().unwrap();
        let n = rig
            .nodes
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(name))
            .unwrap();
        game::viewer::turret_pose::bind_of(rig, n)
    };
    let off = |a: Mat4, b: Mat4| {
        a.to_cols_array()
            .iter()
            .zip(b.to_cols_array())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max)
    };
    // 6 s idle.
    for _ in 0..180 {
        tick_motion(&mut sim);
        gpu.pose_stations(&sim);
    }
    for d in &gpu.stations {
        let name = class_of(&sim, d.pos);
        let speed = if name == "com_item_weaponrecharge" {
            0.5
        } else {
            1.5
        };
        // At least one turn (>= 135 deg / speed) fits in 6 s; never more
        // than the whole time at full speed.
        let y = d.anim.yaw.abs();
        assert!(y > 0.0 && y <= speed * 6.0 + 1e-3, "{name}: yaw {y}");
    }
    // The model's own segments ride the body: the first draw is at
    // world . R_y(yaw) . node . bind^-1 = world . R_y(yaw) on a node at bind.
    assert!(
        off(
            node_of(&gpu, fx7, "bone_root"),
            bind(&gpu, fx7, "bone_root")
        ) < 1e-4
    );
    // Serving for 1.25 s.
    let (fx7_yaw, gonk_yaw) = (gpu.stations[fx7].anim.yaw, gpu.stations[gonk].anim.yaw);
    let (mut spun, mut bobbed) = (0.0f32, 0.0f32);
    for _ in 0..38 {
        for s in sim.stations.iter_mut() {
            s.serving = s.pos == fx7_pos || s.pos == gonk_pos;
        }
        tick_motion(&mut sim);
        gpu.pose_stations(&sim);
        let sock = "Equipment_Operator_Arm_Socket";
        spun = spun.max(off(node_of(&gpu, fx7, sock), bind(&gpu, fx7, sock)));
        bobbed = bobbed.min(node_of(&gpu, gonk, "gonk1").w_axis.y);
    }
    // Over a second and more: the socket's +-pi swing and the 0.08 m bob.
    assert!(spun > 1.0, "the socket spun {spun}");
    assert!(bobbed < -0.075, "the gonk bobbed {bobbed} m");
    let (a, g) = (&gpu.stations[fx7].anim, &gpu.stations[gonk].anim);
    assert_eq!((a.state, g.state), (3, 3));
    // The pass's write comes before the frame's animation (§4), so the
    // tick that saw the serve did not turn either.
    assert_eq!((a.yaw, g.yaw), (fx7_yaw, gonk_yaw), "no turn while serving");
    let t = a.timer;
    for (n, phase) in [("pH_tester_arm10", 0.0), ("pH_tester_arm6", 1.0)] {
        let want = bind(&gpu, fx7, n)
            * Mat4::from_rotation_x(0.15 * ((t - phase) * std::f32::consts::TAU).sin());
        let got = node_of(&gpu, fx7, n);
        assert!(off(got, want) < 1e-4, "{n}: {got:?} vs {want:?}");
    }
    let sock = node_of(&gpu, fx7, "Equipment_Operator_Arm_Socket");
    let want = bind(&gpu, fx7, "Equipment_Operator_Arm_Socket")
        * Mat4::from_rotation_z(std::f32::consts::PI * (t * std::f32::consts::TAU).sin());
    assert!(off(sock, want) < 1e-4);
    let body = node_of(&gpu, gonk, "gonk1");
    let dy = 0.08 * ((g.timer * std::f32::consts::TAU).cos() - 1.0) * 0.5;
    assert!(
        (body.w_axis.y - dy).abs() < 1e-5,
        "{} vs {dy}",
        body.w_axis.y
    );
    println!(
        "nab1: 15 droids drawn; serving 38 ticks: the socket's matrix up to {spun:.3} off its bind, \
         the gonk's body down to {bobbed:.4} m; idle yaws {:?}",
        gpu.stations.iter().map(|d| (d.anim.yaw * 100.0).round() / 100.0).collect::<Vec<_>>()
    );
    // The pass that serves nobody: the FX-7 turns again.
    for _ in 0..3 {
        for s in sim.stations.iter_mut() {
            s.serving = false;
        }
        tick_motion(&mut sim);
        gpu.pose_stations(&sim);
    }
    assert_ne!(gpu.stations[fx7].anim.yaw, fx7_yaw);
}
