---
id: ATUR
title: The BF2 sniper's portable auto-turret: the dispenser throws a `portableturret` that falls, lands upright, turns on the nearest enemy by its stationary controller, fires its cannon in salvos inside MaxRange, takes damage by BuildingScale, expires at its Lifetime and is replaced past MaxItems
milestone: M3
value: 3
phase: build
status: parked
deps: REMOTE
files: engine/crates/core/src/autoturret.rs,engine/crates/core/src/autoturret/tests.rs,engine/crates/core/src/lib.rs,engine/crates/core/src/weapon.rs,engine/crates/core/src/mine.rs,engine/crates/core/src/remote.rs,engine/crates/core/src/projectile.rs,engine/crates/core/src/ai/abilities.rs,engine/crates/core/src/ai/hover_drive.rs,engine/crates/core/src/sim.rs,engine/crates/core/src/sim/turrets.rs,engine/crates/core/src/sim/turrets/tests.rs,engine/crates/core/src/sim/state.rs,engine/crates/core/src/sim/step.rs,engine/crates/core/src/sim/combat.rs,engine/crates/core/src/sim/blast.rs,engine/crates/core/src/sim/mines.rs,engine/crates/core/src/sim/remotes.rs,engine/crates/core/src/sim/pickups.rs,engine/crates/core/src/sim/vehicles.rs,engine/crates/core/src/sim/ordnance.rs,engine/crates/core/src/sim/repair.rs,engine/crates/game/src/roster.rs,engine/crates/game/src/roster/turrets.rs,engine/crates/game/src/soldier_vis/runtime.rs,engine/crates/game/src/render_blend.rs,engine/crates/game/src/fx.rs,engine/crates/game/src/event_sounds.rs,engine/crates/game/src/main/headless.rs,engine/crates/game/tests/suite/auto_turrets.rs,engine/crates/game/tests/suite/main.rs,engine/crates/game/tests/suite/roster.rs,engine/crates/game/tests/data/class_kits.txt,engine/crates/core/src/ai/arsenal.rs,engine/crates/mungefs/src/model.rs,engine/crates/bake/src/soldier/output.rs,engine/crates/bake/src/main.rs,engine/crates/bake/src/glb.rs,engine/crates/game/src/assets.rs,engine/crates/game/src/assets/mesh.rs,engine/crates/game/src/viewer/upload.rs,engine/crates/game/src/hud_feed.rs,engine/crates/ui/src/hud_tree.rs,engine/crates/ui/src/hud_draw.rs,engine/crates/render/src/hud.rs,scripts/rebake.sh
declares: sim
needs: review — local implementation and disc census available; a live PCSX2 comparison (throw arc, landing, aim feel, expiry / replacement explosion) remains outstanding
effort:
branch: main
resume: 2026-10-28 local implementation on main, revised after the user's playtest (look, hover, HUD icon, bot spam), not committed; see docs/loop/reports/20261028_ATUR.md. No upstream landing or live PCSX2 capture.
created: 2026-10-28
---
## Why
`analysis/re-portable-turret.md` (`[O]`), issue #45. Every BF2 sniper carries a portable auto-turret in slot 4, channel
1; the class kit dropped the slot (14 classes counting variants).

## What to build
1. **The weapon** (§0, §5): the dispenser throw of the `OrdnanceName` turret at the charge-scaled speed plus the
   owner's velocity, the owner's team; past `MaxItems` the oldest is killed and replaced.
2. **The turret** (§2, §3, §4): the `portableturret` class keys with the constructor defaults; the half-step fall;
   landing on a contact with n.y > 0.7, the friction / rebound bounce otherwise; `Lifetime` expiry; the −500 floor.
3. **Its AI and gun** (§6): the stationary controller's target choice (view 100 m, fight inside MinRange..MaxRange
   under the gun's `AITarget` mask, held to MaxRange + 10), the idle look, the hover stick turn under `MaxTurnSpeed` /
   `MaxPitchSpeed` and `PitchLimits`, fire inside MaxRange within 22.5°, the gun's own salvos; kills credit the owner.
4. **Damage**: bolts and blasts hit it by its `HealthType` scale (Building); destroyed, expired or replaced, it explodes
   with its `ExplosionName`.
5. **Game** (§7): the `com_weap_inf_dropturret` model with its side sheet `com_prop_dropturret_rep`, hovering on its
   own geometry above the landing point, front (+Z) to the aim, drawn between ticks; the band's UV scroll in the world
   and on the HUD's item icon; its explosion look and sound; the headless log.
6. **Bots** (§6.1): press the dispenser when the target is a soldier on foot in a cluster of his side (the grenade's
   group test) and the weapon is ready; no range or live-turret test, so the ammo, the reload and `MaxItems` set the
   rate. The weapon choice never takes the dispenser.

## Acceptance (run and paste into the report)
- Unit tests: class keys and constructor defaults; lifetime and destroyed ends; land and bounce; the turn law and pitch
  limits; deploy lands upright on the ground; an enemy in range is shot dead for the owner; one beyond MaxRange is
  tracked but not fired on; blasts and bolts by BuildingScale until it explodes; expiry at 60 s; a second throw
  replaces the first; a bot throws only at a soldier in a cluster of his side, not at a lone one or one with a friend
  of the bot by him; over a long fight a bot keeps at most `MaxItems` alive and throws once a round, a reload apart;
  the BF2 scroll bytes parse to the band's rate; a HUD mesh part scrolls with the HUD clock; the render blend draws it
  between ticks; turrets are hashed and replay exactly.
- Census: every carrying BF2 class builds the slot (slot 4, channel 1, 1 clip), reads the disc's dispenser / turret /
  gun / explosion values, deploys, lands, fires at an enemy and expires at 60 s ± 0.2 (14 classes); the baked model
  (common and side) carries the band's scroll and its sheet is baked.
- Gates: `sim` declared.
