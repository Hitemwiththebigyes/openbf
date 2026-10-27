# A3 — Lua-bindings diff → subsystem map

Comparing the Lua script-callback bindings of the two PS2 executables:

- **BF1** = SWBFRONT.ELF (Lua 4.0.1), `analysis/bf1-bindings.txt`
- **BF2** = SWBFRNT2.ELF (Lua 5.0.2), `analysis/bf2-bindings.txt`

Reproduce with `python3 scripts/bindings_diff.py` (grouped listing) or `--md`
(the tables in this file). Classification is heuristic by name; the rule table
lives in the script. Decomp bodies quoted below were pulled from
`analysis/SWBFRONT.ELF-decomp.c` / `analysis/SWBFRNT2.ELF-decomp.c`, where the
binding functions are already renamed (headers of the form
`/* ==== <addr> <name> ==== */`).

## Headline numbers

| | count |
|---|---|
| BF1 bindings (unique names) | **483** |
| BF2 bindings (unique names) | **768** (769 lines; `ScriptCB_SetNumBots` is bound at two addresses, 00193e28 and 0019ba28) |
| shared (by name) | **436** |
| BF1-only | **47** |
| BF2-only | **332** |

So ~90% of BF1's script surface survives verbatim in BF2, and BF2 nearly
doubles it. The expected ≈436/47/332 from the task brief is confirmed once the
BF2 duplicate is deduplicated.

### Subsystem sizes (shared / BF1-only / BF2-only)

| subsystem | total | shared | BF1-only | BF2-only |
|---|---|---|---|---|
| net/online (**dropped**) | 180 | 120 | 12 | 48 |
| profile/save/memcard (**mostly dropped**) | 72 | 35 | 11 | 26 |
| hud/interface | 114 | 87 | 0 | 27 |
| misc (world/env/memory/effects) | 70 | 35 | 2 | 33 |
| shell/menu/movie | 65 | 52 | 2 | 11 |
| mission/objective | 54 | 11 | 2 | 41 |
| ai/hero | 53 | 10 | 11 | 32 |
| soldier/unit | 48 | 5 | 0 | 43 |
| sound/music/vo | 47 | 35 | 0 | 12 |
| team/reinforcements | 34 | 14 | 1 | 19 |
| controller/input | 33 | 19 | 4 | 10 |
| camera | 14 | 6 | 2 | 6 |
| vehicle | 13 | 3 | 0 | 10 |
| spawn/cp | 11 | 2 | 0 | 9 |
| stats/awards | 7 | 2 | 0 | 5 |

The pattern: the **shell/UI/online plumbing is essentially the same engine**
(hud/interface, shell, sound, controller are almost all shared), while the
**growth is all in runtime mission scripting** — in BF1 the mission side of Lua
is little more than *setup* (declare teams, unit counts, memory pools, walker
types before load); BF2 turns Lua into a live *mission logic* language
(objectives, timers, entity queries, effects, AI goals, spawning). This is the
single biggest architectural difference the bindings reveal.

## Dropped regions — do NOT reverse-engineer

These bind into online services that are dead (GameSpy shut down 2014, Sony
DNAS shut down 2016) or into PS2 memory-card UX flow. They are the largest
region of the binding surface (~250 of 815 unique names across both games) and
none of it is needed for a playable decomp. Full lists are in the
`net/online` and `profile/save/memcard` tables at the bottom.

- **DNAS** (Sony auth): 8 bindings in BF1, 10 in BF2 (`ScriptCB_*DNAS*` —
  ShouldCheck/DoneCheck/HasSeenEULA/Start/IsDone/GetError/AllowSkip, plus
  BF2-only `CancelDNAS`). Dead service; shell scripts can be stubbed to
  "skip DNAS".
- **GameSpy**: session lists, lobbies, quickmatch, friends, login, clans,
  leaderboards (`ScriptCB_Begin/Cancel/Get/Update Session|Lobby|Quickmatch|
  Friends|Login...`), BF2-only `ScriptCB_IsLegalGamespyString`, BF1-only
  `ScriptCB_UploadJournals`/`DownloadJournals` (GameSpy stat journals) and the
  BF1-only `*DSSessionList*` family (dedicated-server session list).
- **Voice chat** (BF2-only, ~15 `ScriptCB_*Voice*` bindings): network voice —
  dropped with the rest of online.
- **Leaderboards/online stats** (BF2-only): `ScriptCB_EnumerateLeaderBoard*`,
  `SearchStatsRank`, `GetPlayerIDAtRank`, `SetLeaderBoardID`, ... — server-side.
- **Memory card / profiles**: `ScriptCB_*MemoryCard*`, `*Memcard*`,
  `*ExistOnCard*`, profile add/delete/mark/reload. Needed only insofar as the
  shell refuses to boot without a card check; candidates for stubbing, not RE.
  The **save-payload** side (BF1 `SaveSPState`/metagame saves, BF2 campaign
  saves `ScriptCB_StartSaveCampaign`/`SaveCampaignState`) is gameplay-adjacent
  and may eventually matter, but the card I/O beneath it does not.

Caveat: a handful of split-screen/system bindings (`ScriptCB_IsSplitscreen`,
`InMultiplayer`, `SetNumBots`...) get swept into net/online by the heuristic
but are used by local play too — check before writing anything in that table
off entirely.

## AI evidence (for the deferred infantry-AI decision: BF1 vs BF2 vs merge)

Factual inventory; no recommendation here.

### What each game exposes

| capability | BF1 | BF2 |
|---|---|---|
| Global difficulty (shell option) | `ScriptCB_SetDifficulty`/`GetDifficulty` (both) | same (both) |
| `ScriptCB_SetAIDifficulty` | present but a **no-op stub** (`return 0;` at 0017e7d8) | present but a **no-op stub** (`return 0;` at 0019c478) |
| Per-team mission difficulty | `SetPlayerTeamDifficulty`, `SetEnemyTeamDifficulty` + `...Easy/Medium/Hard` delta variants (**8 bindings, BF1-only**) | consolidated into one `SetAIDifficulty(a, b [, "easy"/"medium"/"hard"])` (0018dfc8, BF2-only) |
| Formation control | `SetAIFormationRadius` (BF1-only, 00185bb8 → FUN_00260ba8) | gone — no formation binding |
| AI goal system | **absent** (no `AIGoal` strings anywhere in the BF1 ELF) | `AddAIGoal`/`DeleteAIGoal`/`ClearAIGoals`; goal classes in strings: `AIGoal, ConquestGoal, CTFGoal, CTFGoalD, DeathmatchGoal, DefendGoal, DestroyGoal, FollowGoal` |
| Planning graph | **absent** (no `Planning` strings in BF1) | `Toggle/Block/UnblockPlanningGraphArcs` (region-name driven; `BlockPlanningGraphArcs` resolves two region-name lists then calls FUN_002ebe28 on planner object FUN_002ec9b8(-1)) |
| Barriers | **absent** (no `Barrier` strings in BF1) | `Enable/Disable/ToggleBarriers` (by name group, FUN_001923a0) |
| Hint nodes | **class hierarchy in strings**: `BaseHint, SnipeHint, PatrolHint, CoverHint, JetJumpHint, MinefieldHint, LandHint, JetJumpHintHelper, SnipeHelper` + debug var `ai.showhints` | only `BaseHint` string survives; no subclass RTTI names. `AddMissionHint` is unrelated (loading-screen tip text; the `HintOnScreen*` strings are HUD marker states) |
| Per-CP / spatial AI control | none | `AICanCaptureCP` (per-CP allow flag + team), `AISnipeSuitabilityDist`, `SetAIViewMultiplier`, `SetAIDamageThreshold`, `AllowAISpawn` (writes a bool at team+0x29d), `ForceAIOutOfVehicles`, `DisableAIAutoBalance`/`EnableAIAutoBalance` |
| Scripted spawning/orders | none | `SpawnCharacter`, `SelectCharacterClass/Team`, `AddAIGoal` per team **or per character**, `TranslateAICommand` (maps command ids → `waitforpickup`(4), `followchr`(0x10), `stopfollowchr`(0x12), `getintovehicle`(0x13), `getoutofvehicle`(0x14)) — i.e. BF2's squad-command system reaches Lua |
| Shared tuning knobs (identical in both) | `SetTeamAggressiveness`, `SetStationaryWaitTime`, `SetAIVehicleNotifyRadius`, `SetAttackerSnipeRange`, `SetDefenderSnipeRange`, `SetHeroClass` | same |

### What the decomp bodies say

- **BF1 difficulty is a pair of clamped global ints.**
  `SetPlayerTeamDifficulty` (00185360) writes `DAT_005875ec` clamped to
  [`DAT_00517db8`, `DAT_00517dbc`] then calls a recompute fn (FUN_00237af0);
  the `...Easy/Medium/Hard` variants *add a delta* to `DAT_005875f4` but only
  when a flag in the current profile (offset +0x4c, i.e. the shell difficulty
  setting) matches — so BF1 mission scripts bias AI skill per shell
  difficulty. BF2 replaced the whole family with one function taking an
  explicit level string hashed to 1/2/3 and forwarding to FUN_002a2988.
- **BF2's goal system is real engine machinery, not sugar.** `AddAIGoal`
  (00191d58) parses team index (<8), a goal-name string (hashed and switch-
  matched to an enum), a weight, and an optional target that can be an
  *entity* or a *path/CP index*, then constructs a goal via FUN_002e79a8 and
  returns a goal handle to Lua (`DeleteAIGoal` takes it back). Combined with
  the `ConquestGoal/CTFGoal/DefendGoal/...` class strings, BF2 ships a
  per-team goal-arbitration planner that BF1 has no trace of.
