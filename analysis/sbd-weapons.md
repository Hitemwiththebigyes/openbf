# Super battle droid weapons

Issue: https://github.com/sprucey149/openbf/issues/27. Sources read locally on 2026-10-27.

## Disc evidence

[V] `game/tests/suite/sbd.rs::both_discs_keep_their_sbd_source_values` bakes the original
`extracted/bf1/lvl/DATA/_LVL_PS2/SIDE/CIS.LVL` with `COMMON/ENG/COMMON.LVL`, and
BF2's `SIDE/CIS.LVL` with `INGAME.LVL`, into temporary class databases. BF1's soldier
is `cis_inf_battledroid`; BF2's is `cis_inf_rifleman`, through its default parent.
The weapon names share `cis_weap_inf_wrist_`; BF1's secondary suffix is `shrapnel`,
BF2's `trishot`. These are readings of class data, not new executable RE.

| Property | BF1 | BF2, selected for both map sets |
|---|---|---|
| Blaster rounds / total clips | 55 / 5 | 50 / 5 |
| Blaster shot delay | 0.20 s | 0.25 s |
| Blaster bolt damage | flat 60 | 100, falling to 60 over age 0.4–0.6 s |
| Blaster bolt speed / lifetime | 80 m/s / 1.5 s | 160 m/s / 0.8 s |
| Blaster axis spread | 0.25 degrees pitch and yaw | same |
| Blaster spread growth / recovery / threshold / limit | 1.1 / 4.8 / 1.6 / 6 degrees | 1.2 / 4.8 / 1.6 / 5 degrees |
| Secondary ammo / trigger / bolts | unlimited / fresh press / 3 | same |
| Secondary shot delay | 0.50 s | 0.66 s |
| Secondary reload time, unused for unlimited ammo | 1.75 s | same |
| Secondary heat per bolt / recovery / clear threshold | 0.15 / 0.4 per s / 0.25 | 0.20 / 0.4 per s / 0.25 |
| Secondary random pitch / yaw spread | 0 / 0 degrees | same |
| Secondary spread growth / recovery / threshold / limit | 0.1 / 4.8 / 1.6 / 6 degrees | same |
| Secondary bolt damage, speed and lifetime | same ordnance as blaster | same ordnance as blaster |
| Wrist rocket rounds / total clips | 1 / 4 | 1 / 3 |
| Wrist rocket trigger / reload | fresh press / 2 s | same |
| Wrist rocket initial salvo delay | 0 s | 0.21 s |
| Wrist rocket shot delay | 0 s; reload paces shots | same |
| Wrist rocket direct damage | flat 200 | 100, rising to 250 over age 0–0.5 s |
| Wrist rocket speed / lifetime | 50 m/s / 3 s | 80 m/s / 5 s |
| Wrist rocket splash / inner and outer radius | 100 / 3.5 and 5.5 m | 250 / 1 and 3 m |
| Wrist rocket axis spread | 0.25 degrees pitch and yaw | same |

Total clips include the loaded clip: selected blaster ammunition is 50 loaded plus
200 reserve; selected rockets are one loaded plus two reserve. Secondary rounds and
clips are authored as -1 and 0, respectively. No wrist weapon authors a charge time;
blaster and secondary have zero initial salvo delay. Rockets have no authored heat.

[V] Both secondary weapons author count `0xa4dfafc0 = 3` and three ordered
`0xa4120aa8` pitch/yaw pairs, in degrees: `(-0.1, -0.8)`, `(0.8, 0)`, `(-0.1, 0.8)`.
The existing executable reading in [re-blaster-damage.md, 3.3](re-blaster-damage.md#33-shot-spawn-bf1-fun_002892a8-bf2-fun_0032f208-same-structure)
establishes their conversion to radians and cyclic indexing per emitted bolt.
They are fixed tilts in addition to posture and random spread. The runtime had
passed zero tilt to every shot. Its pattern index now participates in replay hashing.

## One gameplay decision and implementation

BF2's complete SBD weapon classes and slot ammunition are used on both map sets.
`roster::load_classdb` retains a separate BF2 CIS database for a BF1 CIS roster;
`ClassRoster::build` uses it only for `cis_inf_battledroid`'s weapons. Native BF1
soldier movement, body, animations and other classes keep their own data. The BF1
secondary animation bank is retained under the selected `trishot` name. Raw disc
databases remain available for the source-value test. Both discs' baked class files
are therefore required for a BF1 CIS battle, as they already are for the combined game.

The roster previously rejected the wrist rocket because channel 1 accepted thrown
grenades and repair tools only. It now also accepts this SBD cannon. Channel 1
routes its emitted shots to normal projectile spawning, including the muzzle aim,
collision sweep, shot and reload events, impact damage and explosion.

BF2 `hudtransforms` already maps the three weapon names to `hud_cis_wristblaster`,
`hud_cis_trishot` and `hud_cis_wristrocket`. Rebaking these three SIDE/CIS models
with `hud_model_to_glb` retains their vertex colors. The first two badges are
untextured red geometry; the rocket uses `hud_imp_wristrocket`. No replacement art
or HUD layout change is needed. The normal side rebake now includes them.

## Reproduction and evidence

From `engine/`:

```text
cargo test -p game --test suite sbd -- --nocapture
cargo test -p core shot_pattern_advances_wraps_and_participates_in_replay_hash
cargo run -p bake -- weapon ../extracted/bf2/lvl/DATA/_LVL_PS2/SIDE/CIS.LVL hud_cis_wristblaster,hud_cis_trishot,hud_cis_wristrocket assets_baked/side_bf2
```

The headless battle fires one primary bolt (100 damage, 249 rounds remaining),
switches and fires three secondary bolts (300 damage, unlimited ammo), then fires
channel 1's rocket (130 direct damage at the fixture's 0.1 s bolt age plus 250 splash,
two rockets remaining). Directions, damage and ammunition match on both map sets.
The state-machine test also checks secondary overheating and recovery and consumes
all three rockets. The HUD regression freshly bakes each mesh, checks its color
attribute, and lays out all three icons through the actual HUD renderer.

