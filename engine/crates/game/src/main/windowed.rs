//! The windowed game (winit + gilrs): one `App` state machine for the
//! BF2-style shell, the BF1 loading screen and the battle's per-frame loop —
//! [`run_windowed`] for `--map`, [`run_shell`] for the shell — and the glue
//! only a window uses: event sounds, the pause menu's actions and cheat words,
//! the profile autosave and the title-bar line.

use super::battle::{
    hud_model, load_and_upload, load_hud_view, make_walk, spawn_model, squad_icons, walk_draws,
};
use super::{BattleSetup, ViewerArgs};
use anyhow::Result;
use core::sim::PadState;
use game::viewer::{camera_from, upload_unit_cube, SceneGpu, CLEAR, GOLDEN_CAM};
use game::walk::WalkSim;
use render::{Camera, DrawCmd, MeshHandle, Renderer};
use std::path::PathBuf;
use std::sync::Arc;

/// Start take `take` of Music `music` from the disc, silent-safe: a take
/// the indexes do not hold, or an LVL that is not there, plays nothing.
fn start_take(
    audio: &mut audio::Audio,
    m: &game::music::BattleMusic,
    music: usize,
    take: usize,
    volume_db: f32,
) -> Option<audio::MusicVoice> {
    let track = m.track(music)?;
    let t = track.takes.get(take)?.clone();
    audio
        .play_stream(t, volume_db, track.def.looping)
        .map_err(|e| eprintln!("music: {e:#}"))
        .ok()
}
/// Start the sounds of the tick the sim just stepped (`game::event_sounds`).
fn play_event_sounds(
    audio: &mut audio::Audio,
    sl: &mut game::sounds::SoundLib,
    foley: Option<&mut game::foley::Foley>,
    engines: &mut game::engine_sound::EngineSounds,
    effects: &[(String, glam::Vec3)],
    aspect: f32,
    walk: &WalkSim,
) {
    let listener = game::event_sounds::listener(walk);
    game::event_sounds::each(walk, listener.pos, |name, pos| {
        sl.play(audio, name, pos, &listener);
    });
    // The hurt and death cries, one voice per unit (card UCRY).
    let mut asks = Vec::new();
    game::event_sounds::cries(walk, |c| {
        let hash = c.name.map_or(0, game::classdb::fnv1a_lowercase);
        asks.push((c.unit, hash, c.pos));
    });
    sl.play_cries(audio, &asks, &listener);
    // The stations' ambient chatter, one handle per station (card PWST).
    sl.play_station_ambient(audio, &game::event_sounds::station_asks(walk), &listener);
    // The sounds of the explosions' effects (`Fx::take_sounds`).
    for (name, pos) in effects {
        sl.play(audio, name, *pos, &listener);
    }
    // The layers whose delay ends this tick (card WSND).
    sl.update(audio, core::TICK_DT, &listener);
    // The vehicles' engine layers (card VSND).
    let mut voices = Vec::new();
    engines.tick(walk, |h, l| sl.engine(h, l), &mut voices);
    sl.loop_voices(audio, &voices, &listener);
    // The soldiers' steps and stance sounds (card FOLY).
    if let Some(foley) = foley {
        let view = game::foley::View::of(&walk.camera(), aspect);
        foley.tick(walk, &view, |s| {
            sl.play_by_hash(audio, s.hash, s.pos, &listener);
        });
    }
}

/// The world's ambience streams and loops for the tick the sim just
/// stepped (card AMBS); the battle's end stops every sound
/// (re-ambience-music 0.4).
fn play_ambience(
    audio: &mut audio::Audio,
    sl: &mut game::sounds::SoundLib,
    ambience: &game::ambience::Ambience,
    walk: &WalkSim,
) {
    let listener = game::event_sounds::listener(walk);
    if walk.sim.outcome.is_some() {
        sl.stop_ambient();
    } else {
        sl.ambient_voices(audio, ambience, &listener);
        if game::dev::on() && walk.sim.tick.is_multiple_of(10 * core::TICK_HZ as u64) {
            eprintln!(
                "ambience: {} of {} emitters sounding",
                sl.ambient_playing(),
                ambience.emitters.len()
            );
        }
    }
}

/// The playtest black box for the battle about to start — dev mode only
/// (`game::blackbox`, card BLKB); `None` in normal play.
fn start_blackbox(
    scene: &game::assets::Scene,
    walk: &WalkSim,
    mode: &str,
    setup: &BattleSetup,
    sounds: Option<&mut game::sounds::SoundLib>,
) -> Option<game::blackbox::BlackBox> {
    use game::blackbox::{BlackBox, Meta};
    if !game::blackbox::on() {
        return None;
    }
    // The baked directory's name (`bf2_kam1`): what the sweep calls the map.
    let map = scene.dir.file_name().map_or_else(
        || scene.manifest.map.clone(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut meta = Meta::for_run(
        &map,
        mode,
        &scene.manifest.source_game,
        setup.seed,
        setup.difficulty.label(),
    );
    meta.mission = setup.mission_file.clone().unwrap_or_default();
    meta.player_team = setup.player_team;
    let dir = game::blackbox::default_dir();
    println!("blackbox: on, F9 writes {}/last.json", dir.display());
    let mut bb = BlackBox::start(true, meta, walk, scene.collision_body_labels(), dir)?;
    bb.install_panic_hook();
    // Its sound rows (card SNDB): the mixer's starts, by property name.
    if let Some(sl) = sounds {
        sl.handed = Some(Vec::new());
        let root = scene.dir.parent().unwrap_or(&scene.dir).to_path_buf();
        bb.sound_names(game::event_sounds::sound_names(&scene.dir, walk), root);
    }
    Some(bb)
}

/// Hand the black box the one-shots the mixer was given since the last call.
fn note_sounds(
    bb: &mut game::blackbox::BlackBox,
    sounds: Option<&mut game::sounds::SoundLib>,
    tick: u64,
) {
    if let Some(sl) = sounds {
        let loops = sl.loops_playing();
        if let Some(handed) = sl.handed.as_mut() {
            bb.sounds(tick, handed, loops);
        }
    }
}

/// The window's first size: 1280x720 logical, or `OPENBF_WINDOW_SIZE=WxH`
/// physical pixels (a measurement knob, card ISPD: the same frame at a
/// laptop panel's resolution).
fn window_size() -> winit::dpi::Size {
    let want = std::env::var("OPENBF_WINDOW_SIZE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
    });
    match want {
        Some((w, h)) if w > 0 && h > 0 => winit::dpi::PhysicalSize::new(w, h).into(),
        _ => winit::dpi::LogicalSize::new(1280, 720).into(),
    }
}

/// The weather's own random stream starts here (never the sim's).
const WEATHER_SEED: u32 = 0x7ea7_4e12;

/// The velocity of what the player's camera tracks — the soldier, or the
/// vehicle ridden: what tilts a rain streak (re-weather.md §3.1).
fn tracked_velocity(walk: &WalkSim) -> glam::Vec3 {
    let Some(me) = walk.sim.unit(walk.player) else {
        return glam::Vec3::ZERO;
    };
    match me.mount.and_then(|m| walk.sim.vehicle(m.vehicle)) {
        Some(v) => v.vel,
        None => me.soldier.vel,
    }
}

/// This frame's rain splashes (re-weather.md §5): each point dropped by a
/// ray from 50 m over the camera, 100 m down, onto terrain or buildings —
/// the ordnance ray's mask — and the map's `GroundEffect` started there.
fn rain_splashes(
    w: &mut game::weather::Weather,
    fx: &mut game::fx::Fx,
    walk: &WalkSim,
    aspect: f32,
    dt: f32,
) {
    let Some(name) = w.precip.ground_effect.clone() else {
        return;
    };
    let view = game::weather::View::from_camera(&walk.camera(), aspect);
    let sim = &walk.sim;
    let down = glam::Vec3::NEG_Y;
    let hits = w.splashes(dt, &view, 1, &mut |top| {
        let body = sim
            .collision
            .ray(top, down, 100.0, core::collision::Layers::ORDNANCE)
            .map(|(t, _)| t);
        let ground = sim.terrain_ray(top, down, 100.0, 0.5);
        let t = match (body, ground) {
            (Some(a), Some(b)) => a.min(b),
            (a, b) => a.or(b)?,
        };
        Some(top + down * t)
    });
    for hit in hits {
        fx.spawn(&name, game::fx::emitter::Frame::at(hit));
    }
}

/// PS2 BF2 pause-menu cheat words → openbf effects (analysis/re-cheat-codes.md
/// §6). Every word toggles. The returned line is the confirmation shown in the
/// HUD feed (the PS2 plays `shell_menu_accept`; that property is not in the
/// extracted BF2 SOUND/SHELL.LVL — open).
fn apply_cheat_word(
    word: &str,
    walk: &mut WalkSim,
    hud_hidden: &mut bool,
    sfx_slow: &mut bool,
    sounds: Option<&mut game::sounds::SoundLib>,
) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    match word {
        "sos" => {
            walk.sim.cheats.toggle("invincible");
            format!(
                "cheat sos: invincibility {}",
                on(walk.sim.cheats.invincible)
            )
        }
        "ammo" => {
            walk.sim.cheats.toggle("ammo");
            format!(
                "cheat ammo: infinite ammo {}",
                on(walk.sim.cheats.infinite_ammo)
            )
        }
        "hud" => {
            *hud_hidden = !*hud_hidden;
            format!(
                "cheat hud: HUD {}",
                if *hud_hidden { "hidden" } else { "shown" }
            )
        }
        "slow" => {
            *sfx_slow = !*sfx_slow;
            let rate = if *sfx_slow { 0.3 } else { 1.0 };
            if let Some(sl) = sounds {
                sl.rate_mul = rate;
            }
            format!("cheat slow: positional SFX pitch x{rate}")
        }
        "80s" => "cheat 80s: low-res soldiers (no soldier LOD models yet: no-op)".into(),
        "at" => "cheat at: comic hit effects (no melee AddHitEffect yet: no-op)".into(),
        "alias" => "cheat alias: PS2 GS post-filter toggle (no-op on PC)".into(),
        other => format!("cheat {other}: unknown"),
    }
}

