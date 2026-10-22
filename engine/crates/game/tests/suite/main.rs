//! The game crate's integration tests, linked as ONE test binary (card WEARY, SSD wear): every file directly under
//! `tests/` is its own binary that links the whole engine (~130 MB each), so the plain test files live here as
//! modules instead. A test's id moves from `<file> <name>` to `suite <file>::<name>`; the counts are unchanged.
//! Still their own binaries, beside this directory: the golden tests and `walkabout_stats` (gates.py and goldens.sh
//! find them by name), and `texture_tier` / `capture_display_ps2`, which set the process-wide texture tier
//! (`game::assets::set_texture_tier`) that `cargo test` would share across every test here.
//! A new test file goes in this directory with a `mod` line below.

mod all_maps_load;
mod ambience_ps2;
mod announcer_ps2;
mod battle_replay;
mod collision;
mod event_messages;
mod glb_material;
mod global_sound;
mod hud_layout;
mod load_scene;
mod locate_ps2;
mod music_ps2;
mod nav_walls;
mod roster;
mod route_patch;
mod shell_layout;
mod skinned_glb;
mod skirt_climb;
mod sound_sublevels;
mod squad_orders_ps2;
mod station_draw;
mod turret_gunner;
mod turret_pose;
mod turret_views;
mod unit_calls;
mod unit_vo;
mod unit_voice;
mod vehicle_class;
mod vehicle_pads;
mod vehicle_rider;
mod water_anim;
mod water_floor;
mod water_look;
mod weapon_classdb;
mod weather_ps2;
