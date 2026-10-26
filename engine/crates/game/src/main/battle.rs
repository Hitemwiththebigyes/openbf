//! The battle every mode builds and shows: a baked map loaded and uploaded
//! with its common assets ([`load_and_upload`]), the two teams from the class
//! data, the `WalkSim` set up from the CLI or the shell ([`make_walk`]), and
//! what a frame draws of it besides the scene — the marker, target, bolt and
//! post boxes ([`walk_draws`]), the squad icons, and the HUD and spawn-screen
//! models the `ui` crate paints.

use super::BattleSetup;
use anyhow::Result;
use game::viewer::{upload_scene, SceneGpu};
use game::walk::WalkSim;
use render::{DrawCmd, MeshHandle, Renderer};

/// Walk-mode soldier marker tint (orange — reads on sand and rock).
const MARKER_COLOR: [f32; 4] = [0.9, 0.45, 0.1, 1.0];

pub(super) fn load_and_upload(
    r: &mut Renderer,
    dir: &std::path::Path,
    mode: &str,
) -> Result<(SceneGpu, game::assets::Scene)> {
    let (scene, load) = game::assets::load_battle_scene(dir, mode)?;
    if let Some(layer) = &load.layer {
        println!("mode layer '{layer}' merged");
    }
    match &load.common {
        Some((cd, Ok(n))) => println!("common assets merged from {} (+{n} models)", cd.display()),
        Some((cd, Err(e))) => eprintln!("common assets at {} unreadable: {e:#}", cd.display()),
        None => {}
    }
    let gpu = upload_scene(r, &scene)?;
    println!(
        "map '{}': {} draws ({} instances, {} skipped classes)",
        scene.manifest.map,
        gpu.draws.len(),
        scene.manifest.instances.len(),
        gpu.skipped.len()
    );
    for (class, n) in &gpu.skipped {
        println!("  skipped {n:3}x {class} (no drawable model)");
    }
    Ok((gpu, scene))
}

/// The two squad-command icons (brief D11), both out of `common_bf1`: BF2
/// ships neither, so a BF2 map shows the BF1 icon — `PROVISIONAL (ours)` —
/// and a bake without them shows nothing at all.
///
/// **OFF in normal play** (card SQMK): loaded only when
/// `game::squad_vis::markers_on_env` says so — dev mode (on by default; `OPENBF_DEV=0` is off), or
/// `OPENBF_SQUAD_MARKER=1` for this overlay alone (`=0` hides it in dev mode).
/// The icon's shape is its disc alpha (`analysis/re-hud-icons.md` §2 [V] —
/// 1,594 of `squad_cmd_highlight`'s 4,096 texels at 0), which the bake keeps.
///
/// The PLACEMENT is the recorded PROVISIONAL deviation: the PS2 draws these
/// two sheets as its 2D D-pad command MENU at HUD (−120, −50) — the cross at
/// the centre, one highlight arm rotated four ways around it (§2 "Where") —
/// and never over a bot in the world. PLAN §DESIGN SPECS Phase D decision 3
/// named the sheets, not that placement; the in-world marker is ours.
pub(super) fn squad_icons(
    r: &mut render::Renderer,
    assets_dir: &std::path::Path,
) -> (Option<render::TextureHandle>, Option<render::TextureHandle>) {
    if !game::squad_vis::markers_on_env() {
        return (None, None);
    }
    let dir = assets_dir.join("common_bf1").join("textures");
    let load = |r: &mut render::Renderer, n: &str| {
        game::viewer::upload_texture_file(r, &dir, n).ok().flatten()
    };
    (load(r, "squad_cmd_highlight"), load(r, "squad_cmd_cross"))
}

fn marker_draw(cube: MeshHandle, walk: &WalkSim) -> DrawCmd {
    DrawCmd::flat(cube, MARKER_COLOR, walk.marker_transform())
}

