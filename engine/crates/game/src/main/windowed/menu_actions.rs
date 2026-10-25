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