/// Battle state as one line — the window title and the round-over stdout line
/// (the on-screen HUD is `ui::hud::draw_hud`).
fn hud_title(walk: &WalkSim) -> String {
    let b = &walk.sim.battle;
    let posts = |t: u8| walk.sim.cps.iter().filter(|c| c.owner == Some(t)).count();
    let me = &walk.sim.units[walk.player.index as usize];
    let status = match walk.sim.outcome {
        Some(core::sim::OUTCOME_DRAW) => "TIME UP — both teams win".to_string(),
        Some(w) => format!("ROUND OVER — team {} wins", w + 1),
        None if !me.alive => "respawning...".to_string(),
        None => format!(
            "HP {:.0} | ammo {}/{}",
            me.health.cur, me.weapon.clip, me.weapon.spare
        ),
    };
    format!(
        "openbf — tickets {}/{} | posts {}/{} | {}",
        b.teams[0].tickets(),
        b.teams[1].tickets(),
        posts(0),
        posts(1),
        status
    )
}

// ── Windowed viewer (winit 0.30 ApplicationHandler + gilrs) ──────────────────

#[path = "windowed/career.rs"]
mod career;
use career::record_round;
/// The BF2 PS2 shell's front screens (card BSHLA).
#[path = "windowed/front.rs"]
mod front;
use front::FrontShell;
#[path = "windowed/menu_actions.rs"]
mod menu_actions;
use menu_actions::{persist_setting, resolve_direct_settings};
#[path = "windowed/mouse.rs"]
mod mouse;

enum Screen {
    Shell,
    Loading,
    Battle,
}

/// BF1 load-screen playback state (Screen::Loading).
struct Loading {
    map_name: String,
    map_dir: PathBuf,
    started: std::time::Instant,
    /// egui textures of the sequence (empty = no load screen baked).
    textures: Vec<ui::egui::TextureHandle>,
    /// The final image has been presented; load the map next frame.
    load_pending: bool,
    beep: Option<audio::LoopHandle>,
}

/// Seconds per load-screen image (the PS2 zoom sequence paces about 1 s/step).
const LOAD_STEP_SECS: f32 = 1.0;

struct App {
    /// Direct launch target (--map); None = started in the shell.
    map: Option<PathBuf>,
    screen: Screen,
    shell: ui::shell::ShellModel,
    shell_last_map: usize,
    assets_dir: PathBuf,
    missions_dir: PathBuf,
    loading: Option<Loading>,
    audio: Option<audio::Audio>,
    sounds: Option<game::sounds::SoundLib>,
    /// The playing take of the battle's music (card AMBM); dropping it
    /// stops it.
    music_voice: Option<audio::MusicVoice>,
    /// The battle's soldier foley (card FOLY), when the map has a table.
    foley: Option<game::foley::Foley>,
    /// The battle's vehicle engines (card VSND).
    engines: game::engine_sound::EngineSounds,
    /// The world's fixed sound emitters (card AMBS).
    ambience: game::ambience::Ambience,
    cam: Camera,
    walk_requested: bool,
    setup: BattleSetup,
    mode: String,
    frames: u64,
    outcome_reported: bool,
    overlay: Option<ui::overlay::EguiOverlay>,
    /// The message box's lines (kills, posts, the player's orders).
    feed: game::event_feed::EventFeed,
    paused: bool,
    pause_menu: game::pause::PauseMenu,
    /// Placeholder muzzle/impact effects (visual only — `game::fx`).
    fx: game::fx::Fx,
    /// The map's precipitation and wind (visual only — `game::weather`).
    weather: Option<game::weather::Weather>,
    /// The posts' holograms (visual only — `game::holo`).
    holos: game::holo::Holos,
    /// The laser bolts' ribbons (card FXB — `game::bolt_vis`).
    bolts: game::bolt_vis::BoltVis,
    fx_quad: Option<MeshHandle>,
    fx_sprite: Option<render::TextureHandle>,
    /// D11's two icons: `squad_cmd_highlight` over a commanded bot,
    /// `squad_cmd_cross` on a Hold's anchor.
    squad_icons: (Option<render::TextureHandle>, Option<render::TextureHandle>),
    /// BF2's HUD for the battle in play (a fresh one per battle: its fades
    /// start over).
    hud_view: Option<game::hud_feed::HudView>,
    /// The playtest black box (dev mode; `None` in normal play) and an F9
    /// press waiting for the frame to write it.
    blackbox: Option<game::blackbox::BlackBox>,
    snapshot_requested: bool,
    /// Quit chosen in the pause menu; handled after the frame.
    quit_clicked: bool,
    /// The frame record (dev mode, card HITCH) and `OPENBF_FRAME_PROFILE=<csv>`'s
    /// row of CPU / GPU ms per battle frame (`game::frame_profile`).
    frame_profile: Option<game::frame_profile::FrameProfile>,
    profile: game::profile::Profile,
    round_recorded: bool,
    /// Previous frame's menu-relevant buttons (edge detection).
    prev_menu_buttons: u32,
    /// Separate from menu/D-pad state; reset when the sampled controller changes.
    gameplay_stick: GameplayStick<gilrs::GamepadId>,
    /// The menu bits the keyboard holds (the shell only: arrows, Enter as
    /// Cross + Start, Escape / Backspace as Back; card BSHLA).
    keys_held: u32,
    /// The bits pressed since the last frame read them: a tap that is let
    /// go inside one frame still counts.
    keys_tapped: u32,
    kbm: game::kbm::Kbm,
    /// The PS2 shell's front screens (`None` until the shell first draws).
    front: Option<FrontShell>,
    /// One click of L3 sprints (`input::SprintLatch`).
    sprint_latch: input::SprintLatch,
    /// `OPENBF_AUTOPILOT=1` in dev mode: [`autopilot_pad`] drives the player.
    autopilot: bool,
    /// `OPENBF_QUIT_AT=<s>` in dev mode: the game quits (as the window's
    /// close does) when the battle's clock reaches it - an unattended run's
    /// end with its exit snapshot and frame summary.
    quit_at: Option<u64>,
    /// PS2 BF2 Morse cheat recogniser — process lifetime, never reset (authentic).
    morse: ui::menu::MorseCheats,
    /// `hud` cheat: HUD hidden.
    hud_hidden: bool,
    /// `slow` cheat: positional SFX at 0.3× pitch.
    sfx_slow: bool,
    window: Option<Arc<winit::window::Window>>,
    renderer: Option<Renderer>,
    gpu: Option<SceneGpu>,
    walk: Option<(WalkSim, MeshHandle)>,
    /// Animated soldier models (per team) — `None` before a battle starts.
    soldier_vis: Option<game::soldier_vis::SoldierVisuals>,
    /// The spawn screens' 3-D soldiers (card SSELB).
    spawn_preview: game::spawn_preview::Preview,
    /// The battle's vehicle models (card VDRAW) — empty before a battle.
    vehicle_vis: game::viewer::VehicleVisuals,
    gilrs: Option<gilrs::Gilrs>,
    /// A connected pad's dead camera stick / FIRE (`input::log_pads`), shown
    /// on every screen until a pad that resolves replaces it (card PADM).
    pad_warning: Option<String>,
    last: std::time::Instant,
}

