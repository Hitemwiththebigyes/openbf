# SWBF Modern Port — PLAN

(Trimmed 2026-10-08 from 934 lines. Everything cut was MOVED verbatim to `analysis/plan/`; each section below says where. Old section names — §Guardrails, §Milestones, §Architecture, §LOCKED FEATURE SET, §DESIGN SPECS — still resolve here or in the pointer table.)

# SWBF Modern Port — PLAN

North star: ONE game from two. A single Rust codebase for Windows/Linux/macOS that plays
like the PS2 Battlefronts (not the PC ports): controller-first with BF1 aim assist (keyboard + mouse added
2026-10-25 as a deliberate deviation, no assist on the mouse), all
maps of both games, BF2's shell/menus, BF1's loading screens and beeps, the better
subsystem from either game wherever they differ, "classic-plus" wgpu rendering (same
content, modernized look), and LAN multiplayer (cap 16). Original discs remain the asset
source; we ship no Pandemic assets or code — players bring their own ISOs.

## Architecture (locked)
1. **Rust runtime + wgpu renderer**; cross-platform via Vulkan/Metal/DX12.
2. **Offline asset bake** (LVL → glTF/KTX2/OGG/RON once per ISO); runtime never parses LVL.
3. **Data-driven missions, no Lua VM.** **RESOLVED 2026-08-30 (the maintainer): the Lua
   mod ecosystem is NOT a goal.** "A lot of what is out there just makes
   everything messy." If a specific mod is worth having, it gets ported into
   native Rust as a first-class feature rather than run as a script. This closes
   the "mlua shim later" footnote and the question Phoenix raised (see
   `analysis/community-projects.md`): we accept not running community Lua mods.
4. **Decomp = behavioral reference**, never source to port; PCSX2 side-by-side is the oracle.
5. **Content fidelity is non-negotiable; visuals are "classic-plus."** Same models, maps,
   silhouettes, palette; modern lighting/shadows/post ON by default (Minecraft
   Vibrant-Visuals philosophy), strict-faithful render mode as a toggle.
6. **Modes are composable rulesets over rosters.** Any mode's rules can run on any map's
   roster (the maintainer: hunt/hero-assault with conquest rules). Mode × map × era = data, not code.
7. **Session cheat registry.** BF2-style pause-menu cheat codes; effects last the session,
   reset on restart. Extensible (see Cheats below).

## LOCKED FEATURE SET (the maintainer, 2026-08-30 onward) — index
One line per decision; the full text, with every number, is VERBATIM in `analysis/plan/FEATURE_SET.md` (`[Ln]` = line n of that file). Read the full text before touching the area. Changing any of it is the maintainer's call.

