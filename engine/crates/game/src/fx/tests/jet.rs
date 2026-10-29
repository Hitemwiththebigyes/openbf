//! Card JETP2: a soldier in a jet stance carries his class's `JetEffect`;
//! it stops when the jet ends or he dies, and a new jet spawns a new one.
use super::*;
use core::soldier::Stance;
use core::unit::UnitSpec;

fn library(game: &str, faction: &str) -> Option<Library> {
    let root = crate::assets::baked_root();
    let common = root.join(format!("common_{game}"));
    if !common.join("effects.ron").exists() {
        eprintln!("SKIP: {} is not baked", common.display());
        return None;
    }
    Some(Library::load(&root, &common, game, &[faction]))
}

#[test]
fn a_jet_carries_its_effect_on_the_back_and_stops_it_at_the_end() {
    for (game, faction, name) in [
        ("bf2", "rep", "rep_sfx_jetpack"),
        ("bf2", "imp", "imp_sfx_jetpack_darktrooper"),
        ("bf1", "rep", "jetpack"),
        ("bf1", "imp", "darktrooper_jetpack"),
    ] {
        let Some(lib) = library(game, faction) else {
            continue;
        };
        assert!(lib.get(name).is_some(), "{game} {name} baked");
        let mut fx = Fx::with_library(lib);
        let mut sim = Sim::empty(2);
        let h = sim
            .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::new(4.0, 10.0, -2.0)))
            .unwrap();
        let look = |_: &core::unit::Unit| Some(name);
        let set = |sim: &mut Sim, stance: Stance| {
            sim.units[h.index as usize].soldier.stance = stance;
        };
        // On foot: nothing.
        fx.follow_jets(&sim, look);
        assert!(fx.jets().is_empty(), "{game} {name}");
        for jet in [Stance::Hover, Stance::Jet] {
            set(&mut sim, jet);
            fx.follow_jets(&sim, look);
            assert_eq!(fx.jets(), [h], "{game} {name} {jet:?}");
            // One effect per soldier, however long he jets; it rides him.
            sim.units[h.index as usize].soldier.pos.x += 3.0;
            fx.follow_jets(&sim, look);
            fx.update(0.3);
            assert_eq!(fx.jets(), [h]);
            let e = fx.live().iter().find(|e| e.jet == Some(h)).unwrap();
            let u = &sim.units[h.index as usize].soldier;
            let want = crate::fx::jet::jet_frame(u.pos, u.yaw, u.vel);
            assert!(e.frame.pos.abs_diff_eq(want.pos, 1e-5));
            assert!(
                (e.frame.pos.y - u.pos.y - 1.481).abs() < 1e-4,
                "on the back"
            );
            // The frame's x runs up the spine: the exhaust (-x) goes down.
            assert!(e.frame.rot.x_axis.abs_diff_eq(Vec3::Y, 1e-5));
            assert!(e.emitted().iter().sum::<u32>() > 0, "{name} emits");
            assert!(!e.is_stopped());
            // The jet ends (a tap, empty fuel, a landing): stopped.
            set(&mut sim, Stance::Fall);
            fx.follow_jets(&sim, look);
            assert!(fx.jets().is_empty());
            assert!(fx
                .live()
                .iter()
                .any(|e| e.name() == Some(name) && e.is_stopped()));
        }
        // A death in the jet stops it too.
        set(&mut sim, Stance::Hover);
        fx.follow_jets(&sim, look);
        assert_eq!(fx.jets(), [h]);
        sim.units[h.index as usize].alive = false;
        fx.follow_jets(&sim, look);
        assert!(fx.jets().is_empty());
        // Its particles live out, then it is gone.
        fx.update(5.0);
        assert!(fx.live().iter().all(|e| e.name() != Some(name)), "{name}");
    }
}

#[test]
fn a_class_without_a_jet_effect_spawns_nothing() {
    let Some(lib) = library("bf2", "rep") else {
        return;
    };
    let mut fx = Fx::with_library(lib);
    let mut sim = Sim::empty(1);
    let h = sim
        .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
        .unwrap();
    sim.units[h.index as usize].soldier.stance = Stance::Hover;
    fx.follow_jets(&sim, |_| None);
    assert!(fx.is_empty());
    fx.follow_jets(&sim, |_| Some("no_such_effect"));
    assert!(fx.is_empty());
}

/// The drawn bone moves a running jet effect at once and places the next.
#[test]
fn the_drawn_bone_carries_the_effect() {
    let Some(lib) = library("bf2", "rep") else {
        return;
    };
    let mut fx = Fx::with_library(lib);
    let mut sim = Sim::empty(1);
    let h = sim
        .spawn(&UnitSpec::player_in_slot(0, 0, Vec3::ZERO))
        .unwrap();
    sim.units[h.index as usize].soldier.stance = Stance::Hover;
    // A quarter turn about z: x up.
    let bone = Mat4::from_cols(
        Vec3::Y.extend(0.0),
        Vec3::NEG_X.extend(0.0),
        Vec3::Z.extend(0.0),
        Vec3::new(1.0, 2.0, 3.0).extend(1.0),
    );
    let mut frames = vec![None; sim.units.len()];
    frames[h.index as usize] = Some(bone);
    fx.set_jet_frames(frames.clone());
    fx.follow_jets(&sim, |_| Some("rep_sfx_jetpack"));
    let e = fx.live().iter().find(|e| e.jet == Some(h)).unwrap();
    assert!(e.frame.pos.abs_diff_eq(Vec3::new(1.0, 2.0, 3.0), 1e-5));
    assert!(e.frame.rot.x_axis.abs_diff_eq(Vec3::Y, 1e-5));
    frames[h.index as usize] = Some(Mat4::from_translation(Vec3::X * 9.0));
    fx.set_jet_frames(frames);
    let e = fx.live().iter().find(|e| e.jet == Some(h)).unwrap();
    assert!(e.frame.pos.abs_diff_eq(Vec3::X * 9.0, 1e-5));
}
