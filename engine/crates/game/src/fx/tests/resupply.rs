use super::*;
use core::sim::{Event, Sim};
use core::unit::UnitSpec;

fn library(game: &str) -> Option<Library> {
    let root = crate::assets::baked_root();
    let common = root.join(format!("common_{game}"));
    if !common.join("effects.ron").exists() {
        eprintln!("SKIP: {} is not baked", common.display());
        return None;
    }
    Some(Library::load(&root, &common, game, &[]))
}

fn draws(fx: &Fx) -> Vec<ParticleDraw> {
    let mut out = Vec::new();
    for e in fx.live() {
        e.draws(
            fx.library().unwrap().era,
            Vec3::new(0.0, 2.0, 8.0),
            Vec3::NEG_Z,
            &mut |_| None,
            &mut out,
        );
    }
    out
}

#[test]
fn both_games_draw_the_authored_red_and_blue_child_emitters() {
    for (game, names) in [
        ("bf1", ["Powerup_red", "Powerup_blue"]),
        ("bf2", ["com_sfx_powerup_red", "com_sfx_powerup_blue"]),
    ] {
        for (i, name) in names.into_iter().enumerate() {
            let Some(lib) = library(game) else { continue };
            let mut fx = Fx::with_library(lib);
            assert!(fx.spawn(name, Frame::at(Vec3::ZERO)));
            fx.update(0.35);
            let out = draws(&fx);
            assert!(
                !out.is_empty(),
                "{game} {name} must draw its child particles"
            );
            assert_eq!(format!("{out:?}"), format!("{:?}", draws(&fx)));
            assert!(
                out.iter().any(|p| {
                    p.color[3] > 0
                        && if i == 0 {
                            p.color[0] > p.color[2]
                        } else {
                            p.color[2] > p.color[0]
                        }
                }),
                "{game} {name} must have the authored tint"
            );
            fx.update(2.0);
            assert!(fx.is_empty());
        }
    }
}

#[test]
fn bf1_gain_bursts_are_spaced_along_the_parent_path() {
    let Some(lib) = library("bf1") else { return };
    let mut fx = Fx::with_library(lib);
    assert!(fx.spawn("Powerup_red", Frame::at(Vec3::ZERO)));
    fx.update(0.35);
    let mut heights: Vec<i32> = draws(&fx)
        .iter()
        .filter_map(|p| match p.shape {
            render::ParticleShape::Sprite { pos, .. } | render::ParticleShape::Quad { pos, .. } => {
                Some((pos.y * 1000.0).round() as i32)
            }
            _ => None,
        })
        .collect();
    heights.sort_unstable();
    heights.dedup();
    assert!(heights.len() >= 6, "child bursts collapsed to {heights:?}");
}

fn curve(age: f32) -> Option<Vec3> {
    Some(Vec3::new(age, age * age, 0.0))
}

#[test]
fn a_last_replay_step_of_a_tenth_or_less_moves_the_newest_node() {
    let (nodes, now) = children::path(0.35, curve).unwrap();
    assert!((now - 0.35).abs() < 1e-6);
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].time, 0.0);
    assert!((nodes[0].span - 0.35).abs() < 1e-6);
    let end = curve(0.35).unwrap();
    assert!(nodes[0].vel.abs_diff_eq(end / (0.35 + 0.0001), 1e-4));
    assert_eq!(nodes[1].pos, end);
    assert_eq!(nodes[1].vel, Vec3::ZERO);
}

