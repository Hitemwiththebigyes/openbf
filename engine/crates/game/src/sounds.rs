//! Runtime sound library: `sounds.ron` (bake sounds) + lazily decoded clips.
//! Properties are looked up by the fnv1a_lowercase hash of the class-prop
//! string; playback picks a weighted-random take and attenuates by the
//! property's distances and rolloff with the PS2's own level function
//! ([`attenuation`], analysis/re-sound-distance.md), panning by the
//! listener-relative lateral position with the PS2's per-ear law and bus
//! gains ([`ears_3d`], [`bus_table`]; card MIXH, analysis/re-mix-headroom.md).
//! A layered property starts each layer
//! at its own delay ([`SoundLib::start`], [`SoundLib::tick`]; card WSND,
//! analysis/re-weapon-sounds.md §2-3), and every start draws its own pitch
//! and gain and passes the property's `PlayInterval` / `PlayProbability`.
//! A vehicle's engine layers are looping voices whose gain, rate and pan
//! are set every tick ([`SoundLib::loop_voices`]; card VSND,
//! analysis/re-vehicle-sounds.md §2.3).
//!
//! A take is a WAV in the map's own bank, or — for a `soundstreamproperties`
//! (`stream`/`segments`, baked by item 15) — an OGG under
//! `audio/global_bf{1,2}/`, decoded from the game's GLOBAL/CW/GCW stream LVL:
//! almost all of the speech on both discs is there, not in the map banks
//! (`engine/artifacts/global_sound/README.md`; RE `analysis/re-sound-global.md`).
//! A map's SOUND LVL also carries streams of its OWN — the mono `<map>_emt`
//! takes and its music — baked beside its bank into `<map>/sounds/streams/`
//! (queue item VO), and those are searched FIRST.
//! The map's game also brings in its resident CORE.LVL bank
//! (`core_bf{1,2}/sounds/`, the shell/load definitions) behind [`SoundLib::global`].

use crate::classdb::fnv1a_lowercase;
use anyhow::{Context, Result};
use core::announcer::Line;
use glam::Vec3;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The RON's two structs live in the `manifest` crate — ONE definition
/// shared with `bake::sounds`, which writes the file. Before this each side
/// carried its own copy and a field added to one and forgotten on the other
/// only showed up as a sound that never played. `SoundProp` is re-exported
/// because the rest of this module (and `SoundLib`'s callers) name it.
pub use manifest::{SoundProp, SoundsRon};

/// The fields the runtime needs out of a stream index — `global.ron` under
/// `audio/global_bf{1,2}/`, or `streams.ron` under a map's own
/// `sounds/streams/`. `bake::global_sound::GlobalSoundRon` writes more (the
/// per-segment offsets a music streamer would seek with); serde ignores the
/// rest.
#[derive(Deserialize, Debug, Default)]
struct GlobalRon {
    /// Segment id → OGG path relative to that directory.
    files: Vec<(u32, String)>,
    /// The stream table, for the segments that have NO file: it says whether
    /// the disc holds that audio at all, which is the difference between a
    /// music cue the bake indexes on purpose and a name whose take is on no
    /// disc (see [`Census`]).
    #[serde(default)]
    streams: Vec<IndexStream>,
}

#[derive(Deserialize, Debug)]
struct IndexStream {
    #[serde(default)]
    name_hash: u32,
    /// The LVL the stream lives in (`nab.lvl`, `global.lvl`).
    #[serde(default)]
    source: String,
    #[serde(default)]
    channels: u32,
    #[serde(default)]
    block_size: u32,
    /// Byte offset of the stream's data in that file.
    #[serde(default)]
    data_offset: u64,
    /// False for the stereo music streams: indexed, never decoded (the maintainer's
    /// decision — music stays streamed).
    baked: bool,
    segments: Vec<IndexSegment>,
}

#[derive(Deserialize, Debug)]
struct IndexSegment {
    id: u32,
    #[serde(default)]
    frequency: u32,
    /// Offset inside the stream's data.
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    size: usize,
}

/// A `Music` record ready to play (card AMBM): its times, and each take of
/// its stream property as the place of its ADPCM on the disc.
#[derive(Debug, Clone)]
pub struct MusicTrack {
    pub def: core::music::MusicDef,
    /// The stream property's `Gain`.
    pub gain: f32,
    /// One per `def.takes` entry, in the property's list order.
    pub takes: Vec<audio::StreamTake>,
    /// The takes' segment ids, for the log.
    pub ids: Vec<u32>,
}

/// A level 0..1 in decibels for the mixer, floored at -60 dB.
pub fn level_db(level: f32) -> f32 {
    20.0 * level.max(0.001).log10()
}

/// The takes of every stream the bake did not decode, by (stream, segment):
/// each streams from the copy of its ADPCM the bake wrote under the index's
/// own directory `dir` ([`manifest::music_body`]).
fn note_music(g: &GlobalRon, dir: &Path, out: &mut HashMap<(u32, u32), audio::StreamTake>) {
    for s in g.streams.iter().filter(|s| !s.baked && s.block_size > 0) {
        let path = dir.join(manifest::music_body(&s.source, s.data_offset));
        for seg in &s.segments {
            out.entry((s.name_hash, seg.id))
                .or_insert_with(|| audio::StreamTake {
                    path: path.clone(),
                    offset: seg.offset,
                    size: seg.size,
                    channels: s.channels,
                    frequency: seg.frequency,
                    block_size: s.block_size as usize,
                });
        }
    }
}

/// A definition that can actually make a noise: it names bank samples, stream
/// segments, or layers that do.
fn audible(p: &SoundProp) -> bool {
    !p.samples.is_empty() || !p.layers.is_empty() || !p.segments.is_empty()
}

/// Why a definition that names stream segments makes no sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Silence {
    /// Its takes are music segments: indexed, never decoded (the maintainer's decision).
    Music,
    /// The index knows the segment; no OGG was written for it.
    Unbaked,
    /// No stream on the disc carries the take — a cut line.
    Cut,
}

/// What [`SoundLib::census`] counts, over the map bank's distinct properties.
/// The last three partition the definitions that name stream segments and
/// resolve to no file, and they are what queue item VO measured:
/// `music` is by design (the maintainer's decision — the stereo music streams are
/// indexed, never decoded), `unbaked` is a segment the index knows and the
/// bake skipped, and `cut` names a take that is in NO stream on the disc.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Census {
    /// Distinct properties in the map's own bank.
    pub props: usize,
    /// …resolving through a bank WAV.
    pub wav: usize,
    /// …resolving through a WAV or a streamed segment's OGG.
    pub any: usize,
    /// Silent: every take is a music segment, which stays streamed.
    pub music: usize,
    /// Silent: the index knows the segment but no OGG was written for it.
    pub unbaked: usize,
    /// Silent: no stream on the disc carries the take at all (cut lines).
    pub cut: usize,
}

pub struct Listener {
    pub pos: Vec3,
    /// Engine yaw (0 = −Z, positive left).
    pub yaw: f32,
}

/// What a definition plays, for the referees: (stream, segment ids, sample
/// ids, layers).
pub type Audio = (Option<u32>, Vec<u32>, Vec<u32>, Vec<u32>);

/// One baked `sounds/` directory — the map's bank, or the resident CORE one.
struct Bank {
    dir: PathBuf,
    /// One definition per name over the whole LVL ([`merge_props`]): what a
    /// name the battle's own sub-levels do not define falls through to.
    props: HashMap<u32, SoundProp>,
    /// One definition per (sub-level, name): [`merge_props`] over each
    /// sub-level's own records.
    by_level: HashMap<(u32, u32), SoundProp>,
    sample_paths: HashMap<u32, String>,
    /// The `soundparameterized` records by name: one per sub-level that
    /// defines it, in file order.
    engines: HashMap<u32, Vec<manifest::EngineSound>>,
    /// The `Music` records by name.
    music: HashMap<u32, manifest::MusicRon>,
}

/// One property per name hash, from records in chunk order: the last
/// definition that can make a noise.
///
/// A planet's SOUND LVL is one `lvl_` sub-level per map and era (BF1
/// KAS.LVL: `kas1gcw`, `kas1cw`, `kas2gcw`, `kas2cw`) and the PS2 loads the
/// ONE its mission script names (`ReadDataFile("sound\\kas.lvl;kas2gcw")`),
/// so a battle's definition of a name is its own sub-level's
/// ([`SoundLib::find`], card SNDE). The disc: the commander's post lines
/// carry the same names in every sub-level, with the `gcw_vo` stream's
/// segments in the gcw ones and `cw_vo`'s in the cw ones; the RAM of 160 of
/// 160 BF1 battle save states holds one era's names beside that era's stream
/// only (`engine/artifacts/lead_tmp/audit-annc2/`: `sublevel_defs_kas.txt`,
/// `ram_defs_all_bf1.txt`). Before the card this merge ran over the whole
/// LVL and every battle heard its last sub-level's definitions
/// (`<planet>2cw`): 256 names over 16 of the 28 sub-levels BF1's missions
/// load, 201 of them streamed voice.
///
/// The merge is still used twice. Over ONE sub-level's records, for a name
/// that sub-level defines more than once (PROVISIONAL, ours: which of two
/// definitions in one sub-level the PS2 keeps is not read). And over the
/// whole LVL, for a name none of the battle's sub-levels defines - an army
/// the mission does not field (the port's era mix, card ANNC2), which the
/// PS2 never has to answer.
/// Counting SEGMENTS as audible cannot change which duplicate wins on
/// this bake: measured over all 34 baked banks, no hash pairs a
/// sample/layer definition with a segment-only one (0 of 34).
fn merge_props(records: Vec<SoundProp>) -> HashMap<u32, SoundProp> {
    let mut props: HashMap<u32, SoundProp> = HashMap::new();
    for p in records {
        let keep = match props.get(&p.name_hash) {
            Some(prev) => audible(&p) || !audible(prev),
            None => true,
        };
        if keep {
            props.insert(p.name_hash, p);
        }
    }
    props
}

/// [`merge_props`] over each sub-level's own records, by (sub-level, name).
fn by_level(records: &[SoundProp]) -> HashMap<(u32, u32), SoundProp> {
    let mut levels: HashMap<u32, Vec<SoundProp>> = HashMap::new();
    for p in records.iter().filter(|p| p.level != 0) {
        levels.entry(p.level).or_default().push(p.clone());
    }
    let mut out = HashMap::new();
    for (level, records) in levels {
        out.extend(
            merge_props(records)
                .into_iter()
                .map(|(name, p)| ((level, name), p)),
        );
    }
    out
}

impl Bank {
    fn load(dir: &Path) -> Option<Bank> {
        let txt = pak::read_to_string(dir.join("sounds.ron")).ok()?;
        let ron: SoundsRon = ron::from_str(&txt)
            .map_err(|e| eprintln!("sound bank {} unreadable: {e}", dir.display()))
            .ok()?;
        let mut engines: HashMap<u32, Vec<manifest::EngineSound>> = HashMap::new();
        for e in ron.engines.into_iter().filter(|e| !e.layers.is_empty()) {
            engines.entry(e.name_hash).or_default().push(e);
        }
        let mut music = HashMap::new();
        for m in ron.music {
            music.entry(m.name_hash).or_insert(m);
        }
        Some(Bank {
            dir: dir.to_path_buf(),
            engines,
            music,
            by_level: by_level(&ron.props),
            props: merge_props(ron.props),
            sample_paths: ron.samples.into_iter().collect(),
        })
    }

    fn sample_path(&self, id: u32) -> Option<PathBuf> {
        self.sample_paths.get(&id).map(|rel| self.dir.join(rel))
    }
}

/// A start the library decided on: the take and what the mixer is told.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    /// The property that starts (a layered record's layer, not the record).
    pub hash: u32,
    pub path: PathBuf,
    pub gain: f32,
    pub pitch: f32,
    pub pos: Vec3,
    /// The final gain of the bus the property plays on ([`bus_table`]).
    pub bus: f32,
    /// A 2-D property: full gain in both ears, wherever it starts.
    pub flat: bool,
}

impl Start {
    /// What each ear gets (left, right) for `listener`: the gain x the bus,
    /// split by [`ears_3d`] unless the property is 2-D.
    pub fn ears(&self, listener: &Listener) -> (f32, f32) {
        let a = self.gain * self.bus;
        if self.flat {
            (a, a)
        } else {
            ears_3d(a, side(listener, self.pos))
        }
    }
}

/// A one-shot the mixer was handed, for the playtest black box
/// (`crate::blackbox`, card SNDB): kept only while [`SoundLib::handed`] is on.
#[derive(Debug, Clone, PartialEq)]
pub struct Handed {
    /// The property (a 2-D line's too).
    pub hash: u32,
    pub path: PathBuf,
    /// The linear gain: the property's x the level.
    pub gain: f32,
    /// What each ear was handed (left, right): the gain x the bus, split by
    /// [`ears_3d`] for a 3-D sound.
    pub ears: (f32, f32),
    /// The playback rate it was told.
    pub pitch: f32,
    /// Metres from the listener; 0 for a 2-D line.
    pub dist: f32,
    /// The take's length at that rate, seconds.
    pub secs: f32,
}

/// A unit's cry on its voice (card UCRY): sounding until `ends` on the
/// library's clock; `shot` cuts it in the mixer (None headless).
struct Cry {
    ends: f32,
    shot: Option<audio::Shot>,
}

