//! Data-gated: the streamed VO and the resident CORE bank item 15 baked are
//! reachable from the runtime library — the two lines the queue names, plus a
//! census of how many of each map's sound properties can now make a noise.
//! Skips cleanly when nothing has been baked (`scripts/rebake.sh`).

use std::path::PathBuf;

/// The bake under test. `OPENBF_SOUND_ROOT=<dir>` points the census at a
/// SCRATCH bake, which is how item VO's numbers were measured before the
/// shared tree was rebaked (the per-map streams only exist after a rebake).
fn root() -> PathBuf {
    match std::env::var("OPENBF_SOUND_ROOT") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => game::assets::baked_root(),
    }
}

fn map_dir(map: &str) -> Option<PathBuf> {
    let d = root().join(map);
    d.join("sounds/sounds.ron").exists().then_some(d)
}

/// `<map>/sounds/names.ron`, for reporting a silent property by name.
fn names(map: &str) -> std::collections::HashMap<u32, String> {
    #[derive(serde::Deserialize)]
    struct NamesRon {
        names: Vec<(u32, String, ron::Value)>,
    }
    let Some(d) = map_dir(map) else {
        return Default::default();
    };
    std::fs::read_to_string(d.join("sounds/names.ron"))
        .ok()
        .and_then(|t| ron::from_str::<NamesRon>(&t).ok())
        .map(|n| n.names.into_iter().map(|(h, s, _)| (h, s)).collect())
        .unwrap_or_default()
}

fn lib(map: &str) -> Option<game::sounds::SoundLib> {
    game::sounds::SoundLib::load(&map_dir(map)?)
}

/// BF1's tactical chatter lives in SOUND/GCW.LVL, not in TAT.LVL's banks
/// (`engine/artifacts/global_sound/README.md`): "follow me" must resolve to an
/// OGG under `audio/global_bf1/` — through the index's `files`, so under the
/// stream that baked it (`99d69d17`) and NOT under the property's own stream
/// hash (`acc55d97`, gcw_tac_vo) — and decode to the disc's 0.55 s.
#[test]
fn bf1_tactical_chatter_resolves_to_a_streamed_take() {
    let Some(sl) = lib("tat1") else {
        eprintln!("SKIP: tat1 not baked");
        return;
    };
    assert!(sl.has("all_inf_com_chatter_tac_follow"));
    let path = sl
        .take_path("all_inf_com_chatter_tac_follow", 0)
        .expect("first take resolves");
    let tail: PathBuf = path
        .iter()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .collect();
    assert_eq!(
        tail,
        PathBuf::from("global_bf1/99d69d17/c045c279.ogg"),
        "{}",
        path.display()
    );
    // The property names three takes; all three are baked.
    for nth in 0..3 {
        assert!(
            sl.take_path("all_inf_com_chatter_tac_follow", nth)
                .is_some(),
            "take {nth}"
        );
    }
    assert!(sl.take_path("all_inf_com_chatter_tac_follow", 3).is_none());
    if !audio::Audio::backend_compiled() {
        eprintln!("SKIP decode: built without the audio backend");
        return;
    }
    let data = audio::decode(&path).expect("decode the streamed take");
    let secs = data.duration_secs();
    println!(
        "follow me: {} — {secs:.3} s, {} frames at {} Hz",
        path.display(),
        data.frames(),
        data.sample_rate()
    );
    // The disc's ADPCM run is 12,236 frames at 22,071 Hz = 0.554 s
    // (`audio/global_bf1/global.ron`); our Vorbis encode of it decodes to
    // 12,672 frames = 0.574 s — the encoder's own padding, 3.6 % long, which
    // is a property of the item-15 bake, not of this lookup.
    assert_eq!(data.sample_rate(), 22_071, "the segment's own rate");
    assert!(
        (0.55..0.60).contains(&secs),
        "{secs} s is not the disc's 0.554 s line"
    );
}

/// HOLD 9's `shell_menu_accept` (0x6a942216) is a definition of BF2's CORE.LVL
/// resident bank, not of any map (`analysis/re-sound-global.md`): a BF2 map's
/// library must find it, and its sample is the disc's 11,060 Hz B75D27CD.
#[test]
fn bf2_shell_menu_accept_resolves_in_the_resident_core_bank() {
    let Some(sl) = lib("tat2") else {
        eprintln!("SKIP: tat2 not baked");
        return;
    };
    let prop = sl.global(0x6a94_2216).expect("resident CORE definition");
    assert_eq!(prop.pitch, 1.1, "the disc's pitch");
    assert_eq!(prop.samples.len(), 1);
    assert!(sl.has("shell_menu_accept"), "playable from a map's library");
    let path = sl.take_path("shell_menu_accept", 0).expect("its sample");
    let tail: PathBuf = path
        .iter()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .collect();
    assert_eq!(
        tail,
        PathBuf::from("sounds/1dff06ae/b75d27cd.wav"),
        "{}",
        path.display()
    );
    assert!(
        path.iter().any(|c| c == "core_bf2"),
        "the BF2 disc's bank: {}",
        path.display()
    );
    if !audio::Audio::backend_compiled() {
        eprintln!("SKIP decode: built without the audio backend");
        return;
    }
    let data = audio::decode(&path).expect("decode the resident sample");
    println!(
        "shell_menu_accept: {} — {} frames at {} Hz",
        path.display(),
        data.frames(),
        data.sample_rate()
    );
    assert_eq!(data.sample_rate(), 11_060);
}