/// `OPENBF_AUTOPILOT=1` (dev, card HITCH): the player's pad walks forward,
/// wanders left and right, fires in bursts and presses the spawn screens'
/// accept and the use button now and then, so an unattended windowed run
/// moves the view over the map (the frame recorder's replays). A function of
/// the sim tick only.
fn autopilot_pad(tick: u64) -> PadState {
    use core::sim::buttons as b;
    let t = tick as f32 * core::TICK_DT;
    let mut buttons = 0;
    if t % 5.0 < 1.5 {
        buttons |= b::FIRE;
    }
    if t % 0.5 < 0.25 {
        buttons |= b::SPAWN_ACCEPT;
    }
    if t % 7.0 < 0.2 {
        buttons |= b::ACTION;
    }
    PadState {
        lx: 0,
        ly: PadState::quantize(1.0),
        rx: PadState::quantize(
            0.35 * core::fmath::sin_cos(t * 0.4).0 + 0.1 * core::fmath::sin_cos(t * 1.7).0,
        ),
        ry: 0,
        buttons,
        ..PadState::default()
    }
}

/// Decode every take the battle can play, behind the loading (card HITCH:
/// a take decoded at its first play cost its frame up to 5 ms).
fn preload_sounds(sl: &mut game::sounds::SoundLib) {
    let (n, bytes, ms) = sl.preload();
    println!(
        "sounds: {n} takes decoded at load ({:.1} MB, {ms:.0} ms)",
        bytes as f64 / (1024.0 * 1024.0)
    );
}

/// `OPENBF_QUIT_AT=<seconds>` (dev mode): the battle tick the game quits at.
fn quit_at() -> Option<u64> {
    let s: f32 = std::env::var("OPENBF_QUIT_AT").ok()?.parse().ok()?;
    game::dev::on().then_some((s * core::TICK_HZ as f32) as u64)
}

/// Open gilrs through `input::open_gilrs` and log how each pad resolved.
fn open_pads(mapping: &input::Mapping) -> (Option<gilrs::Gilrs>, Option<String>) {
    match input::open_gilrs() {
        Ok(g) => {
            let w = input::log_pads_with(&g, mapping);
            (Some(g), w)
        }
        Err(e) => {
            eprintln!("gilrs unavailable ({e}); no controller input");
            (None, None)
        }
    }
}

struct GameplayStick<Id> {
    controller: Option<Id>,
    arms: u32,
}

impl<Id> Default for GameplayStick<Id> {
    fn default() -> Self {
        Self {
            controller: None,
            arms: 0,
        }
    }
}

impl<Id: PartialEq> GameplayStick<Id> {
    fn select(&mut self, controller: Option<Id>) {
        if self.controller != controller || controller.is_none() {
            self.arms = 0;
        }
        self.controller = controller;
    }
}