/// Soldier marker + enemy boxes (magnet target highlighted red, health
/// darkens the box) + in-flight bolts (team-tinted).
pub fn walk_draws(cube: MeshHandle, walk: &WalkSim, draws: &mut Vec<DrawCmd>, models: [bool; 2]) {
    let player_team = walk.sim.units[walk.player.index as usize].team;
    if walk.player_on_foot()
        && !walk.player_body_hidden()
        && !models[usize::from(player_team.min(1))]
    {
        draws.push(marker_draw(cube, walk));
    }
    for t in walk.target_draws() {
        if models[usize::from(t.team.min(1))] {
            continue; // drawn as an animated model instead
        }
        let base = if t.magnet {
            [0.85, 0.15, 0.15]
        } else {
            [0.35, 0.35, 0.4]
        };
        let k = 0.4 + 0.6 * t.health_frac;
        draws.push(DrawCmd::flat(
            cube,
            [base[0] * k, base[1] * k, base[2] * k, 1.0],
            t.transform,
        ));
    }
    for (m, team) in walk.bolt_draws() {
        let color = if team == 0 {
            [1.0, 0.25, 0.1, 1.0]
        } else {
            [0.3, 1.0, 0.3, 1.0]
        };
        draws.push(DrawCmd::flat(cube, color, m));
    }
}

/// Team templates from the baked class data (factions default to the map's
/// source game); falls back to the built-in BF1 trooper with a warning.
fn team_templates(
    scene: &game::assets::Scene,
    b: &BattleSetup,
    sim: &mut core::sim::Sim,
) -> [game::walk::TeamTemplate; 2] {
    let defaults = game::roster::default_factions(&scene.manifest.source_game);
    let factions: [&str; 2] = match &b.teams {
        Some(t) => [t[0].as_str(), t[1].as_str()],
        None => defaults,
    };
    match game::roster::FactionRoster::pair(
        &b.classes_dir,
        &scene.manifest.source_game,
        factions,
        b.mission.as_ref(),
    ) {
        Ok((rosters, warnings)) => {
            for w in &warnings {
                eprintln!("{w}");
            }
            // One `Sim::weapon_classes` index per weapon NAME (a Vec scan —
            // the sim never iterates a hash map), and the per-team class
            // TABLE the AI mix and the spawn-select both read.
            let mut names: Vec<(String, usize)> = Vec::new();
            let mut team = 0usize;
            rosters.map(|r| {
                let mut table = Vec::new();
                let mut visuals = Vec::new();
                for c in &r.classes {
                    let uc = c.register(sim, &mut names);
                    println!(
                        "team {} class {}: {} — min {} — {} HP, {:.1} m/s, {} ({} dmg, {} m/s bolts){}",
                        r.faction,
                        c.class,
                        c.label,
                        c.min,
                        c.max_health,
                        c.consts.max_speed,
                        c.weapon.name,
                        c.weapon.ordnance.max_damage,
                        c.weapon.ordnance.velocity,
                        if c.jet { "  [jet pack]" } else { "" }
                    );
                    table.push(uc);
                    let mut visual = game::walk::ClassVisual::from_roster(c);
                    visual.held_hud = c.held.iter().map(|h| (h.slot, h.hud.clone())).collect();
                    visual.held_models = c.held.iter().map(|h| (h.slot, h.geometry.clone())).collect();
                    visuals.push(visual);
                }
                sim.classes[team] = table;
                team += 1;
                game::walk::TeamTemplate::from_roster(&r, &sim.classes[team - 1][0], visuals)
            })
        }
        Err(e) => {
            eprintln!("class data unavailable ({e:#}); using built-in BF1 troopers");
            let t = game::walk::TeamTemplate::default_trooper();
            [t.clone(), t]
        }
    }
}

/// Card SSELA: the shell battle's player starts behind the spawn screens -
/// off the field until it accepts a side, a post and a class, and back after
/// a death only through the spawn map and the class accept
/// (`Sim::open_spawn_screens`). Every shell battle is Instant Action, where
/// the PS2 offers the side select ("can switch sides", re-spawn-flow §1), so
/// the screens open on it; the side the player accepts is the player's team
/// (`mission::pick`'s `player_team` only orders the armies). The CLI walk,
/// the headless tools and the golden scenes keep the auto-accept spawn;
/// `OPENBF_SPAWN_SCREENS=0` turns the screens off here too. The screens
/// stand over the mission's camera shots until the first spawn (card SSELB,
/// `game::spawn_backdrop`).
pub(super) fn open_spawn_screens(walk: &mut WalkSim, mission: Option<&game::mission::MissionDef>) {
    if std::env::var("OPENBF_SPAWN_SCREENS").is_ok_and(|v| v == "0") {
        return;
    }
    if !walk.sim.open_spawn_screens(0, true) {
        eprintln!("spawn screens: slot 0 holds no live unit; the respawn stays automatic");
        return;
    }
    walk.backdrop = mission.and_then(|m| {
        game::spawn_backdrop::Backdrop::new(m.camera_shots(), game::spawn_backdrop::LCG_RESEED)
    });
    walk.follow_backdrop();
}