/// The shell's library (`SoundLib::load_shell`) reaches the CORE.LVL clicks
/// with the PS2's sample, pitch, gain and `shellfx` bus 0.4, and the music's
/// four takes at `shellmusic` 0.6 (`analysis/re-bf2-menus.md` §4.2-§4.4).
#[test]
fn the_shell_menu_clicks_resolve_with_the_ps2_pitch_and_bus() {
    use ui::shell_feel::MenuSound as S;
    let root = root();
    let shell = game::shell_movie::shell_dir(&root);
    let Some(mut sl) = game::sounds::SoundLib::load_shell(&shell, &root) else {
        eprintln!("SKIP: the BF2 shell sounds are not baked");
        return;
    };
    if !sl.has("shell_select_change") {
        eprintln!("SKIP: core_bf2 not baked");
        return;
    }
    for (s, sample, pitch) in [
        (S::Select, "b21a7dea", 1.5),
        (S::Accept, "b75d27cd", 1.1),
        (S::Exit, "ebaa708c", 0.8),
        (S::Cancel, "1c2ecdee", 1.0),
        (S::Error, "1c2ecdee", 1.0),
        (S::Key, "c0d52579", 1.5),
        (S::Backspace, "c0d52579", 1.0),
    ] {
        let starts = sl.play_2d(None, s.name());
        let [st] = starts.as_slice() else {
            panic!("{}: {starts:?}", s.name());
        };
        let file = st.path.file_stem().and_then(|f| f.to_str());
        assert_eq!(file, Some(sample), "{}", s.name());
        assert!(st.path.iter().any(|c| c == "core_bf2"), "{}", s.name());
        assert_eq!(
            (st.pitch, st.gain, st.bus),
            (pitch, 1.0, 0.4),
            "{}",
            s.name()
        );
        assert!(st.flat, "{}: 2-D", s.name());
    }
    let (gain, takes) = sl
        .stream_takes(ui::shell_ps2::MUSIC)
        .expect("shell_soundtrack's takes");
    assert_eq!(takes.len(), 4);
    assert!((gain - 0.6).abs() < 1e-6, "gain 1 x shellmusic 0.6: {gain}");
    let secs: Vec<f32> = takes.iter().map(audio::StreamTake::secs).collect();
    println!("shell_soundtrack takes: {secs:?}");
    // §4.4 lists 51.4, 51.4, 64.4, 56.6 s; the takes' own ADPCM lengths
    // read up to 0.5 s shorter.
    for (s, want) in secs.iter().zip([51.4, 51.4, 64.4, 56.6]) {
        assert!((s - want).abs() < 0.6, "{secs:?}");
    }
}

/// A BF1 map's library must not reach into BF2's speech, or the reverse.
#[test]
fn a_map_takes_its_own_disc_s_banks() {
    let (Some(bf1), Some(bf2)) = (lib("tat1"), lib("tat2")) else {
        eprintln!("SKIP: tat1/tat2 not baked");
        return;
    };
    // Both discs' CORE.LVL define `shell_menu_accept` the same way, down to
    // the sample hash (measured: the two baked WAVs are byte-identical), so
    // the separation to check is WHICH bank a map reads it from.
    for (name, sl, core) in [("tat1", &bf1, "core_bf1"), ("tat2", &bf2, "core_bf2")] {
        let p = sl
            .take_path("shell_menu_accept", 0)
            .expect("{name}: resident");
        assert!(p.iter().any(|c| c == core), "{name}: {}", p.display());
    }
    for (name, sl, want) in [("tat1", &bf1, "global_bf1"), ("tat2", &bf2, "global_bf2")] {
        assert!(sl.census().any > 0, "{name}: nothing resolves");
        let one = sl
            .take_path("all_inf_com_chatter_tac_follow", 0)
            .or_else(|| sl.take_path("all_inf_com_chatter_tac_hold", 0));
        if let Some(p) = one {
            assert!(
                p.iter().any(|c| c == want),
                "{name} pulled a take from the other disc: {}",
                p.display()
            );
        }
    }
}

