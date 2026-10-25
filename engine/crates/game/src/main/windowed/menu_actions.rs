use game::walk::WalkSim;

/// Autosave the settings a menu action changed.
pub(super) fn persist_setting(
    profile: &mut game::profile::Profile,
    a: ui::menu::MenuAction,
    walk: &WalkSim,
) {
    use ui::menu::MenuAction;
    match a {
        MenuAction::SetAssist(_) => profile.settings.assist_intensity = walk.sim.assist_intensity,
        MenuAction::SetAiCap(_) => profile.settings.ai_alive_cap = Some(walk.sim.ai_alive_cap),
        MenuAction::SetFov(_) => profile.settings.fov_scale = walk.fov_scale,
        MenuAction::SetMusic(v) => profile.settings.music_volume = v,
        MenuAction::SetMouseSensitivity(v) => {
            profile.settings.mouse_sensitivity = v.clamp(
                game::kbm::SENSITIVITY_RANGE.0,
                game::kbm::SENSITIVITY_RANGE.1,
            )
        }
        MenuAction::ToggleInvertY => profile.settings.invert_y = !profile.settings.invert_y,
        _ => return,
    }
    if let Err(e) = profile.save() {
        eprintln!("profile autosave failed: {e:#}");
    }
}

/// Apply a pause-menu action to the battle/session.
pub(super) fn apply_menu_action(walk: &mut WalkSim, a: ui::menu::MenuAction, paused: &mut bool) {
    use ui::menu::MenuAction;
    match a {
        MenuAction::Resume => *paused = false,
        MenuAction::ToggleCheat(i) => {
            let id = walk.sim.cheats.entries()[i].0;
            walk.sim.cheats.toggle(id);
            println!("cheat {id}: {}", walk.sim.cheats.entries()[i].2);
        }
        MenuAction::SetAssist(v) => walk.sim.assist_intensity = v.clamp(0.0, 1.5),
        MenuAction::SetAiCap(c) => walk.sim.ai_alive_cap = c,
        MenuAction::SetMusic(v) => {
            if let Some(m) = walk.music.as_mut() {
                m.volume = v.clamp(game::music::VOLUME_RANGE.0, game::music::VOLUME_RANGE.1);
            }
        }
        MenuAction::SetFov(k) => {
            walk.fov_scale = k.clamp(game::walk::FOV_SCALE_RANGE.0, game::walk::FOV_SCALE_RANGE.1)
        }
        MenuAction::SetMouseSensitivity(_) | MenuAction::ToggleInvertY => {}
        // Quit: --map launches exit, shell launches return to the shell —
        // decided by the caller (needs App state).
        MenuAction::Quit => {}
    }
}