#[test]
fn the_replay_nodes_match_live_bf1_captures() {
    let p0 = Vec3::new(137.961_52, 99.389_33, 60.547_295);
    let cases = [
        (
            0.266_923_13,
            vec![p0, Vec3::new(137.761_32, 100.689_13, 61.251_97)],
            vec![(0.0, Vec3::new(-0.750_011_1, 4.869_584, 2.639_987_5))],
        ),
        (
            0.367_026_84,
            vec![p0, Vec3::new(137.686_22, 101.121_48, 61.516_243)],
            vec![(0.0, Vec3::new(-0.749_874_5, 4.718_137, 2.639_274))],
        ),
        (
            0.467_130_54,
            vec![
                p0,
                Vec3::new(137.736_51, 100.835_32, 61.339_294),
                Vec3::new(137.611_15, 101.523_79, 61.780_518),
            ],
            vec![
                (0.0, Vec3::new(-0.750_020_3, 4.819_972, 2.639_999)),
                (0.3, Vec3::new(-0.750_109_6, 4.119_349, 2.639_991_3)),
            ],
        ),
    ];
    for (age, samples, expect) in cases {
        let end = *samples.last().unwrap();
        let (nodes, now) = children::path(age, |a| {
            Some(if a == 0.0 {
                samples[0]
            } else if (a - 0.3).abs() < 1e-6 && samples.len() == 3 {
                samples[1]
            } else {
                end
            })
        })
        .unwrap();
        assert!((now - age).abs() < 1e-5, "{age}: now {now}");
        assert_eq!(nodes.len(), expect.len() + 1, "{age}: {nodes:?}");
        for (node, (time, vel)) in nodes.iter().zip(&expect) {
            assert!((node.time - time).abs() < 1e-5, "{age}: {nodes:?}");
            assert!(node.vel.abs_diff_eq(*vel, 1e-3), "{age}: {node:?} vs {vel}");
        }
        let last = nodes.last().unwrap();
        assert_eq!(last.pos, end);
        assert_eq!(last.vel, Vec3::ZERO);
    }
}

#[test]
fn a_longer_last_replay_step_keeps_every_sample_with_secant_velocities() {
    let (nodes, _) = children::path(0.5, curve).unwrap();
    let times: Vec<f32> = nodes.iter().map(|n| n.time).collect();
    assert_eq!(nodes.len(), 3, "{times:?}");
    let (p3, p5) = (curve(0.3).unwrap(), curve(0.5).unwrap());
    assert!(nodes[0].vel.abs_diff_eq(p3 / 0.3, 1e-4));
    assert!(nodes[1].vel.abs_diff_eq((p5 - p3) / 0.2, 1e-3));
    assert_eq!(nodes[2].vel, Vec3::ZERO);
    let mid = children::frame_at(&nodes, 0.15);
    assert!(mid.pos.abs_diff_eq(p3 * 0.5, 1e-5));
    assert_eq!(mid.rot, glam::Mat3::IDENTITY);
    assert!(children::frame_at(&nodes, 0.5).pos.abs_diff_eq(p5, 1e-5));
}

#[test]
fn an_ammo_gain_follows_the_unit_and_plays_once_while_its_effect_is_alive() {
    let Some(lib) = library("bf1") else { return };
    let mut fx = Fx::with_library(lib);
    let mut sim = Sim::empty(1);
    let unit = sim.spawn(&UnitSpec::player(0, Vec3::ZERO)).unwrap();
    sim.units[unit.index as usize].soldier.pos = Vec3::ZERO;
    let event = Event::StationServed {
        station: 0,
        unit: Some(unit),
        vehicle: None,
        pos: Vec3::X * 3.0,
        health: false,
        ammo: true,
        energy: false,
    };
    sim.events = vec![event, event];
    fx.ingest_sim(&sim);
    assert_eq!(fx.live().len(), 1);
    fx.update(0.35);
    let before = fx.live()[0].particles(Era::Bf1);
    sim.events.clear();
    sim.units[unit.index as usize].soldier.pos += Vec3::X * 2.0;
    fx.ingest_sim(&sim);
    assert_eq!(fx.live()[0].frame.pos, Vec3::X * 2.0);
    let after = fx.live()[0].particles(Era::Bf1);
    for (a, b) in before.iter().flatten().zip(after.iter().flatten()) {
        assert!((b.pos - a.pos).abs_diff_eq(Vec3::X * 2.0, 0.0001));
    }
    assert!(!draws(&fx).is_empty());
}