/// A layer waiting for its delay (B1 `FUN_0045bc10` / `FUN_0045be28`).
struct Timer {
    sound: u32,
    /// Where the record was started: the layer plays there.
    pos: Vec3,
    /// Seconds after arming at which it starts.
    fire: f32,
    elapsed: f32,
    /// What a full pool keeps the timer by.
    value: f32,
}

/// An emitter's voice ([`SoundLib::ambient_voices`]).
enum AmbientVoice {
    /// A looping bank sample or baked take; `None` when it has none.
    Loop(Option<audio::Voice>),
    /// A take streamed from the disc (the stereo ambience streams).
    Stream(Option<audio::MusicVoice>),
    /// A one-shot asked again when its take has ended, at [`SoundLib`]'s
    /// clock.
    Shot { until: f32 },
}

/// An emitter's take ([`SoundLib::ambient_take`]).
#[derive(Debug, Clone)]
pub enum AmbientTake {
    Stream(audio::StreamTake),
    File(PathBuf),
}

/// What an emitter sounds like at the listener ([`SoundLib::ambient_heard`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientHeard {
    /// Index into the emitter list.
    pub index: usize,
    pub dist: f32,
    /// The PS2's level by distance with the emitter's distances (0..1).
    pub level: f32,
    /// The property's gain x the level x its bus.
    pub gain: f32,
}

/// The PS2's pool of waiting layers (B1 0x751e40, B2 0x8380a0: 0x4e timers).
pub const LAYER_TIMERS: usize = 78;

/// Why [`SoundLib::ask_line`] queued no line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// No such property, or it names no take.
    Unknown,
    /// The take drawn is inside its own lock (BF1: the request is silent).
    TakeLocked,
    /// The property is inside its interval.
    Interval,
    /// Its probability roll refused it (the interval is spent).
    Probability,
    /// The speaker is beyond the property's mute distance.
    Far,
}

/// A unit line's queue key stays under a commander line's
/// (`core::announcer::COMMANDER_KEY`): on the PS2 a commander line is never
/// queued behind chatter (re-announcer.md 1.5).
pub const CHATTER_KEY_MAX: f32 = 0.999;

pub struct SoundLib {
    /// Layers waiting for their delay.
    timers: Vec<Timer>,
    /// Seconds of [`SoundLib::tick`] so far: the `PlayInterval` clock.
    now: f32,
    /// Property -> the time its `PlayInterval` lets it start again.
    next_ok: HashMap<u32, f32>,
    /// (property, take) -> the time the take's own lock ends (card UCALL).
    take_ok: HashMap<(u32, usize), f32>,
    map: Bank,
    /// The battle's sound sub-levels by name hash, best first
    /// (`crate::foley::sublevels`): the one the mission's script loads, then
    /// the map's others. [`SoundLib::find`] takes a name's definition from
    /// the first that has one.
    levels: Vec<u32>,
    /// The battle's armies of another era than its mission's sub-levels
    /// (`crate::foley::foreign_armies`): a name with the prefix is looked up
    /// in that sub-level first ([`SoundLib::find_named`]).
    foreign: Vec<(String, u32)>,
    /// The map game's resident CORE.LVL bank, when it is baked.
    core: Option<Bank>,
    /// `<assets>/audio/global_bf{1,2}` and its segment id → OGG index; empty
    /// when the global VO is not baked (the map still plays its own WAVs).
    global_dir: PathBuf,
    global_files: HashMap<u32, String>,
    /// `<map dir>/sounds/streams` and the same index for the map's OWN
    /// streams (queue item VO); empty on a bake made before that item.
    map_streams_dir: PathBuf,
    map_stream_files: HashMap<u32, String>,
    /// Segment ids either index KNOWS but holds no OGG for → whether the
    /// stream they live in is one the bake never decodes (the stereo music).
    /// Only [`SoundLib::census`] reads it; it is what separates "streamed
    /// music, by design" from "this take is on no disc".
    indexed: HashMap<u32, bool>,
    /// The streamed music's takes on the disc, by (stream, segment id).
    music_takes: HashMap<(u32, u32), audio::StreamTake>,
    /// Decoded clips, keyed by PATH: a bank sample id and a segment id are
    /// hashes of different things and may collide.
    cache: HashMap<PathBuf, audio::SoundData>,
    /// The engine layers playing, by (vehicle, layer); `None` for a layer
    /// with no take to loop.
    loops: HashMap<(u32, u8), Option<audio::Voice>>,
    /// The world's emitters sounding, by index into
    /// `crate::ambience::Ambience::emitters` ([`SoundLib::ambient_voices`]).
    ambient: HashMap<usize, AmbientVoice>,
    /// Each unit's ONE voice for its hurt and death cries, by unit slot
    /// (card UCRY, [`SoundLib::cry`]).
    cries: HashMap<u32, Cry>,
    /// Each station's ambient handle: sounding until this time on the
    /// library's clock, by station index (card PWST, [`SoundLib::station_ambient`]).
    station_voices: HashMap<u16, f32>,
    /// Weighted-pick state (audio-only randomness; never touches the sim).
    seed: u32,
    pub played: u64,
    /// Dev: every one-shot handed to the mixer since the list was last
    /// drained (the black box's sound rows); `None` keeps none.
    pub handed: Option<Vec<Handed>>,
    /// Pitch multiplier for every positional sound (PS2 `slow` cheat: 0.3).
    pub rate_mul: f32,
    /// The map's game's bus gains ([`bus_table`]).
    buses: Vec<(u32, f32)>,
}

/// The 3-D listener's rolloff factor: the level function multiplies a
/// property's `rolloff` by it. 10.0 from the listener's constructor (B1
/// `FUN_00449f78`, B2 `FUN_0053da70`) and in the RAM of every one of 329
/// reference states; no writer found (analysis/re-sound-distance.md `[O]`).
pub const LISTENER_ROLLOFF: f32 = 10.0;

/// A 3-D sound's level 0..1 at `dist` metres from the listener: the PS2's
/// function (B1 `FUN_0044cac8`, B2 `FUN_005407e8`, the same on both; `[O]`
/// analysis/re-sound-distance.md). Full between the roll-in and the minimum
/// distance; beyond the minimum `(R - x) / (x k + R)` with `x` the distance
/// past it, `R = max - min` and `k = 10 rolloff` - about `R / (k x)` over
/// most of the range, reaching 0 at the maximum; silent from the mute
/// distance out. Inside the roll-in distance it falls towards the listener
/// the same way. A 2-D (`flat`) property is not attenuated at all.
pub fn attenuation(dist: f32, p: &SoundProp) -> f32 {
    if p.flat {
        return 1.0;
    }
    let k = LISTENER_ROLLOFF * p.rolloff;
    let level = |x: f32, r: f32| {
        let den = x * k + r;
        if den > 0.0 {
            ((r - x) / den).max(0.0)
        } else {
            0.0
        }
    };
    match p.roll_in_distance {
        Some(rin) if dist < rin => level(rin - dist, rin),
        _ if dist <= p.min_distance => 1.0,
        _ if dist >= p.mute_distance.unwrap_or(p.max_distance) => 0.0,
        _ => level(dist - p.min_distance, p.max_distance - p.min_distance),
    }
}

/// Where a sound at `pos` sits for `listener`: the direction to it dotted
/// with the listener's right, -1 (left) .. 1 (right); 0 at the listener.
pub fn side(listener: &Listener, pos: Vec3) -> f32 {
    let (sy, cy) = listener.yaw.sin_cos();
    let right = Vec3::new(cy, 0.0, -sy);
    let d = pos - listener.pos;
    let dist = d.length();
    if dist < 1e-3 {
        return 0.0;
    }
    (d.dot(right) / dist).clamp(-1.0, 1.0)
}

/// The PS2's per-ear split of a 3-D voice of amplitude `a` (gain x level x
/// bus) at side `s` ([`side`]): p = 0.5 + 0.5 s, left = a (1 - p), right =
/// a p - linear, so the ears sum to `a` and a centred sound is half `a` in
/// each (output mode 3, every reference state of both games; B1
/// `FUN_0044cb78` 510409-510411, B2 `FUN_00540b98`; `[O]`
/// analysis/re-mix-headroom.md 1.1 claim 4). A 2-D sound is `(a, a)`.
pub fn ears_3d(a: f32, s: f32) -> (f32, f32) {
    let p = 0.5 + 0.5 * s.clamp(-1.0, 1.0);
    (a * (1.0 - p), a * p)
}

/// What the mixer is told for a mono take that must reach the ears at
/// `(left, right)`: (volume dB, pan -1..1). kira pans by L x sqrt(1 - m)
/// sqrt 2, R x sqrt(m) sqrt 2 with m = (pan + 1) / 2, so amplitude
/// sqrt((l^2 + r^2) / 2) at m = r^2 / (l^2 + r^2) lands on (l, r) exactly.
pub fn mixer_pair(ears: (f32, f32)) -> (f32, f32) {
    let (l, r) = ears;
    let e = l * l + r * r;
    if e <= 0.0 {
        return (level_db(0.0), 0.0);
    }
    let pan = 2.0 * r * r / e - 1.0;
    (level_db((e * 0.5).sqrt()), pan.clamp(-1.0, 1.0))
}

/// The final gain of each mixer bus a property may name, by name hash, for
/// the map's game (`source_game` `bf1` / `bf2`): the bus's disc gain x its
/// parents' x the option slider over it (`[O]` analysis/re-mix-headroom.md
/// 1.4 claims 10-12; the effects voice x 0.63 on BF2 is the RAM's
/// `+0x44 = +0xf4 x bus +0x28`). The sliders are the ones every reference
/// state holds (BF1 effects 10 voice 8, BF2 effects 7 voice 9 chatter 10):
/// PROVISIONAL [H] as the factory defaults (FEEL_NOTES). A bus not listed
/// (`root`, the music's) plays at 1.0. The `voiceover` duck of `main` is
/// not here (card VODK).
pub fn bus_table(game: &str) -> Vec<(u32, f32)> {
    let rows: &[(&str, f32)] = match game {
        "bf2" => &[
            ("soundfx", 0.63),
            ("battlechatter", 0.9),
            ("voiceover", 0.63),
            ("ambience", 0.378),
        ],
        _ => &[("soundfx", 1.0), ("voiceover", 0.64), ("ambience", 0.6)],
    };
    let mut t: Vec<(u32, f32)> = rows.iter().map(|(n, g)| (fnv1a_lowercase(n), *g)).collect();
    // The unnamed bus under `root`: 0.4 x the effects slider (1.4 claim 10).
    t.push((UNNAMED_BUS, if game == "bf2" { 0.4 * 0.7 } else { 0.4 }));
    t
}

/// The bus 0x82c7f467 (its name is not on the discs' candidate lists).
const UNNAMED_BUS: u32 = 0x82c7_f467;

fn read_index(path: &Path) -> GlobalRon {
    // A missing index is normal (an unbaked group) and stays silent; one that
    // is there and will not parse is a bake defect that would otherwise
    // surface only as voice lines that never play.
    let Ok(txt) = pak::read_to_string(path) else {
        return GlobalRon::default();
    };
    ron::from_str::<GlobalRon>(&txt).unwrap_or_else(|e| {
        eprintln!("sound index {} unreadable: {e}", path.display());
        GlobalRon::default()
    })
}

/// Record every segment id the index KNOWS: `true` when its stream is one the
/// bake never decodes (music). A file, when there is one, always wins — this
/// map is only consulted for segments that resolved to nothing.
fn note_indexed(g: &GlobalRon, out: &mut HashMap<u32, bool>) {
    for s in &g.streams {
        for seg in &s.segments {
            let e = out.entry(seg.id).or_insert(!s.baked);
            *e &= !s.baked;
        }
    }
}

impl SoundLib {
    /// [`SoundLib::load`] for a battle of `mission` (card SNDE): the
    /// sub-level its script loads first.
    pub fn load_for(map_dir: &Path, mission: &crate::mission::MissionDef) -> Option<SoundLib> {
        let mut lib = SoundLib::load(map_dir)?;
        let f = mission.factions();
        lib.for_battle(
            map_dir,
            [f[0].as_str(), f[1].as_str()],
            &mission.sound_levels(),
        );
        Some(lib)
    }

    /// The battle's sound sub-levels, best first (`crate::foley::sublevels`).
    pub fn set_levels(&mut self, levels: Vec<u32>) {
        self.levels = levels;
    }

    /// The library of a battle of `factions` on the map at `map_dir` whose
    /// mission loads the sub-levels `named` (card SNDE): those first, and
    /// an army of the other era by its own era's sub-level.
    pub fn for_battle(&mut self, map_dir: &Path, factions: [&str; 2], named: &[String]) {
        self.levels = crate::foley::sublevels(map_dir, factions, named);
        self.foreign = crate::foley::foreign_armies(map_dir, factions, named);
    }

    /// [`SoundLib::for_battle`] for `walk`'s teams and mission.
    pub fn for_walk(&mut self, map_dir: &Path, walk: &crate::walk::WalkSim) {
        let f = [0, 1].map(|t| walk.teams[t].faction.as_str());
        self.for_battle(map_dir, f, &walk.sound_levels);
    }

