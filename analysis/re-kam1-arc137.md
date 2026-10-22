# OPUS RE (2026-10-21) - [O] UNAUDITED
# RE: BF2 Kamino's arc 137 (hubs 121 <-> 123) - what the PS2 has between the two hubs, and what its bots do on it (card KA137)

**The question** (card KA137; the maintainer's INBOX 2026-10-01 14:40, item 5): the port's BF2 KAM1 arc 137 is drawn across
~20 m of sea with no collision under it, and the port's bots walked to its lip and stood there (RCYC recycled them at
(54, 54, 144); the STFAL audit saw whole-round stands at (52, 53, 139)). Is there a bridge the bake drops, a mode
layer, or a jump / jet flag on the arc?

**Answer.** None of the three. The arc is a plain soldier-only arc over open sea on the disc, the PS2 loads it as its
live graph, and the PS2's own bots walk it and **fall into the sea and die**. The port already keeps soldiers off it
(card BNDS's floor walk marks it unwalkable from both ends; the router rejects a dead arc), which is a deliberate
deviation from the PS2 and stays. On today's main the card's acceptance holds with no further change: 0 recycles,
0 stands and 0 falls near the spot over 24 seeds (section 6). The second half merged into the card (hub 92 under two
decks) has no symptom left on main and is split off as card KA92 (section 7).

Tags: [V] read (lines cited) or measured (files cited), [V-disc] read from the disc bytes, [V-RAM] read out of PS2 RAM in
a save state, [V-PS2] measured live in PCSX2, [V-port] measured on the port, [D] derived, [H] hypothesis. B2 =
`analysis/SWBFRNT2.ELF-decomp.c`. Scratch, tools and outputs: `engine/artifacts/lead_tmp/ka137/` (below `ka137/`). Port
indices = the disc's indices in the chunk the port bakes (hub 121 = `Hub123`, hub 122 = `Hub139`, hub 123 = `Hub140`).

## 1. Which graph the PS2 uses [O]

- [V] B2 `FUN_00177058` dispatches a level's chunk with tag 0x6e616c70 ('plan') to `FUN_002ec900` (B2:18268-18269).
  `FUN_002ec900` (B2:276828-276848) first frees the graph already in `DAT_005fe8b0` (`FUN_002ead80(DAT_005fe8b0,3)`,
  then `= 0`), allocates 0x3c bytes, constructs and stores the new graph there and fills it from the chunk
  (`FUN_002eb2b0`, B2:275760). Writers of `DAT_005fe8b0`: `FUN_002ec8e0`, `FUN_002ec8f0` (both `= 0`) and
  `FUN_002ec900` (`re_check.py writers b2 DAT_005fe8b0`, 5 writes). So with several `plan` chunks in one level the LAST
  one loaded is the live graph (RE_LOG 2026-09-01 said the same; the bake's `map.rs` takes the last top-level chunk).
  One exception: when `FUN_002eb2b0` fails (an empty chunk), B2:276843-276846 free the new graph and leave
  `DAT_005fe8b0 = 0` - an empty LAST chunk would clear the graph. KAM1's empty chunk is the first, so it is freed at once.
- [V-disc] KAM1.LVL carries three `plan` chunks, all at the top level, none inside an `lvl_` sub-level
  (`ka137/tools/lvltree.py`): payloads 0x4da90 (0 hubs, 0 arcs), 0x4dab8 (124 hubs, 141 arcs), 0x5f888 (143 hubs,
  172 arcs). The sub-levels (0x648a1cc2 `kamino1_campaign` at 0x3d8e78, 0x0ef868de `kamino1_conquest` at 0x4027f8,
  0x7db45a36 at 0x4045ec) hold worlds, lights and (the campaign one) models and collision, but no `plan`; the bytes
  'plan' occur three times in the file, each one of these chunk tags. Both conquest missions (`kam1c_con`, `kam1g_con`)
  load `kam\kam1.lvl` with `kamino1_conquest`.
- [V-RAM] In two KAM1 save states (`pcsx2/ref/states/bf2_kam1_ai0_gcw_sideselect.p2s`,
  `bf2_kam1_walk_cp2_cp3_b000.p2s`) the graph at `*(0x5fe8b0)` has **143 hubs and 172 arcs** (graph+4, graph+0xc; arcs
  stride 0x30, hubs stride 0x70: `FUN_002eb2b0`'s allocations, B2 ~275939-275966, and `FUN_002ea7d0`'s arc record
  +0 / +4 hub pointers, +8 mask, +0xc flags; the array header before the hubs also reads 143), and its arc 137 joins
  hubs 121 and 123 with mask 1, flags 0 (`ka137/tools/ramgraph.py`, `ka137/out/ramgraph.txt`). The port bakes the
  same chunk.

## 2. The arc on the disc [O]

- [V-disc] Arc 137 = the 26-byte ARCS record at KAM1.LVL 0x7670e: `Connection182`, hubA 0x79 = 121, hubB 0x7b = 123,
  typeMask **1 (SOLDIER only)**, flags **0** (raw `436f6e6e656374696f6e313832000000797b0100000000000000`). Hub 121
  `Hub123` (26.191, 69.66, 130.877) r 3.107; hub 123 `Hub140` (67.134, 69.66, 145.171) r 2.757; every hub of the chunk
  has y 69.66 (the editor's pick height, RE_LOG "Hub y is the editor's pick height"). Its neighbour arc 136
  `Connection181` (121 -> 122, mask 15) carries flags 3 (one_way + jump). In the older 124-hub chunk the same hubs
  have the same x, z and radius and y 0.0.
- [V-disc] The older 124-hub chunk has no arc between `Hub123` and `Hub140` (there `Hub123`'s one slot is
  `Connection183`, to `Hub122`); the names `Connection181` and `Connection182` occur once each in the whole file, both
  in the last chunk, while `Connection180/183/184` occur in both [receipt:47c4a2fa] [receipt:fca8fd9b]. The direct arc
  and the jump arc were added together in the final graph.
- [V-disc] The final chunk's SOLDIER table routes over arc 137 for **56 destinations from hub 121 and 74 from hub 123**,
  each the sole entry (row 0, weight 31) (`ka137/tools/rawplan.py`). The PS2 chooser never reads an arc's mask or
  flags (RE_LOG "BF2 arc `flags`").
- [V] A flags-0 arc is an ordinary leg: B2 `FUN_002c0220` reads the chosen arc's flags (the arc pointer at +0x50 of the
  path object at follower+0x4c, then +0xc; B2:245945-245950) and on 0 (B2:245951) sends the plain path request
  `FUN_002f3528` (B2:245952-245957); only a non-zero word builds the three-point jump path and its type-9 message
  (B2:245959-245998); `FUN_002bfce0` the same (B2:245689-245736). No jump, no jet.
- [V-disc] No bridge placed between the hubs: of KAM1.LVL's 545 world instances (15 worlds, sub-levels included), the
  only ones within 25 m of the arc's hub line (10 m before hub 121 to 10 m past hub 123) are two gun turrets
  (`tur_weap_built_gunturret8` (35.6, 52.6, 125.3), `gunturret14` (58.6, 51.4, 157.0)) and three sound regions
  (`ka137/tools/near_insts.py`; region records aside: the campaign's `flyer` box is not geometry). KAM1 places 18
  bridge instances [receipt:47c4a2fa]; the nearest, `kam_bldg_aaa_bridge2` at (81.6, 51.8, 154.4), lies 4 m off the
  line's EXTENSION 17 m past hub 123, beyond it. An instance's position is only its pivot, so this census cannot rule
  out a large model reaching over the line (the walkway itself is such a model): the PS2 drop and the PS2 falls in
  sections 3-4 do.

## 3. What is there on the PS2: open sea [O]

- [V-PS2] `ka137/ps2/gap_probe.py` (WADE's probe; no-AI GCW side-select state, SW renderer; log `ka137/ps2/run1.log`,
  rows `ka137/ps2/out/kam1_drop_*.csv`): a player soldier held 0.5 s with zero velocity and released
  - at (29.0, 62.5, 131.0), hub 121's deck: lands at **61.314**;
  - at (46.0, 54.5, 132.5), the walkway: lands at **52.681**;
  - at (44.0, 62.5, 137.1), **on arc 137's hub line: no floor** - he falls to y -46.0 and his health goes 400 -> 0.
  - the two bot falls of section 4 left the line's eastern end at ~(58.5, 144) and fell through (53-55, 142-143):
    no floor there either.
- Frames (`ka137/ps2/out/`): `kam1_deck_arc.png` (from hub 121's deck along the arc's bearing: the deck's rail, then
  hub 123's domed building), `kam1_deck_down.png` (pitch -25: the sea below the deck, the railed walkway rising toward
  the building), `kam1_walk_back.png` (from the walkway near hub 123 back along the arc: the railed walkway is the only
  way and ends under hub 121's platform), `kam1_drop_gap_mid.png` (the dropped soldier falling past the platform
  pillars toward the sea).
- [V-port] The port's collision agrees: FLOOR_GRID (main 2198036) has the walkway at 52.7 from (34, 122) to (62, 144),
  hub 121's deck at 61.3 (x 22-32), and no floor on the arc's line from x ~33 to ~53 (`ka137/out/fg_70.txt`); 724
  steady PS2 bot samples in the box lie within 1 m of the port's floor - the disagreements are bots seated in the two
  turrets (5,833 samples, `veh` 1; the port's grid reads the turret's top) and 14 cells of 1-9 rows each, all in the air
  during the arc-136 jumps (`ka137/tools/ps2_vs_port_floor.py`). Hub 123 sits inside the domed building: its resolved
  floor 51.5 is the interior's, the grid from y 70 reads the dome's roof (59-66).
- So the arc joins hub 121's deck (61.3) to hub 123 (51.5, inside the domed building) straight over the sea; the walk
  that exists is the railed walkway at 52.7, 8.6 m below hub 121's deck, which arc 135 (122 -> 123) follows; from the
  walkway nothing climbs to hub 121 - the deck connects to it only by the one-way jump 136 coming DOWN.

## 4. What the PS2's bots do on it [O]

- [V-PS2] MOUTH's follower captures (`lead_tmp/mouth/ps2/bf2_kam1_m{2,3}_units.csv`, 2 x 8 game minutes, 16 AI a side):
  the path follower walked arc 137 four times (both nodes on the two hubs' spheres), **all from 123 toward 121**:
  - m2 unit 32 (t 164.0-175.2): the walkway's rails carried him to its west end, (33.3, 52.7, 126.4), 8.6 m under hub
    121's deck; he jittered there with the stick pushed for ~4 s (171.5-175.2), then took a new leg;
  - m2 unit 32 (182.6-184.2): dropped the leg on the walkway after 1.6 s;
  - m2 unit 32 (203.0-205.3) and m3 unit 38 (359.9-362.1): walked the hub line straight out of the building, **off the
    ledge beside the walkway mouth at ~(58.5, 144) and into the sea**; each row empties 0.5 s after y 44 and the unit
    reappears at a spawn 5-19 s later (`ka137/out/ps2_arc137_walks.txt`, `ps2_falls.txt`).
  - No walk from 121 toward 123 in these captures (both verifiers' own scans, one with the node test loosened to 0.3 m).
- [V-PS2] The neighbour jump arc 136 (121 -> 122): 3 jumps; m2 unit 32 (t 142.8) landed on the walkway at (43.4, 52.7,
  128.9) by 144.4 and m3 unit 30 (171.1) at (40.6, 52.7, 124.6) by 172.7 - both lost health on landing (400 -> 371.3,
  94.2 -> 65.4) and then boarded `gunturret8`; m3 unit 35 (112.4) fell into the sea and died (row empty at 114.86)
  (`ka137/out/ps2_arc136_jumps.txt`, corrected by the verifiers). Three sea deaths in 16 minutes round these two arcs.
- [D] The PS2's arc 137 is an authoring defect the original lives with: its bots follow the table onto it, the rails
  save some, the rest drown. Not a behaviour to copy.

## 5. What the port does, and the deviation [O]

- [V-port] Main 2198036: `NAV_FLOORS=1` prints `nf bf2_kam1 arc 137 121-123 dead 1 1` - card BNDS's floor walk
  (`Sim::arc_unwalkable`, `core/src/sim/bounds.rs`) finds it unwalkable from both ends, and the router rejects a dead
  arc for a soldier on foot and re-routes (`core/src/ai/route.rs` 883, 929-951: the table without the arc, then the BFS
  over floored hubs, then the PS2's any-other-arc).
- **Named deviation (kept):** the PS2 walks arc 137 and its bots fall into the sea; the port's soldiers never take it.
  Where the original's bots annoy, the port does better (root CLAUDE.md, "Bots"). To undo: drop the `dead` test for
  this arc in `route.rs`; expect PS2-like sea deaths at (53-58, 141-144).

## 6. The card's acceptance on today's main [V-port]

`ka137/rto_2198036` (main's `round_to_outcome`, built 2026-10-21 23:09 from 2198036), **24 seeds x max 60,000 ticks**
with the recycle observer on (`STUCK_CENSUS=1 SWEEP_DEATHS=1`, plus BOT_SAMPLES; `ka137/tools/run8r.sh`, outputs
`ka137/main24r/`): every round ends with an outcome (team 0 wins in all 24, 414-741 s); **3 recycles map-wide, none
near the spot** - (-185.1, 68.3, 32.0) seed 2 and (-185.4, 68.3, 30.4) seed 16 (leg 100 -> 103, cp1), (-37.8, 62.6,
1.1) seed 3. Within 10 m of (54, 144): **0 stands of 20 s or more** (a bot life within 3 m of one point - a superset of
STFAL's LEDGESTAY >= 600 ticks), **0 `ledge` steer rows, 0 lives below y 45**, over 433.5 unit-seconds of bot rows in
the circle; the same at (52, 139) (412 unit-s) (`ka137/tools/stands.py`). No bot row within 2 m of the arc's line over
the sea (x 34-52), and no bot went between hub 121's deck and hub 123 either way (`ka137/tools/trips.py`): the port
routes round the pair. Hub 121's deck saw 5 bot lives (31.5 unit-s), hub 123 84 lives (333 unit-s). The lowest bot
feet in any row of the 24 rounds is y 47; the deaths lines count 0 falls, 0 region deaths. The zeros hold inside the
circles only: the map has 15-35 stands of 20 s or more per seed elsewhere. Seeds 1-8's result lines are byte-identical
to a run without the observers (`ka137/main8/`), and `ROUND_VERBOSE` (`ka137/main8v/`) shows `straight 0` fallbacks on
all 8. The RCYC (120,000-tick rounds, 2026-10-01) and STFAL counts predate the BNDS / TGAP / HUBP routing changes, in
rounds that did not end.

## 7. The second half: hub 92 under two decks (split to card KA92)

[V-port] Main 2198036: hub 92 (`Hub129`, the pod room, (82.4, 149.2) r 5.07) resolves its floor to 50.11 while a deck at
52.9-54.6 lies under its eastern half (FLOOR_GRID from y 56); arc 101 is dead from 92, arc 102 is dead from 92, and
the hub is `trapped` (card TGAP) - the router sends no soldier into it unless his goal is there, so the 228 m straight
plan the 2026-10-06 note saw no longer happens (`straight 0` on 8 seeds). No PS2 unit came within 14 m of the hub's
centre in 21 minutes of captures (m1-m3), so the PS2 gives no level for it. It is the port's floor pick, not an RE
question.

## Checks run
- `re_check.py fn b2 18269 276834 276838 245951 245952 245691`; `writers b2 DAT_005fe8b0`; absence receipts
  [receipt:47c4a2fa] (Connection182, the bridge classes), [receipt:fca8fd9b] (Connection181, Connection183).
- Disc: `ka137/tools/{lvltree,rawplan,near_insts}.py`; RAM: `ka137/tools/ramgraph.py`; PS2 live: `ka137/ps2/gap_probe.py`;
  captures: `ka137/tools/ps2_falls.py` + the walk / jump counts in `ka137/out/`; port: `run8.sh`, `run8r.sh`,
  `stands.py`, `trips.py`, `ps2_vs_port_floor.py`, FLOOR_GRID / NAV_FLOORS / ROUND_VERBOSE on `ka137/rto_2198036`.
- Verifiers (workflow, replacing the single verifier): `ka137-re-verify`, 5 agents (`effort: medium`) in 3 batches - the
  ELF + RAM batch (3 claims) to one, the disc batch (5 claims, 2 absence) and the measurement batch (6 claims) each to two
  that never saw each other's answer: 25 verdicts, 20 SUPPORTED, 5 PARTLY, 0 NOT SUPPORTED. Folded in: the empty-last-chunk
  case, the path object's +0x50, the layout's line numbers, the sub-level contents, the 18 bridge instances and the
  nearest one on the line's extension, the arc-136 landing points (two were turret seats), the respawn times and the
  8.6 m deck height. The second verifier overturned one claim: the first run's "0 recycle lines" had the recycle
  observer OFF (`round_to_outcome.rs` 3725 needs STUCK_CENSUS or SWEEP_DEATHS); it was re-measured on 24 seeds with
  it on and given to two fresh verifiers (`ka137-re-verify-2`, 2 agents): both SUPPORTED from their own counts. Verdict
  table: `ka137/out/verifiers.txt`.
- Still [H]: nothing load-bearing.