pub(super) fn run_windowed(v: ViewerArgs) -> Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let profile = game::profile::Profile::load_or_default();
    let (gilrs, pad_warning) = open_pads(&profile.settings.controller_bindings);
    let setup = BattleSetup::from_args(&v, v.seed)?;
    let audio = audio::Audio::new();
    let frame_profile = game::frame_profile::FrameProfile::from_env();
    let autopilot = game::dev::on() && std::env::var("OPENBF_AUTOPILOT").is_ok_and(|v| v == "1");
    let quit_at = quit_at();
    let mut setup = setup;
    resolve_direct_settings(&mut setup, v.difficulty, &profile.settings);
    let assets_dir = v
        .map
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let mut app = App::new(
        assets_dir,
        PathBuf::from("missions"),
        setup,
        gilrs,
        pad_warning,
        audio,
        profile,
    );
    app.frame_profile = frame_profile;
    app.autopilot = autopilot;
    app.quit_at = quit_at;
    app.map = Some(v.map);
    app.screen = Screen::Battle;
    app.cam = camera_from(v.cam);
    app.walk_requested = v.walk;
    app.mode = v.mode;
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl App {
    fn new(
        assets_dir: PathBuf,
        missions_dir: PathBuf,
        setup: BattleSetup,
        gilrs: Option<gilrs::Gilrs>,
        pad_warning: Option<String>,
        audio: Option<audio::Audio>,
        profile: game::profile::Profile,
    ) -> Self {
        let mut kbm = game::kbm::Kbm::default();
        kbm.follow(&profile.settings);
        Self {
            map: None,
            screen: Screen::Shell,
            shell: ui::shell::ShellModel::default(),
            shell_last_map: usize::MAX,
            loading: None,
            audio,
            sounds: None,
            music_voice: None,
            foley: None,
            engines: Default::default(),
            ambience: Default::default(),
            assets_dir,
            missions_dir,
            cam: camera_from(GOLDEN_CAM),
            walk_requested: true,
            setup,
            mode: "con".to_string(),
            frames: 0,
            outcome_reported: false,
            overlay: None,
            feed: game::event_feed::EventFeed::default(),
            paused: false,
            pause_menu: game::pause::PauseMenu::default(),
            quit_clicked: false,
            frame_profile: None,
            profile,
            round_recorded: false,
            fx: game::fx::Fx::default(),
            weather: None,
            holos: game::holo::Holos::default(),
            bolts: game::bolt_vis::BoltVis::default(),
            fx_quad: None,
            fx_sprite: None,
            squad_icons: (None, None),
            hud_view: None,
            blackbox: None,
            snapshot_requested: false,
            prev_menu_buttons: 0,
            gameplay_stick: GameplayStick::default(),
            keys_held: 0,
            keys_tapped: 0,
            kbm,
            front: None,
            sprint_latch: input::SprintLatch::default(),
            autopilot: false,
            quit_at: None,
            morse: ui::menu::MorseCheats::new(),
            hud_hidden: false,
            sfx_slow: false,
            window: None,
            renderer: None,
            gpu: None,
            walk: None,
            soldier_vis: None,
            spawn_preview: Default::default(),
            vehicle_vis: Default::default(),
            gilrs,
            pad_warning,
            last: std::time::Instant::now(),
        }
    }

    /// Fly-cam: left stick moves (yaw-relative), right stick looks, triggers
    /// fly down/up, south button = speed boost.
    fn drive_fly_camera(&mut self, dt: f32) {
        let Some(g) = &mut self.gilrs else { return };
        if input::pump(g) {
            self.pad_warning = input::log_pads_with(g, &self.profile.settings.controller_bindings);
            self.gameplay_stick.select(None);
        }
        let Some((_, pad)) = g.gamepads().next() else {
            return;
        };
        let dead = |v: f32| if v.abs() < 0.15 { 0.0 } else { v };
        let lx = dead(pad.value(gilrs::Axis::LeftStickX));
        let ly = dead(pad.value(gilrs::Axis::LeftStickY));
        let rx = dead(pad.value(gilrs::Axis::RightStickX));
        let ry = dead(pad.value(gilrs::Axis::RightStickY));
        let up = pad
            .button_data(gilrs::Button::RightTrigger2)
            .map_or(0.0, |b| b.value());
        let down = pad
            .button_data(gilrs::Button::LeftTrigger2)
            .map_or(0.0, |b| b.value());
        let boost = if pad.is_pressed(gilrs::Button::South) {
            4.0
        } else {
            1.0
        };

        const LOOK_SPEED: f32 = 2.2;
        const MOVE_SPEED: f32 = 60.0;
        self.cam.yaw -= rx * LOOK_SPEED * dt;
        self.cam.pitch = (self.cam.pitch + ry * LOOK_SPEED * dt).clamp(-1.55, 1.55);
        let fwd = self.cam.forward();
        let right = self.cam.right();
        self.cam.position +=
            (fwd * ly + right * lx + glam::Vec3::Y * (up - down)) * (MOVE_SPEED * boost * dt);
    }

    /// Populate the shell from the baked maps + profile.
    fn init_shell(&mut self) {
        let maps = game::assets::discover_maps(&self.assets_dir);
        self.shell = ui::shell::ShellModel {
            maps: maps
                .iter()
                .map(|(n, g, _)| (n.clone(), g.clone()))
                .collect(),
            mode: "con".into(),
            teams: [0, 1],
            ai_per_team: 8,
            base_tickets: [250, 250],
            ticket_pct: 100,
            time_limit: 0,
            cursor: 0,
            career_line: format!(
                "{} — {} rounds, {} wins, {} kills / {} deaths",
                self.profile.name,
                self.profile.career.rounds,
                self.profile.career.wins,
                self.profile.career.kills,
                self.profile.career.deaths
            ),
            mission_found: false,
            ..Default::default()
        };
        if let Some(last) = &self.profile.settings.last_map {
            if let Some(i) = maps.iter().position(|(n, _, _)| n == last) {
                self.shell.map_idx = i;
            }
        }
        self.screen = Screen::Shell;
        self.shell_last_map = usize::MAX;
        self.seed_shell_from_map();
    }

    /// When the selected map changes: defaults from its mission file (or the
    /// source game's default matchup).
    fn seed_shell_from_map(&mut self) {
        if self.shell.map_idx == self.shell_last_map {
            return;
        }
        self.shell_last_map = self.shell.map_idx;
        let Some((name, game)) = self.shell.maps.get(self.shell.map_idx).cloned() else {
            return;
        };
        let fidx = |f: &str| {
            ui::shell::FACTIONS
                .iter()
                .position(|x| *x == f)
                .unwrap_or(0)
        };
        let path = self
            .missions_dir
            .join(format!("{name}_{}.ron", self.shell.mode));
        match game::mission::MissionDef::load(&path) {
            Ok(m) => {
                let f = m.factions();
                self.shell.teams = [fidx(&f[0]), fidx(&f[1])];
                self.shell.base_tickets = m.tickets();
                self.shell.ai_per_team = m.initial_ai(1, false).min(32);
                self.shell.mission_found = true;
            }
            Err(_) => {
                let d = game::roster::default_factions(&game);
                self.shell.teams = [fidx(d[0]), fidx(d[1])];
                self.shell.base_tickets = [250, 250];
                self.shell.ai_per_team = 8;
                self.shell.mission_found = false;
            }
        }
    }

    /// Shell → loading screen (BF1 sequence + beep), then the battle.
    fn begin_loading(&mut self) {
        // The shell music stops with the shell.
        if let Some(f) = self.front.as_mut() {
            f.music = None;
        }
        let maps = game::assets::discover_maps(&self.assets_dir);
        let Some((name, source_game, dir)) = maps.get(self.shell.map_idx).cloned() else {
            eprintln!("no baked map selected");
            return;
        };
        let mut textures = Vec::new();
        // A map's own screen, else the game's generic one (BF2 LOAD/COMMON.LVL
        // `loadscreen`, baked into common_bf2/load).
        let screen = game::assets::load_loadscreen(&dir).or_else(|| {
            game::assets::load_loadscreen(&self.assets_dir.join(format!("common_{source_game}")))
        });
        if let (Some(overlay), Some(ls)) = (&self.overlay, screen) {
            // BF2 stills: one image per load (pick rotates with the clock).
            let sequence: Vec<(String, game::assets::CpuTexture)> = if ls.pool {
                let n = ls.sequence.len();
                let pick = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as usize)
                    % n.max(1);
                ls.sequence.into_iter().skip(pick).take(1).collect()
            } else {
                ls.sequence
            };
            for (tex_name, t) in sequence {
                let img = ui::egui::ColorImage::from_rgba_unmultiplied(
                    [t.width as usize, t.height as usize],
                    &t.rgba,
                );
                textures.push(overlay.ctx().load_texture(
                    tex_name,
                    img,
                    ui::egui::TextureOptions::LINEAR,
                ));
            }
        }
        // BF1 loading beep (load_xtracking = the 440 Hz ui_load_lp at pitch
        // 2.1, baked by `bake beep`).
        let beep = self.audio.as_mut().and_then(|a| {
            a.play_loop(&self.assets_dir.join("audio/load_xtracking.ogg"), 1.0, -8.0)
                .ok()
        });
        self.loading = Some(Loading {
            map_name: name,
            map_dir: dir,
            started: std::time::Instant::now(),
            textures,
            load_pending: false,
            beep,
        });
        self.screen = Screen::Loading;
    }

    /// Loading frame: paint the current step; after the last step is shown,
    /// perform the (synchronous) map load on the following frame.
    fn redraw_loading(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(l) = self.loading.as_mut() else {
            self.screen = Screen::Shell;
            return;
        };
        if l.load_pending {
            if let (Some(a), Some(b)) = (self.audio.as_mut(), l.beep.as_mut()) {
                a.stop(b);
            }
            let (name, dir) = (l.map_name.clone(), l.map_dir.clone());
            self.loading = None;
            self.start_battle(name, dir);
            return;
        }
        let steps = l.textures.len().max(1);
        let elapsed = l.started.elapsed().as_secs_f32();
        let step = ((elapsed / LOAD_STEP_SECS) as usize).min(steps - 1);
        let last_shown = step + 1 >= steps && elapsed >= LOAD_STEP_SECS * steps as f32;
        let image = l.textures.get(step).map(|t| t.id());
        let caption = l.map_name.to_uppercase();
        if let (Some(r), Some(overlay), Some(window)) =
            (&mut self.renderer, &mut self.overlay, &self.window)
        {
            let paint = overlay.begin(window, |ctx| {
                ui::loading::draw_loading(ctx, image, &caption, step, steps);
            });
            if let Err(e) = r.render_with_overlay(
                &Camera::default(),
                &[],
                [0.0, 0.0, 0.0, 1.0],
                Some(&mut |dev, q, enc, view, size| overlay.paint(&paint, dev, q, enc, view, size)),
            ) {
                eprintln!("render: {e}");
                event_loop.exit();
            }
        }
        if last_shown {
            if let Some(l) = self.loading.as_mut() {
                l.load_pending = true;
            }
        }
    }

    /// Build resources in launch order. Shell supplies its newly resolved setup;
    /// direct launch uses the setup already stored on the App.
    fn battle_resources(
        &mut self,
        renderer: &mut Renderer,
        scene: &game::assets::Scene,
        setup: Option<&BattleSetup>,
    ) -> (WalkSim, MeshHandle) {
        let cube = upload_unit_cube(renderer).expect("cube");
        self.fx_quad = game::viewer::upload_billboard_quad(renderer).ok();
        self.fx_sprite = game::viewer::upload_fx_sprite(renderer).ok();
        let dir = self.assets_dir.clone();
        self.squad_icons = squad_icons(renderer, &dir);
        self.hud_view = load_hud_view(&dir, renderer);
        self.bolts = game::bolt_vis::BoltVis::new(&self.assets_dir, &scene.manifest.source_game);
        let setup = setup.unwrap_or(&self.setup);
        let mut walk = make_walk(scene, setup);
        walk.blend = game::render_blend::Blend::from_env();
        self.foley = game::foley::Foley::load(&scene.dir, &walk);
        self.engines = game::engine_sound::EngineSounds::new(&scene.dir, &walk);
        self.ambience =
            game::ambience::Ambience::load(&scene.manifest, &scene.dir, &setup.classes_dir);
        if let Some(sl) = self.sounds.as_mut() {
            sl.for_walk(&scene.dir, &walk);
            preload_sounds(sl);
        }
        self.blackbox = start_blackbox(scene, &walk, &self.mode, setup, self.sounds.as_mut());
        self.feed = game::event_feed::EventFeed::for_walk(
            &self.assets_dir,
            &scene.manifest.source_game,
            &walk,
        );
        self.fx = game::fx::Fx::for_walk(
            &self.assets_dir,
            &scene.dir,
            &scene.manifest.source_game,
            &walk,
        );
        self.fx.set_water(&scene.manifest);
        self.fx.preload(renderer);
        self.weather = game::weather::Weather::load(
            &scene.manifest,
            &self.assets_dir,
            &scene.dir,
            WEATHER_SEED,
        );
        self.soldier_vis = Some(game::soldier_vis::SoldierVisuals::for_walk(
            renderer,
            &self.assets_dir,
            scene,
            &walk,
        ));
        self.vehicle_vis = game::viewer::VehicleVisuals::load(
            renderer,
            scene,
            &self.assets_dir,
            &walk.vehicle_geometry,
        );
        // What the battle can show later, behind the loading (card HITCH).
        self.vehicle_vis.preload(renderer, &walk.sim);
        self.bolts.preload(renderer, &walk);
        walk.sim.assist_intensity = self.profile.settings.assist_intensity;
        walk.fov_scale = self.profile.settings.fov_scale;
        if let Some(m) = walk.music.as_mut() {
            m.volume = game::music::volume_from_env(self.profile.settings.music_volume);
        }
        // THE MAINTAINER'S RULE: the aim assist is the PLAYER'S switch — no difficulty
        // tier reads it (the PS2 BF2's Elite clears it, RE §9.3; not ours).
        walk.sim.assist_armed = self.profile.settings.aim_assist;
        walk.sim.human_damage_on = self.profile.settings.human_damage_on;
        walk.sim.team_handicap_on = self.profile.settings.team_handicap_on;
        (walk, cube)
    }

    /// Load the map + build the battle with the shell's settings.
    fn start_battle(&mut self, name: String, dir: PathBuf) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let (gpu, scene) = match load_and_upload(renderer, &dir, &self.mode) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("failed to load map {name}: {e:#}");
                return;
            }
        };
        self.sounds = game::sounds::SoundLib::load(&dir);
        self.music_voice = None;
        // The mission whose pair was picked (Republic against CIS runs the
        // map's Clone Wars script), the player's army on its own team there;
        // a cross-era pair runs the map's default mission (card ERAM).
        let armies = self.shell.teams.map(|t| ui::shell::FACTIONS[t]);
        let pick = game::mission::pick(&self.missions_dir, &name, &self.shell.mode, armies);
        let player_team = pick.as_ref().map_or(0, |p| p.player_team);
        let mut teams = armies.map(str::to_string);
        if player_team == 1 {
            teams.swap(0, 1);
        }
        let mission_file = pick.as_ref().and_then(|p| super::file_name(&p.path));
        match &pick {
            Some(p) => println!(
                "mission: {} for {} against {} ({}), the player on team {}",
                p.path.display(),
                armies[0],
                armies[1],
                if p.own {
                    "its own pair"
                } else {
                    "a cross-era pair"
                },
                player_team + 1
            ),
            None => println!("mission: none for {name} {}", self.shell.mode),
        }
        let mission = pick.map(|p| p.mission);
        let setup = BattleSetup {
            seed: 42u32.wrapping_add(self.profile.career.rounds),
            // The shell round takes the PROFILE's tier (no CLI flag here).
            difficulty: self.profile.settings.difficulty,
            spawn: (0.0, 0.0),
            spawn_set: false,
            targets: false,
            ai: Some(self.shell.ai_per_team),
            // The shell has its own alive-AI slider (MenuAction::SetAiCap).
            ai_cap: None,
            // The mission's counts (else 250) scaled by the shell's %.
            tickets: 250,
            tickets_set: false,
            ticket_pct: self.shell.ticket_pct,
            time_limit: self.shell.time_limit,
            cheats: core::sim::Cheats::default(),
            teams: Some(teams.clone()),
            classes_dir: self.assets_dir.join("classes"),
            mission,
            mission_file,
            player_team,
        };
        // Release the renderer field borrow while installing the other App resources.
        let mut renderer = self.renderer.take().expect("loaded renderer");
        let (mut walk, cube) = self.battle_resources(&mut renderer, &scene, Some(&setup));
        self.renderer = Some(renderer);
        if let Some(cap) = self.profile.settings.ai_alive_cap {
            walk.sim.ai_alive_cap = cap;
        }
        super::battle::open_spawn_screens(&mut walk, setup.mission.as_ref());
        self.profile.settings.last_map = Some(name);
        self.profile.settings.last_mode = Some(self.mode.clone());
        self.profile.settings.last_teams = Some(teams);
        if let Err(e) = self.profile.save() {
            eprintln!("profile autosave failed: {e:#}");
        }
        self.walk = Some((walk, cube));
        self.gpu = Some(gpu);
        self.setup = setup;
        self.paused = false;
        self.kbm.release_keys();
        self.outcome_reported = false;
        self.round_recorded = false;
        self.screen = Screen::Battle;
        self.start_frame_record();
    }

    /// A battle's first frame is next: the frame record starts over and the
    /// black box writes it into its snapshots (card HITCH).
    fn start_frame_record(&mut self) {
        if let Some(p) = self.frame_profile.as_mut() {
            p.reset();
            if let Some(bb) = self.blackbox.as_mut() {
                bb.frame_record(p.record());
            }
        }
    }

    /// The black box's `exit` snapshot, written before the battle goes, and
    /// the frame record's summary line.
    fn blackbox_exit(&mut self) {
        if let Some(h) = self.frame_profile.as_ref().and_then(|p| p.hitches()) {
            println!("frame record: {}", h.summary());
        }
        if let (Some(bb), Some((walk, _))) = (self.blackbox.as_mut(), self.walk.as_ref()) {
            bb.write("exit", Some(walk), true);
        }
        self.blackbox = None;
    }

    /// Battle → shell (records the round).
    fn leave_battle(&mut self) {
        self.blackbox_exit();
        if let Some((walk, _)) = self.walk.as_mut() {
            if !self.round_recorded {
                self.round_recorded = true;
                record_round(&mut self.profile, walk, self.feed.battle_stats());
            }
        }
        self.walk = None;
        self.music_voice = None;
        self.gpu = None;
        self.hud_view = None;
        self.paused = false;
        self.kbm.release_keys();
        self.release_mouse();
        if let Some(sl) = self.sounds.as_mut() {
            sl.stop_loops();
        }
        self.init_shell();
    }

    /// Raw menu buttons this frame as edge bits: 1 up, 2 down, 4 left,
    /// 8 right, 16 confirm (Cross), 32 back (Circle), 64 Start.
    fn menu_edges(&mut self) -> u32 {
        let mut now = 0u32;
        if let Some(g) = &mut self.gilrs {
            if input::pump(g) {
                self.pad_warning =
                    input::log_pads_with(g, &self.profile.settings.controller_bindings);
                self.gameplay_stick.select(None);
                if let Some(front) = self.front.as_mut() {
                    front.reset_control_device();
                }
            }
            if let Some((_, pad)) = g.gamepads().next() {
                use gilrs::Button;
                let bits = [
                    (Button::DPadUp, 1),
                    (Button::DPadDown, 2),
                    (Button::DPadLeft, 4),
                    (Button::DPadRight, 8),
                    (Button::South, 16),
                    (Button::East, 32),
                    (Button::Start, 64),
                ];
                for (b, bit) in bits {
                    if pad.is_pressed(b) {
                        now |= bit;
                    }
                }
                // The shell's Back is Triangle, as its help line shows
                // (re-bf2-shell §3.4); Circle too, as everywhere else.
                if matches!(self.screen, Screen::Shell) && pad.is_pressed(Button::North) {
                    now |= 32;
                }
                // The left stick presses the same arms as the D-pad.
                now |= input::stick_arms(
                    pad.value(gilrs::Axis::LeftStickX),
                    pad.value(gilrs::Axis::LeftStickY),
                    self.prev_menu_buttons & 15,
                );
            }
        }
        let shell = matches!(self.screen, Screen::Shell);
        now |= game::kbm::menu_bits(self.keys_held, shell);
        let tapped = game::kbm::menu_bits(std::mem::take(&mut self.keys_tapped), shell);
        let edges = (now & !self.prev_menu_buttons) | tapped;
        self.prev_menu_buttons = now;
        edges
    }

    /// Sample the first gamepad into a sim PadState (walk mode).
    fn sample_pad(&mut self) -> PadState {
        let Some(g) = &mut self.gilrs else {
            self.gameplay_stick.select(None);
            return PadState::default();
        };
        if input::pump(g) {
            self.pad_warning = input::log_pads_with(g, &self.profile.settings.controller_bindings);
            // A disconnect/reconnect can reuse the same gilrs ID between frames.
            self.gameplay_stick.select(None);
        }
        match g.gamepads().next() {
            Some((id, pad)) => {
                self.gameplay_stick.select(Some(id));
                let (state, arms) = input::pad_from_gamepad_with_arms(
                    &pad,
                    &self.profile.settings.controller_bindings,
                    self.gameplay_stick.arms,
                );
                self.gameplay_stick.arms = arms;
                state
            }
            None => {
                self.gameplay_stick.select(None);
                PadState::default()
            }
        }
    }
}