Local, ignored screenshots live in `engine/artifacts/lead_tmp/sbd/`: `before.png`,
`trishot-after.png`, `blaster-after.png`, and `bf1-after.png`. They are not distributed.
For the tri-shot capture, set `HUD_CLASS=cis_weap_inf_wrist_trishot`, `HUD_AMMO=inf`,
`HUD_GRENADES=3`, `HUD_GRENADE_KEY=cis_weap_inf_wrist_rocket`, and
`HUD_GRENADE_CLASS=cis_weap_inf_wrist_rocket`, then run:

```text
cargo run -p game --example hud_capture -- bf2_uta1 1280 720 artifacts/lead_tmp/sbd/trishot-after.png cis_weap_inf_wrist_trishot
```

The blaster capture uses `HUD_CLASS=cis_weap_inf_wrist_blaster`, `HUD_AMMO=250`,
and the matching final argument. `bf1-after.png` uses `bf1_nab1` with the selected
tri-shot and rocket names. Captures are layout evidence with the existing fixed
gallery camera; the headless battle test supplies the weapon and damage evidence.

## Map-specific sound and effect presentation

The maintainer clarified that the sound problem was a BF1 map using BF2 sound.
Shared gameplay does not require shared audio. BF1 SBD rosters copy the native
BF1 weapons' fire, fire-loop, reload, empty, equip, ordnance and collision sound
properties onto the selected BF2 gameplay classes. BF2 rosters retain BF2 audio.
The tri-shot maps to BF1's `cis_weap_inf_wrist_shrapnel` for this lookup.
Explosion sound properties are copied without replacing damage or blast radii.

The map's bank retains priority. A missing SBD fire/equip/reload definition falls
back to its own game's Geonosis bank, with the authored gains, pitch and distance
properties. Imported samples receive collision-free resident IDs and use the
normal packed-asset reader. Diagnostic names include the registered weapons.
Reload events carry their weapon class so a secondary rocket cannot use the
primary weapon's reload cue.

BF1's tri-shot take `bf1_geo1/sounds/6f4b07cb/bcd626dd.wav` is 18,018 Hz and
20,328 frames. It matches an independent FFmpeg decode of that sample's SPU2
ADPCM from BF1 `SOUND/GEO.LVL` exactly. Sample provenance and decoding are
verified; the complete mix has not been compared in a live PS2 firing capture.

The canonical rocket names `cis_sfx_sbd_missiles_red`. BF1 CIS battles import
that original effect from BF2's CIS layer, with its texture fallbacks. Each effect
owns its source era's particle rules. A missile with an authored trail uses that
particle presentation rather than the generic solid box. Its explosion effect
resolves from the existing common layer.

## Review and validation

Primary and secondary soldier projectiles use one spawning helper, preserving
shot order, aim, collision targets and random draws. Tilt properties use the
existing inherited-property readers. Effect layers use one RON reader; callers
cannot override an effect's source era with an ignored argument.

The SBD suite covers both discs' values, identical gameplay on both map sets,
overheat/recovery and ammunition exhaustion, the HUD's original meshes,
map-specific audible fire cues, secondary reload identity and cue, all baked
maps' sound fallbacks, and actual rocket trails. The trail check compares its
emitters to the original bake, emitted particles across map sets, nonempty draws
and absence of the solid box. Tests skip optional local disc/bake data when it
is absent, following the existing test convention.

Ignored local evidence is under `engine/artifacts/lead_tmp/sbd/`. The gameplay
replay, events and sound log accompany `fidelity-frames/walk_00068.png`, which
shows the red rocket flare and both HUD icons. Earlier `fidelity-*` artifacts
predate the final map-specific sound correction. `review-*` logs record final
checks and distribution verification.

The source checkout lacks `engine/artifacts/loop/STATUS.md` and the private loop
state, so validation runs the public CI commands directly. The size/map checker
has Windows path-separator mismatches and pre-existing missing exported paths;
changed source-file counts are recorded in `engine/CRATES.md`.

The original-game firing capture remains outstanding. An isolated PCSX2 attempt
produced only a pause-menu recording because mapped pad inputs did not reach the
game. It is not acceptance evidence; the issue remains in progress for that
comparison. No copyrighted assets or local captures are committed.