/// Build the walk/battle sim for a loaded scene per the CLI flags.
pub(super) fn make_walk(scene: &game::assets::Scene, b: &BattleSetup) -> WalkSim {
    let mut sim = core::sim::Sim::empty(b.seed);
    // The tier is set BEFORE `setup_conquest` spawns anyone: every bot takes
    // its skill at spawn (D2).
    sim.difficulty = b.difficulty;
    // BF1 has no 10 m omni in the view cone (RE §9.1); BF2 does.
    sim.omni_radius = core::sim::omni_radius_for(&scene.manifest.source_game);
    sim.water_level = scene.manifest.water.as_ref().map(|w| w.height);
    sim.ground_rules = core::soldier::GroundRules::for_game(&scene.manifest.source_game);
    sim.item_pool = game::roster::item_pool(&scene.manifest.source_game, &scene.manifest.map);
    sim.cheats = b.cheats;
    sim.battle = core::battle::Battle::new(match &b.mission {
        _ if b.tickets_set => [b.tickets, b.tickets],
        Some(m) => m
            .tickets()
            .map(|n| core::sim::scale_reinforcements(n, b.ticket_pct)),
        None => [core::sim::scale_reinforcements(b.tickets, b.ticket_pct); 2],
    });
    sim.time_limit = (b.time_limit > 0).then(|| b.time_limit * 60);
    if let Some(m) = &b.mission {
        sim.ai_alive_cap = m.ai_alive_cap;
        m.apply_death_drains(&mut sim.battle);
    }
    // --ai-cap wins over the mission, and applies with or without one.
    if let Some(cap) = b.ai_cap {
        sim.ai_alive_cap = cap;
    }
    let teams = team_templates(scene, b, &mut sim);
    let seat = usize::from(b.player_team.min(1));
    let mut walk = WalkSim::with_teams_seat(
        sim,
        scene.heightfield(),
        glam::Vec3::new(b.spawn.0, 0.0, b.spawn.1),
        teams,
        game::layout::nav_graph(&scene.manifest, &scene.dir),
        b.player_team,
    );
    // Model collision: the scene has already had its mode layer applied (see
    // `Scene::collision_world` — building it earlier misses every BF2 conquest
    // instance).
    scene.install_collision(&mut walk.sim);
    walk.camera_blockers = game::camera_collide::CameraBlockers::from_scene(scene);
    walk.fov_y_ps2 = game::walk::ps2_fov_y(&scene.manifest.source_game);
    if b.targets {
        walk.spawn_dummy_targets();
    }
    // --mission implies a conquest setup: AI counts from the mission unless
    // --ai overrides them.
    let ai_counts: Option<[u32; 2]> = match (b.ai, &b.mission) {
        (Some(n), _) => Some([n, n]),
        (None, Some(m)) => Some([m.initial_ai(0, seat == 0), m.initial_ai(1, seat == 1)]),
        (None, None) => None,
    };
    let mut vehicle_classes: Vec<String> = Vec::new();
    let mut layout = game::layout::MapLayout::from_manifest(&scene.manifest);
    // The mission owns the command-post roster, the posts it kills, the
    // ownership it re-authors and its death regions (game::layout).
    if let Some(m) = &b.mission {
        let a = layout.apply_mission(m);
        walk.sim.death_regions = a.death_regions.clone();
        let blocked = m.apply_arc_blocks(walk.sim.nav.as_mut());
        if blocked != 0 {
            println!("mission: route groups {blocked:#b} blocked (plnp)");
        }
        let switched = m.apply_barriers(walk.sim.nav.as_mut());
        if switched != 0 {
            println!("mission: {switched} barriers switched (Enable/DisableBarriers)");
        }
        println!(
            "mission: {} posts (roster dropped {}, killed {}), {} re-owned, {} death regions{}",
            a.kept,
            a.dropped_by_roster,
            a.killed,
            a.reassigned,
            a.death_regions.len(),
            if a.missing_death_regions.is_empty() {
                String::new()
            } else {
                format!(" ({} missing)", a.missing_death_regions.len())
            }
        );
    }
    if let Some(counts) = ai_counts {
        walk.add_posts(&layout);
        // The vehicle spawn pads follow the posts just added (card VPAD) and
        // go in before the opening wave, which spawns at the command walkers
        // they are about to build (card CWSP).
        let factions = [walk.teams[0].faction.clone(), walk.teams[1].faction.clone()];
        match game::vehicle_pads::install(
            &mut walk.sim,
            &scene.manifest,
            &scene.dir,
            &b.classes_dir,
            &factions,
        ) {
            Ok(p) => {
                println!(
                    "vehicle pads: {} ({} classes){}",
                    p.pads.len(),
                    p.classes.len(),
                    if p.absent.is_empty() {
                        String::new()
                    } else {
                        format!(", no class for {:?}", p.absent)
                    }
                );
                walk.vehicle_geometry = p.geometry;
                walk.vehicle_seats = p.seats;
                walk.vehicle_riders = p.riders;
                walk.vehicle_engines = p.engines;
                walk.vehicle_labels = p.labels;
                walk.vehicle_classes = p.classes.clone();
                vehicle_classes = p.classes;
            }
            Err(e) => eprintln!("vehicle pads unavailable ({e:#})"),
        }
        walk.opening_wave(counts, !b.spawn_set);
        // The commander speaks the mission's lines over the posts and the
        // battle just set up.
        if let Some(m) = &b.mission {
            walk.sound_levels = m.sound_levels();
            walk.voice = Some(game::announce::Commander::for_battle(m, &layout, &walk));
            // ...and the music is the script's too, by the reinforcements
            // the teams start with.
            let tickets = [0, 1].map(|t| walk.sim.battle.teams[t].tickets());
            let bf2 = m.source_game == "bf2";
            walk.music = Some(game::music::BattleMusic::new(&m.music, bf2, tickets));
        }
        println!(
            "conquest: {} posts, the player on team {} ({}), AI {}/{}, tickets {}/{}, alive-AI cap {}, difficulty {}, omni {:.0} m",
            layout.cps.len(),
            seat + 1,
            walk.teams[seat].faction,
            counts[0],
            counts[1],
            walk.sim.battle.teams[0].tickets(),
            walk.sim.battle.teams[1].tickets(),
            walk.sim.ai_alive_cap,
            walk.sim.difficulty.label(),
            walk.sim.omni_radius,
        );
    } else if !b.spawn_set {
        // No conquest setup and no pinned spawn: the player is standing at
        // the authored default, which is off the walkable surface on seven
        // BF2 maps (`WalkSim::rescue_spawn`).
        if let Some((post, y)) = walk.rescue_spawn(&layout) {
            println!(
                "spawn: no ground at the authored point — moved to command post {post} (y {y:.2})"
            );
        }
    }
    walk.minimap = Some(game::minimap::MapStatic::load(
        scene,
        &b.classes_dir,
        &walk,
        &vehicle_classes,
        b.mission.as_ref().and_then(|m| m.era.as_deref()),
    ));
    walk
}

