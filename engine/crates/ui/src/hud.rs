//! The HUD's model and its two halves (card BHUD2): [`feed`] publishes the
//! model as the events BF2's own HUD tree listens to (`crate::hud_tree`,
//! drawn by `crate::hud_draw` and the renderer's HUD pass) — the bars and
//! counts, the event lines of `player1systemMessages` and the statistics
//! pop-up (card EVMSG, `analysis/re-event-messages.md`), the target's name
//! and health (card HTGT, `analysis/re-target-hud.md`) — and [`draw_hud`]
//! paints in egui what that tree does not carry yet: the round banner, the
//! round timer, the pad warning.

use crate::hud_tree::{EventValue, Hud, INFINITE};
use egui::{Align2, Color32, FontId, Frame, RichText, Vec2};

/// The held weapon as the HUD shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudWeapon {
    /// What the PS2's `.change` event carries (`re-hud.md` §3 [V][O]): the
    /// class's `HUDTag`, else its own `GeometryName`, else its name. The
    /// `hudtransforms` table turns it into a mesh.
    pub key: String,
    /// The weapon class's own name. `[ours]` a BF1 class keys BF2's table by
    /// it when `key` (its own `GeometryName`) has no entry there: BF2's
    /// classes of the same name carry no geometry and send their name.
    pub class: String,
    /// `[ours]` a weapon with no BF2 mesh: its `IconTexture` sheet, drawn in
    /// the mesh's place.
    pub icon: Option<String>,
    /// The localised name (shown for a moment on a change).
    pub name: String,
    /// Clip + spare rounds as ONE number (`re-hud.md` §3 [V][O]); `None` =
    /// infinite.
    pub total: Option<u32>,
    /// The clip's fill, 0..1 (the ring around the reticle).
    pub clip_fraction: f32,
}

/// One event line for the message box (`player1.message` after its
/// `player1.message.color`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudMessage {
    pub text: String,
    pub color: [u8; 3],
}

/// The message colours (`re-event-messages.md` §1.2, §2: PS2 RAM [M]).
pub mod message_color {
    /// The viewer's own team.
    pub const OWN: [u8; 3] = [1, 86, 213];
    /// The other team.
    pub const ENEMY: [u8; 3] = [200, 100, 100];
    /// Nobody's (team index 0: a death with no killer).
    pub const NEUTRAL: [u8; 3] = [192, 192, 192];
    /// Squad orders and medals (`DAT_0076c240`).
    pub const CYAN: [u8; 3] = [32, 223, 223];
    /// Bonuses and the mission's `ShowMessageText` (`DAT_0076c250`).
    pub const YELLOW: [u8; 3] = [223, 223, 32];
}

/// The player's statistics as BF2 publishes them (`re-event-messages.md` §5
/// [V] `FUN_003b5de8`): five numbers, of which the 1-player layout shows the
/// points and the kills; a change in any of the five opens the pop-up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HudStats {
    /// Ordinary and hero points together.
    pub points: i32,
    /// Kills less two per team kill.
    pub kills: i32,
    pub deaths: i32,
    pub hero_points: i32,
    pub captures: i32,
}

/// What the reticle picks, as BF2's target readout shows it
/// (`re-target-hud.md` §3 [V][O]): the class's name over a bar of its health
/// (or its shield while that holds), in the relation's colour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudTarget {
    /// Which object: a change of id is a new target (disable, then the new
    /// name and value, §3).
    pub id: u64,
    /// The class's localised name (`entity.<side>.<rest>`, §4).
    pub name: String,
    /// Health / max health, 0..1.
    pub health_fraction: f32,
    /// The viewer's relation to the target's team: > 0 friend, < 0 enemy,
    /// 0 neutral.
    pub relation: i8,
    /// The target's team is 0 (props, empty vehicles, unowned turrets):
    /// white whatever the relation.
    pub team0: bool,
    /// While a shield holds: its localised label ("Shield") and its
    /// fraction, shown in the bar's place.
    pub shield: Option<(String, f32)>,
}

