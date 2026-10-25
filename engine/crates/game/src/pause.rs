//! Pause-menu battle dispatch, shared by the windowed caller and BUGX scenes.
//! Respawn confirmation uses the existing death flow (analysis/re-spawn-flow §7/§6).
use crate::walk::WalkSim;

/// The Respawn row's text-table key: both discs' `ifs_pausemenu.luac` read
/// `game.pause.suicide` ("Respawn"; its popup is `game.pause.suicide_prompt`).
pub const RESPAWN_LABEL_KEY: &str = "game.pause.suicide";

#[derive(Default)]
pub struct PauseMenu {
    pub cursor: usize,
    pub confirm: bool,
}

/// Apply a pause-menu action to the battle/session.
pub fn apply_menu_action(
    walk: &mut WalkSim,
    a: ui::menu::MenuAction,
    paused: &mut bool,
    menu: &mut PauseMenu,
) {
    use ui::menu::MenuAction;
    match a {
        MenuAction::Resume => {
            menu.confirm = false;
            *paused = false;
        }
        MenuAction::RequestRespawn => {
            if *paused && walk.sim.outcome.is_none() && walk.sim.unit(walk.player).is_some() {
                menu.confirm = true;
                menu.cursor = 0; // No is the safe initial selection.
            }
        }
        MenuAction::CancelRespawn => {
            menu.confirm = false;
            menu.cursor = 0;
        }
        MenuAction::Respawn => {
            if *paused && menu.confirm && walk.sim.outcome.is_none() {
                walk.request_respawn();
                *paused = false;
            }
            menu.confirm = false;
            menu.cursor = 0;
        }
        MenuAction::ToggleCheat(i) => {
            let id = walk.sim.cheats.entries()[i].0;
            walk.sim.cheats.toggle(id);
            println!("cheat {id}: {}", walk.sim.cheats.entries()[i].2);
        }
        MenuAction::SetAssist(v) => walk.sim.assist_intensity = v.clamp(0.0, 1.5),
        MenuAction::SetAiCap(c) => walk.sim.ai_alive_cap = c,
        MenuAction::SetMusic(v) => {
            if let Some(m) = walk.music.as_mut() {
                m.volume = v.clamp(crate::music::VOLUME_RANGE.0, crate::music::VOLUME_RANGE.1);
            }
        }
        MenuAction::SetFov(k) => {
            walk.fov_scale = k.clamp(
                crate::walk::FOV_SCALE_RANGE.0,
                crate::walk::FOV_SCALE_RANGE.1,
            )
        }
        // Quit: --map launches exit, shell launches return to the shell —
        // decided by the caller (needs App state).
        MenuAction::Quit => {}
    }
}