/// BF2's HUD from the bake at `assets_dir`, painted into `r`'s frames;
/// `None` — said once — when the bake holds none.
pub(super) fn load_hud_view(
    assets_dir: &std::path::Path,
    r: &render::Renderer,
) -> Option<game::hud_feed::HudView> {
    match game::hud_feed::HudView::for_renderer(assets_dir, r) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("hud: no BF2 HUD in the bake ({e:#}); bars and counts not drawn");
            None
        }
    }
}

/// HUD model from the battle state. Takes `feed`'s new event lines: each
/// model must reach the HUD (`HudView::update`) or its lines are lost.
pub(super) fn hud_model(
    walk: &WalkSim,
    feed: &mut game::event_feed::EventFeed,
) -> ui::hud::HudModel {
    let me = &walk.sim.units[walk.player.index as usize];
    let b = &walk.sim.battle;
    let on_foot = me.alive && me.mount.is_none();
    let spawn = spawn_model(walk, feed).and_then(|s| s.page);
    let slot = me.spec.player_slot.map_or(0, usize::from);
    let weapon = on_foot.then(|| {
        let class = walk.sim.weapon_classes.get(me.weapon_class);
        let rounds = class.map_or(0, |c| c.rounds_per_clip);
        // The weapon in the hands: the primary, or the channel-0 slot the
        // player switched to (GREN).
        let hud = walk.teams[usize::from(me.team.min(1))]
            .class(me.spec.class)
            .hud_of(me.weapon_slot);
        ui::hud::HudWeapon {
            key: hud.key.clone(),
            class: hud.class.clone(),
            icon: hud.icon.clone(),
            // The localised weapon name waits for the Locl tables.
            name: String::new(),
            total: class.and_then(|c| me.weapon.rounds(c)),
            clip_fraction: if rounds > 0 {
                me.weapon.clip as f32 / rounds as f32
            } else {
                1.0
            },
        }
    });
    // The secondary channel's current weapon: the grenade row (GREN).
    let weapon2 = on_foot
        .then(|| me.cur1.and_then(|k| me.held.get(usize::from(k))))
        .flatten()
        .and_then(|w| {
            let class = walk.sim.weapon_classes.get(w.class)?;
            let hud = walk.teams[usize::from(me.team.min(1))]
                .class(me.spec.class)
                .hud_of(w.slot);
            Some(ui::hud::HudWeapon {
                key: hud.key.clone(),
                class: hud.class.clone(),
                icon: hud.icon.clone(),
                name: String::new(),
                total: w.state.rounds(class),
                clip_fraction: if class.rounds_per_clip > 0 {
                    w.state.clip as f32 / class.rounds_per_clip as f32
                } else {
                    1.0
                },
            })
        });
    let reticle = on_foot.then(|| {
        walk.sim
            .reticle_pick
            .first()
            .copied()
            .flatten()
            .map_or(0, |(_, rel)| rel)
    });
    ui::hud::HudModel {
        team_names: [walk.teams[0].faction.clone(), walk.teams[1].faction.clone()],
        tickets: [b.teams[0].tickets(), b.teams[1].tickets()],
        bleed: [b.teams[0].bleed_per_sec, b.teams[1].bleed_per_sec],
        hp: me.health.cur,
        hp_max: me.health.max,
        energy: me.soldier.energy,
        energy_max: me.consts.energy_bar,
        weapon,
        weapon2,
        reticle,
        // The reticle's pick by name and health (card HTGT).
        target: on_foot.then(|| feed.target(&walk.sim, slot)).flatten(),
        reticle_y: walk.reticle_y(),
        dead: !me.alive,
        // A human waits for no spawn wave (card SSELA): its timer only.
        respawn_in: (!me.alive).then(|| {
            me.respawn_at.map_or(0.0, |t| {
                t.saturating_sub(walk.sim.tick) as f32 / core::TICK_HZ as f32
            })
        }),
        outcome: walk.sim.outcome,
        time_left: walk.sim.time_limit.map(|l| {
            (u64::from(l) * u64::from(core::TICK_HZ)).saturating_sub(walk.sim.tick) as f32
                / core::TICK_HZ as f32
        }),
        messages: feed.take(),
        ambush: feed.take_ambush(),
        stats: Some(feed.stats(&walk.sim, walk.player)),
        player_team: me.team,
        map: walk.minimap.as_ref().map(|m| match &spawn {
            // The spawn map page shows the Map element's Spawn view.
            Some(ui::spawn_draw::SpawnPage::Map { .. }) => m.spawn_model(walk, slot),
            _ => m.model(walk),
        }),
        squad: feed.squad(walk.sim.tick),
        // The spawn screen up, drawn in the HUD's frame (card SSELB).
        spawn,
        capture: game::capture_feed::for_unit(
            &walk.sim,
            walk.player.index as usize,
            game::capture_feed::Rules::PORT,
        ),
    }
}

