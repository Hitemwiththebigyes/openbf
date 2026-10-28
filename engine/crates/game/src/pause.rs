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
        MenuAction::Quit | MenuAction::SetMouseSensitivity(_) | MenuAction::ToggleInvertY => {}
    }
}

/// The pause menu's click for one frame (menuref "In battle" and its sound
/// table): Triangle resumes with the exit click, or in the Respawn popup
/// cancels with the cancel click; Cross on No cancel, on any other row
/// accept; a move select; a value step accept (as Game Options). `confirm`
/// is whether the popup was up before the press. Start's accept on opening
/// is the caller's.
pub fn pause_sound(
    confirm: bool,
    input: ui::menu::MenuInput,
    from: usize,
    to: usize,
    actions: &[ui::menu::MenuAction],
) -> Option<ui::shell_feel::MenuSound> {
    use ui::menu::MenuAction;
    use ui::shell_feel::MenuSound;
    if input.back {
        return Some(if confirm {
            MenuSound::Cancel
        } else {
            MenuSound::Exit
        });
    }
    if actions.contains(&MenuAction::CancelRespawn) {
        return Some(MenuSound::Cancel);
    }
    if !actions.is_empty() {
        return Some(MenuSound::Accept);
    }
    (from != to).then_some(MenuSound::Select)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui::menu::{MenuAction, MenuInput};
    use ui::shell_feel::MenuSound;

    /// menuref "In battle": move select, Triangle exit, the popup's No and
    /// Triangle cancel, Cross accept; nothing when nothing happens.
    #[test]
    fn the_pause_menu_asks_for_the_ps2_clicks() {
        let back = MenuInput {
            back: true,
            ..Default::default()
        };
        let cross = MenuInput {
            confirm: true,
            ..Default::default()
        };
        let down = MenuInput {
            down: true,
            ..Default::default()
        };
        assert_eq!(
            pause_sound(false, back, 0, 0, &[MenuAction::Resume]),
            Some(MenuSound::Exit)
        );
        assert_eq!(
            pause_sound(true, back, 0, 0, &[MenuAction::CancelRespawn]),
            Some(MenuSound::Cancel)
        );
        assert_eq!(
            pause_sound(true, cross, 0, 0, &[MenuAction::CancelRespawn]),
            Some(MenuSound::Cancel)
        );
        assert_eq!(
            pause_sound(false, cross, 2, 2, &[MenuAction::RequestRespawn]),
            Some(MenuSound::Accept)
        );
        assert_eq!(pause_sound(false, down, 0, 1, &[]), Some(MenuSound::Select));
        assert_eq!(pause_sound(false, cross, 3, 3, &[]), None);
    }
}