    /// Decode every take the battle can play (card HITCH): each sample of
    /// the map's and the core bank, and each segment a property of either
    /// names, into the cache - at the battle's load, so no first shot,
    /// footstep or line decodes a file inside a frame. Returns the takes,
    /// their decoded bytes and the ms it took.
    pub fn preload(&mut self) -> (usize, u64, f32) {
        let t0 = std::time::Instant::now();
        let mut paths: Vec<PathBuf> = Vec::new();
        for bank in std::iter::once(&self.map).chain(self.core.as_ref()) {
            paths.extend(bank.sample_paths.values().map(|rel| bank.dir.join(rel)));
            let props = bank.props.values().chain(bank.by_level.values());
            let segs: Vec<u32> = props.flat_map(|p| p.segments.iter().map(|s| s.0)).collect();
            paths.extend(segs.into_iter().filter_map(|id| self.segment_path(id)));
        }
        paths.sort();
        paths.dedup();
        let (mut n, mut bytes) = (0, 0u64);
        for path in paths {
            if self.cache.contains_key(&path) {
                continue;
            }
            // A take that will not decode stays out, as at its first play.
            if let Ok(data) = audio::decode(&path) {
                n += 1;
                bytes += data.frames() as u64 * 8;
                self.cache.insert(path, data);
            }
        }
        (n, bytes, t0.elapsed().as_secs_f32() * 1000.0)
    }

    /// `<map dir>/sounds/sounds.ron`, or None when the map has no sounds baked.
    /// Without a battle the sub-levels are the map's own, as
    /// `crate::foley::sublevels` orders them for no faction.
    ///
    /// The map dir's PARENT is the baked root, so the same call also picks up
    /// the streamed VO index and the resident CORE bank of the map's source
    /// game (`manifest.ron`'s `source_game`: BF1 maps and tat1 → `bf1`, BF2
    /// maps and tat2 → `bf2`). A map dir with no readable manifest gets its own
    /// bank only — there is nothing that says which disc's speech it would use.
    pub fn load(map_dir: &Path) -> Option<SoundLib> {
        let map = Bank::load(&map_dir.join("sounds"))?;
        let game = pak::read_to_string(map_dir.join("manifest.ron"))
            .ok()
            .and_then(|txt| ron::from_str::<manifest::ManifestHead>(&txt).ok())
            .map(|h| h.source_game);
        let buses = bus_table(game.as_deref().unwrap_or(""));
        let root = map_dir.parent();
        let mut indexed: HashMap<u32, bool> = HashMap::new();
        let mut music_takes = HashMap::new();
        let (core, global_dir, global_files) = match (root, game) {
            (Some(root), Some(game)) => {
                let core = Bank::load(&root.join(format!("core_{game}")).join("sounds"));
                let dir = root.join("audio").join(format!("global_{game}"));
                let g = read_index(&dir.join("global.ron"));
                note_indexed(&g, &mut indexed);
                note_music(&g, &dir, &mut music_takes);
                (core, dir, g.files.into_iter().collect())
            }
            _ => (None, PathBuf::new(), HashMap::new()),
        };
        // The map's own streams, baked beside its bank (item VO).
        let map_streams_dir = map_dir.join("sounds").join("streams");
        let m = read_index(&map_streams_dir.join("streams.ron"));
        note_indexed(&m, &mut indexed);
        note_music(&m, &map_streams_dir, &mut music_takes);
        Some(SoundLib {
            map,
            levels: crate::foley::sublevels(map_dir, ["", ""], &[]),
            foreign: Vec::new(),
            core,
            global_dir,
            global_files,
            map_streams_dir,
            map_stream_files: m.files.into_iter().collect(),
            timers: Vec::new(),
            now: 0.0,
            next_ok: HashMap::new(),
            take_ok: HashMap::new(),
            indexed,
            music_takes,
            cache: HashMap::new(),
            loops: HashMap::new(),
            ambient: HashMap::new(),
            cries: HashMap::new(),
            station_voices: HashMap::new(),
            seed: 0x1234_5678,
            played: 0,
            handed: None,
            rate_mul: 1.0,
            buses,
        })
    }

    /// The map bank's definition of a name: the first of the battle's
    /// sub-levels that defines it (the PS2 holds the one its mission loads,
    /// [`merge_props`]), else the whole LVL's.
    fn map_prop(&self, hash: u32) -> Option<&SoundProp> {
        let own = |l: &u32| self.map.by_level.get(&(*l, hash));
        self.levels
            .iter()
            .find_map(own)
            .or_else(|| self.map.props.get(&hash))
    }

    /// The bake keeps each record's sub-level (false before card SNDE).
    pub fn has_levels(&self) -> bool {
        !self.map.by_level.is_empty()
    }

    /// Every name the map's bank defines in any sub-level, for the referees.
    pub fn map_names(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.props.keys().copied()
    }

    /// The sub-level whose definition of `name` this battle plays (0 = the
    /// resident CORE bank's or a bake without sub-levels); `None` for a name
    /// the library does not have. For the referees.
    pub fn level_of(&self, name: &str) -> Option<u32> {
        self.find_named(name).map(|(p, _)| p.level)
    }

    /// [`SoundLib::audio_of`] by name ([`SoundLib::find_named`]).
    pub fn audio_of_name(&self, name: &str) -> Option<Audio> {
        let (p, _) = self.find_named(name)?;
        let ids = |v: &[(u32, f32)]| v.iter().map(|(id, _)| *id).collect();
        Some((p.stream, ids(&p.segments), ids(&p.samples), p.layers))
    }

    /// The sub-level `level` of the map's bank defines `name`.
    pub fn defines(&self, level: &str, name: &str) -> bool {
        let key = (fnv1a_lowercase(level), fnv1a_lowercase(name));
        self.map.by_level.contains_key(&key)
    }

    /// [`SoundLib::find`] by name: a line of an army the mission's
    /// sub-levels are not of the era of is its own era's sub-level's when
    /// the map has one (`crate::foley::foreign_armies`).
    fn find_named(&self, name: &str) -> Option<(SoundProp, bool)> {
        let hash = fnv1a_lowercase(name);
        let lower = name.to_ascii_lowercase();
        let own = self
            .foreign
            .iter()
            .filter(|(prefix, _)| lower.starts_with(prefix.as_str()))
            .find_map(|(_, level)| self.map.by_level.get(&(*level, hash)));
        match own {
            Some(p) => Some((p.clone(), false)),
            None => self.find(hash),
        }
    }

    /// What a name's definition plays, for the referees: (stream, segment
    /// ids, sample ids, layers).
    pub fn audio_of(&self, hash: u32) -> Option<Audio> {
        let (p, _) = self.find(hash)?;
        let ids = |v: &[(u32, f32)]| v.iter().map(|(id, _)| *id).collect();
        Some((p.stream, ids(&p.segments), ids(&p.samples), p.layers))
    }

    /// A property this library can look up: the map's bank first
    /// ([`SoundLib::map_prop`]), then the resident CORE bank. The bool says
    /// which bank held it — a sample id is only meaningful in its own bank.
    fn find(&self, hash: u32) -> Option<(SoundProp, bool)> {
        if let Some(p) = self.map_prop(hash) {
            return Some((p.clone(), false));
        }
        self.core
            .as_ref()
            .and_then(|c| c.props.get(&hash))
            .map(|p| (p.clone(), true))
    }

    /// The `Music` record `name` with its stream property's takes (card
    /// AMBM): None when the map's bank has no such record or property. A
    /// take the indexes do not hold is left out.
    pub fn music(&self, name: &str) -> Option<MusicTrack> {
        let rec = self.map.music.get(&fnv1a_lowercase(name))?;
        let (prop, _) = self.find(rec.stream)?;
        let stream = prop.stream?;
        let (mut takes, mut ids) = (Vec::new(), Vec::new());
        for (id, _) in &prop.segments {
            if let Some(t) = self.music_takes.get(&(stream, *id)) {
                takes.push(t.clone());
                ids.push(*id);
            }
        }
        Some(MusicTrack {
            def: core::music::MusicDef {
                name: name.to_owned(),
                fade_in: rec.fade_in,
                fade_out: rec.fade_out,
                min_playback: rec.min_playback,
                min_inactive: rec.min_inactive,
                max_playback: rec.max_playback,
                priority: rec.priority,
                looping: prop.looping,
                takes: takes.iter().map(audio::StreamTake::secs).collect(),
            },
            gain: prop.gain,
            takes,
            ids,
        })
    }

    /// A streamed property's takes in its list order and its gain (the
    /// shell's `shell_soundtrack`, card BSHLA); `None` when it names no
    /// indexed stream.
    pub fn stream_takes(&self, name: &str) -> Option<(f32, Vec<audio::StreamTake>)> {
        let (p, _) = self.find_named(name)?;
        let stream = p.stream?;
        let takes: Vec<audio::StreamTake> = p
            .segments
            .iter()
            .filter_map(|(id, _)| self.music_takes.get(&(stream, *id)).cloned())
            .collect();
        (!takes.is_empty()).then_some((p.gain, takes))
    }

    /// A `soundparameterized` record (a vehicle's `EngineSound`) by name
    /// hash: the map bank's definition in the first of `levels` (the
    /// battle's sub-levels, `crate::foley::sublevels`) that has one - NAB's
    /// fighter tank has other graphs in `nab2gcw` than in `nab1cw` - else
    /// its last one (a class the battle's own sub-levels do not define, as
    /// [`merge_props`]), else the resident CORE bank's.
    pub fn engine(&self, hash: u32, levels: &[u32]) -> Option<&manifest::EngineSound> {
        let core = self.core.as_ref().and_then(|c| c.engines.get(&hash));
        let defs = self.map.engines.get(&hash).or(core)?;
        let of = |l: &u32| defs.iter().find(|e| e.level == *l);
        levels.iter().find_map(of).or(defs.last())
    }

    /// What an engine layer's voice is told (B1 `FUN_00461220`): gain =
    /// clamp(property gain x factor, 0, 1) x the level by distance x the
    /// bus, to each ear by [`ears_3d`] (a 2-D layer: both in full), rate =
    /// max(0.1, property pitch x factor); `None` for an unknown property
    /// and from its mute distance out, where the layer is not played (the
    /// PS2's 39-voice limit is not copied).
    pub fn engine_level(
        &self,
        v: &crate::engine_sound::Voice,
        listener: &Listener,
    ) -> Option<((f32, f32), f32)> {
        let (p, _) = self.find(v.sound)?;
        let d = (v.pos - listener.pos).length();
        if !p.flat && d >= p.mute_distance.unwrap_or(p.max_distance) {
            return None;
        }
        let gain = (p.gain * v.gain_factor).clamp(0.0, 1.0) * attenuation(d, &p) * self.bus_of(&p);
        let ears = if p.flat {
            (gain, gain)
        } else {
            ears_3d(gain, side(listener, v.pos))
        };
        Some((ears, (p.pitch * v.pitch_factor).max(0.1)))
    }

    /// The final gain of the bus `p` plays on ([`bus_table`]; 1.0 for one
    /// the table does not list).
    fn bus_of(&self, p: &SoundProp) -> f32 {
        self.buses
            .iter()
            .find(|b| b.0 == p.bus)
            .map_or(1.0, |b| b.1)
    }

    /// Play this tick's engine layers: a voice per (vehicle, layer), started
    /// when the layer first plays in range, set to its gain, rate and pan
    /// over one tick, stopped when the layer is no longer listed.
    pub fn loop_voices(
        &mut self,
        audio: &mut audio::Audio,
        voices: &[crate::engine_sound::Voice],
        listener: &Listener,
    ) {
        let mut live = std::collections::HashSet::new();
        for v in voices {
            let Some((ears, rate)) = self.engine_level(v, listener) else {
                continue;
            };
            let key = (v.vehicle, v.layer);
            live.insert(key);
            let (db, pan) = mixer_pair(ears);
            let rate = f64::from((rate * self.rate_mul).max(0.05));
            if let Some(voice) = self.loops.get_mut(&key) {
                if let Some(voice) = voice {
                    voice.set(db, rate, pan, core::TICK_DT);
                }
                continue;
            }
            let path = self.find(v.sound).and_then(|(p, core)| {
                let id = self.pick_sample(&p.samples)?;
                self.bank(core)?.sample_path(id)
            });
            let voice = path
                .filter(|p| self.sound(audio, p).is_ok())
                .and_then(|p| audio.play_voice(&self.cache[&p], db, rate, pan));
            self.played += u64::from(voice.is_some());
            self.loops.insert(key, voice);
        }
        self.loops.retain(|k, _| live.contains(k));
    }

    /// Stop every engine layer's voice (the pause menu, the battle's end);
    /// the next [`SoundLib::loop_voices`] starts the live ones again.
    pub fn stop_loops(&mut self) {
        self.loops.clear();
        self.ambient.clear();
    }

    /// Stop the world's emitters (the battle's end); the next
    /// [`SoundLib::ambient_voices`] starts them again.
    pub fn stop_ambient(&mut self) {
        self.ambient.clear();
    }

    /// Each emitter's level at `ears` (none for a property this bank does
    /// not hold): the PS2's level function ([`attenuation`]) with the
    /// emitter's distances in place of the property's where the world set
    /// them (`crate::ambience::with_distances`).
    pub fn ambient_heard(&self, amb: &crate::ambience::Ambience, ears: Vec3) -> Vec<AmbientHeard> {
        let mut out = Vec::new();
        for (index, e) in amb.emitters.iter().enumerate() {
            let Some((p, _)) = self.find(e.hash) else {
                continue;
            };
            let p = crate::ambience::with_distances(e, &p);
            let dist = (e.pos - ears).length();
            let level = attenuation(dist, &p);
            out.push(AmbientHeard {
                index,
                dist,
                level,
                gain: p.gain * level * self.bus_of(&p),
            });
        }
        out
    }

