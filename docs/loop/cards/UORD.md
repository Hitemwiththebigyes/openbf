---
id: UORD
title: Primaries that are not bolts fly straight in the port: missile (the torpedo launchers), shell (the IMP and CIS pilots' mortar), emitterordnance (the clone pilot's arc caster), bullet (the jet trooper's EMP launcher) - 13 of the 34 fielded BF1 classes
milestone: M3
value: 4
phase: build
status: blocked
deps: HVYW
files: engine/crates/core/src/weapon.rs, engine/crates/core/src/sim/ordnance.rs, engine/crates/core/src/sim/combat.rs, engine/crates/core/src/sim/guns.rs, engine/crates/game/src/roster.rs, engine/crates/core/src/ai/arsenal.rs, engine/CRATES.md
declares: sim; sweep; arena
needs: restore UORD replay rows, census tools, baked maps/classes/vehicles, missions and approved validation device
effort:
branch:
resume: Cloud preflight 2026-10-24 08:53 UTC: blocked before implementation; 26/29 prerequisite paths missing. Restore four uord/ps2 replay JSONs, audit-uord mutations, hvcen census and real bakes/missions; verify current main then loopctl start UORD. Preserve RE result/build acceptance and BF2 waver PROVISIONAL. Report engine/artifacts/loop/reports/20261024_0853_card_UORD.md; inventory lead_tmp/uord_20261024_0852/inventory.json.
created: 2026-09-29
alpha: yes
---

## Why
UAUD1 (analysis/unit-audit-bf1.md §3.1). The port builds every primary's ordnance as a straight bolt: core/src/weapon.rs 180-181 "Gravity
is deliberately NOT read ... bolts fly straight". The disc's ordnance BASEs
(`engine/artifacts/lead_tmp/unit-audit/census_ia.md`, "Weapons") say otherwise for 13 of the 34 fielded classes.

## Evidence
- missile: `*_weap_inf_torpedo_launcher_ord` (vanguard x3, shock trooper x2, ARC trooper, assault droid; the Wookiee
  rocketeer local).
- shell: `imp_weap_inf_mortar_launcher_ord1` (pilottie, pilotatst, pilotatat), `cis_weap_inf_mortar_launcher_ord1`.
- emitterordnance: `rep_weap_inf_arccaster_ord` (clone pilot).
- bullet: `rep_weap_inf_rocket_launcher_ord` (jet trooper).

## Acceptance
- RE ([O]) per ordnance class, both games: flight (guidance and lock time, the gravity arc, the emitter's beam or
  arc), damage and splash.
- Build in `core`; sim / sweep declared. Grenades (sticky / grenade kinds) are GREN's; vehicle weapons share these
  classes (VTUR).

## Notes
- UAUD2 (2026-09-29, analysis/unit-audit-bf2.md §3.1): BF2 has 26 of 49 fielded primaries that are not bolts: missile 12 (9 rocket
  launchers, 2 EMP launchers, the MagnaGuard's Bulldog), bullet 10 (9 shotguns, the clone commander's chaingun),
  emitterordnance 4 (3 incinerators, the dark trooper's arc caster).

## Planner note (2026-09-30, M-ALPHA)
`alpha: yes` — rockets and mortars that fly straight. Value 3 (kept). Showcase maps: BF1 NAB1, BF1 TAT1 (the port's `tat1`), BF1 YAV2, BF2 UTA1, BF2 MYG1, BF2 HOT1 (PLAN §Milestones M-ALPHA).

## Note from GREN (2026-09-30, analysis/re-grenades.md §4.1, §7.3)
The shell's flight is read and measured there: gravity = `Gravity` x -9.8 added to velocity.y before the move, 30 Hz,
position += velocity x min(life, dt); under water velocity -= (0.7 dt / (0.7 dt + 1)) x (vx, vy + 0.5, vz) (B1
`FUN_0027e478`, B2 `FUN_003202d0`); launch speed = `Velocity` lerped toward the `NextCharge` class's by the charge (B1
`FUN_002762a0`); the life timer and `ExplosionExpire` are the base's (B1 `FUN_00272e98`). The AI's aim for any ordnance
with gravity is the low-arc solver (§7.3). The mortar launchers are `grenade` WEAPONS (the throw cycle of §3) firing a
`sticky` / `shell` ordnance — card GREN builds that cycle and the sticky.

- GREN landed 2026-10-02 (d4886ba): a channel-0 slot (primary or the sidearm a player switches to with R3) is still
  built by `weapon_from_db` as a bolt weapon, so the mortar launchers (a `grenade` weapon + `sticky` ordnance on
  channel 0: BF1 pilots' primary, the smuggler's and BF2 officers' second slot) fire straight bolts. The thrown path
  exists (`thrown_weapon_from_db`, `Sim::launch` — the primary's shot loop would only need to call `launch`); it was
  kept off channel 0 because bots hold these as primaries and have no ballistic aim yet (card GRAI).

## Note (planner 2026-10-12 23:40): raised 3 -> 4, after HVYW
- Card HVYW (built on branch `hvyw`, not landed) makes rocket troops fire at vehicles and at groups of 3+ - and its
  report says a rocket at a group hits ONE man, because the port's missile is a straight bolt with no blast. So the
  blast (the ordnance's explosion: radius, damage by distance, who it pushes) is the first slice of this card, before
  the arcs of the shell and the grenade launcher. Measure with HVYW's census (`lead_tmp/hvyw/tools/hvcen.py`):
  soldiers hit per rocket fired at a group, against the PS2's ExplosionName numbers on the disc.
- `deps: HVYW` for order (both edit `core/src/weapon.rs`). Undo: `loopctl.py set UORD value=3 deps=`.

## RE result (2026-10-22, lane 2) -> phase build: missiles + shells + the lock
`analysis/re-ordnance-kinds.md` (OPUS RE [O], workflow uord-re-verify: 52 verdicts, 44 SUPPORTED, 8 PARTLY fixed, 0
refuted; live PCSX2 rows in `lead_tmp/uord/ps2/`). Split: the bolt family's per-tick unit sweep -> card UORDS (value
4); the emitter -> card UORDE (value 3). This card builds, in `core` + the class loader:
1. **The missile** (BF1 B1 FUN_0027d6b8, BF2 FUN_0031f2e8; doc §4.3): constant speed (BF2: MinSpeed -> Velocity at
   Acceleration, unless Acceleration > 9000); each tick ramp = min(1, ramp + dt), turn = ramp x TurnRate; rotate the
   direction toward `desired` by min(turn dt, |dir x desired|); waver about sin(ph1) p1 + sin(ph2) p3 by |v| x ramp x
   WaverTurn x dt, phases += 2 dt WaverRate U; no gravity. desired = the launch direction, or normalize(target point -
   pos) while the target lives (soldier: feet + (0, 1.4 / 0.6 / 0.2, 0) by stance - BF2 the class table; vehicle: the
   collision-sphere centre or the Targetable body). BF2 bot missiles at non-flyers add the AI aim error (re-soldier-ai
   17.2, no range factor).
2. **Missile and shell flight**: no predictive ray; a prev -> current sweep every tick against terrain, world, hulls,
   units, water (foliage passes); first contact = direct damage + the impact explosion at point + 0.2 n, then gone;
   life-out = ExplosionExpire (both kinds). Shell: v.y += Gravity x -9.8 x dt above water (the sticky's water drag
   under), LifeSpan default 3.0. Rebound / Friction on these classes are dead keys.
3. **The lock** (doc §5): a `launcher` weapon with missile ordnance and LockTime > 0 dwells while it holds a target:
   human = the magnet target (Target* kinds: vehicles for the torpedo / rocket launchers, BF2 bulldog soldiers; BF2's
   HUD-lock cone 25.8 deg), bot = its combat target (team + Target* / BF2 AITarget* kinds, no cone); the clock
   restarts on a new target; locked at LockTime (BF1 torpedo 1.5 s, BF2 rocket 1.0 s, BF2 EMP 0 = never); each shot
   consumes it; the missile gets the target only when locked. A cannon-class missile weapon (the wrist rocket) passes
   its held target at every shot. Bots do NOT wait for the lock (no AI fire path reads it).
Slices left as notes: the BF1 mortar's charge stage on channel 0 (tap = `_ord1` shell, hold > 0.675 s = the `_ord`
sticky; `FUN_00285108`); the lock HUD (BF1's shrinking square B1:262320-262339, BF2 `player%d.missileLock*`) -> a
HUD card when this lands; the BF2 shotgun tracers (visual).

### Acceptance (build)
- Replay tests on the PS2's own rows (copy 20-40 rows of `lead_tmp/uord/ps2/b1_m_free.json`, `b2_m_free.json`,
  `b1_homing.json`, `b2_homing.json` into a test as constants): the port's missile step predicts the next velocity to
  <= 1e-3 m/s (the PS2 law fits 1,400 pairs to 1.2e-4), the BF2 speed to 1e-3, and the homing desired direction to
  0.01 deg from the target's previous-tick point. Mutation-check: reversed rotation, no ramp, waver on the wrong axis.
- Lock tests: a torpedo launcher held on a hover tank locks at 1.5 s (BF1) / a rocket launcher at 1.0 s (BF2); a press
  at 1.4 s fires unguided; a target change restarts the clock; a shot consumes the lock; a torpedo never locks a
  soldier; a bot fires without waiting.
- Shell test: `imp_weap_inf_mortar_launcher_ord1` at 45 deg from flat ground bursts in the air at 3.0 s
  (ExplosionExpire); at 20 deg it explodes on its first ground contact; no bounce.
- Census (`lead_tmp/hvyw/tools/hvcen.py` or its twin): rockets fired at vehicles, share that hit a moving vehicle,
  main beside the branch (expected up); 8 x 32 sweep 256 / 256; team kills not above GRAIA's bar.

## Note (AUDIT 2026-10-22, lane 2: re-ordnance-kinds.md PARTLY, doc fixed in f0f0dfb)
- Pick the replay rows by what they exercise: of the 1,400 pairs, 442 BF1 pairs (two 40 m/s, two 80 m/s missiles in
  `b1_m_free.json`) never changed velocity - any TurnRate fits them - and the 582 BF2 free pairs never turn (desired =
  dir; all four BF2 classes there have WaverTurn 0): they test the speed law only. The turn / waver / ramp tests take
  the four REP torpedoes of `b1_m_free.json` (0xd32c10, 0xd33870, 0xd35290, 0xd327f0: 263 pairs, law 1.1e-4, null
  0.014), `b1_homing.json` (27) and `b2_homing.json` shots 0-1 (87; shot 2 is a TurnRate-0 class that
  `homing_check.py` scores with the rocket's values and misreads at 2.67 m/s). A mutation of the BF2 waver (axis,
  sign, phase order) cannot fail on these rows: either capture the BF2 EMP launcher (ord WaverRate 0.2, WaverTurn
  0.015) or label the BF2 waver PROVISIONAL. `replay_b1.py`'s default ALL line (max 0.755) groups by speed and is not
  the doc's figure. Mutation runs: `lead_tmp/audit-uord/mut/M1..M6` (sign, sin-vs-angle clamp, ramp, phase order,
  speed law, target height).
- IMP's torpedo ordnance authors no Waver key (REP / ALL 0.5 only): read the class, never a side table.
- The mortar shell at 45 deg bursts ~6.8 m up after 90 updates of v-then-position steps (not 7.7 m): use that number
  in the shell test if it asserts a height.