/// BF2's `target.teamColor` values (`re-target-hud.md` §3 [V][V-RAM][O]:
/// `FUN_0036fec8`'s four words, read in RAM as 0156d5ff, c86464ff, ffffffff,
/// dfdf20ff).
pub mod target_color {
    pub const FRIEND: [u8; 3] = [1, 86, 213];
    pub const ENEMY: [u8; 3] = [200, 100, 100];
    /// A team-0 target, and the colour sent when the target is lost.
    pub const NONE: [u8; 3] = [255, 255, 255];
    /// A non-zero team neutral to the viewer.
    pub const NEUTRAL: [u8; 3] = [223, 223, 32];
}

impl HudTarget {
    /// The bar's and the reticle's colour (`FUN_003b3398` B2:410224–410253).
    pub fn color(&self) -> [u8; 3] {
        match self.relation {
            _ if self.team0 => target_color::NONE,
            r if r > 0 => target_color::FRIEND,
            r if r < 0 => target_color::ENEMY,
            _ => target_color::NEUTRAL,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HudModel {
    /// Faction keys or display names, team 0 / team 1.
    pub team_names: [String; 2],
    pub tickets: [u32; 2],
    /// Reinforcements lost per second to the post bleed, team 0 / team 1.
    pub bleed: [f32; 2],
    pub posts: [usize; 2],
    pub hp: f32,
    pub hp_max: f32,
    /// Sprint stamina 0..energy_max.
    pub energy: f32,
    pub energy_max: f32,
    pub clip: u32,
    pub spare: u32,
    pub reloading: bool,
    /// The weapon in hand; `None` hides the weapon group (dead, or riding).
    pub weapon: Option<HudWeapon>,
    /// The secondary channel's current weapon — the grenade row (card GREN);
    /// `None` = the row is off.
    pub weapon2: Option<HudWeapon>,
    /// The reticle: `None` hides it; else the relation of what the crosshair
    /// picks (−1 enemy, 1 friend, 0 nothing) — `Sim::reticle_pick`. A
    /// [`HudModel::target`] colours it instead.
    pub reticle: Option<i8>,
    /// The reticle's pick as the target readout shows it (card HTGT);
    /// `None` = nothing picked.
    pub target: Option<HudTarget>,
    /// Where the reticle sits: a fraction of the window's height from the
    /// top (`game::walk::WalkSim::reticle_y`; [`reticle_y`] is the PS2's
    /// place for a camera section's tilt).
    pub reticle_y: f32,
    pub dead: bool,
    /// Seconds until the dead player's spawn slot (member timer ∨ spawn wave).
    pub respawn_in: Option<f32>,
    /// Winning team once the round is decided; 2 = the time limit ran out
    /// on equal reinforcements and both teams win (`core::sim::OUTCOME_DRAW`).
    pub outcome: Option<u8>,
    /// Seconds left on the round's time limit (BF2's `ShowTimer`, queue
    /// TLO); `None` = no limit, and nothing is drawn.
    pub time_left: Option<f32>,
    /// The event lines that arrived since the last frame, oldest first.
    /// Each is shown once: hand a frame's lines over and forget them.
    pub messages: Vec<HudMessage>,
    /// `None` publishes no statistics (no pop-up).
    pub stats: Option<HudStats>,
    /// The local player's team (colors the "you" side).
    pub player_team: u8,
    /// The minimap (card BMAP); `None` = no Map element is shown.
    pub map: Option<crate::hud_map::MapModel>,
    /// The squad order cross (card SQHUD); `None` = no order is shown.
    pub squad: Option<crate::hud_squad::SquadCross>,
    /// The command-post capture icon (card CPBAR); `None` = not in a post's
    /// capture region, or nothing to show there.
    pub capture: Option<crate::hud_capture::CaptureIcon>,
    /// The edges the player's hits lit since the last frame (card HATK,
    /// `crate::hud_ambush::edge`); each restarts its chevron once.
    pub ambush: Vec<crate::hud_ambush::Edge>,
    /// The spawn screen drawn in the HUD's frame (card SSELB,
    /// `crate::spawn_draw`); `None` = none up.
    pub spawn: Option<crate::spawn_draw::SpawnPage>,
}

pub const TEAM_COLORS: [Color32; 2] = [
    Color32::from_rgb(242, 100, 40),
    Color32::from_rgb(80, 220, 90),
];

/// The reticle's place for a chase camera that looks `tilt` radians below
/// the aim (`analysis/re-aim-assist.md` §10.4): tan(tilt) / tan(fov_y / 2) of
/// the half height above the centre, as a fraction of the height from the
/// top. BF1 (5 deg, 0.34131): line 178.5 of 480; BF2 (10 deg, 0.48838):
/// 153.35.
pub fn reticle_y(tilt: f32, tan_half_fov_y: f32) -> f32 {
    0.5 - 0.5 * tilt.tan() / tan_half_fov_y
}

/// The reticle's colour by what it picks: enemy, nothing, friend — BF2's
/// `target.teamColor`, which the reticle listens to (`re-target-hud.md` §3
/// [V][O]: BF2's friend is blue, not BF1's green). Nothing is the tree's own
/// white — the PS2's reticle element holds ff ff ff in RAM with no target
/// (HOT1 state, element 0x187d000).
pub const RETICLE_COLORS: [[u8; 3]; 3] = [
    target_color::ENEMY,
    target_color::NONE,
    target_color::FRIEND,
];

/// Publish `m` as the 1-player HUD's events (`re-hud.md` §3). Change-gated
/// like the PS2's publishers, so call it every frame. `has_mesh` answers
/// whether a model of that name can be drawn (the NameMesh fallback).
pub fn feed(hud: &mut Hud, m: &HudModel, has_mesh: &dyn Fn(&str) -> bool) {
    let mut put = |event: &str, v: EventValue| hud.publish(event, v, has_mesh);
    put("initialize", EventValue::None);
    let alive = !m.dead;

    // Health and stamina: shown while alive.
    if alive {
        let frac = if m.hp_max > 0.0 { m.hp / m.hp_max } else { 0.0 };
        put("player1.health", EventValue::Number(m.hp.max(0.0)));
        put(
            "player1.healthFraction",
            EventValue::Number(frac.clamp(0.0, 1.0)),
        );
        let e = if m.energy_max > 0.0 {
            m.energy / m.energy_max
        } else {
            0.0
        };
        put(
            "player1.energyFraction",
            EventValue::Number(e.clamp(0.0, 1.0)),
        );
    }

    // The weapon in hand (row 1) and the secondary channel's (row 2: the
    // grenade and its count, clip + reserve — `re-grenades.md` §2.5).
    if !(alive && m.weapon2.is_some()) {
        put("player1.weapon2.disable", EventValue::None);
    }
    if alive && m.weapon.is_some() {
        put("player1.weaponsEnable", EventValue::None);
    }
    for (row, w) in [(1, &m.weapon), (2, &m.weapon2)] {
        let (Some(w), true) = (w, alive) else {
            continue;
        };
        // Row 2 needs row 1's group on; it is never shown alone.
        if row == 2 && m.weapon.is_none() {
            continue;
        }
        let ev = |name: &str| format!("player1.weapon{row}.{name}");
        // The mesh: the PS2's `.change` through the NameMesh table. `[ours]`
        // what its tree has no event for: a weapon with no model shows its
        // icon sheet, else nothing in the last one's place (the backdrop
        // stays). Each path forgets the other's event, so a switch back
        // sends again.
        let drawable = hud.name_mesh(&ev("change"), &w.key, has_mesh).is_some();
        let shown = match (&w.icon, drawable) {
            (Some(sheet), _) => Some(format!("bitmap:{sheet}")),
            (None, false) => Some(String::new()),
            (None, true) => None,
        };
        match shown {
            Some(mesh) => {
                hud.forget(&ev("change"));
                hud.publish(&ev("mesh"), EventValue::Text(mesh), has_mesh);
            }
            None => {
                hud.forget(&ev("mesh"));
                hud.publish(&ev("change"), EventValue::Text(w.key.clone()), has_mesh);
            }
        }
        let mut put = |event: &str, v: EventValue| hud.publish(event, v, has_mesh);
        put(&ev("name"), EventValue::Text(w.name.clone()));
        // An infinite weapon sends the count as -1 beside `ammoInfinite`
        // (`re-hud.md` §3 [V][O]); the count's Text prints nothing for it.
        match w.total {
            Some(n) => put(&ev("totalAmmoBullets"), EventValue::Number(n as f32)),
            None => {
                put(&ev("totalAmmoBullets"), EventValue::Number(INFINITE));
                put(&ev("ammoInfinite"), EventValue::None);
            }
        }
        put(
            &ev("totalClipFraction"),
            EventValue::Number(w.clip_fraction.clamp(0.0, 1.0)),
        );
        // `[ours]` a counted weapon takes the infinity ring away.
        if hud.falls(&format!("hud.gate.weapon{row}.infinite"), w.total.is_none()) {
            if let Some(i) = hud.find(&format!("player1weapon{row}ammoInfinite")) {
                hud.elems[i].fader.hide();
            }
            hud.forget(&ev("ammoInfinite"));
        }
    }

    // The minimap's Small view: while there is a controlled soldier.
    hud.map = m.map.clone();
    hud.squad = m.squad.clone();
    hud.capture = m.capture;
    for &e in &m.ambush {
        hud.ambush.hit(e);
    }
    hud.spawn = m.spawn.clone();
    let mut put = |event: &str, v: EventValue| hud.publish(event, v, has_mesh);
    if alive && m.map.is_some() {
        put("player1.map.enable", EventValue::None);
    }

    // Reinforcements: team1 is the viewer's own side, team2 the other
    // (`FUN_003b59a0`); the count pulses once per lost reinforcement.
    let own = usize::from(m.player_team.min(1));
    for (slot, team) in [(1, own), (2, 1 - own)] {
        put(
            &format!("player1.team{slot}.reinforcements"),
            EventValue::Number(m.tickets[team] as f32),
        );
        let period = if m.bleed[team] > 0.0 {
            1.0 / m.bleed[team]
        } else {
            f32::MAX
        };
        put(
            &format!("player1.team{slot}.bleedRate"),
            EventValue::Number(period),
        );
    }

    // The reticle sits where the aim ray goes on screen (card AIMA).
    // The target first: a lost one sends white, then the reticle's own.
    let target = m.target.as_ref().filter(|_| alive && m.reticle.is_some());
    feed_target(hud, target, has_mesh);
    let mut put = |event: &str, v: EventValue| hud.publish(event, v, has_mesh);
    if let (Some(rel), true) = (m.reticle, alive) {
        put(
            "player1.weapon1.reticule.position",
            EventValue::Position([0.5, m.reticle_y]),
        );
        let c = match target {
            Some(t) => t.color(),
            None => RETICLE_COLORS[(rel.clamp(-1, 1) + 1) as usize],
        };
        put("player1.weapon1.target.teamColor", EventValue::Color(c));
    }

    // The event lines: the colour first, then the text (the three senders,
    // `re-event-messages.md` §2 [V]). Never change-gated: the same line
    // twice is two messages.
    for msg in &m.messages {
        hud.send(
            "player1.message.color",
            EventValue::Color(msg.color),
            has_mesh,
        );
        hud.send(
            "player1.message",
            EventValue::Text(msg.text.clone()),
            has_mesh,
        );
    }

    // The statistics (`FUN_003b5de8` [V]): each number that differs from the
    // last one sent goes out with its change as text — "+n" when it rose,
    // the bare negative number when it fell, nothing the first time — and
    // one `changed` after them opens the pop-up.
    if let Some(st) = m.stats {
        let mut changed = false;
        for (name, v) in [
            ("points", st.points),
            ("kills", st.kills),
            ("deaths", st.deaths),
            ("heropoints", st.hero_points),
            ("captures", st.captures),
        ] {
            let event = format!("player1.statistic.{name}");
            let last = match hud.last(&event) {
                Some(EventValue::Number(l)) => Some(*l as i32),
                _ => None,
            };
            if last == Some(v) {
                continue;
            }
            let delta = v - last.unwrap_or(0);
            let text = match delta {
                0 => String::new(),
                d if d > 0 => format!("+{d}"),
                d => format!("{d}"),
            };
            hud.publish(&event, EventValue::Number(v as f32), has_mesh);
            hud.send(&format!("{event}Delta"), EventValue::Text(text), has_mesh);
            changed = true;
        }
        if changed {
            hud.send("player1.statistic.changed", EventValue::None, has_mesh);
        }
    }

    // Hiding: a disable the PS2 sends once, sent here on the falling edge;
    // what was published is forgotten so the next show sends it again.
    let hide = |hud: &mut Hud, gate: &str, on: bool, disables: &[&str], forget: &[&str]| {
        if hud.falls(gate, on) {
            for d in disables {
                hud.send(d, EventValue::None, has_mesh);
            }
            for f in forget {
                hud.forget(f);
            }
        }
    };
    hide(
        hud,
        "hud.gate.alive",
        alive,
        &["player1.healthDisable", "player1.energyDisable"],
        &[
            "player1.health",
            "player1.healthFraction",
            "player1.energyFraction",
        ],
    );
    hide(
        hud,
        "hud.gate.weapon",
        alive && m.weapon.is_some(),
        &["player1.weaponsDisable", "player1.weapon1.disable"],
        &[
            "player1.weaponsEnable",
            "player1.weapon1.change",
            "player1.weapon1.mesh",
            "player1.weapon1.name",
            "player1.weapon1.totalAmmoBullets",
            "player1.weapon1.ammoInfinite",
            "player1.weapon1.totalClipFraction",
        ],
    );
    hide(
        hud,
        "hud.gate.weapon2",
        alive && m.weapon.is_some() && m.weapon2.is_some(),
        &["player1.weapon2.disable"],
        &[
            "player1.weapon2.change",
            "player1.weapon2.mesh",
            "player1.weapon2.name",
            "player1.weapon2.totalAmmoBullets",
            "player1.weapon2.ammoInfinite",
            "player1.weapon2.totalClipFraction",
        ],
    );
    hide(
        hud,
        "hud.gate.reticle",
        alive && m.reticle.is_some(),
        &[
            "player1.weapon1.reticule.disable",
            "player1.weapon1.target.disable",
        ],
        &["player1.weapon1.reticule.position"],
    );
    hide(
        hud,
        "hud.gate.map",
        alive && m.map.is_some(),
        &["player1.map.disable"],
        &["player1.map.enable"],
    );
}

/// The target readout's events (`re-target-hud.md` §3 [V][O],
/// `FUN_003b3398`): on a change of target `disable` and `disableShieldName`
/// first, then — no new target — `teamColor` white, else the new `className`;
/// every update with a target its colour and the shield's fraction and name
/// while one holds, else `disableShieldName` and the health's fraction.
fn feed_target(hud: &mut Hud, t: Option<&HudTarget>, has_mesh: &dyn Fn(&str) -> bool) {
    const ID: &str = "hud.target.id";
    let id = EventValue::Text(t.map(|t| t.id.to_string()).unwrap_or_default());
    if hud.last(ID) != Some(&id) {
        let first = hud.last(ID).is_none();
        hud.publish(ID, id, has_mesh);
        if !first {
            hud.send("player1.weapon1.target.disable", EventValue::None, has_mesh);
            hud.send(
                "player1.weapon1.target.disableShieldName",
                EventValue::None,
                has_mesh,
            );
        }
        for e in [
            "player1.weapon1.target.className",
            "player1.weapon1.target.healthFraction",
            "player1.weapon1.target.shieldName",
        ] {
            hud.forget(e);
        }
        if t.is_none() {
            hud.forget("player1.weapon1.target.teamColor");
            hud.publish(
                "player1.weapon1.target.teamColor",
                EventValue::Color(target_color::NONE),
                has_mesh,
            );
        }
    }
    let Some(t) = t else {
        return;
    };
    let mut put = |event: &str, v: EventValue| hud.publish(event, v, has_mesh);
    put(
        "player1.weapon1.target.className",
        EventValue::Text(t.name.clone()),
    );
    put(
        "player1.weapon1.target.teamColor",
        EventValue::Color(t.color()),
    );
    let fraction = match &t.shield {
        Some((label, f)) => {
            put(
                "player1.weapon1.target.shieldName",
                EventValue::Text(label.clone()),
            );
            *f
        }
        None => {
            if hud.last("player1.weapon1.target.shieldName").is_some() {
                hud.forget("player1.weapon1.target.shieldName");
                hud.send(
                    "player1.weapon1.target.disableShieldName",
                    EventValue::None,
                    has_mesh,
                );
            }
            t.health_fraction
        }
    };
    hud.publish(
        "player1.weapon1.target.healthFraction",
        EventValue::Number(fraction.clamp(0.0, 1.0)),
        has_mesh,
    );
}

fn panel() -> Frame {
    Frame::new()
        .fill(Color32::from_black_alpha(140))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .corner_radius(4.0)
}

/// A controller-mapping warning (card PADM: a pad whose camera stick or
/// FIRE resolved to nothing), bottom-centre above the HUD bars, on every
/// screen — never a silently dead camera.
pub fn draw_pad_warning(ctx: &egui::Context, text: &str) {
    egui::Area::new(egui::Id::new("pad_warning"))
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -96.0))
        .show(ctx, |ui| {
            panel().show(ui, |ui| {
                ui.set_max_width(640.0);
                ui.label(
                    RichText::new(text)
                        .font(FontId::proportional(15.0))
                        .color(Color32::from_rgb(255, 120, 90)),
                );
            });
        });
}

/// The egui half of the HUD for this frame (module doc).
pub fn draw_hud(ctx: &egui::Context, m: &HudModel) {
    // The round timer, top centre under the reinforcement counts.
    if let Some(t) = m.time_left {
        let secs = t.max(0.0).ceil() as u32;
        egui::Area::new(egui::Id::new("hud_timer"))
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 12.0))
            .show(ctx, |ui| {
                panel().show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{}:{:02}", secs / 60, secs % 60))
                            .font(FontId::monospace(18.0))
                            .color(Color32::WHITE),
                    );
                });
            });
    }

    // Center banner: round over / respawning.
    let banner = match m.outcome {
        Some(2) => Some(("TIME UP — BOTH TEAMS WIN".to_string(), Color32::WHITE)),
        Some(w) => Some((
            format!(
                "ROUND OVER — {} WINS",
                m.team_names[usize::from(w.min(1))].to_uppercase()
            ),
            TEAM_COLORS[usize::from(w.min(1))],
        )),
        None if m.dead => Some((
            match m.respawn_in {
                Some(s) => format!("RESPAWNING IN {}", s.ceil() as u32),
                None => "RESPAWNING…".to_string(),
            },
            Color32::LIGHT_GRAY,
        )),
        None => None,
    };
    if let Some((text, color)) = banner {
        egui::Area::new(egui::Id::new("hud_banner"))
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, -40.0))
            .show(ctx, |ui| {
                panel().show(ui, |ui| {
                    ui.label(RichText::new(text).size(30.0).color(color).strong());
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> HudModel {
        HudModel {
            team_names: ["rep".into(), "cis".into()],
            tickets: [150, 143],
            bleed: [0.0; 2],
            posts: [3, 2],
            hp: 320.0,
            hp_max: 400.0,
            energy: 55.0,
            energy_max: 100.0,
            clip: 24,
            spare: 96,
            reloading: false,
            weapon: None,
            weapon2: None,
            reticle: Some(0),
            target: None,
            reticle_y: 0.5,
            dead: false,
            respawn_in: None,
            outcome: None,
            time_left: Some(125.0),
            messages: Vec::new(),
            stats: None,
            player_team: 0,
            map: None,
            squad: None,
            capture: None,
            ambush: Vec::new(),
            spawn: None,
        }
    }

    /// The egui half lays out headless: every area produces shapes and nothing
    /// depends on a fixed aspect (two viewport sizes both render).
    #[test]
    fn hud_lays_out_headless_at_two_aspects() {
        for (w, h) in [(1280.0, 720.0), (2560.0, 1080.0)] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(w, h))),
                ..Default::default()
            };
            // Anchored areas size themselves on frame 1 and lay out on
            // frame 2 (egui_kittest runs the same warm-up), so judge frame 2.
            let _ = ctx.run(input.clone(), |ctx| draw_hud(ctx, &model()));
            let out = ctx.run(input, |ctx| draw_hud(ctx, &model()));
            assert!(!out.shapes.is_empty(), "{w}x{h}: no shapes");
            // Shapes sharing a clip rect merge into one primitive; count
            // geometry instead.
            let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
            let indices: usize = prims
                .iter()
                .map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(m) => m.indices.len(),
                    _ => 0,
                })
                .sum();
            assert!(indices > 100, "{w}x{h}: {indices} indices");
        }
        // Banner states don't panic.
        let ctx = egui::Context::default();
        let mut m = model();
        m.outcome = Some(1);
        m.dead = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| draw_hud(ctx, &m));
        m.outcome = Some(2);
        m.time_left = Some(0.0);
        let _ = ctx.run(egui::RawInput::default(), |ctx| draw_hud(ctx, &m));
    }

    /// The target readout's feed (`re-target-hud.md` §3): a picked enemy at
    /// full then half health sends its name, 1.0 then 0.5 and the enemy red
    /// to the bar and the reticle alike; a friend is blue, a team-0 object
    /// white; a switch keeps the readout up; losing the pick fades it out
    /// and turns the bar white.
    #[test]
    fn the_target_readout_follows_the_pick() {
        let mut hud = crate::hud_tree::fixture::target_hud();
        let fill = hud.find("player1targethealth_fill").unwrap();
        let name = hud.find("player1targetinfo_classNameNEW").unwrap();
        let shield = hud.find("player1targetinfo_shieldName").unwrap();
        let color = "player1.weapon1.target.teamColor";
        let enemy = HudTarget {
            id: 7,
            name: "Clone Trooper".into(),
            health_fraction: 1.0,
            relation: -1,
            team0: false,
            shield: None,
        };
        let mut m = model();
        m.target = Some(enemy.clone());
        let step = |hud: &mut Hud, m: &HudModel| {
            feed(hud, m, &|_| false);
            hud.advance(1.0 / 30.0);
        };
        step(&mut hud, &m);
        assert_eq!(hud.elems[name].text, "Clone Trooper");
        assert_eq!(hud.elems[fill].value, 1.0);
        assert_eq!(hud.elems[fill].fader.state, 2, "shown at once");
        assert_eq!(hud.elems[name].fader.state, 2);
        assert_eq!(hud.elems[fill].color, [200.0, 100.0, 100.0]);
        assert_eq!(hud.last(color), Some(&EventValue::Color([200, 100, 100])));
        m.target.as_mut().unwrap().health_fraction = 0.5;
        step(&mut hud, &m);
        assert_eq!(hud.elems[fill].value, 0.5);
        assert!(hud.elems[fill].flash.is_some_and(|f| f.fall()));
        // A friend at the same health: a new target, kept up.
        m.target = Some(HudTarget {
            id: 8,
            name: "Super Battle Droid".into(),
            relation: 1,
            ..enemy.clone()
        });
        step(&mut hud, &m);
        assert_eq!(hud.elems[name].text, "Super Battle Droid");
        assert_eq!(hud.elems[fill].color, [1.0, 86.0, 213.0]);
        assert_eq!(hud.elems[fill].fader.state, 2, "a switch keeps the bar up");
        assert_eq!(hud.elems[name].fader.state, 2);
        // A team-0 turret is white, whatever the relation; a droideka's
        // shield shows its label and the shield's fraction.
        m.target = Some(HudTarget {
            id: 9,
            name: "Laser Turret".into(),
            team0: true,
            shield: Some(("Shield".into(), 0.25)),
            ..enemy.clone()
        });
        step(&mut hud, &m);
        assert_eq!(hud.elems[fill].color, [255.0; 3]);
        assert_eq!(hud.elems[shield].text, "Shield");
        assert_eq!(hud.elems[shield].fader.state, 2);
        assert_eq!(hud.elems[fill].value, 0.25);
        m.target.as_mut().unwrap().shield = None;
        step(&mut hud, &m);
        assert!(hud.elems[shield].fader.state >= 3, "the shield line goes");
        assert_eq!(hud.elems[fill].value, 1.0);
        // A neutral team: yellow.
        m.target = Some(HudTarget {
            id: 10,
            relation: 0,
            ..enemy.clone()
        });
        step(&mut hud, &m);
        assert_eq!(hud.elems[fill].color, [223.0, 223.0, 32.0]);
        // Lost: everything fades out over its 1 s, keeping the last value;
        // the bar turns white at once.
        m.target = None;
        step(&mut hud, &m);
        for i in [fill, name] {
            assert_eq!(
                hud.elems[i].fader.state, 4,
                "{} fades out",
                hud.elems[i].name
            );
        }
        assert_eq!(hud.elems[fill].color, [255.0; 3]);
        assert_eq!(hud.last(color), Some(&EventValue::Color([255, 255, 255])));
        for _ in 0..31 {
            step(&mut hud, &m);
        }
        assert_eq!(hud.elems[fill].fader.state, 0, "gone after 1 s");
    }

    /// BF2's reticle colours (`re-target-hud.md` §3): friend blue, not BF1's
    /// green.
    #[test]
    fn the_reticle_takes_bf2s_target_colours() {
        assert_eq!(
            RETICLE_COLORS,
            [[200, 100, 100], [255, 255, 255], [1, 86, 213]]
        );
    }
}