/// Shell entry: window + shell screen; battles start from the menu.
pub(super) fn run_shell(assets_dir: PathBuf, missions_dir: PathBuf) -> Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let profile = game::profile::Profile::load_or_default();
    let (gilrs, pad_warning) = open_pads(&profile.settings.controller_bindings);
    let audio = audio::Audio::new();
    let setup = BattleSetup {
        seed: 42,
        difficulty: core::skill::Difficulty::default(),
        spawn: (0.0, 0.0),
        spawn_set: false,
        targets: false,
        ai: None,
        ai_cap: None,
        tickets: 250,
        tickets_set: false,
        ticket_pct: 100,
        time_limit: 0,
        cheats: core::sim::Cheats::default(),
        teams: None,
        classes_dir: assets_dir.join("classes"),
        mission: None,
        mission_file: None,
        player_team: 0,
    };
    let autopilot = game::dev::on() && std::env::var("OPENBF_AUTOPILOT").is_ok_and(|v| v == "1");
    let quit_at = quit_at();
    let frame_profile = game::frame_profile::FrameProfile::from_env();
    let mut app = App::new(
        assets_dir,
        missions_dir,
        setup,
        gilrs,
        pad_warning,
        audio,
        profile,
    );
    app.frame_profile = frame_profile;
    app.autopilot = autopilot;
    app.quit_at = quit_at;
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_title("openbf")
                        .with_inner_size(window_size()),
                )
                .expect("create window"),
        );
        let mut renderer = Renderer::new_windowed(window.clone()).expect("renderer");
        self.overlay = Some(ui::overlay::EguiOverlay::new(
            &window,
            renderer.device(),
            renderer.surface_format(),
        ));
        let Some(map) = self.map.clone() else {
            // Shell start: no map yet.
            self.init_shell();
            self.window = Some(window);
            self.renderer = Some(renderer);
            self.last = std::time::Instant::now();
            return;
        };
        self.sounds = game::sounds::SoundLib::load(&map);
        self.music_voice = None;
        if let Some(sl) = &self.sounds {
            println!(
                "sound library loaded ({})",
                if sl.has("all_weap_inf_rifle_fire") || sl.has("rep_weap_inf_rifle_fire") {
                    "weapon set present"
                } else {
                    "weapon set partial"
                }
            );
        }
        match load_and_upload(&mut renderer, &map, &self.mode) {
            Ok((gpu, scene)) => {
                if self.walk_requested {
                    let (walk, cube) = self.battle_resources(&mut renderer, &scene, None);
                    self.profile.settings.last_map = Some(scene.manifest.map.clone());
                    self.profile.settings.last_mode = Some(self.mode.clone());
                    self.profile.settings.last_teams =
                        Some([walk.teams[0].faction.clone(), walk.teams[1].faction.clone()]);
                    if let Err(e) = self.profile.save() {
                        eprintln!("profile autosave failed: {e:#}");
                    }
                    println!(
                        "profile '{}': {} rounds, {} kills / {} deaths",
                        self.profile.name,
                        self.profile.career.rounds,
                        self.profile.career.kills,
                        self.profile.career.deaths
                    );
                    self.walk = Some((walk, cube));
                    self.start_frame_record();
                }
                self.gpu = Some(gpu);
            }
            Err(e) => {
                eprintln!("failed to load map: {e:#}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last = std::time::Instant::now();
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.release_mouse();
    }

    fn device_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _device: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            self.kbm.motion(delta.0, delta.1);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        if self.capture_keyboard(&event) {
            return;
        }
        if let (Some(o), Some(w)) = (&mut self.overlay, &self.window) {
            let _ = o.on_window_event(w, &event);
        }
        self.mouse_window_event(&event);
        match event {
            WindowEvent::Focused(false) => {
                self.keys_held = 0;
                self.keys_tapped = 0;
                self.prev_menu_buttons = 0;
            }
            WindowEvent::CloseRequested => {
                if let Some((walk, _)) = self.walk.as_mut() {
                    if !self.round_recorded {
                        self.round_recorded = true;
                        record_round(&mut self.profile, walk, self.feed.battle_stats());
                    }
                }
                self.blackbox_exit();
                event_loop.exit()
            }
            // F9: the playtest black box writes its snapshot (dev mode).
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key:
                            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F9),
                        state: winit::event::ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => self.snapshot_requested = self.blackbox.is_some(),
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                let bit = game::kbm::menu_key(code);
                if state == winit::event::ElementState::Pressed {
                    self.keys_held |= bit;
                    // The system's key repeat steps the arrows, never a
                    // confirm or a back.
                    if !repeat || bit < 16 {
                        self.keys_tapped |= bit;
                    }
                } else {
                    self.keys_held &= !bit;
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let dt = self.last.elapsed().as_secs_f32().min(0.1);
                self.last = std::time::Instant::now();
                // The renderer's animation clock, for UV-scrolling materials
                // (a map's water). Wrapped so `uv + velocity * time` keeps
                // its f32 precision in a long session — see Renderer::time.
                if let Some(r) = self.renderer.as_mut() {
                    (r.time, r.water_time) = render::advance_clocks(r.time, r.water_time, dt);
                }

                self.sync_grab();
                if matches!(self.screen, Screen::Shell) {
                    self.redraw_shell(event_loop, dt);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }
                if matches!(self.screen, Screen::Loading) {
                    self.redraw_loading(event_loop);
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    return;
                }

                if let Some(p) = self.frame_profile.as_mut() {
                    p.begin();
                }
                let mut extra: Vec<DrawCmd> = Vec::new();
                let mut skinned: Vec<render::SkinnedDrawCmd> = Vec::new();
                // The loaded map's own clear colour (its fog, where it authors
                // fog); the shell and the pre-map screens keep the default.
                let clear = self.gpu.as_ref().map_or(CLEAR, |g| g.clear);
                let mut hud: Option<ui::hud::HudModel> = None;
                let mut spawn: Option<ui::spawn::SpawnModel> = None;
                let mut menu: Option<ui::menu::MenuModel> = None;
                let edges = if self.walk.is_some() {
                    self.menu_edges()
                } else {
                    0
                };
                if self.walk.is_some() && edges & 64 != 0 {
                    self.paused = !self.paused;
                    self.pause_menu.confirm = false;
                    self.pause_menu.cursor = 0;
                }
                self.sync_grab();
                let camera = if self.walk.is_some() {
                    let pad = if self.paused {
                        self.kbm.drop_taps();
                        PadState::default()
                    } else {
                        let (on_foot, sprinting) =
                            self.walk.as_ref().map_or((false, false), |(w, _)| {
                                (
                                    w.player_on_foot(),
                                    w.soldier().stance == core::soldier::Stance::Sprint,
                                )
                            });
                        let pad = match self.autopilot {
                            true => autopilot_pad(self.walk.as_ref().map_or(0, |w| w.0.sim.tick)),
                            false => {
                                let pad = self.sample_pad();
                                self.kbm.pad(pad)
                            }
                        };
                        self.sprint_latch.apply(pad, on_foot, sprinting)
                    };
                    let paused = self.paused;
                    let (walk, cube) = self.walk.as_mut().unwrap();
                    let mut stuck_alarm = false;
                    let mut look = PadState::default();
                    if paused {
                        // The engines fall silent with the sim.
                        if let Some(sl) = self.sounds.as_mut() {
                            sl.stop_loops();
                        }
                        let model = ui::menu::MenuModel {
                            cheats: walk.sim.cheats.entries().to_vec(),
                            assist_intensity: walk.sim.assist_intensity,
                            ai_alive_cap: walk.sim.ai_alive_cap,
                            fov_scale: walk.fov_scale,
                            fov_deg: walk.fov_y().to_degrees(),
                            music_volume: walk
                                .music
                                .as_ref()
                                .map_or(self.profile.settings.music_volume, |m| m.volume),
                            mouse_sensitivity: self.profile.settings.mouse_sensitivity,
                            invert_y: self.profile.settings.invert_y,
                            cursor: self.pause_menu.cursor,
                            respawn_confirm: self.pause_menu.confirm,
                            // The disc's key; English fallback is §7's label.
                            respawn_label: (walk.sim.outcome.is_none()
                                && walk.sim.unit(walk.player).is_some())
                            .then(|| {
                                self.feed
                                    .screen_text(game::pause::RESPAWN_LABEL_KEY, "Respawn")
                            }),
                            in_shell: None,
                            outcome_line: walk.sim.outcome.map(|w| match w {
                                core::sim::OUTCOME_DRAW => "Time up — both teams win".to_string(),
                                _ => format!(
                                    "Round over — {} wins",
                                    walk.teams[usize::from(w.min(1))].faction
                                ),
                            }),
                        };
                        let input = ui::menu::MenuInput {
                            up: edges & 1 != 0,
                            down: edges & 2 != 0,
                            left: edges & 4 != 0,
                            right: edges & 8 != 0,
                            confirm: edges & 16 != 0,
                            back: edges & 32 != 0,
                        };
                        let (cursor, actions) = ui::menu::navigate(&model, input);
                        // The PS2 matcher runs beside the menu on the same
                        // edges (the cursor wiggle while typing is authentic).
                        if let Some(word) = self.morse.feed(input) {
                            let line = apply_cheat_word(
                                word,
                                walk,
                                &mut self.hud_hidden,
                                &mut self.sfx_slow,
                                self.sounds.as_mut(),
                            );
                            eprintln!("{line}");
                            self.feed
                                .say(line, ui::hud::message_color::YELLOW, walk.sim.tick);
                        }
                        self.pause_menu.cursor = cursor;
                        let mut quit = false;
                        for a in actions {
                            game::pause::apply_menu_action(
                                walk,
                                a,
                                &mut self.paused,
                                &mut self.pause_menu,
                            );
                            persist_setting(&mut self.profile, a, walk);
                            quit |= a == ui::menu::MenuAction::Quit;
                            self.kbm.follow(&self.profile.settings);
                            // The music follows its slider while paused.
                            if let (ui::menu::MenuAction::SetMusic(_), Some(m), Some(v)) =
                                (a, walk.music.as_ref(), self.music_voice.as_mut())
                            {
                                let level = m.level().map_or(0.0, |(_, l)| l);
                                v.set_volume(game::sounds::level_db(level), 0.1);
                            }
                        }
                        if quit {
                            self.quit_clicked = true;
                        }
                        menu = Some(ui::menu::MenuModel {
                            cursor: self.pause_menu.cursor,
                            respawn_confirm: self.pause_menu.confirm,
                            cheats: walk.sim.cheats.entries().to_vec(),
                            assist_intensity: walk.sim.assist_intensity,
                            ai_alive_cap: walk.sim.ai_alive_cap,
                            fov_scale: walk.fov_scale,
                            fov_deg: walk.fov_y().to_degrees(),
                            music_volume: walk
                                .music
                                .as_ref()
                                .map_or(self.profile.settings.music_volume, |m| m.volume),
                            mouse_sensitivity: self.profile.settings.mouse_sensitivity,
                            invert_y: self.profile.settings.invert_y,
                            ..model
                        });
                    } else {
                        // Each stepped tick's events are read in the hook,
                        // once: a frame steps no tick (every other one at
                        // 60 Hz), one or several, and `Sim::events` keeps
                        // only the last tick's (card EVTK).
                        self.bolts.age(walk, dt);
                        let aspect = self.renderer.as_ref().map_or(16.0 / 9.0, |r| {
                            let (w, h) = r.size();
                            w as f32 / h.max(1) as f32
                        });
                        // The class screen's unlocks read the player's
                        // points as the feed has scored them (card SSELB,
                        // re-spawn-flow §4.3).
                        let points = self.feed.stats(&walk.sim, walk.player).points;
                        walk.sim.set_spawn_points(0, points);
                        // Station serving is observed for every sim tick, even when a
                        // frame catches up several ticks before posing the scene.
                        if let Some(gpu) = self.gpu.as_mut() {
                            gpu.step_stations(&walk.sim);
                        }
                        let mut tick_pad = pad;
                        let ticks = walk.advance_with(dt, std::slice::from_ref(&pad), |walk| {
                            let hook_t0 = std::time::Instant::now();
                            if let Some(gpu) = self.gpu.as_mut() {
                                gpu.step_stations(&walk.sim);
                            }
                            self.fx.ingest(walk);
                            let effect_sounds = self.fx.take_sounds();
                            if let Some(vis) = self.soldier_vis.as_mut() {
                                vis.note_shots(walk);
                                self.bolts.set_fire_points(vis.fire_points(walk));
                            }
                            self.bolts.track(walk);
                            if let (Some(audio), Some(sl)) =
                                (self.audio.as_mut(), self.sounds.as_mut())
                            {
                                play_event_sounds(
                                    audio,
                                    sl,
                                    self.foley.as_mut(),
                                    &mut self.engines,
                                    &effect_sounds,
                                    aspect,
                                    walk,
                                );
                                play_ambience(audio, sl, &self.ambience, walk);
                            }
                            self.feed.tick(&walk.sim, walk.player);
                            if let Some(bb) = self.blackbox.as_mut() {
                                stuck_alarm |= bb.tick(walk, Some(tick_pad));
                                note_sounds(bb, self.sounds.as_mut(), walk.sim.tick);
                            }
                            if let Some(p) = self.frame_profile.as_mut() {
                                let hook = hook_t0.elapsed().as_secs_f64() * 1000.0;
                                p.tick(walk.sim.ai_wall_ms, hook);
                            }
                            tick_pad.clear_look();
                        });
                        self.kbm.consumed(&pad, ticks);
                        if walk.flush_class_kills(&mut self.profile) {
                            if let Err(e) = self.profile.save() {
                                eprintln!("profile autosave failed: {e:#}");
                            }
                        }
                        look = self.kbm.look();
                        if let Some(p) = self.frame_profile.as_mut() {
                            p.lap("sim_ms");
                        }
                        self.fx.update(dt);
                        if let Some(w) = self.weather.as_mut() {
                            w.update(dt);
                            rain_splashes(w, &mut self.fx, walk, aspect, dt);
                        }
                        if let Some(vis) = self.soldier_vis.as_mut() {
                            vis.update(walk, dt);
                        }
                        if let (Some(audio), Some(sl)) = (self.audio.as_mut(), self.sounds.as_mut())
                        {
                            // The commander: this frame's requests join the
                            // voice queue, and an idle voice starts its head.
                            // A squad answer plays the take drawn for it,
                            // at its unit.
                            let ears = game::event_sounds::listener(walk);
                            if let Some(v) = walk.voice.as_mut() {
                                let tick = walk.sim.tick;
                                // The units' combat calls through the
                                // bank's gates join the queue first.
                                v.gate_calls(sl, ears.pos);
                                v.frame(&walk.sim, walk.player, dt, |line| {
                                    if let Some(bb) = self.blackbox.as_mut() {
                                        bb.voice(tick, &line.name);
                                    }
                                    match (line.take, line.at) {
                                        (Some(take), Some(at)) => sl.play_take(
                                            audio,
                                            &line.name,
                                            usize::from(take),
                                            at,
                                            &ears,
                                        ),
                                        _ => sl.play_voice(audio, &line.name),
                                    }
                                });
                            }
                            if let Some(bb) = self.blackbox.as_mut() {
                                note_sounds(bb, Some(sl), walk.sim.tick);
                            }
                        }
                        // The music: the script's Musics by the
                        // reinforcements, one streamed take at a time. The
                        // frame runs with or without a sound device (card
                        // MUSV: the dev log is the window's referee); only
                        // the voice needs one.
                        if let (Some(m), Some(sl)) = (walk.music.as_mut(), self.sounds.as_ref()) {
                            if !m.bound() {
                                let missing = m.bind(sl);
                                if !missing.is_empty() {
                                    eprintln!("music: not in the bank: {missing:?}");
                                }
                            }
                            let db = game::sounds::level_db;
                            let tickets = [0, 1].map(|t| walk.sim.battle.teams[t].tickets());
                            for c in m.frame(&walk.sim, walk.player, dt) {
                                if game::dev::on() {
                                    eprintln!("{}", m.log_line(&c, tickets));
                                }
                                self.music_voice = match (c, self.audio.as_mut()) {
                                    (core::music::Change::Started { music, take }, Some(audio)) => {
                                        start_take(audio, m, music, take, db(0.0))
                                    }
                                    _ => None,
                                };
                            }
                            if let (Some(v), Some((_, level))) =
                                (self.music_voice.as_mut(), m.level())
                            {
                                v.set_volume(db(level), dt);
                            }
                        }
                    }
                    // The black box: this frame's time, and a snapshot when
                    // F9 was pressed or the stuck alarm fired — written on its
                    // own thread, with a line in the message box.
                    if let Some(bb) = self.blackbox.as_mut() {
                        let size = self.renderer.as_ref().map_or((0, 0), |r| r.size());
                        bb.frame(dt, [size.0, size.1]);
                        let reason = if std::mem::take(&mut self.snapshot_requested) {
                            Some("hotkey")
                        } else {
                            stuck_alarm.then_some("stuck")
                        };
                        if let Some(reason) = reason {
                            bb.write(reason, Some(walk), false);
                            self.feed.say(
                                format!(
                                    "snapshot saved ({reason}, {:.1} s)",
                                    walk.sim.tick as f32 * core::TICK_DT
                                ),
                                ui::hud::message_color::YELLOW,
                                walk.sim.tick,
                            );
                        }
                    }
                    if let Some(p) = self.frame_profile.as_mut() {
                        p.lap("update_ms");
                    }
                    // From here to the draw lists' end the walk holds the
                    // pose blended between its last two ticks (card RINT):
                    // the camera, units and vehicles drawn at the display's
                    // rate. Restored below, before anything else reads it.
                    game::render_blend::blend_in_looking(walk, &look);
                    let models = self
                        .soldier_vis
                        .as_ref()
                        .map_or([false; 2], |v| [v.has_model(0), v.has_model(1)]);
                    walk_draws(*cube, walk, &mut extra, models);
                    if let Some(r) = self.renderer.as_mut() {
                        extra.extend(self.vehicle_vis.draws(r, &walk.sim));
                    }
                    if let Some(vis) = self.soldier_vis.as_ref() {
                        skinned = vis.draws(walk);
                        extra.extend(vis.weapon_draws(walk));
                    }
                    self.frames += 1;
                    if self.frames.is_multiple_of(10) {
                        if let Some(w) = &self.window {
                            w.set_title(&hud_title(walk));
                        }
                    }
                    if walk.sim.outcome.is_some() && !self.outcome_reported {
                        self.outcome_reported = true;
                        println!("{}", hud_title(walk));
                    }
                    if walk.sim.outcome.is_some() && !self.round_recorded {
                        self.round_recorded = true;
                        record_round(&mut self.profile, walk, self.feed.battle_stats());
                    }
                    // The tick hook above has read the events; the model
                    // carries the new lines to the HUD's message box.
                    hud = Some(hud_model(walk, &mut self.feed));
                    // The capture icon is off under the pause menu, as the
                    // PS2's (card CPBAR, re-capture-display 3.4).
                    if let Some(m) = hud.as_mut().filter(|_| paused) {
                        m.capture = None;
                    }
                    // Spawn screen while dead: the post and class it shows
                    // are the sim's (card SPSEL), moved by the pad bits.
                    spawn = spawn_model(walk, &self.feed);
                    let camera = walk.camera();
                    if let (Some(p), Some(me)) =
                        (self.frame_profile.as_mut(), walk.sim.unit(walk.player))
                    {
                        p.drawn(camera.position, me.soldier.pos);
                    }
                    camera
                } else {
                    self.drive_fly_camera(dt);
                    self.cam
                };
                if let Some(quad) = self.fx_quad {
                    extra.extend(self.fx.draws(quad, self.fx_sprite, &camera));
                    if let Some((walk, _)) = &self.walk {
                        extra.extend(game::squad_vis::squad_draws(
                            &walk.sim,
                            0,
                            quad,
                            self.squad_icons.0,
                            self.squad_icons.1,
                            &camera,
                        ));
                    }
                }

                if let (Some(r), Some(gpu)) = (&mut self.renderer, &mut self.gpu) {
                    // The water moves on the renderer's clock (queue OC).
                    if let Err(e) = gpu.animate_water_live(r, camera.position) {
                        eprintln!("water: {e:#}");
                    }
                    // The drawn doors ride the sim's (queue DRV), the bolts
                    // the sim's bolts (card FXB).
                    match &self.walk {
                        Some((walk, _)) => {
                            gpu.pose_doors(&walk.sim.doors);
                            gpu.pose_turrets(&walk.sim, &walk.vehicle_seats);
                            gpu.pose_stations(&walk.sim);
                            self.holos.advance(walk);
                            game::point_light::submit(
                                r,
                                gpu,
                                walk,
                                &mut self.bolts,
                                camera.position,
                                &self.holos,
                            );
                            let flashes = self
                                .soldier_vis
                                .as_ref()
                                .map(|v| v.flash_draws(walk))
                                .unwrap_or_default();
                            let mut particles = flashes;
                            particles.extend(self.holos.draws(&gpu.holo, walk, &camera));
                            if let Some(w) = self.weather.as_mut() {
                                particles.extend(w.frame_draws(r, &camera, tracked_velocity(walk)));
                            }
                            self.fx.submit_with(r, &camera, particles);
                        }
                        None => {
                            r.set_bolts(Vec::new());
                            r.set_particles(Vec::new());
                            r.set_terrain_lights(Default::default());
                        }
                    }
                    // The player's own view: a zoomed turret gunner's
                    // building is left out of it (card VTURZ).
                    let mut draws: Vec<DrawCmd> = match &self.walk {
                        Some((walk, _)) => gpu.view_draws(&walk.sim, walk.player),
                        None => gpu.draws.clone(),
                    };
                    draws.append(&mut extra);
                    // The PS2's spawn screens (the HUD pass's, card SSELB):
                    // the soldier standing in the camera shot.
                    let page = spawn
                        .as_ref()
                        .and_then(|s| s.page.as_ref())
                        .filter(|_| self.hud_view.is_some() && !self.hud_hidden);
                    if let Some(vis) = self.soldier_vis.as_ref() {
                        let step = if self.paused { 0.0 } else { dt };
                        let (body, gun) = self.spawn_preview.draws(vis, page, &camera, step);
                        skinned.extend(body);
                        draws.extend(gun);
                    }
                    // The tick's own pose back (card RINT): the menu's
                    // actions and the next tick read it.
                    if let Some((walk, _)) = self.walk.as_mut() {
                        walk.blend_out();
                    }
                    let hud_hidden = self.hud_hidden;
                    let pad_warning = self.pad_warning.clone();
                    // BF2's own HUD under the egui lines and the spawn
                    // screen (dead, it is the counts and the message box:
                    // the line that says who killed you); the `hud` cheat
                    // hides it. It is fed either way — a model's event
                    // lines are handed over once. Paused, its fades hold.
                    let (w, h) = r.size();
                    let hud_dt = if self.paused { 0.0 } else { dt };
                    let mut hud_view = match (&mut self.hud_view, &hud) {
                        (Some(hv), Some(model)) => {
                            hv.update(model, hud_dt, [w, h]);
                            (!hud_hidden).then_some(hv)
                        }
                        _ => None,
                    };
                    if let Some(p) = self.frame_profile.as_mut() {
                        p.lap("draws_ms");
                    }
                    let result = match (&mut self.overlay, &self.window, hud) {
                        (Some(overlay), Some(window), Some(model)) => {
                            let mut clicked: Vec<ui::menu::MenuAction> = Vec::new();
                            let paint = overlay.begin(window, |ctx| {
                                if !hud_hidden {
                                    // The spawn screen owns the death message.
                                    let m = match &spawn {
                                        Some(_) => ui::hud::HudModel {
                                            dead: false,
                                            ..model.clone()
                                        },
                                        None => model.clone(),
                                    };
                                    ui::hud::draw_hud(ctx, &m);
                                    if let Some(s) = &spawn {
                                        // The PS2's screen is the HUD pass's;
                                        // without one the panel stands in.
                                        match hud_view.is_some() {
                                            true => ui::spawn::draw_spawn(ctx, s),
                                            false => ui::spawn::draw_spawn(
                                                ctx,
                                                &ui::spawn::SpawnModel {
                                                    page: None,
                                                    ..s.clone()
                                                },
                                            ),
                                        }
                                    }
                                }
                                if let Some(m) = &menu {
                                    clicked = ui::menu::draw_menu(ctx, m);
                                }
                                if let Some(w) = &pad_warning {
                                    ui::hud::draw_pad_warning(ctx, w);
                                }
                            });
                            let (n, bytes) = paint.texture_uploads();
                            if n > 0 {
                                render::made::note("egui texture", Some("egui"), bytes, 0.0);
                            }
                            if let Some((walk, _)) = self.walk.as_mut() {
                                for a in clicked {
                                    game::pause::apply_menu_action(
                                        walk,
                                        a,
                                        &mut self.paused,
                                        &mut self.pause_menu,
                                    );
                                    persist_setting(&mut self.profile, a, walk);
                                    self.kbm.follow(&self.profile.settings);
                                    if a == ui::menu::MenuAction::Quit {
                                        self.quit_clicked = true;
                                    }
                                }
                            }
                            r.render_frame(
                                &camera,
                                &draws,
                                &skinned,
                                clear,
                                Some(&mut |dev, q, enc, view, size| {
                                    if let Some(hv) = hud_view.as_mut() {
                                        hv.paint(dev, q, enc, view, size);
                                    }
                                    overlay.paint(&paint, dev, q, enc, view, size)
                                }),
                            )
                        }
                        _ => r.render_frame(&camera, &draws, &skinned, clear, None),
                    };
                    if let Some(p) = self.frame_profile.as_mut() {
                        p.lap("render_ms");
                        let tick = self.walk.as_ref().map_or(0, |w| w.0.sim.tick);
                        p.end(f64::from(dt) * 1000.0, tick, r);
                    }
                    if let Err(e) = result {
                        eprintln!("render: {e}");
                        event_loop.exit();
                    }
                }
                // Without a renderer the draw window above is skipped.
                if let Some((walk, _)) = self.walk.as_mut() {
                    walk.blend_out();
                }
                if let (Some(at), Some((walk, _))) = (self.quit_at, self.walk.as_ref()) {
                    self.quit_clicked |= walk.sim.tick >= at;
                }
                if self.quit_clicked {
                    self.quit_clicked = false;
                    if self.map.is_some() {
                        self.blackbox_exit();
                        event_loop.exit();
                    } else {
                        self.leave_battle();
                    }
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(cap: Option<u32>) -> BattleSetup {
        BattleSetup {
            seed: 42,
            difficulty: Default::default(),
            spawn: (0.0, 0.0),
            spawn_set: false,
            targets: false,
            ai: None,
            ai_cap: cap,
            tickets: 250,
            tickets_set: false,
            ticket_pct: 100,
            time_limit: 0,
            cheats: Default::default(),
            teams: None,
            classes_dir: PathBuf::new(),
            mission: None,
            mission_file: None,
            player_team: 0,
        }
    }

    #[test]
    fn direct_settings_resolve_before_battle_construction() {
        use core::skill::Difficulty::{Easy, Elite};
        let settings = game::profile::Settings {
            difficulty: Elite,
            ai_alive_cap: Some(16),
            ..Default::default()
        };
        let mut battle = setup(Some(64));
        resolve_direct_settings(&mut battle, None, &settings);
        assert_eq!(battle.difficulty, Elite);
        assert_eq!(battle.ai_cap, Some(64));
        let mut battle = setup(None);
        resolve_direct_settings(&mut battle, Some(Easy), &settings);
        assert_eq!(battle.difficulty, Easy);
        assert_eq!(battle.ai_cap, Some(16));
        let mut battle = setup(None);
        resolve_direct_settings(&mut battle, None, &game::profile::Settings::default());
        assert_eq!(battle.difficulty, core::skill::Difficulty::default());
        assert_eq!(battle.ai_cap, None, "leave the mission's cap available");
    }

    #[test]
    fn gameplay_stick_holds_through_wobble_and_resets_on_controller_changes() {
        let mut latch = GameplayStick::<u8>::default();
        latch.select(Some(1));
        let mut previous = 0;
        let mut edges = 0;
        for y in [0.61, 0.59, 0.61, 0.34, 0.61] {
            latch.arms = input::stick_arms(0.0, y, latch.arms);
            edges += (latch.arms & !previous != 0) as u32;
            previous = latch.arms;
        }
        assert_eq!(edges, 2, "one initial edge, then one after release");
        latch.select(Some(1));
        assert_eq!(latch.arms, 1, "same controller retains the held arm");
        latch.select(Some(2));
        assert_eq!(latch.arms, 0);
        latch.arms = input::stick_arms(0.0, 0.59, latch.arms);
        assert_eq!(latch.arms, 0, "new controller must reach the on threshold");
        latch.arms = 1;
        latch.select(None);
        latch.select(Some(2));
        assert_eq!(input::stick_arms(0.0, 0.59, latch.arms), 0);
    }
}
