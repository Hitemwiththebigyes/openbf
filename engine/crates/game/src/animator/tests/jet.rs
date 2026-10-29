//! Card JETP2: the jet stances play `jetpack_hover` on the baked jet
//! classes of both games, a shot plays over it, and the soldier lands back
//! on his feet.
use super::*;

#[test]
fn both_jet_stances_take_jetpack_hover() {
    for game in [Game::Bf1, Game::Bf2] {
        for stance in [Stance::Jet, Stance::Hover] {
            let i = AnimInput {
                stance,
                ..input(4.0)
            };
            assert_eq!(stance_of(&i, game), Err(Special::JetHover));
        }
        let fall = AnimInput {
            stance: Stance::Fall,
            ..input(4.0)
        };
        assert_eq!(stance_of(&fall, game), Err(Special::Fall));
    }
    assert!(bf1::SPECIAL.contains(&Special::JetHover.name()));
    assert_eq!(bf2::simple_pair(Special::JetHover.name()), Some(0x19));
}

/// One baked jet class's model: (asset, rig), or None without a bake.
fn baked(game: &str, model: &str) -> Option<(crate::assets::SkinnedAsset, Rig)> {
    let root = crate::assets::baked_root().join(format!("side_{game}"));
    let path = root.join(format!("soldiers/{model}.glb"));
    if !path.exists() {
        eprintln!("skip: no {game} {model} bake");
        return None;
    }
    let asset = crate::assets::load_skinned_glb(&path).unwrap();
    let rig = crate::soldier_vis::SoldierVisuals::load_rig(&root, model, &asset).expect("rig");
    Some((asset, rig))
}

#[test]
fn baked_jet_troopers_hover_on_jetpack_hover_and_land_back_on_their_feet() {
    let mut checked = 0;
    for (game, model) in [
        ("bf1", "rep_inf_jettrooper"),
        ("bf1", "imp_inf_darktrooper"),
        ("bf2", "rep_inf_jettrooper"),
        ("bf2", "rep_inf_ep3jettrooper"),
        ("bf2", "imp_inf_darktrooper"),
    ] {
        let Some((asset, rig)) = baked(game, model) else {
            continue;
        };
        let names = |a: &Animator| {
            [a.lower_clip(), a.upper_clip()].map(|c| asset.clips[c.expect("a clip")].name.clone())
        };
        let mut a = Animator::new();
        let dt = 1.0 / 30.0;
        let mut i = AnimInput {
            set: rig.weapon_set("human", "rifle"),
            ..input(0.0)
        };
        a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
        for stance in [Stance::Jump, Stance::Hover, Stance::Jet] {
            i.stance = stance;
            for _ in 0..12 {
                a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
            }
            if stance == Stance::Jump {
                continue;
            }
            assert_eq!(
                a.body(),
                Some(Body::Special(Special::JetHover)),
                "{game} {model}"
            );
            for n in names(&a) {
                assert_eq!(n, "human_rifle_jetpack_hover", "{game} {model} {stance:?}");
            }
            // The clip plays (it loops: 61 frames).
            let phase = a.lower_phase();
            a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
            assert_ne!(a.lower_phase(), phase, "{game} {model}");
        }
        // A shot in the hover: the stand's shoot on the upper body, the legs
        // keep the jet's clip.
        i.stance = Stance::Hover;
        i.fired = true;
        a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
        i.fired = false;
        let [lower, upper] = names(&a);
        assert_eq!(lower, "human_rifle_jetpack_hover", "{game} {model}");
        assert!(upper.contains("shoot"), "{game} {model}: {upper}");
        // Landing out of it: BF1 its soft landing, then both games stand.
        i.stance = Stance::Stand;
        a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
        if game == "bf1" {
            assert_eq!(
                a.body(),
                Some(Body::Special(Special::Landing(false))),
                "{model}"
            );
        }
        for _ in 0..60 {
            a.step(&rig, &asset.clips, &asset.skeleton, &i, dt);
        }
        assert_eq!(a.body(), Some(Body::Feet(0, Move::Idle)), "{game} {model}");
        for n in names(&a) {
            assert!(!n.contains("jetpack"), "{game} {model}: {n}");
        }
        checked += 1;
    }
    println!("{checked} baked jet models checked");
}