- **Navigation differs at the data level.** BF1 strings show `AIPath`,
  `AIPathNode` and the hint-node class family; BF2 adds the planning-graph
  arc/barrier vocabulary and exposes runtime blocking of both. This is
  consistent with the known asset split (BF1 paths vs BF2 planning files).
- **Hero AI:** BF1's hero is a mission-setup detail (`SetHeroClass`, plus the
  shell toggle `ScriptCB_AreHeroesOn`/`SetHeroesOn`, which writes a flag at
  profile+0x40). BF2 has an 11-binding hero subsystem: unlock rules
  (`SetHeroUnlockRule/TeamRule/PlayerRule/RespawnRule`), point-based unlock
  (`UnlockHeroForTeam`, `AcceptHero` — the latter walks the player struct and
  flips the character into the hero unit), SP variants
  (`EnableSPHeroRules`, `EnableSPScriptedHeroes`), net defaults
  (`ScriptCB_Get/SetNetHeroDefaults`), and presentation
  (`ScriptCB_EnableHeroMusic`/`EnableHeroVO`).

**Net factual picture:** the two AI stacks share their low-level soldier
tuning (aggressiveness, snipe ranges, stationary wait, vehicle notify — byte-
compatible binding sets), but the *command layer* is different: BF1 =
hint-nodes + formations + two global difficulty ints, no runtime scriptability;
BF2 = goal planner + planning graph/barriers + per-team/per-character orders +
squad commands, fully Lua-drivable. A "merge" would have to reconcile
different navigation data and a missing-vs-present goal arbiter, not just
different function names.

## Notable BF1-only bindings (47 total)

- **Historical-campaign rules**: `ScriptCB_AreHistoricalRulesOn` /
  `SetHistoricalRulesOn` — BF1's historical campaign toggle, dropped in BF2
  (replaced by the scripted campaign, `ScriptCB_*Campaign*`).
- **Difficulty family** (8 fns) and `SetAIFormationRadius` — see AI section.
- **Metagame (galactic-conquest v1) split-screen saves**:
  `ScriptCB_Start{Save,Load,Delete}MetagameSplitScreen`,
  `GetSavedMetagameListSplitScreen`, `DoesMetagameExistOnCardSplitScreen`,
  `SetMetagameTeams`, `Are/SetMetagameRulesOn`, `Begin/UpdateMPMetagame` — BF1
  ran GC through these; BF2 reworked GC and its save path.
- **SP checkpoint save**: `ScriptCB_SaveSPState`/`IsSPStateSaved` → superseded
  by BF2's campaign-state family.
- **Shell options that became option-blobs in BF2**: `Is/SetAutoAimOn`,
  `Is/SetRumbleOn`, `Is/SetFirstPersonView`, `IsToolTipOn/Auto`,
  `Get/SetFriendlyFire`, `SetHostLimit` — BF2 folds these into
  `ScriptCB_Get/SetGeneralOptions` and `Get/SetGameRules` (spot-check:
  the individual getters vanish while the blob calls appear).
- **Memory setup**: `SetModelMemory`, `SetLevelModelMem` → BF2's
  `SetPS2ModelMemory` + heap fns (`SetupTempHeap`, `ClearTempHeap`,
  `StealArtistHeap`, `SetWorldExtents`).
- **Dead online**: `*DSSessionList*` (5), `Upload/DownloadJournals`,
  `AddBleedThreshold` (BF1 scripted bleed thresholds; BF2 uses
  `SetBleedRate`/`GetBleedRate`).
- The task brief asked about **prone/stance**: no prone/stance/crouch binding
  exists in either list; stance handling is not script-visible in either game.

## Notable BF2-only bindings (332 total — the interesting clusters)

- **Runtime entity API** (~38): `GetEntityPtr/Name/Matrix/Class(Name|Ptr)`,
  `Create/DeleteEntity`, `SetEntityMatrix`, `SetProperty`/`SetClassProperty`,
  `GetObjectHealth/Shield/Team`, `KillObject`, `RespawnObject`,
  `Activate/DeactivateObject`, `GetCharacter{Unit,Team,Vehicle,Remote,...}`,
  `IsCharacterHuman`, `BatchChangeTeams`... — none of this exists in BF1.
- **Objective/timer machinery**: `ActivateObjective`, `CompleteObjective`,
  `MissionVictory`/`MissionDefeat`, `Create/Find/Destroy/Start/StopTimer`,
  `Set{Mission,Victory,Defeat}Timer`, `ShowTimer` — BF1 has only
  `AddMissionObjective` (shared) and hardcoded win logic.
- **Space battles**: `SpaceAssault*` (6), `AddLandingRegion`,
  `FillAsteroidPath/Region`, `EntityFlyer{TakeOff,Land,InitAsFlying,InitAsLanded}`,
  `EnableFlyerPath`, `AddShieldStrength`, `SetGroundFlyerMap`.
- **Heroes** (11) — see AI section.
- **CTF / modes**: `SetFlagGameplayType`, `GetFlagCarrier`,
  `AddFlagCapturePoints`, `CanCharacterInteractWithFlag`, hunt/assault/CON/CTF
  limit getters (`ScriptCB_Get*NumBots`, `Get*TimeLimit/ScoreLimit`).
- **Campaign & awards**: `ScriptCB_StartNewCampaign`, `Start{Save,Load,Delete}Campaign`,
  `SaveCampaignState`..., `GetCareerRank/Medals/PersonalStats`,
  `GetMedalStats`, `GetCareerMedalLevel` (awards/medal progression — offline
  flavor of the stats system; the leaderboard side is dropped-online).
- **Effects/cinematics**: `CreateEffect`/`RemoveEffect`/`AttachEffectTo*`,
  `Play/Pause/RewindAnimation`, `MoveCameraToEntity`, `ScriptCB_PlayInGameMovie`,
  `PlayInGameMusic`, map markers (`MapAdd{Class,Entity,Region}Marker`),
  `Print3D`, popups.
- **Galactic conquest v2** shares the `Metagame` name with BF1 but the
  surface differs (`ScriptCB_MetagameSetSide`, `ClearMetagameState`).

## Open questions

1. Does BF2 still *use* hint nodes internally (only `BaseHint` string
   survives; the subclass RTTI names are gone — renamed, stripped, or
   removed?). Worth one look at BF2 xrefs to `BaseHint` before deciding the
   AI merge question.
2. What exactly do BF2 `SetAIDifficulty`'s two integer args mean (team ids?
   player/enemy counts?) — FUN_002a2988 is the decode point.