    /// Play the world's emitters for `listener`, once per stepped tick
    /// (card AMBS): a looping property is a voice while the listener is
    /// inside its mute distance (the PS2 retries a looping sound out of
    /// range every update, `re-weapon-sounds.md` 3.4); a streamed one is
    /// started with the battle and kept, its level set every tick; a
    /// one-shot is asked again whenever its take has ended, paced by its own
    /// `PlayInterval` / `PlayProbability` ([`SoundLib::start`]'s gates).
    /// A stereo stream has no pan: it is told the mean of the two ears'
    /// levels the per-ear law gives (PROVISIONAL [H]: how the PS2 places a
    /// stereo stream played 3-D was not read).
    pub fn ambient_voices(
        &mut self,
        audio: &mut audio::Audio,
        amb: &crate::ambience::Ambience,
        listener: &Listener,
    ) {
        for h in self.ambient_heard(amb, listener.pos) {
            let e = &amb.emitters[h.index];
            let Some((p, core)) = self.find(e.hash) else {
                continue;
            };
            let p = crate::ambience::with_distances(e, &p);
            let ears = if p.flat {
                (h.gain, h.gain)
            } else {
                ears_3d(h.gain, side(listener, e.pos))
            };
            let (db, pan) = mixer_pair(ears);
            if let Some(take) = self.stream_take(&p) {
                let v = self.ambient.entry(h.index).or_insert_with(|| {
                    let v = audio.play_stream(take, db, p.looping);
                    AmbientVoice::Stream(v.map_err(|e| eprintln!("ambience: {e:#}")).ok())
                });
                if let AmbientVoice::Stream(Some(v)) = v {
                    v.set_volume(db, core::TICK_DT);
                }
                continue;
            }
            if !p.looping {
                let until = match self.ambient.get(&h.index) {
                    Some(AmbientVoice::Shot { until }) => *until,
                    _ => 0.0,
                };
                if self.now < until || h.level <= 0.0 {
                    continue;
                }
                let mut starts = Vec::new();
                self.start_prop_with(e.hash, e.pos, listener.pos, Some(e), &mut starts);
                self.mix(audio, &starts, listener);
                let secs = starts.first().map_or(0.0, |s| {
                    let len = self.cache.get(&s.path).map_or(0.0, |d| d.duration_secs());
                    len / (s.pitch * self.rate_mul).max(0.05)
                });
                self.ambient.insert(
                    h.index,
                    AmbientVoice::Shot {
                        until: self.now + secs,
                    },
                );
                continue;
            }
            if h.level <= 0.0 {
                self.ambient.remove(&h.index);
                continue;
            }
            let rate = f64::from((p.pitch * self.rate_mul).max(0.05));
            if let Some(AmbientVoice::Loop(v)) = self.ambient.get_mut(&h.index) {
                if let Some(v) = v {
                    v.set(db, rate, pan, core::TICK_DT);
                }
                continue;
            }
            let voice = self
                .file_take(&p, core)
                .filter(|path| self.sound(audio, path).is_ok())
                .and_then(|path| audio.play_voice(&self.cache[&path], db, rate, pan));
            self.played += u64::from(voice.is_some());
            self.ambient.insert(h.index, AmbientVoice::Loop(voice));
        }
    }

    /// The take a property streams from the disc (an unbaked stereo
    /// stream's segment, as the music), or None when its take is a file.
    fn stream_take(&self, p: &SoundProp) -> Option<audio::StreamTake> {
        let s = p.stream?;
        let id = p.segments.first()?.0;
        if self.segment_path(id).is_some() {
            return None;
        }
        self.music_takes.get(&(s, id)).cloned()
    }

    /// A file take of `p` (weighted pick): a bank sample or a baked segment.
    fn file_take(&mut self, p: &SoundProp, core: bool) -> Option<PathBuf> {
        match self.pick_sample(&p.samples) {
            Some(id) => self.bank(core).and_then(|b| b.sample_path(id)),
            None => self
                .pick_sample(&p.segments)
                .and_then(|id| self.segment_path(id)),
        }
    }

    /// What [`SoundLib::ambient_voices`] would play for emitter `e`, for the
    /// referees: a disc stream's place, or a file; None when the bank has no
    /// such property or no take of it.
    pub fn ambient_take(&mut self, e: &crate::ambience::Emitter) -> Option<AmbientTake> {
        let (p, core) = self.find(e.hash)?;
        if let Some(t) = self.stream_take(&p) {
            return Some(AmbientTake::Stream(t));
        }
        self.file_take(&p, core).map(AmbientTake::File)
    }

    /// The world's emitters sounding now (streams, loops; a one-shot
    /// between its takes too).
    pub fn ambient_playing(&self) -> usize {
        self.ambient
            .values()
            .filter(|v| match v {
                AmbientVoice::Loop(v) => v.is_some(),
                AmbientVoice::Stream(v) => v.is_some(),
                AmbientVoice::Shot { .. } => true,
            })
            .count()
    }

    pub fn has(&self, name: &str) -> bool {
        self.find_named(name).is_some()
    }

    /// Card UTALK: is `name` a line on a stream (the voice slot's), not a
    /// sample-bank sound?
    pub fn streamed(&self, name: &str) -> bool {
        self.find(fnv1a_lowercase(name))
            .is_some_and(|(p, _)| p.stream.is_some())
    }

    /// A definition from the resident CORE.LVL bank (the shell/load bank both
    /// discs keep in memory — HOLD 9's `shell_menu_accept`), by name hash.
    pub fn global(&self, id: u32) -> Option<&SoundProp> {
        self.core.as_ref().and_then(|c| c.props.get(&id))
    }

    /// The file take `nth` of a property would play — a bank WAV or a streamed
    /// VO segment's OGG — without the weighted pick's randomness. None when
    /// the property is unknown, has fewer takes, or names a clip this bake
    /// does not hold.
    pub fn take_path(&self, name: &str, nth: usize) -> Option<PathBuf> {
        let (p, core) = self.find_named(name)?;
        if !p.samples.is_empty() {
            return self.bank(core)?.sample_path(p.samples.get(nth)?.0);
        }
        self.segment_path(p.segments.get(nth)?.0)
    }

    /// [`SoundLib::take_path`] for a property whose NAME is not cracked — the
    /// per-map `<map>_emt` properties are all unnamed hashes (0 of them are in
    /// the sound-name dictionary), so the item-VO tests address them the only
    /// way the disc does.
    pub fn take_path_by_hash(&self, hash: u32, nth: usize) -> Option<PathBuf> {
        let (p, core) = self.find(hash)?;
        if !p.samples.is_empty() {
            return self.bank(core)?.sample_path(p.samples.get(nth)?.0);
        }
        self.segment_path(p.segments.get(nth)?.0)
    }

    /// How many of the MAP bank's distinct properties name a clip this bake
    /// holds, and what the rest are waiting on. Layers are not followed —
    /// this counts definitions that name their own audio.
    pub fn census(&self) -> Census {
        let mut c = Census {
            props: self.map.props.len(),
            ..Default::default()
        };
        for p in self.map.props.keys().filter_map(|h| self.map_prop(*h)) {
            let w = p
                .samples
                .iter()
                .any(|(id, _)| self.map.sample_paths.contains_key(id));
            let s = p
                .segments
                .iter()
                .any(|(id, _)| self.segment_path(*id).is_some());
            if w {
                c.wav += 1;
            }
            if w || s {
                c.any += 1;
                continue;
            }
            match self.silence(p) {
                Some(Silence::Music) => c.music += 1,
                Some(Silence::Unbaked) => c.unbaked += 1,
                Some(Silence::Cut) => c.cut += 1,
                None => {}
            }
        }
        c
    }

    /// Why a definition that names stream segments still makes no sound, or
    /// None when it names none (a layer-only or bank definition). Callers
    /// have already established that no take resolves to a file.
    fn silence(&self, p: &SoundProp) -> Option<Silence> {
        if p.segments.is_empty() {
            return None;
        }
        let known = |music: bool| {
            p.segments
                .iter()
                .any(|(id, _)| self.indexed.get(id) == Some(&music))
        };
        Some(if known(true) {
            Silence::Music
        } else if known(false) {
            Silence::Unbaked
        } else {
            Silence::Cut
        })
    }

    /// The name hashes behind one column of [`SoundLib::census`], sorted —
    /// what item VO's report lists for `Silence::Cut`.
    pub fn silent_props(&self, why: Silence) -> Vec<u32> {
        let mut out: Vec<u32> = self
            .map
            .props
            .keys()
            .filter_map(|h| self.map_prop(*h))
            .filter(|p| {
                !p.samples
                    .iter()
                    .any(|(id, _)| self.map.sample_paths.contains_key(id))
                    && !p
                        .segments
                        .iter()
                        .any(|(id, _)| self.segment_path(*id).is_some())
                    && self.silence(p) == Some(why)
            })
            .map(|p| p.name_hash)
            .collect();
        out.sort_unstable();
        out
    }

    fn bank(&self, core: bool) -> Option<&Bank> {
        if core {
            self.core.as_ref()
        } else {
            Some(&self.map)
        }
    }

    /// The OGG a stream segment was baked to. The map's OWN streams are
    /// searched FIRST: a per-map stream is the narrower scope — the PS2's
    /// mission Lua opens `sound\\<planet>.lvl`'s `<map>` / `<map>_emt`
    /// alongside `sound\\global.lvl`'s game-wide streams
    /// (re-sound-global.md §1.2), so where one id existed in both, the map's
    /// own copy is the take that map's script named. On this disc pair the
    /// order is moot and measured to be: 0 of the ids in any map's own
    /// streams is also in its game's global index (all 34 maps,
    /// `map_streams_never_shadow_the_global_index`).
    fn segment_path(&self, id: u32) -> Option<PathBuf> {
        if let Some(rel) = self.map_stream_files.get(&id) {
            return Some(self.map_streams_dir.join(rel));
        }
        self.global_files
            .get(&id)
            .map(|rel| self.global_dir.join(rel))
    }