**Movement & feel** (FEATURE_SET.md L5+)
- (the maintainer, 2026-09-03) Aim assist is NEVER off by default, at any difficulty. [L6]
- (the maintainer, 2026-09-03) Universalised unit behaviour. [L9]
- (the maintainer, 2026-10-27) One gameplay, both games: damage, health, stamina, jump, roll, grenades, blast push identical everywhere.
- Stances: one contextual button — moving = roll (both games had roll; [L24]
- Sprint: BF2 system, limited stamina. Infinite stamina exists only as a cheat. [L33]
- Jet/jump-pack: BF2 feel; infinite fuel via cheat. [L34]
- Aim assist on foot: BF1 passive magnetism only — no BF2 square-button X lock-on. [L35]

**Combat** (FEATURE_SET.md L39+)
- Weapon balance: fresh rebalance pass. Baked ODF stats are the starting point, then a tuning pass (documented per weapon, PS2 values kept in data for reference/revert). [L40]
- Awards & medals: KEEP (BF2 system) — grind-to-permanent-unlock stays. [L42]
- Classes: hybrid unlock. In-battle: BF2-style score unlock for special classes, every battle. [L44]

**Heroes** (FEATURE_SET.md L48+)
- Playable heroes (BF2) AND AI-controlled heroes (BF1 presence), both on all content. [L49]
- Heroes are killable, always — no BF1 invincibility ("you see a hero and you just run away — no fun"). [L50]
- Health (the maintainer, 2026-09-23 — reverses the earlier "flat pool, drop the drain/kill-to-heal" call): use BF2's standard hero health pool, the disc's own values (`RE:` them, do not guess), with … [L52]

**AI** (FEATURE_SET.md L60+)
- Infantry AI base: RESOLVED 2026-09-01 (the maintainer, Fable session 8): openbf's OWN AI — modern architecture (authored-graph navigation + local steering, a per-team utility commander, behaviour … [L61]
- (the maintainer, 2026-10-01) AI class weights. An AI spawn (and respawn) draws its class by weight: the team's alive-AI cap is shared out over its classes — the default class (the mission's first: … [L71]
- (the maintainer, 2026-10-01) Bots hold ground and fight, and not as statues. [L79]
- (the maintainer, 2026-10-01) AI bounds and the way back. [L91]
- Unit counts: default = PS2-authentic (16/team), settings slider to raise on modern hardware. [L99]

**Vehicles & space** (FEATURE_SET.md L106+)
- Ground vehicle handling: BF2 (verify BF1/BF2 similarity during RE; [L107]
- Flyers: faithful per source map — BF1 maps keep their flyable craft, BF2 maps stay as shipped. [L109]
- Aiming in vehicles, flyers and space (the maintainer, 2026-09-23 — likely, not yet locked: "will probably need"): keep BF2's lock-on-target mechanic there, while soldiers on foot keep BF1 magnetism. [L111]
- Space Assault: KEEP but needs rework — least-loved mode; [L117]

**Modes & rules** (FEATURE_SET.md L119+)
- Conquest: BF2 rules + BF1 no-overspawn invariant: live units on a team may never exceed remaining reinforcements — when a player respawns into the last slots, excess AI despawn (e.g., 3 … [L120]
- BF1 command-walker post exception approved by the maintainer 2026-10-22: live walkers count for bleed/countdown; verify lifecycle and Hoth generator details against the PS2 (B1WP; FEATURE_SET.md, Modes & rules).
- CTF: KEEP both variants, REWORK later — the maintainer barely played it (childhood reason: the clone commander says "damn" when the flag drops — optional VO filter noted); [L125]
- Hunt & Hero Assault: KEEP + rule-mixing per Architecture #6. [L128]
- BF1 Historical Campaigns: DROP (it is essentially curated conquest battles in movie order with narration stills/VO between; [L129]
- Special teams as options, played the conquest way (the maintainer, 2026-09-28). [L132]
- BF1 Naboo Plains always has the Gungan fambaa and its shield (the maintainer, 2026-09-28), in every mode, as the map's unique feature (the classes load only from `NAB1_H.LVL` on the disc; [L139]
- BF2 story campaign: KEEP. Stretch idea (LATER): extend/mod the story or a small new storyline of our own. [L142]
- Galactic Conquest: KEEP, BF2 galaxy-map + credits version; [L144]
- Era mix-and-match (NEW): traditional matchups by default, option for cross-era battles (e.g., droids vs stormtroopers). [L147]
- New modes (LATER, candidates): firefight/horde (defend a CP against escalating waves), gun game (weapon ladder per kill), king of the hill (single contested CP), elimination (no-respawn … [L150]

**Presentation & audio** (FEATURE_SET.md L155+)
- Widescreen-native (the maintainer, 2026-08-30): 16:9/16:10/ultrawide, hor+ — vertical FOV keeps the traced PS2 values (render::Camera is fov_y-based, so hor+ falls out of the projection; [L156]
- Shell/menus: BF2 theme & flow. Loading screens: BF1 incl. [L163]
- (the maintainer, 2026-09-03) One UI for the whole blended game: BF2's shell, menus and HUD on every map of both games. [L164]
- (the maintainer, 2026-09-03) One voice set, one command set, everywhere. [L171]
- HUD: BF2. [L176]
- End-of-round (the maintainer, 2026-10-27): BF1 scenic map shot + banner, then flip-through stats; "Victory/Defeat in N" when all CPs held; Triangle at a friendly CP = class change on all maps.
- Announcer/VO: per source map. SFX where games differ: BF2. [L178]
- Music: union, per-map source game. [L179]

**Cheats (session-scoped, BF2-style pause-menu codes)** (FEATURE_SET.md L181+)
- From the originals: invincibility, infinite ammo. [L182]
- New: infinite stamina, infinite jet fuel, all-awards-for-session. [L183]
- All reset on app restart; registry designed for easy additions. [L184]
- In MP/splitscreen: host-only control (see Saves, sessions & host authority). [L185]

**Saves, sessions & host authority (added 2026-08-30)** (FEATURE_SET.md L187+)
- Autosave everywhere. Each user profile autosaves continuously: career stats and award/unlock progression in every mode, single-player or multiplayer. [L188]
- Host (P1) sessions. LAN and splitscreen sessions have a designated host. [L192]
- Host-only session authority. In MP/splitscreen, only the host toggles cheats and session rules (e.g., awards on/off). [L195]
- MP Galactic Conquest autosave ownership: OPEN — host-only save vs replicated to all players (so anyone can continue solo) vs hybrid. [L197]

**Classic-plus assets (added 2026-08-30 — Harrisonfog remaster integration)** (FEATURE_SET.md L200+)
- (the maintainer, 2026-09-03) Render quality is a RUNTIME toggle, never a gameplay change. [L201]
- HD textures: bake learns to read PC-format LVL textures (D3D formats, audit A2 §5; [L225]
- Addon maps (`10-addon-maps-keep/`): map VARIANTS in the data model — one canonical entry per map, variants selectable (e.g. [L273]
- The look: reimplemented NATIVELY in wgpu. References in order: Shader Patch source (SleepKiller, MIT — reading github.com/SleepKiller/shaderpatch allowed and encouraged) with Harrisonfog's … [L276]
- Not wanted: Anakin's menu/shell framework (native BF2-style shell per PLAN), Remaster script manager, .exe patches. [L281]
- Replacement models (Part 2): later experiment (skeleton/anim risk) — textures first. [L285]
- Legal: everything stays local-only and uncommitted; [L287]

**Platform & meta** (FEATURE_SET.md L291+)
- Input: controller first; keyboard + mouse added 2026-10-25 as a deliberate deviation (pointer lock, mouse look as angles, no aim assist on the mouse); [L292]
- Splitscreen co-op: KEEP (roadmap: M5, before PACK; [L294]
- LAN: client-server UDP, cap 16, bots server-side. [L296]
- Saves/profiles: modern format; awards/unlock progression persists per profile. [L297]

**Dropped outright (never RE these binary regions)** (FEATURE_SET.md L299+)

## DESIGN SPECS — historical designs and deferred work
Preserved in `analysis/plan/SPECS_AND_DEFERRED.md` (also holds "Deferred design work", 8 items). These are dated designs, not a blanket list of unbuilt requirements; use `engine/CRATES.md` and the owning cards for current implementation and acceptance status: infantry-AI PHASE C design (perception / skill vector / four tiers), PHASE D design (squad commands), P7 vehicle entity shape (fields, hash order, AI dispatch on `Unit.mount`), the alpha MASK classifier. Cards such as VENT cite these.

## Milestones (the maintainer, 2026-09-25 — replaces §Phases and §Per-phase acceptance; their text is in `analysis/PLAN_LOG_2026.md`)
DONE: P0 audits, P1 bake, P2 viewer, P3 soldier, P4 combat, P5 vertical slice, P6 shell, P6.5 infantry AI A–D, P9's two
spine changes (N-pad `Sim::step`, libm-backed `core::fmath`). The loop measures every milestone below on
`engine/artifacts/loop/STATUS.md`'s scoreboard; cards (`engine/artifacts/loop/cards/`) carry a `milestone:`.

**First — M-ALPHA, the public alpha (the maintainer, 2026-09-29; defined by the planner 2026-09-30).** the maintainer shows the game in
public (a GitHub repo and a video) once it reads as a real game, not once every lighting detail matches. *Done when* a
full conquest round with drivable vehicles plays on six showcase maps and looks and sounds like a game in a 5-minute
video. The checklist: real effects (muzzle flash, bolts, impacts, explosions), the real BF2 HUD, gameplay audio,
grenades and pickups, class select on spawn, no soldier or AI out of bounds, no missing walls or sky, no droid chrome.
The showcase maps are BF1 NAB1, BF1 TAT1 (the port's `tat1`), BF1 YAV2, BF2 UTA1, BF2 MYG1 and BF2 HOT1. They were
picked for recognisable settings, ground vehicles, a finished sweep and few open cards naming them. Hoth is the
exception on the last count, kept for the video.
- Cards carry `alpha: yes`.
- What the alpha does not need carries `defer: post-alpha` and waits, without being deleted: heroes, the real shell,
  LAN, campaign, Galactic Conquest, splitscreen, and pure lighting-parity work off the showcase maps.
- *Measured by* STATUS's M-ALPHA scoreboard and the PARITY score on the showcase maps.
- M1–M3 below continue through their alpha cards first.

**Active, interleaved by value (the maintainer, 2026-09-25):**
- **M1 Looks right** — every map reads like the PS2 in character (colour, water, sky/fog, lighting, foliage, missing or extra
  objects), under the modern-port rules of `engine/artifacts/pcsx2/ref/COMPARISON_RULES.md`. *Done when* every
  high-confidence finding in `engine/artifacts/pcsx2/ref/FINDINGS.md` is fixed (its PCSX2 pair re-rendered and reading like
  the PS2) or ruled expected, and the PCSX2 library covers every map. *Measured by* open M1 cards and open findings.
- **M2 Vehicles** (old P7 + P6.5 E) — ground vehicles, then flyers, faithful per map, with vehicle AI. *Done when* every
  vehicle class each map ships spawns, is drivable/flyable by the player and by AI with its weapons, turrets and enter/exit,
  and a PCSX2 spot check per class reads right. *Measured by* vehicle classes spawned / drivable / AI-driven per map.

**Next, in order:**
- **M3 Plays right** — a full conquest round on every map ends by tickets, no unit stuck (the stuck census empty), the round
  sweep `finished 32/32`; PCSX2 gameplay spot checks. *Measured by* the sweep footer and the census.
- **M4 Heroes, space, awards** (old P8): Space Assault faithful first; heroes killable (BF2 hero health, no time drain, regen
  on kills); awards/medals; class meta-unlock.
- **M5 Campaign, Galactic Conquest, splitscreen, polish, Win/Linux/macOS builds** (old P10) — the shell and menus
  (card BSHL) ship a **Multiplayer** entry in BF2's main-menu slot, greyed out and unselectable, behind an `enabled` flag in
  data (the maintainer, 2026-09-30). **Splitscreen is here, before PACK, and is not held** (the maintainer, 2026-09-30 evening; card SSCR):
  local play on one machine — one sim, N cameras, N input devices, no netcode. It starts only after the alpha cards and
  M1–M3's open cards are done; the loop works it out without asking the maintainer. How the shell reaches it is SSCR's call (the
  greyed-out Multiplayer entry is LAN's).
- **M6 PACK, only on the maintainer's go-ahead when the core game is done (card PACK):** one self-contained game file (every baked
  asset inside, no DLLs or helper files) and one Python script that takes the two ISOs, bakes, compiles and emits that
  file. Dev builds keep the assets separate. PACK does not wait for multiplayer, and its design leaves room for it: adding
  netcode later must not force a redesign of the single-file build or the bake script (the maintainer, 2026-09-30).
- **M7 Multiplayer — LAN only, the LAST milestone, HELD until the maintainer says it is ready (the maintainer, 2026-09-30; splitscreen moved
  to M5 the same evening).** 16-player LAN lockstep on the spine already built. Until then every LAN / network /
  lockstep / lobby card carries `needs: maintainer — multiplayer hold` and the `net` crate stays the stub it is. The
  determinism spine stays healthy meanwhile (replay hashes, `fmath`, the sweep's byte-compare) — no netcode is written.
- Post-1.0 (old P11): new modes, space/CTF reworks, campaign extension, GC economy tuning.

**Always on — M0 Health:** ci green; referees that test what they say; files ≤ 1,500 lines (`scripts/size_report.sh`);
disk hygiene; the auditor's error rate low.

## How the project runs (2026-09-25 — replaces §Working mode, §Subagent orchestration, §Escalation policy; archived)
- **Scope and decisions (the maintainer, 2026-09-25; 2026-10-18).** The SOURCE is public (github.com/sprucey149/openbf, exported by
  `scripts/export_public.py`, committed there by the maintainer by hand) and community contributions are wanted; the game's content
  never is: each player builds their own game file from their own discs (`scripts/pack.py`), and no asset, golden or
  packed binary ever enters git or leaves the machine. The loop decides
  everything a faithful-yet-modern port needs — what the PS2 does is settled by evidence, where the maintainer has said nothing it stays
  true to the original and modern in presentation — including feel, recorded in `engine/artifacts/loop/FEEL_NOTES.md` for
  The maintainer's controller pass near the end. Only three things wait for the maintainer: a public export (the maintainer runs it),
  anything outside the repo/machine, and a NEW category of decision, which is asked under STATUS "Questions for the maintainer" with a
  reversible default taken meanwhile.
- **Full autonomy.** The loop (`scripts/autoloop.sh`, standing prompt `engine/artifacts/briefs/LOOP_PROMPT.md`) builds,
  lands, does its own RE and plans; the maintainer starts it, reads `engine/artifacts/loop/STATUS.md`, and steers by writing
  `engine/artifacts/loop/INBOX.md`. Only the maintainer: feel checks, legal questions, LOCKED FEATURE SET changes, pushing.
- **One plan.** This file holds the goals (§Milestones), the feature set and the specs. The backlog is the cards
  (`engine/artifacts/loop/cards/`, `BACKLOG.md` generated from them by `scripts/loopctl.py`); RE lives in `analysis/`;
  history is git, `engine/artifacts/loop/reports/` and `analysis/PLAN_LOG_2026.md`. No other planning documents.
- **Parallel work** only as worktrees on branches with disjoint file sets (`scripts/loopctl.py start <card>`:
  `<repo parent>/openbf-wt/<branch>` with the shared data symlinked and its own target dir
  `engine/artifacts/lead_tmp/<branch>_target`); main only fast-forwards, through `scripts/land.py` under the MAIN claim.
- **Gates** (`scripts/gates.py` against `engine/artifacts/loop/gates.toml`): ci, GPU referees + goldens, sim hashes, the
  round sweep, the arenas, byte-identical bakes. Every move is declared on its card; goldens are quantified and viewed before
  they are committed. Priority one is correct, clean code and a look that matches the discs under the stated deviations —
  speed never buys a weaker gate.
- **RE discipline** (the prompt's §RE, tools `scripts/re_check.py`): [V]/[D]/[H] tags, five self-checks, a foreground verifier,
  results lifted as `[O] UNAUDITED`; auditor sessions re-derive a sample. PROVISIONAL labels on every unproven constant.

## Determinism & capture/replay spine
The sim is **fixed-step** (rate from audit A5), **seeded RNG**, **no wall-clock in sim
logic**. The `game` binary must support from the start:
- `--headless` (no window; wgpu renders offscreen to a texture)
- `--replay <inputlog>` and `--record <inputlog>` (deterministic input playback/capture)
- `--capture-frame <tick> <out.png>` (offscreen render one frame to PNG)
- `--sim-hash <ticks>` (run N ticks, print a hash of sim state)
This single architecture is what makes autonomous testing possible (golden frames,
replay diffs, state hashing) AND is the foundation for lockstep/rollback netcode. Treat
it as non-negotiable.

## Guardrails (legal/safety — never violate)
- The maintainer's own discs; this is personal interoperability work. The engine is ORIGINAL code.
- **Never commit or upload** copyrighted assets (models/textures/audio/movies), the
  extracted ELFs, the decompiled C, or baked assets to any remote or public location. All
  are gitignored and stay local.
- End users supply their OWN discs and run the bake locally. Do not distribute a playable
  build containing LucasArts/Pandemic IP.
- Git stays LOCAL. No remote/GitHub push without explicit the maintainer approval (pending Q4).
- Keep `iso/`, `extracted/`, ELFs read-only.

## Pointers (moved sections, verbatim, `analysis/plan/OPTIMISATION_AND_EXECUTION.md` unless noted)
| Old PLAN.md section | Now |
|---|---|
| Optimisation backlog (perf scope, measured numbers, CPU/GPU items, sequence) | OPTIMISATION_AND_EXECUTION.md, first section |
| Reference: what we hold (decomp corpora, anchors, toolchain) | same file, 2nd section |
| Tech stack (locked; wgpu 27, winit 0.30, parry3d 0.22.1 load-bearing, no ECS) | same file — read before any dependency bump |
| Workspace layout | same file (`engine/CRATES.md` is the live module map) |
| Testing & fidelity ladder (Tier 0-4, the 300-frame median protocol) | same file |
| Fidelity method (RE to Rust) | same file |
| Community research (no-bloat rule) | same file |
| KICKOFF DECISIONS Q1-Q4 (fidelity oracle, autonomy, build order, git) | same file |
| Deferred design work + DESIGN SPECS | `analysis/plan/SPECS_AND_DEFERRED.md` |
| LOCKED FEATURE SET full text | `analysis/plan/FEATURE_SET.md` |
| Old Log, Phases, per-phase acceptance | `analysis/PLAN_LOG_2026.md` |
| RE index | `analysis/INDEX.md` (grep before any RE), `analysis/RE_LOG.md` |