/// The census the item asks for: how many of a map's distinct sound properties
/// name audio this bake holds (bank WAVs, then WAVs + streamed VO), and what
/// the silent remainder is waiting on. `music` and `cut` are by design and by
/// the disc; `unbaked` is the only column a bake can still close.
#[test]
fn census_of_properties_that_can_make_a_noise() {
    let maps = [
        "tat1", "tat2", "bf2_cor1", "bf2_end1", "bf2_myg1", "bf2_yav1",
    ];
    let mut seen = 0;
    println!(
        "{:9} {:>6} {:>7} {:>7} {:>8} {:>7} {:>8} {:>6}",
        "map", "props", "wav", "+vo", "gain", "music", "unbaked", "cut"
    );
    for m in maps {
        let Some(sl) = lib(m) else {
            continue;
        };
        seen += 1;
        let c = sl.census();
        let gain = 100.0 * (c.any - c.wav) as f32 / c.props as f32;
        println!(
            "{m:9} {:6} {:7} {:7} {gain:6.1}% {:7} {:8} {:6}",
            c.props, c.wav, c.any, c.music, c.unbaked, c.cut
        );
        if m != "tat1" {
            // Report 15: roughly 45 % of a BF2 map's names were silent for
            // want of the stream bake.
            assert!(
                gain > 30.0,
                "{m}: the streamed VO added only {gain:.1}% of its properties"
            );
        }
    }
    if seen == 0 {
        eprintln!("SKIP: no baked maps");
    }
}

/// The names item VO could NOT fix, listed rather than counted: a property
/// whose every take is a segment no stream on either disc carries. The count
/// is asserted so a bake that starts LOSING audio is caught; the list is
/// printed for the report.
#[test]
fn the_takes_that_are_on_no_disc_are_named() {
    let maps = [
        "tat1", "tat2", "bf2_cor1", "bf2_end1", "bf2_myg1", "bf2_yav1",
    ];
    let mut seen = 0;
    for m in maps {
        let Some(sl) = lib(m) else {
            continue;
        };
        seen += 1;
        let dict = names(m);
        let cut = sl.silent_props(game::sounds::Silence::Cut);
        let listed: Vec<String> = cut
            .iter()
            .map(|h| dict.get(h).cloned().unwrap_or_else(|| format!("?{h:08x}")))
            .collect();
        println!("{m}: {} on no disc — {}", cut.len(), listed.join(" "));
        // With the map's own stream index baked (`streams.ron`, on all 32
        // maps of today's bake) what is left is measured: 1 on tat1, 4-9 per
        // BF2 map (2026-09-09, over the disc's `snd_` scripts). An older bake
        // has no index, and this column would still hold the map's OWN
        // streams (30 on tat1, 8-12 per BF2 map — the GS report's
        // `notinLVL`), so it skips.
        if !map_dir(m).is_some_and(|d| d.join("sounds/streams/streams.ron").exists()) {
            println!("  (pre-rebake: this map's own streams are not indexed yet)");
            continue;
        }
        assert!(cut.len() <= 9, "{m}: {} cut takes", cut.len());
    }
    if seen == 0 {
        eprintln!("SKIP: no baked maps");
    }
}

/// Item VO's own line: a property that names the map's OWN `<map>_emt` stream
/// resolves to an OGG under `<map>/sounds/streams/`, and that OGG decodes.
/// BF1 TAT.LVL's `tat1_emt` (stream 33d74640, 9 segments, 380,384 B of ADPCM)
/// is named by two properties, neither of which is in the sound-name
/// dictionary. Note the RATE: these takes are 44.1 kHz mono one-shots of
/// 0.6-2.6 s, not the 22 kHz speech the global streams carry.
#[test]
fn a_map_s_own_stream_resolves_to_its_own_directory() {
    let Some(sl) = lib("tat1") else {
        eprintln!("SKIP: tat1 not baked");
        return;
    };
    let Some(p) = sl.take_path_by_hash(0x87c9_194b, 0) else {
        eprintln!("SKIP: tat1 has no per-map streams (bake predates item VO)");
        return;
    };
    assert!(
        p.ends_with("sounds/streams/33d74640/953cf988.ogg"),
        "{}",
        p.display()
    );
    if !audio::Audio::backend_compiled() {
        eprintln!("SKIP decode: built without the audio backend");
        return;
    }
    let data = audio::decode(&p).expect("decode the map's own take");
    println!(
        "tat1 87c9194b take 0: {} — {} frames at {} Hz",
        p.display(),
        data.frames(),
        data.sample_rate()
    );
    assert_eq!(data.sample_rate(), 44_107);
    assert_eq!(data.frames(), 50_624);
}

/// Why the map's own streams may be searched first: no id is in both indexes,
/// so the order decides nothing on this disc pair.
#[test]
fn map_streams_never_shadow_the_global_index() {
    let mut with = 0;
    let mut checked = 0;
    let Ok(rd) = std::fs::read_dir(root()) else {
        eprintln!("SKIP: no bake");
        return;
    };
    let mut dirs: Vec<String> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    dirs.sort();
    for d in dirs {
        let Some(sl) = lib(&d) else { continue };
        checked += 1;
        let global: std::collections::HashSet<u32> = sl.global_ids().collect();
        let mine: Vec<u32> = sl.map_stream_ids().collect();
        if mine.is_empty() {
            continue;
        }
        with += 1;
        let both: Vec<u32> = mine
            .iter()
            .copied()
            .filter(|i| global.contains(i))
            .collect();
        assert!(both.is_empty(), "{d}: {both:?} in both indexes");
    }
    println!("{checked} baked map dirs, {with} with their own baked streams");
    if checked == 0 {
        eprintln!("SKIP: no baked maps");
    }
}