    /// Every segment id the map's own streams hold an OGG for (item VO's
    /// census and its overlap test).
    pub fn map_stream_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.map_stream_files.keys().copied()
    }

    /// Segment ids the global index holds an OGG for.
    pub fn global_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.global_files.keys().copied()
    }

    fn next_rand(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(0x0019_660D)
            .wrapping_add(0x3C6E_F35F);
        (self.seed >> 8) as f32 / 16_777_216.0
    }

    fn pick_sample(&mut self, samples: &[(u32, f32)]) -> Option<u32> {
        let total: f32 = samples.iter().map(|s| s.1.max(0.0)).sum();
        if samples.is_empty() {
            return None;
        }
        if total <= 0.0 {
            return Some(samples[0].0);
        }
        let mut r = self.next_rand() * total;
        for (id, w) in samples {
            r -= w.max(0.0);
            if r <= 0.0 {
                return Some(*id);
            }
        }
        Some(samples[samples.len() - 1].0)
    }

    fn sound(&mut self, audio: &mut audio::Audio, path: &Path) -> Result<()> {
        if self.cache.contains_key(path) {
            return Ok(());
        }
        let _ = audio;
        let data = decode_noted(path).with_context(|| format!("decode {}", path.display()))?;
        self.cache.insert(path.to_path_buf(), data);
        Ok(())
    }

    /// The take a property would play now — the same weighted pick
    /// [`SoundLib::play`] makes, a bank sample or a streamed segment — with
    /// its gain and its length in seconds at the property's pitch. Needs no
    /// mixer (a headless box can time a line); the length is 0.0 without the
    /// audio backend. None when the property is unknown or names no clip
    /// this bake holds.
    pub fn voice_take(&mut self, name: &str) -> Option<(PathBuf, f32, f32)> {
        let (p, core) = self.find_named(name)?;
        let path = match self.pick_sample(&p.samples) {
            Some(id) => self.bank(core).and_then(|b| b.sample_path(id)),
            None => self
                .pick_sample(&p.segments)
                .and_then(|id| self.segment_path(id)),
        }?;
        if !self.cache.contains_key(&path) {
            let data = decode_noted(&path)
                .map_err(|e| eprintln!("decode {}: {e:#}", path.display()))
                .ok()?;
            self.cache.insert(path.clone(), data);
        }
        let secs = self.cache[&path].duration_secs() / p.pitch.max(0.05);
        Some((path, p.gain, secs))
    }

    /// Play a 2-D line (a commander voice-over: no position, no attenuation,
    /// centred — analysis/re-announcer.md §1.6: Mode3D 0) and return its
    /// length in seconds; None when there is no take to play. The `slow`
    /// cheat's pitch is for positional sounds and does not apply.
    pub fn play_voice(&mut self, audio: &mut audio::Audio, name: &str) -> Option<f32> {
        let (path, gain, secs) = self.voice_take(name)?;
        if gain > 0.001 {
            let (pitch, bus) = self
                .find(fnv1a_lowercase(name))
                .map_or((1.0, 1.0), |(p, _)| (p.pitch.max(0.05), self.bus_of(&p)));
            let ears = (gain * bus, gain * bus);
            let (db, pan) = mixer_pair(ears);
            audio.play_oneshot(&self.cache[&path], db, f64::from(pitch), pan);
            self.note(fnv1a_lowercase(name), &path, gain, ears, pitch, 0.0);
            self.played += 1;
        }
        Some(secs)
    }

    /// Play take `nth` of a 3-D line at `pos` (a squad answer: the take was
    /// drawn when the line was asked for, card SQACK) and return its length
    /// in seconds; None when the property has no such take in this bake.
    pub fn play_take(
        &mut self,
        audio: &mut audio::Audio,
        name: &str,
        nth: usize,
        pos: Vec3,
        listener: &Listener,
    ) -> Option<f32> {
        let hash = fnv1a_lowercase(name);
        let (p, _) = self.find(hash)?;
        let path = self.take_path_by_hash(hash, nth)?;
        self.sound(audio, &path).ok()?;
        let pitch = p.pitch.max(0.05);
        let dist = (pos - listener.pos).length();
        let gain = p.gain * attenuation(dist, &p);
        if gain > 0.001 {
            let a = gain * self.bus_of(&p);
            let ears = if p.flat {
                (a, a)
            } else {
                ears_3d(a, side(listener, pos))
            };
            let (db, pan) = mixer_pair(ears);
            audio.play_oneshot(&self.cache[&path], db, f64::from(pitch), pan);
            self.note(hash, &path, gain, ears, pitch, dist);
            self.played += 1;
        }
        Some(self.cache[&path].duration_secs() / pitch)
    }

    /// A unit's line asked for on the battle's voice (card UCALL: BF1's
    /// combat calls), gated as BF1's stream path gates it (B1 `FUN_0045ec68`
    /// 522512-522522, re-unit-chatter.md 1): ONE take is drawn by weight
    /// and a take inside its own lock silences the request; then the
    /// PROPERTY's gate - refused inside its interval, else the interval is
    /// re-armed, and only then the probability rolled ([`SoundLib::start`]'s
    /// gate: one clock per property, shared with its one-shot starts); then
    /// the level at `ears`, and a line of level 0 is not queued. The line's
    /// key is that level x its bus (a chatter template's Bias 0,
    /// re-announcer.md 1.5), kept under a commander line's. Returns the
    /// line for the voice queue, or why not.
    pub fn ask_line(&mut self, name: &str, pos: Vec3, ears: Vec3) -> Result<Line, Refused> {
        self.ask_line_with(name, pos, ears, false)
    }

    /// [`SoundLib::ask_line`]; `step` = BF2's take rule (card UVO2, B2
    /// `FUN_0055bf60` 693184-693246): a drawn take inside its lock gives way
    /// to the next one in the list, round to the first, and only when every
    /// take is locked is the request silent.
    pub fn ask_line_with(
        &mut self,
        name: &str,
        pos: Vec3,
        ears: Vec3,
        step: bool,
    ) -> Result<Line, Refused> {
        let hash = fnv1a_lowercase(name);
        let Some((p, _)) = self.find(hash) else {
            return Err(Refused::Unknown);
        };
        let list = if p.samples.is_empty() {
            &p.segments
        } else {
            &p.samples
        };
        let Some(drawn) = self.pick_index(list) else {
            return Err(Refused::Unknown);
        };
        // The take's own gate (noProb: its weight is not a chance).
        let locked = |t: usize| {
            self.take_ok
                .get(&(hash, t))
                .is_some_and(|until| self.now < *until)
        };
        let n = if step { list.len() } else { 1 };
        let Some(take) = (0..n)
            .map(|i| (drawn + i) % list.len())
            .find(|&t| !locked(t))
        else {
            return Err(Refused::TakeLocked);
        };
        if let Some(&(interval, dev)) = p.take_locks.get(take) {
            if (interval, dev) != (0.0, 0.0) {
                let until = self.now + interval + self.spread(dev);
                self.take_ok.insert((hash, take), until);
            }
        }
        if self.next_ok.get(&hash).is_some_and(|t| self.now < *t) {
            return Err(Refused::Interval);
        }
        if let Some((interval, dev)) = p.play_interval {
            let next = self.now + interval + self.spread(dev);
            self.next_ok.insert(hash, next);
        }
        if p.play_probability.is_some_and(|pr| pr < self.next_rand()) {
            return Err(Refused::Probability);
        }
        let level = attenuation((pos - ears).length(), &p);
        if level <= 0.0 {
            return Err(Refused::Far);
        }
        Ok(Line {
            name: name.to_owned(),
            key: (level * self.bus_of(&p)).min(CHATTER_KEY_MAX),
            mode_3d: true,
            take: u8::try_from(take).ok(),
            at: Some(pos),
            speaker: None,
        })
    }

    /// How long a queued line plays, in seconds, without a mixer (a
    /// headless log's voice queue): its drawn take's, else the weighted
    /// pick's ([`SoundLib::voice_take`]). None = no take in this bake.
    pub fn line_secs(&mut self, line: &Line) -> Option<f32> {
        let Some(nth) = line.take else {
            return self.voice_take(&line.name).map(|t| t.2);
        };
        let hash = fnv1a_lowercase(&line.name);
        let pitch = self.find(hash)?.0.pitch.max(0.05);
        let path = self.take_path_by_hash(hash, usize::from(nth))?;
        if !self.cache.contains_key(&path) {
            let data = decode_noted(&path)
                .map_err(|e| eprintln!("decode {}: {e:#}", path.display()))
                .ok()?;
            self.cache.insert(path.clone(), data);
        }
        Some(self.cache[&path].duration_secs() / pitch)
    }

    /// The index of a weighted draw over `list` ([`SoundLib::pick_sample`]'s
    /// draw).
    fn pick_index(&mut self, list: &[(u32, f32)]) -> Option<usize> {
        let id = self.pick_sample(list)?;
        list.iter().position(|(i, _)| *i == id)
    }

    /// Play a property by name at `pos` for `listener`. Returns true if a
    /// take was played.
    pub fn play(
        &mut self,
        audio: &mut audio::Audio,
        name: &str,
        pos: Vec3,
        listener: &Listener,
    ) -> bool {
        self.play_hash(audio, fnv1a_lowercase(name), pos, listener)
    }

    /// [`SoundLib::play`] for a property known by its hash (the foley
    /// table's slots, `crate::foley`).
    pub fn play_by_hash(
        &mut self,
        audio: &mut audio::Audio,
        hash: u32,
        pos: Vec3,
        listener: &Listener,
    ) -> bool {
        self.play_hash(audio, hash, pos, listener)
    }

    fn play_hash(
        &mut self,
        audio: &mut audio::Audio,
        hash: u32,
        pos: Vec3,
        listener: &Listener,
    ) -> bool {
        let mut starts = Vec::new();
        self.start(hash, pos, listener.pos, &mut starts);
        self.mix(audio, &starts, listener)
    }

    /// One sound update: start the waiting layers whose delay has passed,
    /// heard by `listener` now. Once per stepped tick, after its events.
    pub fn update(&mut self, audio: &mut audio::Audio, dt: f32, listener: &Listener) {
        let mut starts = Vec::new();
        self.tick(dt, listener.pos, &mut starts);
        self.mix(audio, &starts, listener);
    }

    fn mix(&mut self, audio: &mut audio::Audio, starts: &[Start], listener: &Listener) -> bool {
        let mut any = false;
        for s in starts {
            if self.sound(audio, &s.path).is_err() {
                continue;
            }
            let pitch = (s.pitch * self.rate_mul).max(0.05);
            let ears = s.ears(listener);
            let (db, pan) = mixer_pair(ears);
            audio.play_oneshot(&self.cache[&s.path], db, f64::from(pitch), pan);
            let dist = (s.pos - listener.pos).length();
            self.note(s.hash, &s.path, s.gain, ears, pitch, dist);
            self.played += 1;
            any = true;
        }
        any
    }

    /// A one-shot handed to the mixer, for [`SoundLib::handed`]. `path` is
    /// decoded (in the cache).
    fn note(&mut self, hash: u32, path: &Path, gain: f32, ears: (f32, f32), pitch: f32, dist: f32) {
        let secs = self.cache.get(path).map_or(0.0, |d| d.duration_secs()) / pitch;
        if let Some(h) = self.handed.as_mut() {
            h.push(Handed {
                hash,
                path: path.to_path_buf(),
                gain,
                ears,
                pitch,
                dist,
                secs,
            });
        }
    }

    /// A unit's hurt or death cry (card UCRY, `analysis/re-unit-chatter.md`
    /// 2.7, 3.6): ONE voice per unit (B1 entity +0xec, B2 +0x25c) - a cry
    /// still sounding on it is stopped first, whatever this one's draw says
    /// (B1 `FUN_002219d0` / `FUN_00221930` stop, then ask), then `hash`
    /// (the class's `HurtSound` / `DeathSound`; 0 for a class without one)
    /// is asked at `pos` like any one-shot: its `PlayProbability`, its range.
    /// What it starts goes to `out` and holds the unit's voice for the take's
    /// length. Returns whether a sounding cry was cut.
    pub fn cry(
        &mut self,
        unit: u32,
        hash: u32,
        pos: Vec3,
        ears: Vec3,
        out: &mut Vec<Start>,
    ) -> bool {
        let cut = self.cries.remove(&unit).is_some_and(|c| c.ends > self.now);
        if hash == 0 {
            return cut;
        }
        let from = out.len();
        self.start(hash, pos, ears, out);
        // A cry is one sample-bank property with no layers: one start at most.
        if let Some(s) = out.get(from) {
            let (path, pitch) = (s.path.clone(), s.pitch.max(0.05));
            let secs = self.decoded(&path).map_or(0.0, |d| d.duration_secs());
            let ends = self.now + secs / pitch;
            self.cries.insert(unit, Cry { ends, shot: None });
        }
        cut
    }

    /// Whether unit `unit`'s voice holds a cry now (card UCRY).
    pub fn crying(&self, unit: u32) -> bool {
        self.cries.get(&unit).is_some_and(|c| c.ends > self.now)
    }

    /// A tick's cries (card UCRY): each ask `(unit, hash, pos)` in event
    /// order through [`SoundLib::cry`] (its cut, its draw), and per unit
    /// only the LAST ask's start, if its draw let it start: a cry cut in the
    /// tick it began is never heard (a shotgun's eight pellets on one unit
    /// are one cry, not eight starts summed in the mixer).
    pub fn cries(&mut self, asks: &[(u32, u32, Vec3)], ears: Vec3) -> Vec<(u32, Start)> {
        let mut last: Vec<(u32, Option<Start>)> = Vec::new();
        for &(unit, hash, pos) in asks {
            let mut out = Vec::new();
            self.cry(unit, hash, pos, ears, &mut out);
            let start = out.pop();
            match last.iter_mut().find(|l| l.0 == unit) {
                Some(l) => l.1 = start,
                None => last.push((unit, start)),
            }
        }
        last.into_iter()
            .filter_map(|(u, s)| Some((u, s?)))
            .collect()
    }

    /// [`SoundLib::cries`] through the mixer: a cut stops the sounding take.
    pub fn play_cries(
        &mut self,
        audio: &mut audio::Audio,
        asks: &[(u32, u32, Vec3)],
        listener: &Listener,
    ) {
        for (unit, s) in self.cries(asks, listener.pos) {
            if self.sound(audio, &s.path).is_err() {
                continue;
            }
            let pitch = (s.pitch * self.rate_mul).max(0.05);
            let ears = s.ears(listener);
            let (db, pan) = mixer_pair(ears);
            let shot = audio.play_shot(&self.cache[&s.path], db, f64::from(pitch), pan);
            if let Some(c) = self.cries.get_mut(&unit) {
                c.shot = shot;
            }
            let dist = (s.pos - listener.pos).length();
            self.note(s.hash, &s.path, s.gain, ears, pitch, dist);
            self.played += 1;
        }
    }

    /// A tick's station ambient asks (card PWST, re-ambience-music §5.2):
    /// each `(station, hash, pos)` whose handle is not sounding asks its
    /// `AmbientSound` at the station like any one-shot (its
    /// `PlayProbability`, its range: B1 `FUN_002a9750` 235630, B2
    /// `FUN_003674c0` 362446 ask every update while the handle is inactive);
    /// what starts holds the handle for the take's length.
    pub fn station_ambient(&mut self, asks: &[(u16, u32, Vec3)], ears: Vec3) -> Vec<Start> {
        let mut out = Vec::new();
        for &(station, hash, pos) in asks {
            if self
                .station_voices
                .get(&station)
                .is_some_and(|t| *t > self.now)
            {
                continue;
            }
            let from = out.len();
            self.start(hash, pos, ears, &mut out);
            if let Some(s) = out.get(from) {
                let (path, pitch) = (s.path.clone(), s.pitch.max(0.05));
                let secs = self.decoded(&path).map_or(0.0, |d| d.duration_secs());
                self.station_voices.insert(station, self.now + secs / pitch);
            }
        }
        out
    }

    /// [`SoundLib::station_ambient`] through the mixer.
    pub fn play_station_ambient(
        &mut self,
        audio: &mut audio::Audio,
        asks: &[(u16, u32, Vec3)],
        listener: &Listener,
    ) {
        let starts = self.station_ambient(asks, listener.pos);
        self.mix(audio, &starts, listener);
    }

    /// A take decoded without the mixer, for its length ([`decode_noted`]).
    fn decoded(&mut self, path: &Path) -> Option<&audio::SoundData> {
        if !self.cache.contains_key(path) {
            let data = decode_noted(path).ok()?;
            self.cache.insert(path.to_path_buf(), data);
        }
        self.cache.get(path)
    }

    /// Engine layers looping now.
    pub fn loops_playing(&self) -> usize {
        self.loops.values().filter(|v| v.is_some()).count()
    }

    /// U(-1, 1) x `dev` (the PS2's `2 x (r - 0.5) x dev`, B1 `FUN_0044e288`).
    fn spread(&mut self, dev: f32) -> f32 {
        2.0 * (self.next_rand() - 0.5) * dev
    }

    /// What starting property `hash` at `pos` starts now, heard at `ears`
    /// (the mixer is not needed: a headless referee reads `out`). A layered
    /// record starts each layer of delay exactly 0 at once and arms a timer
    /// for any other, at delay + deviation x U(-1, 1), which [`SoundLib::tick`]
    /// fires at `pos` (B1 `FUN_0045ca08`, B2 `FUN_0054fa38`).
    pub fn start(&mut self, hash: u32, pos: Vec3, ears: Vec3, out: &mut Vec<Start>) {
        let Some((p, _)) = self.find(hash) else {
            return;
        };
        for (i, l) in p.layers.iter().enumerate() {
            let (delay, dev) = p.layer_delays.get(i).copied().unwrap_or((0.0, 0.0));
            if delay == 0.0 {
                self.start_prop(*l, pos, ears, out);
                continue;
            }
            let fire = delay + self.spread(dev);
            // PROVISIONAL: the PS2 ranks a waiting layer by its level x a
            // priority term (B1 `FUN_0045bec0`); this is level x priority.
            let value = self.find(*l).map_or(0.0, |(lp, _)| {
                attenuation((pos - ears).length(), &lp) * lp.priority
            });
            let timer = Timer {
                sound: *l,
                pos,
                fire,
                elapsed: 0.0,
                value,
            };
            if self.timers.len() < LAYER_TIMERS {
                self.timers.push(timer);
                continue;
            }
            // Full: the waiting layer of the lowest value below this one's
            // gives way, or this one is dropped (B1 `FUN_0045c060`).
            let low = self
                .timers
                .iter_mut()
                .filter(|t| t.value < value)
                .min_by(|a, b| a.value.total_cmp(&b.value));
            if let Some(t) = low {
                *t = timer;
            }
        }
        self.start_prop(hash, pos, ears, out);
    }

    /// One property's own take (a layer's sound is a `soundproperties`: the
    /// PS2's parser keeps no other, so a layer never nests).
    fn start_prop(&mut self, hash: u32, pos: Vec3, ears: Vec3, out: &mut Vec<Start>) {
        self.start_prop_with(hash, pos, ears, None, out);
    }

    /// [`SoundLib::start_prop`] with a world emitter's distances in place of
    /// the property's (card AMBS).
    fn start_prop_with(
        &mut self,
        hash: u32,
        pos: Vec3,
        ears: Vec3,
        emitter: Option<&crate::ambience::Emitter>,
        out: &mut Vec<Start>,
    ) {
        let Some((p, core)) = self.find(hash) else {
            return;
        };
        let p = match emitter {
            Some(e) => crate::ambience::with_distances(e, &p),
            None => p,
        };
        if p.samples.is_empty() && p.segments.is_empty() {
            return;
        }
        // The start's gates, in the PS2's order (B1 `FUN_0046bf10`): the
        // interval's clock (re-armed whatever the draw says), then the
        // probability.
        if self.next_ok.get(&hash).is_some_and(|t| self.now < *t) {
            return;
        }
        if let Some((interval, dev)) = p.play_interval {
            let next = self.now + interval + self.spread(dev);
            self.next_ok.insert(hash, next);
        }
        if p.play_probability.is_some_and(|pr| pr < self.next_rand()) {
            return;
        }
        // A bank sample when the property has one; otherwise the streamed VO
        // take. No property on either disc carries both (measured over the
        // 34 baked banks: 0 of 69,795 records name samples AND segments).
        let path = match self.pick_sample(&p.samples) {
            Some(id) => self.bank(core).and_then(|b| b.sample_path(id)),
            None => self
                .pick_sample(&p.segments)
                .and_then(|id| self.segment_path(id)),
        };
        let Some(path) = path else {
            return;
        };
        // Refused at level 0 (B1 `FUN_004599e0` 518974).
        let level = attenuation((pos - ears).length(), &p);
        if level <= 0.0 {
            return;
        }
        // Each start's own pitch and gain (B1 `FUN_0045ac00` 519742-519746).
        let pitch = p.pitch + p.pitch_dev.map_or(0.0, |d| self.spread(d));
        let gain = match p.gain_dev {
            Some(d) => (p.gain + self.spread(d)).clamp(0.0, 1.0),
            None => p.gain,
        };
        if gain * level <= 0.001 {
            return;
        }
        out.push(Start {
            hash,
            path,
            gain: gain * level,
            pitch,
            pos,
            bus: self.bus_of(&p),
            flat: p.flat,
        });
    }

    /// Advance the waiting layers by `dt`: one whose time has passed starts
    /// (its level taken now, at the position it was armed with), then the
    /// others age (B1 `FUN_0045be28`, run once per sound update).
    pub fn tick(&mut self, dt: f32, ears: Vec3, out: &mut Vec<Start>) {
        let mut due = Vec::new();
        self.timers.retain_mut(|t| {
            // The slack is f32 rounding of a summed dt (0.3 s is 9 ticks at 30 Hz).
            if t.fire <= t.elapsed + 1e-5 {
                due.push((t.sound, t.pos));
                return false;
            }
            t.elapsed += dt;
            true
        });
        for (sound, pos) in due {
            self.start_prop(sound, pos, ears, out);
        }
        self.now += dt;
    }

    /// Layers waiting for their delay.
    pub fn waiting(&self) -> usize {
        self.timers.len()
    }
}