/// Spawn screen model while the player is dead and the round is live; None
/// otherwise. The post list is the sim's ([`core::sim::Sim::player_spawn_posts`]:
/// the team's posts, then its live command walkers) and the selection is the
/// sim's spawn-map pick for this player's slot (card SPSEL), as is the class
/// row; the class names come from the disc's text through `feed`.
pub(super) fn spawn_model(
    walk: &WalkSim,
    feed: &game::event_feed::EventFeed,
) -> Option<ui::spawn::SpawnModel> {
    let me = &walk.sim.units[walk.player.index as usize];
    if me.alive || walk.sim.outcome.is_some() {
        return None;
    }
    let b = &walk.sim.battle;
    let slot = me.spec.player_slot.map_or(0, usize::from);
    let list = walk.sim.player_spawn_posts(me.team);
    let selected = walk
        .sim
        .spawn_post_selected(slot)
        .and_then(|p| list.iter().position(|q| q.0 == p));
    // A walker's post is named by its class `Label` (card CWSP,
    // re-command-walker-post §5); a command post by its own name.
    let posts: Vec<ui::spawn::SpawnPost> = list
        .iter()
        .map(|&(post, pos)| match post {
            core::unit::SpawnPost::Post(i) => {
                let c = &walk.sim.cps[usize::from(i)];
                ui::spawn::SpawnPost {
                    name: feed
                        .post_names()
                        .get(usize::from(i))
                        .cloned()
                        .unwrap_or_else(|| c.def.name.clone()),
                    // Enemies are neutralizing it right now.
                    contested: c.neutral > 0.0,
                    distance: (pos - me.soldier.pos).length(),
                }
            }
            core::unit::SpawnPost::Walker(h) => ui::spawn::SpawnPost {
                name: walk
                    .sim
                    .vehicle(h)
                    .and_then(|v| walk.vehicle_labels.get(v.class as usize))
                    .and_then(Clone::clone)
                    .unwrap_or_else(|| "Command walker".to_owned()),
                contested: false,
                distance: (pos - me.soldier.pos).length(),
            },
        })
        .collect();
    // A human waits for no spawn wave (card SSELA): its timer only.
    let countdown = {
        let secs = me.respawn_at.map_or(0.0, |t| {
            t.saturating_sub(walk.sim.tick) as f32 / core::TICK_HZ as f32
        });
        (secs > 0.0).then_some(secs)
    };
    // The sim's spawn screen for this slot (card SSELA), when it runs them.
    let screen = walk.sim.spawn_screens.then(|| {
        use core::sim::{SideChoice, SpawnStage};
        match walk.sim.spawn_stage.get(slot).copied().unwrap_or_default() {
            SpawnStage::Side => ui::spawn::SpawnScreen::Side {
                rows: [
                    walk.teams[0].faction.clone(),
                    walk.teams[1].faction.clone(),
                    "Auto Assign".to_owned(),
                ],
                selected: match walk.sim.side_choice.get(slot).copied().unwrap_or_default() {
                    SideChoice::Team1 => 0,
                    SideChoice::Team2 => 1,
                    SideChoice::Auto => 2,
                },
            },
            SpawnStage::Map => ui::spawn::SpawnScreen::Map,
            SpawnStage::Class => ui::spawn::SpawnScreen::Class,
            SpawnStage::Accepted => ui::spawn::SpawnScreen::Waiting,
        }
    });
    let page = game::spawn_page::page(walk, feed, slot);
    // The class row comes from the SIM: the table's order and the selection
    // `Sim::spawn_choice` for this player's slot, which the pad moves and the
    // respawn reads; each name is the disc's (`EventFeed::class_names`), else
    // the roster's label.
    let team = &walk.teams[usize::from(me.team.min(1))];
    let has_table = !walk.sim.classes[usize::from(me.team.min(1))].is_empty();
    let names = feed.class_names(me.team);
    Some(ui::spawn::SpawnModel {
        team_names: [walk.teams[0].faction.clone(), walk.teams[1].faction.clone()],
        player_team: me.team,
        tickets: [b.teams[0].tickets(), b.teams[1].tickets()],
        posts,
        selected,
        classes: if !has_table {
            Vec::new()
        } else if names.len() == team.classes.len() {
            names.to_vec()
        } else {
            team.classes.iter().map(|c| c.label.clone()).collect()
        },
        selected_class: walk
            .sim
            .spawn_choice
            .get(slot)
            .map_or(0, |c| usize::from(*c)),
        countdown,
        screen,
        page,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HTGTW: use the battle's actual setup/install and feed constructors.
    /// The authored building is off its pivot: a radius-zero fallback cannot
    /// stand in for the collision bounds installed for acquisition.
    #[test]
    fn htgtw_battle_installs_turret_bounds_and_readout_names() {
        use core::sim::{AimTarget, PadState};
        let dir = std::env::temp_dir().join(format!("openbf-htgtw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No models or textures: this fixture exercises the setup, not GPU upload.
        std::fs::write(
            dir.join("manifest.ron"),
            r#"(
            map: "htgtw", source_game: "bf2", terrain_name: None,
            sky_name: None, terrain_glb: None, models: [], textures: [],
            classes: [(name: "fixture_dish", base: "armedbuilding", geometry: Some("dish"))],
            geometries: [], regions: [],
            instances: [(class: "fixture_dish", name: "dish1",
                rotation: ((1,0,0),(0,1,0),(0,0,1)),
                translation: (0,0,-15), props: [])],
            collision: [(geometry: "dish", glb: None, prims: [(
                name: "dish_body", kind: "box", dims: (2,2,2),
                rotation: ((1,0,0),(0,1,0),(0,0,1)), translation: (1.5,1.5,0)
            )])]
        )"#,
        )
        .unwrap();
        let props = [
            ("BuildingSection", "TURRET1"),
            ("WeaponName", "fixture_gun"),
            ("MaxHealth", "300"),
            ("Label", "Fixture Dish"),
        ];
        let defs = vec![game::classdb::ClassDef {
            name: "fixture_dish".into(),
            kind: "entc".into(),
            base: "armedbuilding".into(),
            props: props
                .into_iter()
                .map(|(k, v)| (mungefs::fnv1a_lowercase(k), v.into()))
                .collect(),
        }];
        std::fs::write(dir.join("classes.ron"), ron::to_string(&defs).unwrap()).unwrap();
        let mut scene = game::assets::load_scene(&dir).unwrap();
        // A flat synthetic terrain grid grounds the player through make_walk.
        scene.terrain = Some(game::assets::CpuMesh {
            positions: vec![
                [-32.0, 0.0, -32.0],
                [32.0, 0.0, -32.0],
                [-32.0, 0.0, 32.0],
                [32.0, 0.0, 32.0],
            ],
            normals: Vec::new(),
            uvs: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
            submeshes: Vec::new(),
            spec: Vec::new(),
            light: Vec::new(),
            sphere: None,
        });
        let setup = BattleSetup {
            seed: 42,
            difficulty: Default::default(),
            spawn: (0.0, 0.0),
            spawn_set: true,
            targets: false,
            ai: Some(0),
            ai_cap: None,
            tickets: 250,
            tickets_set: false,
            ticket_pct: 100,
            time_limit: 0,
            cheats: Default::default(),
            teams: None,
            classes_dir: dir.join("absent_sides"),
            mission: None,
            mission_file: None,
            player_team: 0,
        };
        let mut walk = make_walk(&scene, &setup);
        assert_eq!(walk.vehicle_classes, ["fixture_dish"]);
        let (h, v) = walk.sim.alive_vehicles().next().expect("installed turret");
        assert_eq!(v.instance, Some(0));
        let (centre, radius) = v.aim_sphere.expect("building sphere stored by install");
        // The fixture box is [-0.5,3.5] on x/y and [-2,2] on z.
        assert!(centre.distance(glam::Vec3::new(1.5, 1.5, 0.0)) < 1e-5);
        assert!((radius - 12.0_f32.sqrt()).abs() < 1e-5);
        let blast = walk.sim.blast_setup[v.class as usize].as_ref().unwrap();
        assert_eq!(blast.sphere_radius, 0.0, "no side-baked blast bounds");
        let player = &mut walk.sim.units[walk.player.index as usize];
        player.soldier.pitch = 0.0;
        player.soldier.yaw = 0.0;
        walk.sim.step(&[PadState::default()]);
        assert_eq!(
            walk.sim.reticle_pick[0],
            Some((AimTarget::Vehicle(h), 0)),
            "picked from 15 m"
        );
        let feed = game::event_feed::EventFeed::for_walk(&dir, "bf2", &walk);
        let target = feed.target(&walk.sim, 0).expect("picked turret readout");
        assert_eq!(target.name, "Fixture Dish");
        assert_eq!(target.health_fraction, 1.0);
        walk.sim.vehicles[h.index as usize].health.cur = 75.0;
        assert_eq!(feed.target(&walk.sim, 0).unwrap().health_fraction, 0.25);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