3. `ScriptCB_SetAIDifficulty` is a stub in both games — confirm nothing in
   shipped shell scripts depends on a return value (both return 0 ≈ lua "no
   results", so likely safe to ignore).
4. The classifier sweeps a few local-play bindings (`SetNumBots`,
   splitscreen) into net/online — before declaring the whole table dead code,
   confirm bot-count and splitscreen paths, which local play uses.
5. BF1's `FUN_00237af0` (difficulty recompute) and the globals
   `DAT_005875ec/f4` — mapping these to actual skill parameters would tell us
   how much of BF1's "difficulty" is real behavior vs spawn-count bias.
6. `TranslateAICommand`'s id space (4..0x14) implies a larger internal
   command enum — enumerate it when the squad-command system is RE'd.

---

# Per-subsystem tables (generated by `scripts/bindings_diff.py --md`)

`—` = not bound in that game. Addresses are the real function addresses from
the binding tables (and match the renamed labels in the decomp corpora).



### sound/music/vo (47: 35 shared, 0 BF1-only, 12 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AudioStreamAppendSegments | 0018c620 | 0019ee98 |
| AudioStreamComplete | 0018c7a0 | 0019f0f8 |
| BroadcastVoiceOver | — | 0018f7f0 |
| CloseAudioStream | 0018c750 | 0019f048 |
| OpenAudioStream | 0018c558 | 0019edd0 |
| PlayAudioStream | 0018be98 | 0019e780 |
| PlayAudioStreamUsingProperties | 0018bd28 | 0019e610 |
| ScaleSoundParameter | 0018d278 | 0019fcb0 |
| ScriptCB_EnableCommandPostVO | — | 001a0580 |
| ScriptCB_EnableHeroMusic | — | 001a05c8 |
| ScriptCB_EnableHeroVO | — | 001a0610 |
| ScriptCB_GetBassManagement | — | 001a0c60 |
| ScriptCB_GetMixConfig | 0018dda8 | 001a0b40 |
| ScriptCB_GetMixConfigChanged | — | 001a0c30 |
| ScriptCB_GetOutputMode | 0018dcc8 | 001a0a18 |
| ScriptCB_GetVolumes | 0018dad8 | 001a0740 |
| ScriptCB_IsShellMusicPlaying | 0018e230 | 001a10e0 |
| ScriptCB_NextMixConfig | 0018ddd8 | 001a0bd0 |
| ScriptCB_NextOutputMode | 0018dd50 | 001a0ac8 |
| ScriptCB_PlayAuditionEffect | 0018dde0 | 001a0c88 |
| ScriptCB_PlayInGameMusic | — | 001a04a0 |
| ScriptCB_PreviousMixConfig | 0018ddd0 | 001a0b70 |
| ScriptCB_PreviousOutputMode | 0018dcf8 | 001a0a50 |
| ScriptCB_ResetSoundToDefault | — | 0019c890 |
| ScriptCB_SetBassManagement | — | 001a0c58 |
| ScriptCB_SetDopplerFactor | — | 001a12e8 |
| ScriptCB_SetMovieAudioBus | 00183288 | 00185968 |
| ScriptCB_SetShellMusic | 0018e128 | 001a0fd0 |
| ScriptCB_SetShellMusicInterval | 0018e198 | 001a1040 |
| ScriptCB_SetVolumes | 0018db98 | 001a0858 |
| ScriptCB_ShellPlayDelayedStream | 0018e298 | 001a1148 |
| ScriptCB_SndBusFade | 0018ba88 | 0019e358 |
| ScriptCB_SndPlaySound | 0018bb88 | 0019e458 |
| ScriptCB_SoundDisable | 0018e408 | 001a12a8 |
| ScriptCB_SoundEnable | 0018e3f0 | 001a1290 |
| ScriptCB_StopInGameMusic | — | 001a0558 |
| ScriptCB_TriggerSoundRegionEnable | — | 001a0658 |
| SetAmbientMusic | 0018d040 | 0019fa68 |
| SetBleedingVoiceOver | 0018ca38 | 0019f2e8 |
| SetDefeatMusic | 0018d208 | 0019fc38 |
| SetLowReinforcementsVoiceOver | 0018cb28 | 0019f3d8 |
| SetOutOfBoundsVoiceOver | 0018da68 | 001a06d8 |
| SetPlanetaryBonusVoiceOver | 0018c820 | 0019f2d8 |
| SetReservesVO | 0018c990 | 0019f2e0 |
| SetSoundEffect | 0018cc38 | 0019f4f0 |
| SetVictoryMusic | 0018d198 | 0019fbc0 |
| StopAudioStream | 0018c1b8 | 0019ea70 |

### net/online (180: 120 shared, 12 BF1-only, 48 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| ScriptCB_AllowSkipDNAS | 0018a950 | 001998c0 |
| ScriptCB_ApplyFilters | — | 00197028 |
| ScriptCB_AutoNetJoin | 00185e58 | 001937b8 |
| ScriptCB_BeginDSSessionList | 00189190 | — |
| ScriptCB_BeginFriends | 0018a5c8 | 001994b8 |
| ScriptCB_BeginJoin | 00189678 | 001987a0 |
| ScriptCB_BeginJoinIP | 00189450 | 00198218 |
| ScriptCB_BeginJoinSpecial | 001894e0 | 00198340 |
| ScriptCB_BeginLeave | 001899b0 | 00198ae8 |
| ScriptCB_BeginLobby | 001873e0 | 00194648 |
| ScriptCB_BeginMPMetagame | 00186e18 | — |
| ScriptCB_BeginQuickmatch | 00189420 | 001981e8 |
| ScriptCB_BeginSessionList | 00188548 | 00195a70 |
| ScriptCB_CanClientLeaveStats | — | 0019b250 |
| ScriptCB_CanSupportMaxPlayers | — | 001985c8 |
| ScriptCB_CancelConnectSockets | 0018ae20 | 00199d50 |
| ScriptCB_CancelDNAS | — | 00199fb0 |
| ScriptCB_CancelDSSessionList | 00189288 | — |
| ScriptCB_CancelFriends | 0018a600 | 001994e8 |
| ScriptCB_CancelJoin | 00189878 | 001989c8 |
| ScriptCB_CancelLeaderBoardPlayerData | 00184310 | 00186ab8 |
| ScriptCB_CancelLeave | 00189aa8 | 00198bf0 |
| ScriptCB_CancelLobby | 00188090 | 001954d0 |
| ScriptCB_CancelLogin | 00187308 | 001945c0 |
| ScriptCB_CancelQuickmatch | 001895d8 | 00198480 |
| ScriptCB_CancelSessionList | 00188868 | 00195f68 |
| ScriptCB_ClearPrevSessionId | — | 00195b08 |
| ScriptCB_ClientGotStats | 0017d2f8 | 001a20c0 |
| ScriptCB_DoFriendAction | 0018a840 | 00199700 |
| ScriptCB_DoneCheckDNAS | 0017acc8 | 0017f058 |
| ScriptCB_DoneCheckPatch | 0017ad80 | 0017f110 |
| ScriptCB_DownloadJournals | 00186bf8 | — |
| ScriptCB_DownloadLeaderBoardData | 001841f0 | 00186980 |
| ScriptCB_EnableJournal | 00186b98 | 00194078 |
| ScriptCB_EndAutoNet | 001862f8 | 00193c78 |
| ScriptCB_EnumerateLeaderBoard | — | 00186ba0 |
| ScriptCB_EnumerateLeaderBoardDone | — | 00186ca8 |
| ScriptCB_GetAmHost | 00186080 | 001939f8 |
| ScriptCB_GetAutoNetGameName | 00178d30 | 0017e6e0 |
| ScriptCB_GetAutoNetIsDedicated | 00178d10 | 0017e6b8 |
| ScriptCB_GetAutoNetMode | 00186308 | 00193c88 |
| ScriptCB_GetAutoNetScript | 00178e00 | 0017e798 |
| ScriptCB_GetAwardStats | 0017df88 | 001a33b8 |
| ScriptCB_GetCareerPersonalStats | — | 001a3b88 |
| ScriptCB_GetCmdlineLogin | — | 00198190 |
| ScriptCB_GetConnectSocketsStatus | 0018ad38 | 00199d18 |
| ScriptCB_GetConnectType | 00186b10 | 00193ff0 |
| ScriptCB_GetDNASError | 0018b3e0 | 0019a100 |
| ScriptCB_GetDSSessionList | 00189290 | — |
| ScriptCB_GetEnumerateLeaderBoardData | — | 00186d20 |
| ScriptCB_GetExtraSessionInfo | — | 00196110 |
| ScriptCB_GetFilters | — | 00196d70 |
| ScriptCB_GetFriendActions | 0018a7a8 | 001995f8 |
| ScriptCB_GetFriendListIcon | 0018a920 | 00199888 |
| ScriptCB_GetFriendStateStr | 0018a670 | 00199580 |
| ScriptCB_GetFriendlyFire | 00179d38 | — |
| ScriptCB_GetIPAddr | 00189bd8 | 00198d38 |
| ScriptCB_GetLeaderBoardPlayerData | 00183fe8 | 00186810 |
| ScriptCB_GetLobbyPlayerCount | 0018b8c0 | 0019a460 |
| ScriptCB_GetLobbyPlayerFlags | 00189ef8 | 00198de0 |
| ScriptCB_GetLobbyPlayerlist | 00187940 | 00194c18 |
| ScriptCB_GetLoginList | 0017a7a8 | 00187e10 |
| ScriptCB_GetMedalStats | — | 001a3850 |
| ScriptCB_GetNetGameDefaults | 00186460 | 0019d530 |
| ScriptCB_GetOnlineService | 00186008 | 00193980 |
| ScriptCB_GetPS2PatchCheckError | 0018b7b0 | 0019a350 |
| ScriptCB_GetPersonalStats | 0017d9d0 | 001a2d18 |
| ScriptCB_GetSessionList | 00188b58 | 00197c50 |
| ScriptCB_GetSessionListPercent | — | 00195b18 |
| ScriptCB_GetSessionSortMode | 00188a98 | 00197170 |
| ScriptCB_GetSpectatorMode | — | 0019bf38 |
| ScriptCB_GetUniqueLoginName | 0017c8b8 | 00189ae8 |
| ScriptCB_GetVoiceCaptureConnected | — | 0019b0d8 |
| ScriptCB_GetVoiceEnable | — | 0019a858 |
| ScriptCB_GetVoiceReceivePreset | — | 0019aae8 |
| ScriptCB_GetVoiceReceiveStatus | — | 0019ae70 |
| ScriptCB_GetVoiceRenderConnected | — | 0019b028 |
| ScriptCB_GetVoiceSendPreset | — | 0019a9e0 |
| ScriptCB_GetVoiceSendStatus | — | 0019ac30 |
| ScriptCB_GetVoiceTVGain | — | 0019b188 |
| ScriptCB_HasNewPatch | 0018b7f8 | 0019a3a0 |
| ScriptCB_HasSeenDNASEULA | 0017acd8 | 0017f068 |
| ScriptCB_HasServerLaunched | 00187f58 | 00195398 |
| ScriptCB_InMultiplayer | 00186108 | 00193a88 |
| ScriptCB_InNetGame | 001860a8 | 00193a28 |
| ScriptCB_InNetSession | 00186140 | 00193ab8 |
| ScriptCB_IsAutoNet | 001862d0 | 00193c48 |
| ScriptCB_IsBadNetworkConnection | — | 001985f0 |
| ScriptCB_IsBootInvitePending | 00186440 | 00193dd0 |
| ScriptCB_IsCmdlineJoinPending | — | 00198148 |
| ScriptCB_IsDNASDone | 0018b278 | 00199fd8 |
| ScriptCB_IsDedicated | 00186258 | 00193bb8 |
| ScriptCB_IsEnableLeaderBoard | — | 00187040 |
| ScriptCB_IsGuest | — | 00194620 |
| ScriptCB_IsJoinDone | 00189918 | 00198a58 |
| ScriptCB_IsLeaveDone | 00189ab0 | 00198bf8 |
| ScriptCB_IsLegalGamespyString | — | 0017d338 |
| ScriptCB_IsLoggedIn | — | 00194360 |
| ScriptCB_IsLoginDone | 001870a8 | 001942f8 |
| ScriptCB_IsNetCableIn | 00189b78 | 00198cd0 |
| ScriptCB_IsNetHWPresent | 00189b50 | 00198ca0 |
| ScriptCB_IsNetworkOn | 00186398 | 00193d20 |
| ScriptCB_IsPS2PatchCheckDone | 0018b4f0 | 0019a1e0 |
| ScriptCB_IsPlayerLoggedIn | 0017b128 | 00188660 |
| ScriptCB_IsQuickmatchDone | 00189610 | 001984b8 |
| ScriptCB_IsQuickmatchPassworded | — | 00198518 |
| ScriptCB_IsSessionReady | 00188900 | 001960c8 |
| ScriptCB_IsSpecialJoinPending | 001893e8 | 00198118 |
| ScriptCB_IsUniqueLoginName | 0017ca00 | 00189c28 |
| ScriptCB_LaunchDSSessionList | 00189258 | — |
| ScriptCB_LaunchJoin | 00189820 | 001989c0 |
| ScriptCB_LaunchLeave | 00189aa0 | 00198be8 |
| ScriptCB_LaunchLobby | 00187fa0 | 001953d8 |
| ScriptCB_LaunchQuickmatch | 00189570 | 001983e8 |
| ScriptCB_LaunchSessionList | 00188758 | 00195e48 |
| ScriptCB_LeaderBoardDownloadDone | 00184298 | 00186a40 |
| ScriptCB_LobbyAction | 00188140 | 00195590 |
| ScriptCB_Logout | 0017c678 | 0018a6d0 |
| ScriptCB_NeedToSelectNetConfig | 0018a970 | 001998e8 |
| ScriptCB_NetWasClient | 00186220 | 00193b88 |
| ScriptCB_NetWasDedicated | 001861b0 | 00193b18 |
| ScriptCB_NetWasDedicatedQuit | 001861e8 | 00193b48 |
| ScriptCB_NetWasHost | 00186178 | 00193ae8 |
| ScriptCB_NextHost | 00189130 | 001980b8 |
| ScriptCB_PauseSessionList | — | 00196090 |
| ScriptCB_PersonalStatsSetTitles | — | 001a2b28 |
| ScriptCB_PreviousHost | 00189160 | 001980e8 |
| ScriptCB_QuitFromStats | 0017cc50 | 0017f7b8 |
| ScriptCB_ResetLeaderBoardPlayerData | 00184330 | 00186b30 |
| ScriptCB_SearchStatsRank | — | 0019b320 |
| ScriptCB_SetAmHost | 00186040 | 001939b0 |
| ScriptCB_SetAsFavorite | — | 00197288 |
| ScriptCB_SetConnectType | 00186a40 | 00193f18 |
| ScriptCB_SetDedicated | 00186280 | 00193be8 |
| ScriptCB_SetFilters | — | 00196e88 |
| ScriptCB_SetFriendlyFire | 00179db0 | — |
| ScriptCB_SetHasNewPatch | 0018b820 | 0019a3d0 |
| ScriptCB_SetHasSeenDNASEULA | 0017ad20 | 0017f0a8 |
| ScriptCB_SetHostLimit | 00186970 | — |
| ScriptCB_SetInNetGame | 001860e0 | 00193a58 |
| ScriptCB_SetLeaderBoardID | — | 00186ad8 |
| ScriptCB_SetNetGameDefaults | 001866c0 | 0019db30 |
| ScriptCB_SetNetLoginName | 00189c10 | 00198d68 |
| ScriptCB_SetPlayerStatsPoints | — | 001a4a38 |
| ScriptCB_SetSessionSortMode | 00188ac8 | 001971a8 |
| ScriptCB_SetSpectatorMode | — | 0019beb0 |
| ScriptCB_SetTeamStatsSortMode | — | 001a2148 |
| ScriptCB_SetVoiceLocalEchoEnable | 0018b908 | 0019a928 |
| ScriptCB_SetVoiceReceiveEnable | — | 0019adb0 |
| ScriptCB_SetVoiceReceivePreset | — | 0019aa68 |
| ScriptCB_SetVoiceSendEnable | — | 0019ab70 |
| ScriptCB_SetVoiceSendPreset | — | 0019a960 |
| ScriptCB_SetupAutoNetIsLan | 00178dc8 | 0017e768 |
| ScriptCB_SetupAutoNetMissions | 00178da8 | 0017e748 |
| ScriptCB_ShouldCheckDNAS | 0017aca0 | 0017f028 |
| ScriptCB_ShouldCheckPatch | 0017ad58 | 0017f0e0 |
| ScriptCB_StartDNAS | 0018b090 | 00199e48 |
| ScriptCB_StartLogin | 001871a0 | 00194458 |
| ScriptCB_StartLoginDedicatedServer | — | 001943b0 |
| ScriptCB_StartPS2PatchCheck | 0018b430 | 0019a128 |
| ScriptCB_TeamStatsGetTeam1 | 0017d860 | 001a2728 |
| ScriptCB_TeamStatsGetTeamColor | — | 001a2780 |
| ScriptCB_TeamStatsSetTitles | — | 001a2910 |
| ScriptCB_TeamStatsValidatePos | 0017d8e0 | 001a2c28 |
| ScriptCB_TrackLoginErrors | 001873b8 | 001945e8 |
| ScriptCB_TrimLoginName | — | 001899c0 |
| ScriptCB_UpdateDSSessionList | 001891c8 | — |
| ScriptCB_UpdateFriends | 0018a608 | 001994f0 |
| ScriptCB_UpdateJoin | 001897f0 | 00198988 |
| ScriptCB_UpdateLeave | 00189a28 | 00198b68 |
| ScriptCB_UpdateLobby | 00187bc8 | 00194fa8 |
| ScriptCB_UpdateMPMetagame | 00186e78 | — |
| ScriptCB_UpdateNetConfigs | 0018ac00 | 00199ba0 |
| ScriptCB_UpdateQuickmatch | 00189538 | 001983b0 |
| ScriptCB_UpdateSessionList | 001885e8 | 00195b70 |
| ScriptCB_UploadJournals | 00186bb8 | — |
| ScriptCB_VoiceEnable | — | 0019a688 |
| ScriptCB_VoteKick | 00188450 | 00195970 |
| ScriptCB_VoteOrNominate | 00188540 | 00195a68 |
| SetTeamAsFriend | 00184d30 | 0018c258 |

### profile/save/memcard (72: 35 shared, 11 BF1-only, 26 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| DeleteAIGoal | — | 00191ce8 |
| DeleteEntity | — | 00190450 |
| IsCampaign | — | 0018e730 |
| ScriptCB_AddProfile | 0017b340 | 00188868 |
| ScriptCB_AreMetagameRulesOn | 0017e710 | — |
| ScriptCB_CheckMemoryCardMark | 0017bf58 | 00189d30 |
| ScriptCB_ClearCampaignState | — | 0018a848 |
| ScriptCB_ClearMetagameState | — | 0018a7b0 |
| ScriptCB_DeleteSavedMetagameState | 0017e6d0 | — |
| ScriptCB_DoInitialMemcardCheck | 0017ac68 | 00188488 |
| ScriptCB_DoesCampaignExistOnCard | — | 0018a660 |
| ScriptCB_DoesMetagameExistOnCard | 0017c5a8 | 0018a290 |
| ScriptCB_DoesMetagameExistOnCardSplitScreen | 0017c610 | — |
| ScriptCB_DoesProfileExistOnCard | 0017c000 | 00189dc0 |
| ScriptCB_GetCurrentProfileName | 0017b1a8 | 00188708 |
| ScriptCB_GetCurrentProfileNetName | 0017b2b8 | 001887f8 |
| ScriptCB_GetGSProfileInfo | 0018a070 | 00199010 |
| ScriptCB_GetProfileName | 0017b228 | 00188770 |
| ScriptCB_GetSavedCampaignList | — | 0018a300 |
| ScriptCB_GetSavedMetagameList | 0017c100 | 00189f30 |
| ScriptCB_GetSavedMetagameListSplitScreen | 0017c328 | — |
| ScriptCB_GetSkipToNTGUIProfileName | 0018b860 | 0019a418 |
| ScriptCB_InMemoryProfileMode | 0017c068 | 00189e38 |
| ScriptCB_IsCampaignStateSaved | — | 0018a858 |
| ScriptCB_IsCurProfileDirty | 0017aaf8 | 00188250 |
| ScriptCB_IsCurrentCampaign | — | 00189990 |
| ScriptCB_IsCurrentMetagame | 0017bdc8 | 00189688 |
| ScriptCB_IsFileExist | — | 00184b78 |
| ScriptCB_IsMemoryCardInsertted | 0017bfc8 | 00189d90 |
| ScriptCB_IsMemoryCardUnformatted | 0017bf90 | 00189d60 |
| ScriptCB_IsMetagameStateSaved | 0017e3b0 | 0018a7c0 |
| ScriptCB_IsMissionSetupSaved | — | 0018a988 |
| ScriptCB_IsProfileDirty | 0017ab40 | 001882d8 |
| ScriptCB_IsSPStateSaved | 0017e3d0 | — |
| ScriptCB_LoadCampaignState | — | 0018a820 |
| ScriptCB_LoadMetagameState | 0017e370 | 0018a788 |
| ScriptCB_LoadMissionSetup | — | 0018a950 |
| ScriptCB_LoadSavePopupResult | 0017abf8 | 00188410 |
| ScriptCB_MakeFakeProfiles | — | 00188f70 |
| ScriptCB_MarkCurrentProfile | — | 001890a8 |
| ScriptCB_MarkMemoryCard | 0017bf38 | 00189d10 |
| ScriptCB_MetagameSetBonus | 0017e6f0 | 0017fa50 |
| ScriptCB_MetagameSetSide | — | 0017fa58 |
| ScriptCB_PostLoadHack | — | 0017ff20 |
| ScriptCB_ReloadMarkedProfile | — | 001890e8 |
| ScriptCB_RestoreDefaultProfile | — | 00189050 |
| ScriptCB_SaveCampaignState | — | 0018a7f0 |
| ScriptCB_SaveMetagameState | 0017e350 | 0018a758 |
| ScriptCB_SaveMissionSetup | — | 0018a920 |
| ScriptCB_SaveSPState | 0017e448 | — |
| ScriptCB_SetGSProfileInfo | 0018a1b0 | 001990e0 |
| ScriptCB_SetMemoryProfileMode | 0017c0a0 | 00189e68 |
| ScriptCB_SetMetagameRulesOn | 0017e738 | — |
| ScriptCB_SetMetagameTeams | 0017e390 | — |
| ScriptCB_SetProfileAsDefault | — | 00189030 |
| ScriptCB_SetProfileNotDirty | 0017aba0 | 001883a0 |
| ScriptCB_StartDeleteCampaign | — | 00189930 |
| ScriptCB_StartDeleteMetagame | 0017be48 | 00189628 |
| ScriptCB_StartDeleteMetagameSplitScreen | 0017bec0 | — |
| ScriptCB_StartDeleteProfile | 0017b8d0 | 00189180 |
| ScriptCB_StartInitialMemcardCheck | 0017afe8 | 001884f8 |
| ScriptCB_StartLoadCampaign | — | 00189700 |
| ScriptCB_StartLoadMetagame | 0017bb70 | 001893f8 |
| ScriptCB_StartLoadMetagameSplitScreen | 0017bbf0 | — |
| ScriptCB_StartLoadProfile | 0017b610 | 00188ba8 |
| ScriptCB_StartNewCampaign | — | 0018a630 |
| ScriptCB_StartNewMetagame | 0017c550 | 0018a260 |
| ScriptCB_StartSaveCampaign | — | 001897a8 |
| ScriptCB_StartSaveMetagame | 0017bcb8 | 001894a0 |
| ScriptCB_StartSaveMetagameSplitScreen | 0017bd40 | — |
| ScriptCB_StartSaveProfile | 0017ba90 | 00189300 |
| ScriptCB_SwapActiveProfileSlots | 0017b498 | 001889d0 |

### stats/awards (7: 2 shared, 0 BF1-only, 5 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| ScriptCB_GetCareerMedalLevel | — | 001a48d8 |
| ScriptCB_GetCareerMedals | — | 001a42c0 |
| ScriptCB_GetCareerRank | — | 001a4208 |
| ScriptCB_GetPlayerIDAtRank | — | 001a4d28 |
| ScriptCB_GetRank | — | 001a4b20 |
| ScriptCB_UnlockUnlockable | 0017a298 | 0017ef08 |
| ScriptCB_UnlockableState | 0017a218 | 0017ee88 |

### ai/hero (53: 10 shared, 11 BF1-only, 32 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AICanCaptureCP | — | 00192610 |
| AISnipeSuitabilityDist | — | 0018def0 |
| AcceptHero | — | 0018cfc8 |
| AddAIGoal | — | 00191d58 |
| AddMissionHint | — | 0018e258 |
| AllowAISpawn | — | 00191490 |
| BlockPlanningGraphArcs | — | 001922a0 |
| ClearAIGoals | — | 00191ca0 |
| DisableAIAutoBalance | — | 0018dfa8 |
| DisableBarriers | — | 001924c0 |
| EnableAIAutoBalance | — | 0018df88 |
| EnableBarriers | — | 00192498 |
| EnableSPHeroRules | — | 0018ced8 |
| EnableSPScriptedHeroes | — | 0018cee0 |
| ForceAIOutOfVehicles | — | 001924e8 |
| ScriptCB_AreHeroesOn | 00179e68 | — |
| ScriptCB_GetASSNumBots | — | 0019b9c8 |
| ScriptCB_GetCONNumBots | — | 0019b908 |
| ScriptCB_GetCTFNumBots | — | 0019b968 |
| ScriptCB_GetDifficulty | 0017c850 | 0019b688 |
| ScriptCB_GetNetHeroDefaults | — | 0019e020 |
| ScriptCB_SetAIDifficulty | 0017e7d8 | 0019c478 |
| ScriptCB_SetDifficulty | 0017c7c0 | 0019b588 |
| ScriptCB_SetHeroesOn | 00179ec0 | — |
| ScriptCB_SetNetHeroDefaults | — | 0019e138 |
| ScriptCB_SetNumBots | 001868b0 | 00193e28 |
| SetAIDamageThreshold | — | 00192690 |
| SetAIDifficulty | — | 0018dfc8 |
| SetAIFormationRadius | 00185bb8 | — |
| SetAIVehicleNotifyRadius | 00185c08 | 0018de60 |
| SetAIViewMultiplier | — | 0018de90 |
| SetAttackerSnipeRange | 001852a0 | 0018d540 |
| SetDefenderSnipeRange | 001852d0 | 0018d548 |
| SetEnemyTeamDifficulty | 00185558 | — |
| SetEnemyTeamDifficultyEasy | 001855b8 | — |
| SetEnemyTeamDifficultyHard | 001856c8 | — |
| SetEnemyTeamDifficultyMedium | 00185640 | — |
| SetHeroClass | 00184f00 | 0018cb18 |
| SetHeroPlayerRule | — | 0018cc90 |
| SetHeroRespawnRule | — | 0018ce18 |
| SetHeroTeamRule | — | 0018cba8 |
| SetHeroUnlockRule | — | 0018cba0 |
| SetPlayerTeamDifficulty | 00185360 | — |
| SetPlayerTeamDifficultyEasy | 001853c0 | — |
| SetPlayerTeamDifficultyHard | 001854d0 | — |
| SetPlayerTeamDifficultyMedium | 00185448 | — |
| SetStationaryWaitTime | 00184bc8 | 0018c000 |
| SetTeamAggressiveness | 00184a80 | 0018be90 |
| ToggleBarriers | — | 00192470 |
| TogglePlanningGraphArcs | — | 00192220 |
| TranslateAICommand | — | 001926d8 |
| UnblockPlanningGraphArcs | — | 00192320 |
| UnlockHeroForTeam | — | 0018cf40 |

### mission/objective (54: 11 shared, 2 BF1-only, 41 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| ActivateBonus | 00184f58 | 0018d160 |
| ActivateObjective | — | 0018e1c8 |
| AddAssaultDestroyPoints | — | 0018c7c0 |
| AddFlagCapturePoints | — | 0018c740 |
| AddMissionObjective | 00185c30 | 0018e128 |
| AddSpaceAssaultDestroyPoints | — | 0018c848 |
| CanCharacterInteractWithFlag | — | 00192bc0 |
| CompleteObjective | — | 0018e210 |
| CreateTimer | — | 0018f4a0 |
| DestroyTimer | — | 0018f540 |
| FindTimer | — | 0018f440 |
| GetFlagCarrier | — | 00192c80 |
| MissionDefeat | — | 00192950 |
| MissionVictory | — | 00192850 |
| ScriptCB_AreHistoricalRulesOn | 0017e760 | — |
| ScriptCB_ClearMissionSetup | — | 0018a978 |
| ScriptCB_EnterMission | 001792e8 | 0017ebf0 |
| ScriptCB_GetAssaultScoreLimit | — | 0019b868 |
| ScriptCB_GetCONMaxTimeLimit | — | 0019b8b0 |
| ScriptCB_GetCTFMaxTimeLimit | — | 0019b810 |
| ScriptCB_GetGameRules | — | 0019c410 |
| ScriptCB_GetHuntMaxTimeLimit | — | 0019b738 |
| ScriptCB_GetHuntScoreLimit | — | 0019b790 |
| ScriptCB_GetInTrainingMission | — | 0018a8d8 |
| ScriptCB_GetLastBattleVictory | 0017e630 | 0017f9e0 |
| ScriptCB_GetMaxMissionQueue | 00178ff0 | 0017e900 |
| ScriptCB_GetMissionTime | — | 0017f670 |
| ScriptCB_IsGameOver | — | 0017f720 |
| ScriptCB_PauseDemoTimer | — | 0017efd0 |
| ScriptCB_RestartMission | 0017cae0 | 0017f3d8 |
| ScriptCB_SetGameRules | — | 0019c2e8 |
| ScriptCB_SetHistoricalRulesOn | 0017e788 | — |
| ScriptCB_SetInTrainingMission | — | 0018a888 |
| ScriptCB_SetLastBattleVictoryValid | 0017e6a8 | 0017fa18 |
| ScriptCB_SetMissionNames | 00179030 | 0017e938 |
| ScriptCB_ShowHuntScoreLimit | — | 0019b7d8 |
| SetDefeatMovie | 0017a6e8 | 0017ef70 |
| SetDefeatTimer | — | 0018f880 |
| SetFlagGameplayType | — | 0018e298 |
| SetHistorical | 001849f8 | 0018be10 |
| SetMissionEndMovie | — | 00192a50 |
| SetMissionTimer | — | 0018f8d8 |
| SetVictoryMovie | 0017a678 | 0017ef68 |
| SetVictoryTimer | — | 0018f930 |
| ShowObjectiveTextPopup | — | 0018e928 |
| ShowTimer | — | 0018f740 |
| SpaceAssaultAddCriticalSystem | — | 0018e380 |
| SpaceAssaultEnable | — | 0018e348 |
| SpaceAssaultGetScoreLimit | — | 0018e6e8 |
| SpaceAssaultLinkCriticalSystems | — | 0018e458 |
| SpaceAssaultSetupBitmaps | — | 0018e580 |
| SpaceAssaultSetupTeamNumbers | — | 0018e520 |
| StartTimer | — | 0018f570 |
| StopTimer | — | 0018f5a0 |

### spawn/cp (11: 2 shared, 0 BF1-only, 9 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AddDeathRegion | 001857a8 | 0018d628 |
| AddLandingRegion | — | 0018d670 |
| GetCommandPostBleedValue | — | 00191b38 |
| GetCommandPostCaptureRegion | — | 00191ad8 |
| GetCommandPostTeam | — | 00191bc8 |
| GetPathPoint | — | 001909c0 |
| MapHideCommandPosts | — | 0018f268 |
| ScriptCB_SetSpawnDisplayGain | — | 0019fa00 |
| SetSpawnDelay | 001851a8 | 0018d348 |
| SetSpawnDelayTeam | — | 0018d3f8 |
| SpawnCharacter | — | 001916e8 |

### team/reinforcements (34: 14 shared, 1 BF1-only, 19 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AddBleedThreshold | 00185090 | — |
| AddReinforcements | — | 0018c5a8 |
| AddTeamPoints | — | 0018c6d8 |
| BatchChangeTeams | — | 00191348 |
| ForceHumansOntoTeam1 | — | 00192ba0 |
| GetBleedRate | — | 0018d270 |
| GetCharacterTeam | — | 00191188 |
| GetNumTeamMembersAlive | — | 00190af0 |
| GetObjectTeam | — | 0018fab0 |
| GetReinforcementCount | — | 0018c548 |
| GetTeamFactionId | — | 0018c918 |
| GetTeamMember | — | 001910c8 |
| GetTeamPoints | — | 0018c678 |
| GetTeamSize | — | 00191068 |
| GetUnitCount | — | 0018c340 |
| ScriptCB_GetAutoAssignTeams | 00186888 | 00193df8 |
| ScriptCB_GetTeamName | 0017d220 | 0017f7d8 |
| ScriptCB_GetTeamstats | 0017d350 | 001a21a0 |
| ScriptCB_SetBleedingRepeatTime | 0018caf0 | 0019f398 |
| ScriptCB_SetCanSwitchSides | 0017e7b0 | 0019bf98 |
| ScriptCB_SetPlayerSide | 0017c6e8 | 0019b4a8 |
| ScriptCB_SetTeamNames | 00179220 | 0017eb20 |
| SelectCharacterTeam | — | 00191220 |
| SetAttackingTeam | 0018d150 | 0019fb80 |
| SetBleedRate | — | 0018d210 |
| SetObjectTeam | — | 0018fa50 |
| SetReinforcementCount | 00184df0 | 0018c3e8 |
| SetTeamAsEnemy | 00184c60 | 0018c180 |
| SetTeamAsNeutral | 00184cc8 | 0018c1f0 |
| SetTeamIcon | 00184bf0 | 0018c030 |
| SetTeamName | 00184a10 | 0018be28 |
| SetTeamPoints | — | 0018c610 |
| SetUnitCount | 00184d98 | 0018c2c8 |
| ShowTeamPoints | — | 0018d110 |

### vehicle (13: 3 shared, 0 BF1-only, 10 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AddWalkerType | 00185a68 | 0018dd18 |
| ClearWalkers | 001859a0 | 0018dc48 |
| EnableFlyerPath | — | 00190948 |
| EnterVehicle | — | 00190708 |
| EntityFlyerInitAsFlying | — | 00190658 |
| EntityFlyerInitAsLanded | — | 001906b0 |
| EntityFlyerLand | — | 00190600 |
| EntityFlyerTakeOff | — | 001905a8 |
| ExitVehicle | — | 00190860 |
| GetCharacterVehicle | — | 00191880 |
| ScriptCB_DisableFlyerShadows | — | 0017fac0 |
| SetGroundFlyerMap | — | 0018df48 |
| SetStayInTurrets | 00185300 | 0018d550 |

### soldier/unit (48: 5 shared, 0 BF1-only, 43 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| ActivateObject | — | 0018f9d0 |
| AddUnitClass | 00184e58 | 0018c978 |
| AttachEffectToObject | — | 00192f00 |
| CreateEntity | — | 001902f0 |
| DeactivateObject | — | 0018fa10 |
| FindEntityClass | — | 0018b2a0 |
| GetCharacterClass | — | 001914f0 |
| GetCharacterControllable | — | 00191a00 |
| GetCharacterRemote | — | 00191940 |
| GetCharacterUnit | — | 001917c0 |
| GetEntityClass | — | 0018fe18 |
| GetEntityClassName | — | 0018fe78 |
| GetEntityClassPtr | — | 00190490 |
| GetEntityMatrix | — | 0018ff70 |
| GetEntityName | — | 0018fdc8 |
| GetEntityPtr | — | 0018fd80 |
| GetObjectHealth | — | 0018fb60 |
| GetObjectLastHitWeaponClass | — | 0018fc80 |
| GetObjectPtr | — | 0018f988 |
| GetObjectShield | — | 0018fbf0 |
| IsCharacterHuman | — | 001912b8 |
| IsObjectAlive | — | 0018fb10 |
| KillObject | — | 0018fcd0 |
| MapAddEntityMarker | — | 0018eda8 |
| MapRemoveEntityMarker | — | 0018ef78 |
| MoveCameraToEntity | — | 0018bb58 |
| PauseAnimation | — | 00190e28 |
| PlayAnimation | — | 00190dc8 |
| PlayAnimationFromTo | — | 00190f68 |
| RespawnObject | — | 0018fd40 |
| RewindAnimation | — | 00190e88 |
| ScriptCB_GetEntityLabel | — | 001874f8 |
| ScriptCB_GetEntityMesh | — | 001872f0 |
| ScriptCB_GetEntityWeaponMesh | — | 001873e8 |
| ScriptCB_GetVKeyboardCharacter | 00178018 | 0017d2a0 |
| ScriptCB_IFModel_QueueAnimation | — | 00183cc0 |
| ScriptCB_IFModel_SetAnimation | 001818d0 | 00183ae8 |
| ScriptCB_IFModel_SetAnimationBanks | — | 00183e80 |
| ScriptCB_IFModel_SetAnimationLooping | — | 00184870 |
| ScriptCB_IFModel_SetAnimationTime | — | 00183fd0 |
| ScriptCB_IFModel_UpdateAnimation | — | 001847e0 |
| ScriptCB_PlayerSuicide | 00185ca0 | 0018e9d0 |
| SelectCharacterClass | — | 00191588 |
| SetAnimationStartPoint | — | 00190f08 |
| SetCarrierClass | 00185148 | 0018d2e0 |
| SetClassProperty | — | 001904d8 |
| SetEntityMatrix | — | 0018fee0 |
| ShowAllUnitsOnMinimap | — | 0018d0d8 |

### camera (14: 6 shared, 2 BF1-only, 6 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AddCameraShot | 00184750 | 0018b8e0 |
| GetMapCameraZoom | — | 0018bd40 |
| ScriptCB_Freecamera | 0017cbc0 | 0017f6d8 |
| ScriptCB_GetNumCameras | — | 0019b3e0 |
| ScriptCB_GetPausingViewport | 00179478 | 0017edc0 |
| ScriptCB_IsFirstPersonView | 00179780 | — |
| ScriptCB_SetFirstPersonView | 00179948 | — |
| ScriptCB_SetIFScreenViewport | 00182080 | 001849a0 |
| SetCameraPosition | 00184930 | 0018bac8 |
| SetCameraRotation | 00184870 | 0018ba00 |
| SetMapCameraOffset | — | 0018bc00 |
| SetMapCameraPitch | — | 0018bca0 |
| SetMapCameraZoom | — | 0018bcf8 |
| SnapMapCamera | — | 0018bda8 |

### hud/interface (114: 87 shared, 0 BF1-only, 27 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| DisableSmallMapMiniMap | — | 00192d88 |
| DrawBeamBetween | — | 0017fad0 |
| DrawParticleAt | — | 0017fce8 |
| GetScreenPosition | — | 00190d28 |
| GetTimerName | — | 0018f6f0 |
| GetTimerRate | — | 0018f6a8 |
| GetTimerValue | — | 0018f618 |
| MapAddClassMarker | — | 0018eff8 |
| MapAddRegionMarker | — | 0018eb58 |
| MapRemoveClassMarker | — | 0018f1d8 |
| MapRemoveRegionMarker | — | 0018ed28 |
| Print3D | — | 00192cf8 |
| ScriptCB_AddIFContainer | 001827b8 | 00185258 |
| ScriptCB_AddIFFlashyText | 001824c8 | 00184f38 |
| ScriptCB_AddIFImage | 00182588 | 00184ff8 |
| ScriptCB_AddIFMaskImage | 00182650 | 001850d0 |
| ScriptCB_AddIFModel | 00182718 | 001851a8 |
| ScriptCB_AddIFScreen | 00181fe8 | 00184908 |
| ScriptCB_AddIFText | 00182408 | 00184e78 |
| ScriptCB_ClosePopup | 00182ac8 | 00185620 |
| ScriptCB_EndIFObj | 00182390 | 00184df8 |
| ScriptCB_EndIFScreen | 00182140 | 00184a68 |
| ScriptCB_GetFontHeight | 0017fbd8 | 00181a88 |
| ScriptCB_GetHUDScale | — | 0019cdf0 |
| ScriptCB_GetIFaceLighting | — | 001876a0 |
| ScriptCB_IFFlashyText_SetBackground | 00180d78 | 00182db0 |
| ScriptCB_IFFlashyText_SetBackgroundRightJustify | 00181050 | 00183088 |
| ScriptCB_IFFlashyText_SetBackgroundSize | 00180ec8 | 00182f00 |
| ScriptCB_IFFlashyText_SetTextColor | 00180c20 | 00182bf0 |
| ScriptCB_IFFlashyText_Setup | 00180af8 | 00182a10 |
| ScriptCB_IFFlashyText_StartFlashiness | 001810f8 | 00183128 |
| ScriptCB_IFImage_GetRect | 00181238 | 00183268 |
| ScriptCB_IFImage_GetTexCoords | 00181400 | 00183438 |
| ScriptCB_IFImage_SetRect | 001812e8 | 00183318 |
| ScriptCB_IFImage_SetTexCoords | 001814b0 | 001834e8 |
| ScriptCB_IFImage_SetTexture | 00181150 | 00183180 |
| ScriptCB_IFMaskImage_GetMaskTexCoords | 00181780 | 001837e8 |
| ScriptCB_IFMaskImage_SetMaskTexCoords | 00181668 | 001836c8 |
| ScriptCB_IFMaskImage_SetMaskTexture | 001815c8 | 00183628 |
| ScriptCB_IFModel_AttachModel | — | 00184070 |
| ScriptCB_IFModel_GetDepth | 00181e60 | 00184650 |
| ScriptCB_IFModel_GetRotation | 00181bb8 | 001843a8 |
| ScriptCB_IFModel_GetScale | 00181a20 | 00184218 |
| ScriptCB_IFModel_GetTranslation | 00181d48 | 00184538 |
| ScriptCB_IFModel_SetDepth | 00181dd8 | 001845c8 |
| ScriptCB_IFModel_SetLighting | 00181f58 | 00184740 |
| ScriptCB_IFModel_SetModel | 00181830 | 00183898 |
| ScriptCB_IFModel_SetOmegaY | 00181ed0 | 001846b8 |
| ScriptCB_IFModel_SetRotation | 00181a90 | 00184280 |
| ScriptCB_IFModel_SetScale | 00181968 | 00184160 |
| ScriptCB_IFModel_SetTranslation | 00181c50 | 00184440 |
| ScriptCB_IFObj_BringToFront | 0017f730 | 00181580 |
| ScriptCB_IFObj_CreateHotSpot | 0017fbc8 | 00181a78 |
| ScriptCB_IFObj_Enable | 0017f820 | 00181670 |
| ScriptCB_IFObj_GetAlpha | 0017f3d0 | 00181230 |
| ScriptCB_IFObj_GetCPointer | 001821f0 | 00184c50 |
| ScriptCB_IFObj_GetColor | 0017f560 | 001813c0 |
| ScriptCB_IFObj_GetPos | 0017eff8 | 00180e58 |
| ScriptCB_IFObj_GetPosition | 0017fb88 | 001819f0 |
| ScriptCB_IFObj_GetVis | 0017fa08 | 00181880 |
| ScriptCB_IFObj_GetWidthHeight | 0017fb08 | 00181978 |
| ScriptCB_IFObj_GetZOrder | 0017f6c8 | 00181520 |
| ScriptCB_IFObj_IsEnabled | 0017f8a8 | 001816f0 |
| ScriptCB_IFObj_IsGroup | 0017f918 | 00181758 |
| ScriptCB_IFObj_MouseOver | 0017fbc0 | 00181a28 |
| ScriptCB_IFObj_RemoveHotSpot | 0017fbd0 | 00181a80 |
| ScriptCB_IFObj_SendToBack | 0017f7a8 | 001815f8 |
| ScriptCB_IFObj_SetAlpha | 0017f330 | 00181190 |
| ScriptCB_IFObj_SetBasicRotation | 0017f060 | 00180ec0 |
| ScriptCB_IFObj_SetColor | 0017f458 | 001812b8 |
| ScriptCB_IFObj_SetEnabled | — | 00181828 |
| ScriptCB_IFObj_SetPos | 0017eee0 | 00180d48 |
| ScriptCB_IFObj_SetPosition | 0017fb40 | 001819b0 |
| ScriptCB_IFObj_SetRelativeRotation | 0017f0c8 | 00180f28 |
| ScriptCB_IFObj_SetRotation | 0017f198 | 00180ff8 |
| ScriptCB_IFObj_SetScreenPosition | 00182228 | 00184c88 |
| ScriptCB_IFObj_SetUseSafezone | 001822e8 | 00184d40 |
| ScriptCB_IFObj_SetVis | 0017f998 | 001817d0 |
| ScriptCB_IFObj_SetWidthHeight | 0017fa70 | 001818e0 |
| ScriptCB_IFObj_SetZOrder | 0017f618 | 00181478 |
| ScriptCB_IFObj_TestHotSpot | — | 00181a30 |
| ScriptCB_IFText_GetDisplayRect | 001804e8 | 00182418 |
| ScriptCB_IFText_GetIntercharacterSpacing | 00180658 | 00182588 |
| ScriptCB_IFText_GetLeading | 00180a60 | 00182980 |
| ScriptCB_IFText_GetTextBox | 001807d0 | 00182700 |
| ScriptCB_IFText_GetTextExtent | 00180458 | 00182388 |
| ScriptCB_IFText_GetTextScaleX | 00180348 | 00182288 |
| ScriptCB_IFText_GetTextScaleY | 001803d0 | 00182308 |
| ScriptCB_IFText_SetFont | 0017fe98 | 00181df8 |
| ScriptCB_IFText_SetIntercharacterSpacing | 001805a8 | 001824d8 |
| ScriptCB_IFText_SetJustify | 0017ff40 | 00181ea0 |
| ScriptCB_IFText_SetLeading | 001809b8 | 001828d8 |
| ScriptCB_IFText_SetString | 0017fc60 | 00181b10 |
| ScriptCB_IFText_SetTextBox | 001806f0 | 00182618 |
| ScriptCB_IFText_SetTextBreak | 00180130 | 00182070 |
| ScriptCB_IFText_SetTextPosition | 0017fe48 | 00181da8 |
| ScriptCB_IFText_SetTextScale | 00180270 | 001821b0 |
| ScriptCB_IFText_SetTextStyle | 00180888 | 001827b8 |
| ScriptCB_IFText_SetUString | 0017fd90 | 00181cf0 |
| ScriptCB_IndexMultipageText | 0017ad90 | 0017f120 |
| ScriptCB_IsPopupOpen | 00182b18 | 00185670 |
| ScriptCB_OpenPopup | 00182a38 | 00185590 |
| ScriptCB_RemoveTexture | — | 00187ca8 |
| ScriptCB_SetHUDScale | — | 0019cdc0 |
| ScriptCB_SetIFScreen | 00182828 | 001852c8 |
| ScriptCB_SetIFaceLighting | — | 001879e0 |
| ScriptCB_ShowLoadDisplay | — | 00187ce8 |
| ScriptCB_ShowMultipageText | 0017aed0 | 0017f268 |
| SetMapNorthAngle | — | 00192b28 |
| SetTimerRate | — | 0018f660 |
| SetTimerValue | — | 0018f5d0 |
| ShowMessageText | — | 0018e7a8 |
| ShowPopup | — | 0018e920 |
| ShowSelectionTextPopup | — | 0018e988 |

### shell/menu/movie (65: 52 shared, 2 BF1-only, 11 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| BeginScreenTransition | — | 00180130 |
| ScriptCB_AreMoviePropertiesPlaying | 00183110 | 001858e8 |
| ScriptCB_ClearError | 00186dd8 | 001942b0 |
| ScriptCB_CloseErrorBox | 001836c8 | 00185ea8 |
| ScriptCB_CloseMovie | 00182dd8 | 00185778 |
| ScriptCB_CloseNetShell | 001863d8 | 00193d68 |
| ScriptCB_DoConsoleCmd | 001838a8 | 00186088 |
| ScriptCB_EnableCursor | — | 001872e8 |
| ScriptCB_EnableScene | — | 0017fe98 |
| ScriptCB_FormatTimeString | 00177f30 | 0017d1c0 |
| ScriptCB_GetConsoleCmds | 001837c0 | 00185fa8 |
| ScriptCB_GetError | 00186d70 | 001941f8 |
| ScriptCB_GetErrorLevel | 00183738 | 00185f20 |
| ScriptCB_GetLanguage | 00177ee0 | 0017d170 |
| ScriptCB_GetLatestError | 00186cd8 | 00194178 |
| ScriptCB_GetPlatform | 00177eb8 | 0017d140 |
| ScriptCB_GetQuitPlayer | 00178fc0 | 0017e8c8 |
| ScriptCB_GetSPProgress | 0017e4d0 | 0017f880 |
| ScriptCB_GetSafeScreenInfo | 00183488 | 00185c90 |
| ScriptCB_GetScreenInfo | 001833b8 | 00185bb0 |
| ScriptCB_GetShellActive | 00178b20 | 0017e0e8 |
| ScriptCB_HideDownloadableContent | — | 0017f3b0 |
| ScriptCB_IsErrorBoxOpen | 00183560 | 00185d78 |
| ScriptCB_IsHorizontalSplitScreen | — | 0019cd40 |
| ScriptCB_IsInShell | 001862a8 | 00193c18 |
| ScriptCB_IsMoviePlaying | 00183090 | 001858b8 |
| ScriptCB_IsScreenInStack | — | 001854e8 |
| ScriptCB_IsSplitscreen | 00178f38 | 0019b418 |
| ScriptCB_IsToolTipAuto | 00179fc8 | — |
| ScriptCB_IsToolTipOn | 00179f70 | — |
| ScriptCB_JumpToNTGUI | 0018ae78 | 00199d90 |
| ScriptCB_LaunchDemo | 00179350 | 0017ec50 |
| ScriptCB_NextToolTipState | 0017a030 | 0019ce20 |
| ScriptCB_OpenErrorBox | 00183600 | 00185dd8 |
| ScriptCB_OpenMovie | 00182ca8 | 00185700 |
| ScriptCB_OpenNetShell | 00186330 | 00193cb8 |
| ScriptCB_PlayInGameMovie | — | 001859f0 |
| ScriptCB_PlayMovie | 00182e90 | 00185798 |
| ScriptCB_PopScreen | 001829a0 | 00185448 |
| ScriptCB_PrevToolTipState | 0017a0e8 | 0019cf00 |
| ScriptCB_PushScreen | 001828e0 | 00185388 |
| ScriptCB_QuitToShell | 0017cc08 | 0017f778 |
| ScriptCB_QuitToWindows | 0017cc48 | 0017f7b0 |
| ScriptCB_Reboot | 00179458 | 0017eda0 |
| ScriptCB_ResetSkipToNTGUI | 0018b8b0 | 0019a450 |
| ScriptCB_ResetToolTips | 0017a1a0 | 0019cfe0 |
| ScriptCB_SetDCMap | — | 0017f348 |
| ScriptCB_SetHorizontalSplitScreen | — | 0019cd68 |
| ScriptCB_SetQuitPlayer | 00178f90 | 0017e890 |
| ScriptCB_SetSPProgress | 0017e550 | 0017f900 |
| ScriptCB_SetSplitscreen | 00178f00 | 0019b350 |
| ScriptCB_ShouldShowDemoPostscreen | 0017a788 | 0017f000 |
| ScriptCB_ShouldShowLegal | 0017a758 | 0017ef78 |
| ScriptCB_SkipToNTGUI | 0018b838 | 0019a3e8 |
| ScriptCB_StopInGameMovie | — | 00185b80 |
| ScriptCB_StopMovie | 001831a8 | 00185938 |
| ScriptCB_UnicodeStrCat | 00178178 | 0017d748 |
| ScriptCB_Unpause | 0017cb28 | 0017f468 |
| ScriptCB_UpdateScreen | 001794d0 | 0017ee68 |
| ScriptCB_WasSplitscreen | 00178f68 | 0019b450 |
| ScriptCB_getlocalizestr | 00178360 | 0017d8d8 |
| ScriptCB_tounicode | 001780b0 | 0017d698 |
| ScriptCB_ununicode | 001782a0 | 0017d810 |
| ScriptCB_usprintf | 00178450 | 0017d9b8 |
| ScriptCB_ustrupper | — | 0017dcb8 |

### controller/input (33: 19 shared, 4 BF1-only, 10 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| ScriptCB_GetControlMode | 00177df8 | 0019d0f0 |
| ScriptCB_GetControlScale | 001795c0 | 0019c650 |
| ScriptCB_GetFunctionIdForAnalogId | 0017a2f8 | 0019ba78 |
| ScriptCB_GetFunctionIdForButtonId | 0017a510 | 0019bce0 |
| ScriptCB_GetMaxControllers | — | 0017e648 |
| ScriptCB_GetNumControllers | 00178c90 | 0017e608 |
| ScriptCB_GetPrimaryController | — | 0017e378 |
| ScriptCB_GetSecondaryController | — | 0017e3b0 |
| ScriptCB_GetSideSelectButtonSetting | — | 0019d140 |
| ScriptCB_GetUnusedControllerPort | 00178cd0 | 0017e678 |
| ScriptCB_GetYAxisFlip | 001794f0 | 0019c580 |
| ScriptCB_IsAutoAimOn | 00179c30 | — |
| ScriptCB_IsControllerBound | — | 0017e488 |
| ScriptCB_IsControllerConnected | — | 0017e400 |
| ScriptCB_IsRumbleOn | 00179aa8 | — |
| ScriptCB_ReadAllControllers | 00178b90 | 0017e170 |
| ScriptCB_ReadLeftstick | 00178e38 | 0017e7c8 |
| ScriptCB_ResetControlsToDefault | 00179708 | 0019c910 |
| ScriptCB_RestorePrimaryController | — | 0017e5a8 |
| ScriptCB_SetAutoAcquireControllers | 00178b68 | 0017e138 |
| ScriptCB_SetAutoAimOn | 00179c88 | — |
| ScriptCB_SetControlMode | 00177d70 | 0019d068 |
| ScriptCB_SetControlScale | 00179650 | 0019c708 |
| ScriptCB_SetFunctionIdForAnalogId | 0017a3a0 | 0019bb28 |
| ScriptCB_SetFunctionIdForButtonId | 0017a5a0 | 0019bda8 |
| ScriptCB_SetHotController | — | 0017e510 |
| ScriptCB_SetIgnoreControllerRemoval | 00178c68 | 0017e5d0 |
| ScriptCB_SetIgnoreInputs | 00182b70 | 001856c8 |
| ScriptCB_SetInputRepeat | — | 00187d90 |
| ScriptCB_SetRumbleOn | 00179b00 | — |
| ScriptCB_SetYAxisFlip | 00179530 | 0019c5b8 |
| ScriptCB_SwapPrimaryController | — | 0017e4e8 |
| ScriptCB_UnbindController | 00178bf0 | 0017e258 |

### misc (70: 35 shared, 2 BF1-only, 33 BF2-only)

| binding | BF1 addr | BF2 addr |
|---|---|---|
| AddShieldStrength | — | 001927c0 |
| AttachEffectToMatrix | — | 00192fd0 |
| ClearTempHeap | — | 0018b870 |
| CreateEffect | — | 00192e28 |
| CreateMatrix | — | 00190b70 |
| EnableBuildingLockOn | — | 0018f308 |
| FillAsteroidPath | — | 0018d990 |
| FillAsteroidRegion | — | 0018d828 |
| GetEffectMatrix | — | 00193088 |
| GetWorldFilename | — | 0018e760 |
| IsEffectActive | — | 00193150 |
| ReadDataFile | 001845e8 | 0018b470 |
| ReadDataFileInGame | — | 0018b790 |
| RemoveEffect | — | 00192eb8 |
| RemoveRegion | — | 0018d6b8 |
| ScriptCB_CheckIfOkToExitMemoryMode | 0017c0c8 | 00189e98 |
| ScriptCB_CheckPlayerListDone | — | 0017f5a8 |
| ScriptCB_ConnectUsingEntry | 0018ac80 | 00199c50 |
| ScriptCB_DoFile | 00177e58 | 0017d0e0 |
| ScriptCB_EffectsEnabled | 0018dca0 | 001a09a8 |
| ScriptCB_EnablePlayback | 00186bd8 | 00194098 |
| ScriptCB_GetAutoGetPlayerCount | 00178d68 | 0017e710 |
| ScriptCB_GetBuildStr | 001794a8 | 0017ee38 |
| ScriptCB_GetCTFCaptureLimit | — | 0019b6f0 |
| ScriptCB_GetDateStr | 00178a80 | 0017e060 |
| ScriptCB_GetGameName | — | 00193e88 |
| ScriptCB_GetGeneralOptions | — | 0019c998 |
| ScriptCB_GetOnlineOpts | 00189dc0 | 0019d3c0 |
| ScriptCB_HWSupport | — | 001a09d8 |
| ScriptCB_IsMetaAllMapsOn | 0017e898 | 0019c480 |
| ScriptCB_IsPAL | 001821c8 | 00184b48 |
| ScriptCB_IsPropertyPlayingNow | 00183320 | 001859a0 |
| ScriptCB_PausingIsPrimary | — | 0017edf8 |
| ScriptCB_ResetGameOptionsToDefault | — | 0019c808 |
| ScriptCB_ResetSkipToPlayerList | — | 0017f598 |
| ScriptCB_SetGameName | 00186910 | 00193eb8 |
| ScriptCB_SetGeneralOptions | — | 0019cae8 |
| ScriptCB_SetMetaAllMapsOn | 0017e8f0 | 0019c4e0 |
| ScriptCB_SetNoticeNoCable | 00189bb0 | 00198d00 |
| ScriptCB_SetOnlineOpts | 00189c78 | 0019d230 |
| ScriptCB_SetSunlight | 001843c8 | 00187068 |
| ScriptCB_SkipToPlayerList | — | 0017f568 |
| ScriptCB_StartPreOp | 0017b058 | 00188550 |
| ScriptCB_ToggleEffects | 0018dcc0 | 001a09d0 |
| ScriptCB_random | 00178818 | 0017de80 |
| ScriptCB_srand | 00178738 | 0017dd98 |
| SetAllowBlindJetJumps | 00184b80 | 0018bfb0 |
| SetBirdFlockMinHeight | 00185750 | 0018d5c0 |
| SetBirdType | 001857e8 | 0018d700 |
| SetEffectActive | — | 001931a8 |
| SetEffectMatrix | — | 001930c8 |
| SetFishType | 00185888 | 0018d7b0 |
| SetLevelModelMem | 00185970 | — |
| SetMaxCollisionDistance | — | 0018db10 |
| SetMaxFlyHeight | 00184b08 | 0018bf20 |
| SetMaxPlayerFlyHeight | 00184b58 | 0018bf80 |
| SetMemoryPoolSize | 001858f8 | 0018db70 |
| SetMinFlyHeight | 00184ae0 | 0018bef0 |
| SetMinPlayerFlyHeight | 00184b30 | 0018bf50 |
| SetModelMemory | 00184700 | — |
| SetNumBirdTypes | 00185778 | 0018d5f0 |
| SetNumFishTypes | 00185858 | 0018d778 |
| SetPS2ModelMemory | — | 0018b338 |
| SetParticleLODBias | — | 0018db40 |
| SetProperty | — | 001901b8 |
| SetState | 001849c0 | 0018bdd0 |
| SetUrbanEnvironment | 00185210 | 0018d4b0 |
| SetWorldExtents | — | 0018b3a8 |
| SetupTempHeap | — | 0018b7f0 |
| StealArtistHeap | — | 0018b410 |