/// Decode a take ([`audio::decode`]) and, for the frame recorder (card
/// HITCH), note it with its bytes and ms (`render::made`: a take decoded in
/// a battle frame is that frame's cost).
fn decode_noted(path: &Path) -> anyhow::Result<audio::SoundData> {
    let t0 = std::time::Instant::now();
    let data = audio::decode(path)?;
    if render::made::on() {
        let name = path.file_name().map(|n| n.to_string_lossy());
        render::made::note(
            "sound",
            name.as_deref(),
            data.frames() as u64 * 8,
            t0.elapsed().as_secs_f32() * 1000.0,
        );
    }
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_and_pan_shapes() {
        // The rifle fire's CORE template on BF1: 5 - 400 m, rolloff 6.
        let mut p = SoundProp {
            name_hash: 1,
            kind: 0,
            gain: 0.7,
            pitch: 1.0,
            min_distance: 5.0,
            max_distance: 400.0,
            rolloff: 6.0,
            priority: 0.85,
            looping: false,
            samples: vec![],
            layers: vec![],
            stream: None,
            segments: vec![],
            mute_distance: None,
            roll_in_distance: None,
            flat: false,
            layer_delays: vec![],
            pitch_dev: None,
            gain_dev: None,
            play_interval: None,
            play_probability: None,
            level: 0,
            bus: 0,
            take_locks: vec![],
        };
        assert_eq!(attenuation(2.0, &p), 1.0);
        assert_eq!(attenuation(5.0, &p), 1.0);
        // (395 - 45) / (45 * 60 + 395) and (395 - 148) / (148 * 60 + 395).
        assert!((attenuation(50.0, &p) - 350.0 / 3095.0).abs() < 1e-6);
        assert!((attenuation(153.0, &p) - 247.0 / 9275.0).abs() < 1e-6);
        assert_eq!(attenuation(400.0, &p), 0.0);
        assert_eq!(attenuation(900.0, &p), 0.0);
        // Falling all the way: a higher rolloff is quieter at every range.
        let mut last = 1.0;
        for d in 6..400 {
            let a = attenuation(d as f32, &p);
            assert!(a < last && a > 0.0, "{d} m: {a}");
            last = a;
        }
        // The mute distance cuts the tail off; the roll-in quietens the near
        // side ((50 - 25 inside) -> 25 / (25 * 60 + 50)); a 2-D sound is flat.
        p.mute_distance = Some(200.0);
        assert!(attenuation(199.0, &p) > 0.0);
        assert_eq!(attenuation(200.0, &p), 0.0);
        (p.min_distance, p.roll_in_distance) = (50.0, Some(50.0));
        assert!((attenuation(25.0, &p) - 25.0 / 1550.0).abs() < 1e-6);
        assert_eq!(attenuation(50.0, &p), 1.0);
        p.flat = true;
        assert_eq!(attenuation(900.0, &p), 1.0);
        let l = Listener {
            pos: Vec3::ZERO,
            yaw: 0.0,
        };
        assert_eq!(
            side(&l, Vec3::new(10.0, 0.0, 0.0)),
            1.0,
            "right of a -Z-facing listener"
        );
        assert_eq!(side(&l, Vec3::new(-10.0, 0.0, 0.0)), -1.0);
        assert!(side(&l, Vec3::new(0.0, 0.0, -10.0)).abs() < 1e-6);
        assert!((side(&l, Vec3::new(10.0, 0.0, -10.0)) - 0.5f32.sqrt()).abs() < 1e-6);
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }

    /// Card MIXH: the PS2's per-ear split of a 3-D voice (L + R = the
    /// amplitude, centred = half in each ear; analysis/re-mix-headroom.md
    /// 1.1 claim 4), and kira's own pan law carrying the pair exactly.
    #[test]
    fn a_3d_voice_splits_linearly_between_the_ears() {
        // kira 0.10.8 `Frame::panned` on a mono frame of 1 at that volume.
        let kira = |(db, pan): (f32, f32)| {
            let a = 10f32.powf(db / 20.0);
            let m = (pan + 1.0) * 0.5;
            let r2 = std::f32::consts::SQRT_2;
            (a * (1.0 - m).sqrt() * r2, a * m.sqrt() * r2)
        };
        for (s, want) in [
            (0.0, (0.4, 0.4)),
            (1.0, (0.0, 0.8)),
            (-1.0, (0.8, 0.0)),
            (0.5, (0.2, 0.6)),
            (-0.25, (0.5, 0.3)),
        ] {
            let got = ears_3d(0.8, s);
            assert!(close(got, want), "side {s}: {got:?}");
            let heard = kira(mixer_pair(got));
            assert!(close(heard, want), "side {s} through kira: {heard:?}");
        }
        // A 2-D pair is the take at its gain, centred.
        let (db, pan) = mixer_pair((0.5, 0.5));
        assert!((db - level_db(0.5)).abs() < 1e-4 && pan.abs() < 1e-6);
        assert!(close(kira((db, pan)), (0.5, 0.5)));
        // Silence stays silent.
        assert!(kira(mixer_pair((0.0, 0.0))).0 < 1e-2);
    }

    /// Card MIXH: what a start hands each ear - the gain x the level x its
    /// BAKED bus's gain for the map's game, split for a 3-D property, full
    /// both ears for a 2-D one. The reference is the PS2's own: BF1 Naboo's
    /// blaster, gain 0.7012 at level 0.8603 in front of the listener, read
    /// 0x134d in both ears' SPU2 registers (`lead_tmp/mixh/ps2/shot1.p2s`;
    /// register = (int)(v x 32767) >> 1, 1.1 claim 1).
    #[test]
    fn a_start_hands_each_ear_the_ps2s_level() {
        let l = Listener {
            pos: Vec3::ZERO,
            yaw: 0.0,
        };
        let bus = fnv1a_lowercase;
        let shot = SoundProp {
            gain: 0.7012 * 0.8603,
            bus: bus("soundfx"),
            ..prop("shot", 1)
        };
        let props = vec![
            shot.clone(),
            SoundProp {
                bus: bus("voiceover"),
                ..prop("line", 2)
            },
            SoundProp {
                bus: bus("voiceover"),
                flat: true,
                ..prop("line_2d", 3)
            },
            SoundProp {
                bus: bus("battlechatter"),
                ..prop("chatter", 4)
            },
            prop("no_bus", 5),
        ];
        let mut sl = lib();
        sl.map.by_level = by_level(&props);
        sl.map.props = merge_props(props);
        let start = |sl: &mut SoundLib, name: &str, pos: Vec3| {
            let mut out = Vec::new();
            sl.start(fnv1a_lowercase(name), pos, l.pos, &mut out);
            out.pop().expect(name).ears(&l)
        };
        let ahead = Vec3::new(0.0, 0.0, -6.0);
        let right = Vec3::new(6.0, 0.0, 0.0);
        let reg = |v: f32| ((v * 32767.0) as i32 & 0xffff) >> 1;
        // BF1: soundfx 1.0, voiceover 0.64.
        let (le, ri) = start(&mut sl, "shot", ahead);
        assert_eq!((reg(le), reg(ri)), (0x134d, 0x134d), "{le} {ri}");
        assert!(close((le, ri), (0.3016, 0.3016)));
        assert!(close(start(&mut sl, "shot", right), (0.0, 0.6032)));
        assert!(close(start(&mut sl, "line", ahead), (0.32, 0.32)));
        assert!(close(start(&mut sl, "line_2d", right), (0.64, 0.64)));
        assert!(close(start(&mut sl, "no_bus", ahead), (0.5, 0.5)));
        // BF2: soundfx 0.63, battlechatter 0.9, voiceover 0.63.
        sl.buses = bus_table("bf2");
        let half = 0.7012 * 0.8603 * 0.63 * 0.5;
        assert!(close(start(&mut sl, "shot", ahead), (half, half)));
        assert!(close(start(&mut sl, "chatter", right), (0.0, 0.9)));
        assert!(close(start(&mut sl, "line_2d", ahead), (0.63, 0.63)));
        // A looping 3-D voice (an engine layer) the same way.
        let voice = |pos| crate::engine_sound::Voice {
            vehicle: 0,
            layer: 0,
            record: 0,
            sound: fnv1a_lowercase("shot"),
            pos,
            gain_factor: 1.0,
            pitch_factor: 1.0,
            speed: 0.0,
        };
        let engine = |sl: &SoundLib, pos| sl.engine_level(&voice(pos), &l).expect("in range").0;
        assert!(close(engine(&sl, ahead), (half, half)));
        assert!(close(engine(&sl, -right), (2.0 * half, 0.0)));
    }

    fn prop(name: &str, sample: u32) -> SoundProp {
        SoundProp {
            name_hash: fnv1a_lowercase(name),
            kind: 0,
            gain: 1.0,
            pitch: 1.0,
            min_distance: 30.0,
            max_distance: 450.0,
            rolloff: 1.0,
            priority: 0.5,
            looping: false,
            samples: if sample == 0 {
                vec![]
            } else {
                vec![(sample, 1.0)]
            },
            layers: vec![],
            stream: None,
            segments: vec![],
            mute_distance: None,
            roll_in_distance: None,
            flat: false,
            layer_delays: vec![],
            pitch_dev: None,
            gain_dev: None,
            play_interval: None,
            play_probability: None,
            level: 0,
            bus: 0,
            take_locks: vec![],
        }
    }

    fn layered(name: &str, layers: &[(&str, f32, f32)]) -> SoundProp {
        SoundProp {
            layers: layers.iter().map(|l| fnv1a_lowercase(l.0)).collect(),
            layer_delays: layers.iter().map(|l| (l.1, l.2)).collect(),
            ..prop(name, 0)
        }
    }

    /// The disc's records the card names (BF1 NAB.LVL; `bake::sounds`'
    /// `layered_records_keep_what_the_ps2_parser_keeps` pins them there).
    fn lib() -> SoundLib {
        let props = vec![
            prop("exp_high", 1),
            prop("exp_low", 2),
            layered(
                "com_weap_inf_ordnance_exp",
                &[
                    ("exp_high", 0.0, 0.0),
                    ("exp_high", 0.1, 0.0),
                    ("exp_high", 0.2, 0.0),
                    ("exp_low", 0.0, 0.0),
                ],
            ),
            prop("near", 3),
            prop("distant", 4),
            // The far layer's own gate on the disc.
            SoundProp {
                play_interval: Some((0.5, 0.0)),
                ..prop("far", 5)
            },
            layered(
                "com_weap_obj_med_exp",
                &[
                    ("near", 0.0, 0.0),
                    ("near", 0.3, 0.1),
                    ("distant", 0.3, 0.1),
                    ("far", 0.5, 0.1),
                ],
            ),
            SoundProp {
                pitch_dev: Some(0.15),
                gain: 0.5,
                gain_dev: Some(0.2),
                ..prop("com_weap_thermaldetonator_exp", 6)
            },
            SoundProp {
                play_probability: Some(0.25),
                ..prop("rare", 7)
            },
            // Card UCALL's unit lines: a property gate, a roll that always
            // refuses, a take with its own lock.
            SoundProp {
                play_interval: Some((20.0, 0.0)),
                ..prop("call_gated", 8)
            },
            SoundProp {
                play_interval: Some((10.0, 0.0)),
                play_probability: Some(0.0),
                ..prop("call_never", 9)
            },
            SoundProp {
                take_locks: vec![(25.0, 0.0)],
                ..prop("call_locked", 10)
            },
            // Card UVO2: three takes, each locked 10 s once drawn.
            SoundProp {
                samples: vec![(7, 1.0), (8, 1.0), (9, 1.0)],
                take_locks: vec![(10.0, 0.0); 3],
                ..prop("vo_three", 0)
            },
            // Card UTALK's low-health voice: no interval, one take locked 1 s.
            SoundProp {
                take_locks: vec![(1.0, 0.0)],
                stream: Some(1),
                ..prop("low_voice", 10)
            },
        ];
        let samples = (1..=10).map(|i| (i, format!("b/{i}.wav"))).collect();
        SoundLib {
            timers: Vec::new(),
            now: 0.0,
            next_ok: HashMap::new(),
            take_ok: HashMap::new(),
            map: Bank {
                dir: PathBuf::from("sounds"),
                by_level: by_level(&props),
                props: merge_props(props),
                sample_paths: samples,
                engines: HashMap::new(),
                music: HashMap::new(),
            },
            levels: Vec::new(),
            foreign: Vec::new(),
            core: None,
            global_dir: PathBuf::new(),
            global_files: HashMap::new(),
            map_streams_dir: PathBuf::new(),
            map_stream_files: HashMap::new(),
            indexed: HashMap::new(),
            music_takes: HashMap::new(),
            cache: HashMap::new(),
            loops: HashMap::new(),
            ambient: HashMap::new(),
            cries: HashMap::new(),
            station_voices: HashMap::new(),
            seed: 0x1234_5678,
            played: 0,
            handed: None,
            rate_mul: 1.0,
            buses: bus_table("bf1"),
        }
    }

    const DT: f32 = 1.0 / 60.0;

    /// Start `name` and run the clock for `secs`: (seconds after the start,
    /// what started), the times on the tick grid.
    fn play(sl: &mut SoundLib, name: &str, secs: f32) -> Vec<(f32, Start)> {
        let mut starts = Vec::new();
        let mut out = Vec::new();
        sl.start(fnv1a_lowercase(name), Vec3::ZERO, Vec3::ZERO, &mut starts);
        out.extend(starts.drain(..).map(|s| (0.0, s)));
        for k in 0..(secs / DT).round() as u32 {
            sl.tick(DT, Vec3::ZERO, &mut starts);
            out.extend(starts.drain(..).map(|s| (k as f32 * DT, s)));
        }
        out
    }

    /// Card WSND: a layer of delay 0 starts with the record, any other at its
    /// delay (the PS2's grenade explosion: the second layer +0.100 s in 11 of
    /// 11, re-weapon-sounds.md §2.5).
    #[test]
    fn a_layered_sound_starts_each_layer_at_its_delay() {
        let mut sl = lib();
        let got = play(&mut sl, "com_weap_inf_ordnance_exp", 1.0);
        let at: Vec<(u32, &str)> = got
            .iter()
            .map(|(t, s)| ((t * 60.0).round() as u32, s.path.to_str().unwrap()))
            .collect();
        assert_eq!(
            at,
            [
                (0, "sounds/b/1.wav"),
                (0, "sounds/b/2.wav"),
                (6, "sounds/b/1.wav"),
                (12, "sounds/b/1.wav")
            ]
        );
        assert_eq!(sl.waiting(), 0);
    }

    /// A delay's deviation: delay + deviation x U(-1, 1), drawn per start
    /// (PS2 logs: 0.234 - 0.400 s for 0.3 +- 0.1, 0.467 - 0.600 for 0.5 +-
    /// 0.1).
    #[test]
    fn delayed_layers_fall_inside_their_deviation_and_cover_it() {
        let mut sl = lib();
        // near (delay 0.3), distant, far: the seen range of each.
        let mut seen = [(f32::MAX, f32::MIN); 3];
        for _ in 0..1000 {
            let got = play(&mut sl, "com_weap_obj_med_exp", 1.0);
            assert_eq!(got.len(), 4, "{got:?}");
            assert_eq!((got[0].0, got[0].1.hash), (0.0, fnv1a_lowercase("near")));
            for (t, s) in &got[1..] {
                let i = ["near", "distant", "far"]
                    .iter()
                    .position(|n| fnv1a_lowercase(n) == s.hash)
                    .unwrap();
                seen[i] = (seen[i].0.min(*t), seen[i].1.max(*t));
            }
        }
        // On the tick grid: the first update at or after the drawn time.
        for (i, (lo, hi)) in [(0.2, 0.4), (0.2, 0.4), (0.4, 0.6)].into_iter().enumerate() {
            let (a, b) = seen[i];
            assert!(a >= lo - 1e-4 && b <= hi + DT + 1e-4, "{i}: {a}..{b}");
            assert!(a < lo + 0.03 && b > hi - 0.03, "{i}: {a}..{b} covers");
        }
    }

    /// Every start draws its own pitch and gain: +- `PitchDev` / `GainDev`
    /// (the explosions' `PitchDev` 0.15).
    #[test]
    fn each_start_draws_its_pitch_and_gain() {
        let mut sl = lib();
        let (mut p, mut g) = ((f32::MAX, f32::MIN), (f32::MAX, f32::MIN));
        for _ in 0..1000 {
            let got = play(&mut sl, "com_weap_thermaldetonator_exp", 0.0);
            let s = &got[0].1;
            p = (p.0.min(s.pitch), p.1.max(s.pitch));
            g = (g.0.min(s.gain), g.1.max(s.gain));
        }
        assert!(
            p.0 >= 0.85 && p.0 < 0.86 && p.1 <= 1.15 && p.1 > 1.14,
            "{p:?}"
        );
        assert!(
            g.0 >= 0.3 && g.0 < 0.31 && g.1 <= 0.7 && g.1 > 0.69,
            "{g:?}"
        );
    }

    /// `PlayInterval` refuses a start inside the interval; `PlayProbability`
    /// takes its share of the others.
    #[test]
    fn a_unit_line_is_gated_by_its_take_then_its_property() {
        let mut sl = lib();
        let near = Vec3::new(0.0, 0.0, 40.0);
        let ask = |sl: &mut SoundLib, n: &str, ears: Vec3| sl.ask_line(n, Vec3::ZERO, ears);
        let wait = |sl: &mut SoundLib, secs: f32| sl.tick(secs, Vec3::ZERO, &mut Vec::new());
        // The property's interval: a second ask inside it is silent.
        let line = ask(&mut sl, "call_gated", near).expect("the first ask");
        assert_eq!(
            (line.take, line.at, line.mode_3d),
            (Some(0), Some(Vec3::ZERO), true)
        );
        assert_eq!(ask(&mut sl, "call_gated", near), Err(Refused::Interval));
        wait(&mut sl, 19.0);
        assert_eq!(ask(&mut sl, "call_gated", near), Err(Refused::Interval));
        wait(&mut sl, 1.5);
        assert!(
            ask(&mut sl, "call_gated", near).is_ok(),
            "the interval is over"
        );
        // A refused roll still spends the interval.
        assert_eq!(ask(&mut sl, "call_never", near), Err(Refused::Probability));
        assert_eq!(ask(&mut sl, "call_never", near), Err(Refused::Interval));
        // A take inside its own lock silences the request.
        assert!(ask(&mut sl, "call_locked", near).is_ok());
        assert_eq!(ask(&mut sl, "call_locked", near), Err(Refused::TakeLocked));
        wait(&mut sl, 25.5);
        assert!(
            ask(&mut sl, "call_locked", near).is_ok(),
            "the lock is over"
        );
        // A speaker past the mute distance is not queued - and has spent
        // the gate (the PS2 rolls it before the level).
        wait(&mut sl, 21.0);
        let far = Vec3::new(0.0, 0.0, 460.0);
        assert_eq!(ask(&mut sl, "call_gated", far), Err(Refused::Far));
        assert_eq!(ask(&mut sl, "call_gated", near), Err(Refused::Interval));
        // A line at full level stays under a commander line's key.
        wait(&mut sl, 21.0);
        let l = ask(&mut sl, "call_gated", Vec3::ZERO).expect("at the ears");
        assert!(
            l.key > 0.9 && l.key < core::announcer::COMMANDER_KEY,
            "{}",
            l.key
        );
        assert_eq!(ask(&mut sl, "no_such_line", near), Err(Refused::Unknown));
    }

    /// Card UVO2, BF2's take rule (B2 `FUN_0055bf60` 693234-693238): a
    /// drawn take inside its lock gives way to the next open one, so three
    /// asks in a row say the three takes and only the fourth is silent
    /// (BF1's one draw: `a_unit_line_is_gated_by_its_take_then_its_property`).
    #[test]
    fn a_bf2_line_steps_past_a_locked_take() {
        let mut sl = lib();
        let ask = |sl: &mut SoundLib, step: bool| {
            sl.ask_line_with("vo_three", Vec3::ZERO, Vec3::ZERO, step)
        };
        let takes: Vec<_> = (0..3)
            .map(|_| ask(&mut sl, true).expect("an open take").take)
            .collect();
        let mut seen = takes.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 3, "{takes:?}");
        assert_eq!(ask(&mut sl, true), Err(Refused::TakeLocked));
    }

    /// Card UTALK (re-unit-chatter.md 2.8): a living unit whose class's
    /// `LowHealthThreshold` is 1.1 asks its `LowHealthSound` at full health,
    /// never again while its own line waits or plays, and again once it has
    /// ended (its take's 1 s lock over); dead, its waiting line is dropped.
    /// A class with the default threshold (0.3) is silent above 30 % health
    /// and asks under it.
    #[test]
    fn a_low_health_voice_asks_again_only_when_its_line_has_ended() {
        use crate::announce::Commander;
        use core::sim::Call;
        let mut sl = lib();
        let mut w = crate::walk::WalkSim::new(1, Vec3::ZERO, None);
        let set = |w: &mut crate::walk::WalkSim, threshold: f32| {
            for t in &mut w.teams {
                for c in &mut t.classes {
                    c.cries.low_health = Some("low_voice".into());
                    c.cries.low_threshold = threshold;
                }
            }
        };
        set(&mut w, 1.1);
        let me = w.player.index;
        let mut v = Commander::new(
            core::announcer::BattleLines::default(),
            ["rep", "cis"],
            0,
            Vec::new(),
            Vec::new(),
        );
        // One tick: the asks, the gates, the queue's frame (the line is 1 s).
        let tick = |v: &mut Commander, sl: &mut SoundLib, w: &crate::walk::WalkSim| {
            v.calls(&w.sim, &w.teams);
            let queued = v
                .gate_calls(sl, Vec3::ZERO)
                .into_iter()
                .filter(|(a, r)| a.unit == me && a.call == Call::LowHealth && r.is_ok())
                .count();
            let started = v.queue.step(1.0 / 30.0, |_| Some(1.0)).is_some();
            sl.tick(1.0 / 30.0, Vec3::ZERO, &mut Vec::new());
            (queued, started)
        };
        assert_eq!(tick(&mut v, &mut sl, &w), (1, true), "asked at full health");
        // While it plays (1 s and the 0.45 s gap): no ask touches a gate.
        for k in 0..40 {
            assert_eq!(tick(&mut v, &mut sl, &w), (0, false), "tick {k}: it sounds");
        }
        let again = (0..10).find(|_| tick(&mut v, &mut sl, &w).0 == 1);
        assert!(again.is_some(), "asked again once the line has ended");
        // A waiting line of a unit that dies is dropped.
        v.queue.playing(5.0);
        let _ = tick(&mut v, &mut sl, &w);
        assert!(v.queue.holds(me));
        w.sim.units[me as usize].alive = false;
        let _ = tick(&mut v, &mut sl, &w);
        assert!(!v.queue.holds(me), "dropped at its death");
        // The default threshold: silent at full health, asked under 30 %.
        let mut sl = lib();
        let mut w = crate::walk::WalkSim::new(1, Vec3::ZERO, None);
        set(&mut w, crate::walk::LOW_HEALTH_THRESHOLD);
        let mut v = Commander::new(
            core::announcer::BattleLines::default(),
            ["rep", "cis"],
            0,
            Vec::new(),
            Vec::new(),
        );
        for _ in 0..60 {
            assert_eq!(tick(&mut v, &mut sl, &w).0, 0, "full health");
        }
        let h = &mut w.sim.units[me as usize].health;
        h.cur = 0.25 * h.max;
        assert_eq!(tick(&mut v, &mut sl, &w).0, 1, "under 30 %");
        // A sample-bank low-health sound (a saber's hum) is not the voice
        // queue's.
        for t in &mut w.teams {
            for c in &mut t.classes {
                c.cries.low_health = Some("rare".into());
            }
        }
        for _ in 0..60 {
            assert_eq!(tick(&mut v, &mut sl, &w).0, 0, "not on a stream");
        }
    }

    #[test]
    fn the_interval_and_the_probability_gate_a_start() {
        let mut sl = lib();
        // Four starts 0.25 s apart: the 0.5 s interval lets every other in.
        let n: usize = (0..4).map(|_| play(&mut sl, "far", 0.25).len()).sum();
        assert_eq!(n, 2);
        let n: usize = (0..4000).map(|_| play(&mut sl, "rare", 0.0).len()).sum();
        assert!((900..1100).contains(&n), "{n} of 4000 at 0.25");
    }

    /// The pool holds 78 waiting layers; a full pool drops a new one that
    /// outranks none.
    #[test]
    fn the_timer_pool_is_bounded() {
        let mut sl = lib();
        let mut out = Vec::new();
        for _ in 0..40 {
            let h = fnv1a_lowercase("com_weap_obj_med_exp");
            sl.start(h, Vec3::ZERO, Vec3::ZERO, &mut out);
        }
        assert_eq!(sl.waiting(), LAYER_TIMERS);
    }

    /// Card SNDE: a name's definition is the battle's own sub-level's, in
    /// the order of its sub-levels; a name they do not define falls through
    /// to the LVL's last audible one.
    #[test]
    fn a_name_resolves_in_the_battles_sub_level_first() {
        let at = |name: &str, level: u32, sample: u32| SoundProp {
            level,
            ..prop(name, sample)
        };
        let mut sl = lib();
        let records = vec![
            at("post_line", 10, 1),
            at("post_line", 20, 2),
            at("other_army", 20, 3),
            // Silent in the battle's own sub-level: the PS2 holds only that.
            at("quiet_here", 10, 0),
            at("quiet_here", 20, 4),
        ];
        sl.map.by_level = by_level(&records);
        sl.map.props = merge_props(records);
        let sample = |sl: &SoundLib, name: &str| {
            let (_, _, samples, _) = sl.audio_of(fnv1a_lowercase(name)).expect(name);
            samples
        };
        // No battle: the whole LVL's last audible definition (the old rule).
        assert_eq!(sample(&sl, "post_line"), [2]);
        sl.set_levels(vec![10]);
        assert_eq!(sample(&sl, "post_line"), [1]);
        assert_eq!(sl.level_of("post_line"), Some(10));
        assert_eq!(sample(&sl, "other_army"), [3], "falls through");
        assert_eq!(sample(&sl, "quiet_here"), [] as [u32; 0]);
        sl.set_levels(vec![30, 20, 10]);
        assert_eq!(sample(&sl, "post_line"), [2], "the first that defines it");
        // An army of the other era: its own era's sub-level, by its prefix.
        sl.set_levels(vec![10]);
        sl.foreign = vec![("post_".to_owned(), 20)];
        assert_eq!(sl.level_of("Post_line"), Some(20));
        assert_eq!(sl.level_of("quiet_here"), Some(10));
        let kas2 = Path::new("x/bf1_kas2");
        let named = ["kas2gcw".to_owned()];
        let foreign = crate::foley::foreign_armies(kas2, ["rep", "imp"], &named);
        assert_eq!(foreign, [("rep_".to_owned(), fnv1a_lowercase("kas2cw"))]);
        assert!(crate::foley::foreign_armies(kas2, ["all", "imp"], &named).is_empty());
        assert!(crate::foley::foreign_armies(kas2, ["rep", "imp"], &[]).is_empty());
    }

    /// A duplicated property whose LAST definition is silent must not silence
    /// the sound: the discs define one property per sublevel and 63 properties
    /// across the 32 baked maps pair an audible definition with an empty one.
    /// A streamed definition counts as audible — it names takes.
    #[test]
    fn duplicate_props_keep_the_audible_definition() {
        let ron = r#"(
            source: "test",
            banks: [],
            samples: [(1, "aa/bb.wav")],
            props: [
                (name_hash: 7, kind: 0, gain: 1.0, pitch: 1.0, min_distance: 5.0,
                 max_distance: 50.0, rolloff: 1.0, priority: 0.5, looping: true,
                 samples: [(1, 1.0)], layers: []),
                (name_hash: 7, kind: 0, gain: 1.0, pitch: 1.0, min_distance: 5.0,
                 max_distance: 50.0, rolloff: 1.0, priority: 0.5, looping: false,
                 samples: [], layers: []),
                (name_hash: 8, kind: 0, gain: 1.0, pitch: 1.0, min_distance: 5.0,
                 max_distance: 50.0, rolloff: 1.0, priority: 0.5, looping: false,
                 samples: [], layers: [], stream: Some(9), segments: [(3, 1.0)]),
                (name_hash: 8, kind: 0, gain: 1.0, pitch: 1.0, min_distance: 5.0,
                 max_distance: 50.0, rolloff: 1.0, priority: 0.5, looping: false,
                 samples: [], layers: []),
            ],
        )"#;
        let parsed: SoundsRon = ron::from_str(ron).expect("ron");
        // The merge `Bank::load` runs, not a copy of it.
        let props = merge_props(parsed.props);
        assert_eq!(props[&7].samples.len(), 1, "the silent duplicate won");
        assert_eq!(props[&8].segments.len(), 1, "the silent duplicate won");
        // A pre-item-15 RON (no `stream`/`segments` at all) still parses.
        assert_eq!(props[&7].stream, None);
        assert!(props[&7].segments.is_empty());
    }

    /// A 1 s silent mono WAV at `path` (a take to time a cry by).
    fn one_second_wav(path: &Path) {
        let rate = 8000u32;
        let data = vec![0u8; rate as usize * 2];
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b).unwrap();
    }

    /// Card UCRY: ONE voice per unit for its cries (B1 entity +0xec, B2
    /// +0x25c; `analysis/re-unit-chatter.md` 2.7) - a second hurt cuts the
    /// first, death cuts a sounding hurt, another unit's cry cuts nothing, a
    /// cry its draw refuses (or a class without the key) still cuts, and a
    /// finished cry leaves nothing to cut.
    #[test]
    fn a_unit_has_one_voice_for_its_cries() {
        let dir = std::env::temp_dir().join(format!("openbf-ucry-{}", std::process::id()));
        let mut l = lib();
        l.map.dir = dir.clone();
        for i in [1, 2, 7] {
            one_second_wav(&dir.join(format!("b/{i}.wav")));
        }
        let (hurt, death, rare) = (
            fnv1a_lowercase("exp_high"),
            fnv1a_lowercase("exp_low"),
            fnv1a_lowercase("rare"),
        );
        let (pos, ears) = (Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        let mut out = Vec::new();
        assert!(!l.cry(3, hurt, pos, ears, &mut out), "nothing to cut yet");
        assert_eq!(out.len(), 1);
        assert!(l.crying(3));
        // Another unit's cry leaves unit 3's sounding.
        assert!(!l.cry(4, hurt, pos, ears, &mut out));
        assert!(l.crying(3) && l.crying(4));
        // Half a second on, a second hurt cuts the first; then death cuts it.
        l.tick(0.5, ears, &mut out);
        assert!(
            l.cry(3, hurt, pos, ears, &mut out),
            "a new hurt cuts the last"
        );
        l.tick(0.5, ears, &mut out);
        assert!(l.cry(3, death, pos, ears, &mut out), "death cuts hurt");
        assert_eq!(out.len(), 4);
        assert!(l.crying(3));
        // The take is 1 s: after it, nothing is cut.
        l.tick(1.01, ears, &mut out);
        assert!(!l.crying(3));
        assert!(!l.cry(3, hurt, pos, ears, &mut out));
        // A class without the key cuts and starts nothing.
        let n = out.len();
        assert!(l.cry(3, 0, pos, ears, &mut out));
        assert_eq!(out.len(), n);
        assert!(!l.crying(3));
        // A refused draw (`rare`: chance 0.25) still cuts the sounding cry.
        let mut refused = 0;
        for _ in 0..40 {
            l.cry(5, hurt, pos, ears, &mut out);
            let n = out.len();
            assert!(
                l.cry(5, rare, pos, ears, &mut out),
                "the cut comes before the draw"
            );
            if out.len() == n {
                refused += 1;
                assert!(!l.crying(5));
            }
        }
        assert!(refused > 10, "{refused}");
        // One tick's asks: per unit only the last one's start (it cut the
        // others as they began); another unit's stands; a last ask with no
        // key leaves its unit silent.
        let asks = [
            (7, hurt, pos),
            (8, hurt, pos),
            (7, hurt, pos),
            (7, death, pos),
            (8, 0, pos),
        ];
        let got = l.cries(&asks, ears);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!((got[0].0, got[0].1.hash), (7, death));
        assert!(l.crying(7) && !l.crying(8));
        let got = l.cries(&[(9, hurt, pos), (10, death, pos)], ears);
        assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), vec![9, 10]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Card PWST: a station's `AmbientSound` holds ONE handle (re-ambience-
    /// music §5.2) - asked every tick while it is not sounding, through the
    /// property's draw; a sounding take blocks every ask of its station until
    /// it ends, another station's handle is its own, a listener out of range
    /// starts nothing.
    #[test]
    fn a_station_has_one_ambient_handle() {
        let dir = std::env::temp_dir().join(format!("openbf-pwst-{}", std::process::id()));
        let mut l = lib();
        l.map.dir = dir.clone();
        for i in [1, 7] {
            one_second_wav(&dir.join(format!("b/{i}.wav")));
        }
        let (sure, rare) = (fnv1a_lowercase("exp_high"), fnv1a_lowercase("rare"));
        let (pos, ears) = (Vec3::new(10.0, 0.0, 0.0), Vec3::ZERO);
        let dt = 1.0 / 30.0;
        let mut sink = Vec::new();
        // A sure draw starts at once, then holds the handle for its 1 s take.
        let mut starts = Vec::new();
        for _ in 0..60 {
            starts.push(
                l.station_ambient(&[(0, sure, pos), (1, sure, pos)], ears)
                    .len(),
            );
            l.tick(dt, ears, &mut sink);
        }
        assert_eq!(starts[0], 2, "both stations start on the first ask");
        assert!(starts[1..30].iter().all(|n| *n == 0), "{starts:?}");
        assert_eq!(starts[30..32].iter().sum::<usize>(), 2, "{starts:?}");
        // A 0.25 draw: once the take ends the asks resume, so the gap past
        // the take is about 1 / 0.25 ticks.
        let (mut n, mut ticks) = (0, 0);
        while n < 200 {
            n += l.station_ambient(&[(2, rare, pos)], ears).len();
            l.tick(dt, ears, &mut sink);
            ticks += 1;
        }
        let gap = ticks as f32 / n as f32 - 30.0;
        assert!(
            (2.5..6.0).contains(&gap),
            "ticks per start past the take {gap}"
        );
        // Out of range (the property mutes from 450 m): no start, no hold.
        let far = Vec3::new(500.0, 0.0, 0.0);
        assert!(l.station_ambient(&[(3, sure, far)], ears).is_empty());
        assert_eq!(l.station_ambient(&[(3, sure, pos)], ears).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
