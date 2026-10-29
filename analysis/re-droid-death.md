# RE: the droid's death - the chunk burst, its odds, its effect and noise, its drop (both games) (card DROIDDEATH)

**The question** (maintainer's playtest note, 2026-10-28): "when droids were dealt enough damage, more than enough to
be killed or a headshot, they would have an explosion animation and have a noise go with it, and their body wouldn't
fall on the floor, just the drop item would appear." Which properties drive it, what is the threshold, which classes,
what effect and sound, what happens to the body and the drop.

**Method.** Read the soldier death block of both games (already located by card KNOCK, `re-knockback.md` §3 / §4),
the class setters of ChunkFrequency, ExplosionName and ChunkGeometryName, and the baked class data of both discs
(`engine/assets_baked/classes/*.ron`, every soldier class and every explosion class it names). Builds on
`re-knockback.md` §0 row 7, §1 step 2 and §3 (card KNOCK, [O] for BF1) and on card DCHK. No new PS2 run.
**Conventions.** B1 = `analysis/SWBFRONT.ELF-decomp.c`, B2 = `analysis/SWBFRNT2.ELF-decomp.c` (line numbers). [V] read
in the decomp or the disc data, [D] derived, [H] hypothesis. BF1 soldier S, class = S+0x958 (= *(decompiler detail omitted)*);
BF2 soldier E, class = E+0x4a4 (= *(decompiler detail omitted)*).

## 0. The answers

| # | question | answer |
|---|---|---|
| 1 | what drives it | three class keys: **`ChunkFrequency`** (0.6), the chunk geometry (**`CHUNKSECTION` / `ChunkGeometryName`**: without one the rule never fires) and **`ExplosionName`** (the effect) [V] |
| 2 | the threshold | **none**. The death block draws one U in [0, 1) at every death; `U <= ChunkFrequency` - or the sticky mark from an earlier push roll - bursts the droid. The damage, the overkill and the hit zone are not read; the overkill only scales the chunks' launch speed (0.1 + 0.018 x overkill, 1.0 from 50) [V]. So **6 deaths in 10** burst, whatever killed them [V]; a blast that pushes rolls once more first (§1), so about **84 in 100** blast kills burst [D: 1 - 0.4 x 0.4]. "More than enough damage / a headshot" is the maintainer's impression of a 60 % (bolts) / 84 % (blasts) rule [D] |
| 3 | which classes | BF1: the eight CIS droid soldier classes (`cis_inf_battledroid`, `_assault`, `_assassindroid`, `_pilotdroid`, `_battledroid_inf / _av / _sniper / _pilot`) [V]. BF2: every class under `cis_inf_default` - the super battle droid (`cis_inf_rifleman`), the battle droids (`rocketeer`, `engineer`, `sniper`, `pilot`, `marine`) and **the Magnaguard** (`cis_inf_officer`, `_officer_hunt`), which inherits the battle droid's `bdroid_chunk1-5` [V]. No other soldier class on either disc has chunk geometry [V]. Heroes (`com_jedi_default`: Grievous, Dooku, Maul, Jango) never burst [V] |
| 4 | the droideka | not this rule: a `walkerdroid`, not a soldier; it ALWAYS explodes at its death (`re-droideka-unit.md` §7.4: `ExplosionName` at the alive bit, again at Dead with its chunks) [V there]. The port does not field the droideka (`roster.rs`: its chain is rejected) |
| 5 | the effect | BF1 `com_inf_droid_exp` = an Effect `droid_explosion`; BF2 `cis_inf_droid_exp` = an Effect `cis_sfx_droid_explosion`. Neither authors Damage, Push or a `SoundProperty` [V: the baked classes]. The blast is detonated at the body's sphere centre (B1 FUN_002701a0, B2 FUN_0030c8f8) [V] |
| 6 | the noise | the class's **`DeathSound` `droid_death`** (every CIS droid class, both discs [V]), which the kill starts on every droid death, burst or not (`re-unit-chatter.md` 2.7) [D: the explosion class has no sound of its own, so the noise heard with the burst is the death cry] |
| 7 | the body | out of the collision world at once and hidden; no death clip, no corpse; the class's chunks are launched [V] |
| 8 | the drop | the same death call as any death (the health interface's vt+0xc = the item drop, B1 FUN_001ffec0) runs after the burst [V] |

## 1. The push roll and the mark [V]

From `re-knockback.md` §1 step 2 (BF1 FUN_002009a0, B1:112603-112612, [O]) and §4 (BF2 FUN_00238b80): a soldier whose
class has chunk geometry draws U at every push call, before the distance test. `U <= ChunkFrequency`: no push, and the
sticky mark (B1 S+0xbfd bit 3, B2 E+0xa74 bit 3), cleared only at the soldier's reset. Else pushed; a dead one's health
is set to -4.0. BF2's push is called only for the dead by the default `PushDeadOnly` (re-knockback §4).

## 2. The death block [V]

**BF2** FUN_00240bf8 (B2:159606-159669): states 0xf, 0x10 (dead) and 9-11 (thrown, tumble, get-up) skip it; with the
alive bit clear (E+0x228 bit 3):
- B2:159607-159612: one LCG draw (DAT_0073d870), `U <= class+0x121c` (**ChunkFrequency**: setter B2:171872 in
  FUN_00252bb0, hash 0x43b50014; ctor default -1.0 at B2:169375 in FUN_00250488) **or the mark** (E+0xa74 & 8);
- B2:159616: a class without chunk slot 0 (class+0x68, written by **`ChunkGeometryName`** 0xda328455 in the chunk
  section reader FUN_00214600, B2:128088-128097) takes the plain death;
- B2:159620-159637: with class+0x1238 (**`ExplosionName`**, hash 0x9b30e763: B2:171384 -> B2:171468) set and no
  network client gate, FUN_0030c8f8 (the blast) at the body;
- B2:159655-159660: chunk scale 1.0 if health <= -50, else -health x 0.018 + 0.1; FUN_00213630 (the chunks),
  FUN_001690c8 (out of the collision world), the draw's vt+0x50 (hidden);
- B2:159669: the death call (vt+0xc) - reached by both branches.

**BF1** FUN_00205cc8 (B1:116901-116968) is its twin: draw on DAT_0066eed0; `class+0x1e58 < U` and no mark (S+0xbfd &
8) = plain death (B1:116905-116911); chunk slot class+0x320 (B1:116917); `class+0x1e6c` (ExplosionName) ->
FUN_002701a0 (B1:116921-116940); the scale (B1:116956-116959); FUN_001e2630 (chunks), FUN_00168810 (out of the
world) (B1:116961-116962); the death call (B1:116968).

**What is not read**: the damage, the killer, the weapon, the hit zone. Only the health enters, and only the chunk
scale [V]. `re-hitzones.md`: BF1's head prim takes x3 damage; BF2 soldiers have no head multiplier - neither reaches
this branch [V there].

## 3. The disc [V]

`engine/assets_baked/classes/*.ron` (both discs' SIDE and COMMON LVLs), through BASE inheritance:
- BF1 `cis_inf_*` droids: ChunkFrequency 0.6, `CHUNKSECTION CHUNK1-5` (`sbdroid_chunk1-5` on the super battle droid,
  `bdroid_chunk1-5` on the rest), ExplosionName `com_inf_droid_exp`, DeathSound `droid_death`.
- BF2 `cis_inf_default`: ChunkFrequency 0.6, `bdroid_chunk1-5`, ExplosionName `cis_inf_droid_exp`, DeathSound
  `droid_death`; `cis_inf_default_officer` re-authors the same ExplosionName. Every fielded CIS soldier class inherits it.
- `com_inf_droid_exp` / `cis_inf_droid_exp`: one row each, `Effect`. Baked: `droid_explosion` in
  `common_bf1/effects.ron`, `cis_sfx_droid_explosion` in `side_bf2/effects/cis/effects.ron`.
- **Not baked**: the chunk models (`bdroid_chunk1-5`, `sbdroid_chunk1-5`): `find engine/assets_baked -iname
  '*droid_chunk*'` is empty.

## 4. The port (card DROIDDEATH, built)

- `core::unit::ChunkDeath` on `UnitClass::chunk` (frequency, explosion class), read by `game::roster::chunk_death`
  (ChunkGeometryName in the chain, else none; frequency default -1.0).
- `Sim::chunk_push_roll` in the blast's push (`sim/blast.rs`): §1 - the mark, no push; a pushed dead droid at -4.0.
  The port pushes the living on BF2 maps too (KNOCK's named deviation), so a living BF2 droid can be marked by a blast
  it survives; on the PS2 only BF2's dead are rolled.
- `Sim::chunk_death` from the death block (`sim/combat.rs`): §2 - the explosion (`Sim::explode`, no owner [H: the
  PS2's owner block {S+0x380, team, class, 0}]), `Event::DroidBurst { unit, pos, team, class, scale }`, the body out
  of the world, `Unit::burst`; the item drop follows unchanged. The kill's `Event::Kill` comes at the damage as before:
  the burst is decided a tick later (or at the landing of a thrown droid), so it is its own event.
- `[ours]`: the port draws U only for a class with a chunk death (the PS2 draws at every soldier's death block), so a
  battle without droids keeps its LCG stream.
- game: `Fx` spawns the explosion's effect at the event; `event_sounds` plays its SoundProperty (none on the disc);
  `soldier_vis` never draws a burst body. The DeathSound already played at the kill (card UCRY).
- **Not built**: the chunks (needs the soldier bake to emit `bdroid_chunk1-5` / `sbdroid_chunk1-5` as models from the
  CIS side LVLs, then VDTH's chunk launch at `scale`); the droideka (not fielded); the death of a soldier the world
  kills (the port's `kill_environment` leaves no body, so such a droid never bursts; the PS2's death block would run).

## 5. Open [H]

- The owner block passed to the burst's blast (S+0x380 / B2 argument 2[0xca]); harmless while no disc class deals damage.
- Whether a rider's death reaches the death block on the PS2 (the port gives a rider no body, hence no burst).
- No PS2 frame pair of the burst was taken (card DCHK acceptance 4 still open).
