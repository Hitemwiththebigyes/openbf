#!/usr/bin/env bash
# Rebake every baked asset the engine ships with, from the local extracted
# discs. Output goes to engine/assets_baked/ (gitignored). Run from anywhere:
#
#   scripts/rebake.sh            # everything
#   scripts/rebake.sh tat1 side  # only the named groups
#
# Groups: REBAKE_GROUPS below; an unknown name is refused (exit 2) before
# anything is built, so a typo cannot bake nothing and report success.
# Sources are read-only (extracted/); nothing here touches iso/.
set -euo pipefail

# land.py reads this line too (it refuses a card whose `rebake=` names another).
REBAKE_GROUPS="tat1 tat2 maps common side classes sounds global-sound load beep"
for g in "$@"; do
  case " $REBAKE_GROUPS " in
    *" $g "*) ;;
    *) echo "rebake: unknown group '$g' (groups: $REBAKE_GROUPS)" >&2; exit 2 ;;
  esac
done

# Under Git for Windows' bash (card WIN, scripts/pack.py) `pwd -W` gives C:/... : the bake.exe it runs reads
# those as they are, comma-joined lists included (MSYS converts only whole /c/... arguments).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && { pwd -W 2>/dev/null || pwd; })"
E="$ROOT/extracted"
B1="$E/bf1/lvl/DATA/_LVL_PS2"
B2="$E/bf2/lvl/DATA/_LVL_PS2"
S1="$E/bf1/sound/DATA/_LVL_PS2"
S2="$E/bf2/sound/DATA/_LVL_PS2"
# scripts/pack.py extracts each disc whole into lvl/ (bake extract-iso): the
# sound LVLs are then beside the rest.
[ -d "$S1" ] || S1="$B1"
[ -d "$S2" ] || S2="$B2"
OUT="$ROOT/engine/assets_baked"

# Classic-plus tier 2: the maintainer's PC SWBF2 install. Any texture the PC ships
# LARGER than the PS2 one adds an optional tier; the PS2 base is preserved
# (bake::tier; the maintainer's chain is
# Harrisonfog HD > PC stock where larger > our upscale > PS2 original).
# BF1 is unaffected — there is no PC copy of BF1. The install is gitignored
# and optional: without it every bake is PS2-only, which is a real difference
# in output, so the script SAYS which way it ran rather than deciding quietly.
PC="$ROOT/PC_swbf2_exe/Star Wars Battlefront II Classic/GameData/data/_lvl_pc"
if [ -d "$PC" ]; then
  echo "PC stock textures: $PC (BF2 maps get the larger PC texture where there is one)"
else
  echo "PC stock textures: NOT PRESENT — baking PS2-only"
  PC=""
fi
# The PC's own SIDE LVLs, for the soldier/weapon bake (BF2 only — there is no
# PC copy of BF1). Empty when the install is absent, and each is checked
# individually so a partial install degrades to PS2 rather than failing.
PCSIDE_REP=""; PCSIDE_CIS=""
if [ -n "$PC" ]; then
  [ -f "$PC/side/rep.lvl" ] && PCSIDE_REP="$PC/side/rep.lvl"
  [ -f "$PC/side/cis.lvl" ] && PCSIDE_CIS="$PC/side/cis.lvl"
fi
# Set PC_ARG to the PC LVL for a BF2 map, or to nothing. An ARRAY, because
# the install path contains spaces ("Star Wars Battlefront II Classic").
PC_ARG=()
pc_arg() {  # <planet dir> <map name>, both lowercase
  PC_ARG=()
  [ -n "$PC" ] || return 0
  local f="$PC/$1/$2.lvl"
  [ -f "$f" ] && PC_ARG=("$f")
  return 0
}

cd "$ROOT/engine"
cargo build -p bake --release
BAKE="${CARGO_TARGET_DIR:-$ROOT/engine/target}/release/bake"
[ -f "$BAKE.exe" ] && BAKE="$BAKE.exe"

SEL=("$@")
want() {
  [ "${#SEL[@]}" -eq 0 ] && return 0
  for g in "${SEL[@]}"; do [ "$g" = "$1" ] && return 0; done
  return 1
}

# Each bake owns a few subdirectories; clear those (and only those) so a
# renamed or dropped output cannot linger as a stale file.
clean() { local t; for t in "$@"; do rm -rf "$t"; done; }

# Every other ground map, baked to assets_baked/<game>_<lvl> (the LVL name,
# not the world name inside it: BF1 and BF2 both ship "hoth", "yavin1", ...).
# Space (SPA*) is P8 and not baked here.
BF1_MAPS="BES/BES1 BES/BES2 END/END1 GEO/GEO1 HOT/HOT1 KAM/KAM1 KAS/KAS1 KAS/KAS2 NAB/NAB1 NAB/NAB2 RHN/RHN1 RHN/RHN2 TAT/TAT2 YAV/YAV1 YAV/YAV2"
BF2_MAPS="COR/COR1 DAG/DAG1 DEA/DEA1 END/END1 FEL/FEL1 GEO/GEO1 HOT/HOT1 KAM/KAM1 KAS/KAS2 MUS/MUS1 MYG/MYG1 NAB/NAB2 POL/POL1 TAN/TAN1 TAT/TAT3 UTA/UTA1 YAV/YAV1"


if want tat1; then
  echo "== TAT1 (BF1) =="
  clean "$OUT/tat1/models" "$OUT/tat1/textures" "$OUT/tat1/collision" "$OUT/tat1/nav"
  "$BAKE" map "$B1/TAT/TAT1.LVL" "$OUT/tat1"
fi
if want tat2; then
  echo "== TAT2 (BF2) =="
  clean "$OUT/tat2/models" "$OUT/tat2/textures" "$OUT/tat2/collision" "$OUT/tat2/nav"
  pc_arg tat tat2
  "$BAKE" map "$B2/TAT/TAT2.LVL" "$OUT/tat2" "${PC_ARG[@]}"
fi
if want common; then
  echo "== common assets =="
  clean "$OUT/common_bf1/models" "$OUT/common_bf1/textures" "$OUT/common_bf1/collision" \
        "$OUT/common_bf2/models" "$OUT/common_bf2/textures" "$OUT/common_bf2/collision"
  "$BAKE" common "$B1/COMMON/ENG/COMMON.LVL" "$OUT/common_bf1"
  PC_ARG=()
  [ -n "$PC" ] && [ -f "$PC/ingame.lvl" ] && PC_ARG=("$PC/ingame.lvl")
  "$BAKE" common "$B2/INGAME.LVL" "$OUT/common_bf2" "${PC_ARG[@]}"
  # The specular pass's lookup table, from each game's own CORE.LVL (the
  # texels are identical on both discs; analysis/re-specular-pass.md §5).
  "$BAKE" specular "$B1/CORE.LVL" "$OUT/common_bf1"
  "$BAKE" specular "$B2/CORE.LVL" "$OUT/common_bf2"
  # BF2's terrain light pass texture (card FXBL, analysis/re-bolt.md §5.3):
  # BF1 draws no light on its terrain (§5.1).
  "$BAKE" omniproj "$B2/CORE.LVL" "$OUT/common_bf2"
  # Each game's English text table (card EVMSG, analysis/re-event-messages.md):
  # the HUD's event lines, team, post and soldier names.
  "$BAKE" locl "$B1/CORE.LVL" "$OUT/common_bf1"
  "$BAKE" locl "$B2/CORE.LVL" "$OUT/common_bf2"
  # BF1's own bitmap fonts (card SQHUD: the squad cross's labels are BF1's
  # gamefont_tiny), prefixed so they sit beside BF2's in one name space.
  "$BAKE" fonts "$B1/CORE.LVL" "$OUT/common_bf1" bf1_
  # BF2's PS2 shell (card BSHLA, analysis/re-bf2-shell.md §1, §4): SHELL.LVL's
  # logo, the background movie's menu loops (ffmpeg transcodes them to frame
  # packs) and SOUND/SHELL.LVL's bank + music index. The movie is read from
  # the extracted tree when `bake extract-iso` kept it, else out of the disc
  # image (a dev tree's extracted discs leave MOVIES/ out).
  # A worktree links extracted/ but not iso/: the disc image is then the main
  # checkout's (git's common dir sits in it).
  MVS="$B2/MOVIES/SHELL.MVS"
  [ -f "$MVS" ] || MVS="$ROOT/iso/SWBF2.iso"
  if [ ! -f "$MVS" ]; then
    MAIN_ROOT="$(dirname "$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir)")"
    MVS="$MAIN_ROOT/iso/SWBF2.iso"
  fi
  clean "$OUT/common_bf2/shell"
  "$BAKE" shell "$B2/SHELL.LVL" "$MVS" "$S2/SOUND/SHELL.LVL" "$B2/CORE.LVL" "$OUT/common_bf2"
fi
if want side; then
  echo "== soldiers + weapons + vehicles =="
  clean "$OUT/side_bf1/soldiers" "$OUT/side_bf1/weapons" "$OUT/side_bf1/textures" \
        "$OUT/side_bf2/soldiers" "$OUT/side_bf2/weapons" "$OUT/side_bf2/textures" \
        "$OUT/side_bf1/vehicles" "$OUT/side_bf2/vehicles" \
        "$OUT/side_bf1/effects" "$OUT/side_bf2/effects"
  # EVERY soldier geometry and every PRIMARY-weapon geometry named by the four
  # default class lists (game::roster::DEFAULT_CLASSES) or by any of the 57
  # mission RONs' per-team class mixes, deduped and grouped by the side LVL
  # that carries it (engine/artifacts/roster/README.md has the derivation and
  # the per-geometry bake table). `bake soldier|weapon` take a COMMA LIST.
  #
  # Note both discs' REP/CIS and ALL/IMP are baked: a BF1 map's clone-wars
  # mission fields BF1 SIDE/REP + SIDE/CIS classes and a BF2 map's GCW
  # mission fields BF2 SIDE/ALL + SIDE/IMP ones.
  A1="$B1/COMMON/ENG/COMMON.LVL"
  "$BAKE" soldier "$B1/SIDE/ALL.LVL" all_inf_marksperson,all_inf_marksperson_snow,all_inf_pilot,all_inf_snowtrooper,all_inf_soldier,all_inf_trooperfleet,all_inf_wookie "$OUT/side_bf1" "$A1"
  "$BAKE" soldier "$B1/SIDE/IMP.LVL" imp_inf_atatpilot,imp_inf_atstpilot,imp_inf_darktrooper,imp_inf_pilot,imp_inf_scout,imp_inf_shocktrooper,imp_inf_snowtrooper,imp_inf_stormtrooper "$OUT/side_bf1" "$A1"
  "$BAKE" soldier "$B1/SIDE/REP.LVL" rep_inf_arctrooper,rep_inf_jettrooper,rep_inf_pilot,rep_inf_trooper "$OUT/side_bf1" "$A1"
  "$BAKE" soldier "$B1/SIDE/CIS.LVL" cis_inf_bdroid,cis_inf_sbdroid "$OUT/side_bf1" "$A1"
  # BF2 SIDE gets the PC-stock tier too, from the matching PC side LVL.
  # This is where the proportional gain is biggest: a first-person weapon
  # sheet is 128x64 on the PS2 and 512x256 on the PC.
  "$BAKE" soldier "$B2/SIDE/REP.LVL" rep_inf_arctrooper,rep_inf_clonecommander,rep_inf_ep3armoredpilot,rep_inf_ep3heavytrooper,rep_inf_ep3jettrooper,rep_inf_ep3sniper,rep_inf_ep3trooper,rep_inf_feluciatrooper,rep_inf_jettrooper,rep_inf_pilot,rep_inf_trooper "$OUT/side_bf2" "$B2/INGAME.LVL" ${PCSIDE_REP:+"$PCSIDE_REP"}
  "$BAKE" soldier "$B2/SIDE/CIS.LVL" cis_inf_bdroid,cis_inf_magnaguard,cis_inf_sbdroid "$OUT/side_bf2" "$B2/INGAME.LVL" ${PCSIDE_CIS:+"$PCSIDE_CIS"}
  "$BAKE" soldier "$B2/SIDE/ALL.LVL" all_inf_bothanspy,all_inf_engineer,all_inf_marksperson,all_inf_markspersonsnow,all_inf_snowtrooper,all_inf_soldier,all_inf_tantive4trooper,all_inf_vanguard,all_inf_wookie "$OUT/side_bf2" "$B2/INGAME.LVL"
  "$BAKE" soldier "$B2/SIDE/IMP.LVL" imp_inf_atatcommander,imp_inf_atatpilot,imp_inf_darktrooper,imp_inf_gunner,imp_inf_scout,imp_inf_shocktrooper,imp_inf_snowtrooper,imp_inf_stormtrooper "$OUT/side_bf2" "$B2/INGAME.LVL"
  # BF2's pistols are baked for the HUD's weapon row (card HUDW): the PS2
  # shows a weapon's own model there, and every BF2 side's second weapon is
  # its pistol (IMP's has been baked since the officer's primary names it).
  # BF1's pistols too since card WHELD: a soldier holds the model of the
  # weapon in his hand, and every class slot the sim can switch to must have
  # its model (game/tests/suite/roster.rs `every_held_slots_hand_model_is_baked`).
  "$BAKE" weapon  "$B1/SIDE/ALL.LVL" all_weap_inf_bowcaster,all_weap_inf_fusioncutter,all_weap_inf_launcher,all_weap_inf_pistol,all_weap_inf_rifle,all_weap_inf_seeker,all_weap_inf_sniperrifle "$OUT/side_bf1"
  "$BAKE" weapon  "$B1/SIDE/IMP.LVL" imp_weap_inf_fusioncutter,imp_weap_inf_launcher,imp_weap_inf_pistol,imp_weap_inf_rifle,imp_weap_inf_sniperrifle "$OUT/side_bf1"
  "$BAKE" weapon  "$B1/SIDE/REP.LVL" rep_weap_inf_fusioncutter,rep_weap_inf_launcher,rep_weap_inf_pistol,rep_weap_inf_rifle,rep_weap_inf_sniperrifle "$OUT/side_bf1"
  "$BAKE" weapon  "$B1/SIDE/CIS.LVL" cis_weap_inf_fusioncutter,cis_weap_inf_launcher,cis_weap_inf_pistol,cis_weap_inf_sniperrifle "$OUT/side_bf1"
  "$BAKE" weapon  "$B2/SIDE/REP.LVL" rep_weap_inf_chaingun,rep_weap_inf_fusioncutter,rep_weap_inf_launcher,rep_weap_inf_pistol,rep_weap_inf_rifle,rep_weap_inf_sniperrifle "$OUT/side_bf2" ${PCSIDE_REP:+"$PCSIDE_REP"}
  "$BAKE" weapon  "$B2/SIDE/CIS.LVL" cis_weap_inf_fusioncutter,cis_weap_inf_launcher,cis_weap_inf_pistol,cis_weap_inf_rifle,cis_weap_inf_sniperrifle "$OUT/side_bf2" ${PCSIDE_CIS:+"$PCSIDE_CIS"}
  "$BAKE" weapon  "$B2/SIDE/ALL.LVL" all_weap_inf_bowcaster,all_weap_inf_fusioncutter,all_weap_inf_launcher,all_weap_inf_pistol,all_weap_inf_rifle,all_weap_inf_seeker,all_weap_inf_sniperrifle "$OUT/side_bf2"
  "$BAKE" weapon  "$B2/SIDE/IMP.LVL" imp_weap_inf_fusioncutter,imp_weap_inf_launcher,imp_weap_inf_pistol,imp_weap_inf_rifle,imp_weap_inf_sniperrifle "$OUT/side_bf2"
  # The torpedo launcher's model is SHARED, not per-side: BF1 IMP/REP/CIS's
  # `*_weap_inf_torpedo_launcher` and BF2 REP's all name `com_weap_inf_torpedo`,
  # whose `modl` is in each game's common LVL and in no side LVL (measured:
  # `bake weapon SIDE/ALL.LVL com_weap_inf_torpedo` -> "modl not found",
  # COMMON.LVL/INGAME.LVL -> 474 verts, 494 tris).
  # So is the thermal detonator's (card WHELD: drawn in the hand while a
  # throw is wound up and before HideOnFire hides it), every side's
  # `*_weap_inf_thermaldetonator` naming `com_weap_inf_grenadethermal`.
  "$BAKE" weapon  "$A1"              com_weap_inf_torpedo,com_weap_inf_grenadethermal,com_weap_inf_grenadethermal_ord "$OUT/side_bf1"
  "$BAKE" weapon  "$B2/INGAME.LVL"   com_weap_inf_torpedo,com_weap_inf_grenadethermal,com_weap_inf_grenadethermal_ord "$OUT/side_bf2"
  # Every vehicle class of the four sides (card VBAKE): models (every LOD),
  # destruction chunks, collision, textures -> side_<game>/vehicles/<side>/.
  for s in all cis imp rep; do
    S=$(echo "$s" | tr a-z A-Z)
    "$BAKE" vehicle "$B1/SIDE/$S.LVL" "$OUT/side_bf1/vehicles/$s"
  done
  # BF1 SIDE/GUN: Naboo Plains' missions read it (nab1c/nab1i/nab1c_h.luac)
  # and three NAB1 pads key its gun_walk_kaadu for the defenders (card KAAD).
  "$BAKE" vehicle "$B1/SIDE/GUN.LVL" "$OUT/side_bf1/vehicles/gun"
  "$BAKE" vehicle "$B2/SIDE/ALL.LVL" "$OUT/side_bf2/vehicles/all"
  "$BAKE" vehicle "$B2/SIDE/IMP.LVL" "$OUT/side_bf2/vehicles/imp"
  "$BAKE" vehicle "$B2/SIDE/REP.LVL" "$OUT/side_bf2/vehicles/rep" ${PCSIDE_REP:+"$PCSIDE_REP"}
  "$BAKE" vehicle "$B2/SIDE/CIS.LVL" "$OUT/side_bf2/vehicles/cis" ${PCSIDE_CIS:+"$PCSIDE_CIS"}
  # Every side LVL's `fx__` particle effects + the textures/models they name
  # (card FXP1, analysis/re-fx.md §1): BF2's sides carry per-class effects
  # (IMP 8, SNW 6, CIS 5, ...); BF1's carry none, and a side with none writes
  # nothing. ALL side LVLs, not only the four baked above: the runtime looks
  # an effect up by hash through whatever side a mission loads.
  for f in "$B1"/SIDE/*.LVL; do
    s=$(basename "$f" .LVL | tr 'A-Z' 'a-z')
    "$BAKE" effects "$f" "$OUT/side_bf1/effects/$s" bf1
  done
  for f in "$B2"/SIDE/*.LVL; do
    s=$(basename "$f" .LVL | tr 'A-Z' 'a-z')
    "$BAKE" effects "$f" "$OUT/side_bf2/effects/$s" bf2
  done
fi
if want classes; then
  echo "== entity/weapon classes =="
  "$BAKE" classes "$B1/SIDE/ALL.LVL"        "$OUT/classes/bf1_all.ron"
  "$BAKE" classes "$B1/SIDE/IMP.LVL"        "$OUT/classes/bf1_imp.ron"
  "$BAKE" classes "$B1/COMMON/ENG/COMMON.LVL" "$OUT/classes/bf1_common.ron"
  "$BAKE" classes "$B2/SIDE/REP.LVL"        "$OUT/classes/bf2_rep.ron"
  "$BAKE" classes "$B2/SIDE/CIS.LVL"        "$OUT/classes/bf2_cis.ron"
  "$BAKE" classes "$B2/INGAME.LVL"          "$OUT/classes/bf2_ingame.ron"
  # Bake all four factions per source game: game::roster::class_files loads
  # the sides in play, including clone-wars on BF1 maps and GCW on BF2 maps.
  "$BAKE" classes "$B1/SIDE/REP.LVL"        "$OUT/classes/bf1_rep.ron"
  "$BAKE" classes "$B1/SIDE/CIS.LVL"        "$OUT/classes/bf1_cis.ron"
  "$BAKE" classes "$B2/SIDE/ALL.LVL"        "$OUT/classes/bf2_all.ron"
  "$BAKE" classes "$B2/SIDE/IMP.LVL"        "$OUT/classes/bf2_imp.ron"
  # A mission's extra side (game::vehicle_pads::mission_side_files).
  "$BAKE" classes "$B1/SIDE/GUN.LVL"        "$OUT/classes/bf1_gun.ron"
  # BF2's building turrets (card VTUR2): their PROPs; each map bake pulls the
  # models, collision and skel nodes of the TUR classes its worlds place.
  "$BAKE" classes "$B2/SIDE/TUR.LVL"        "$OUT/classes/bf2_tur.ron"
fi
if want sounds; then
  # One SOUND LVL per PLANET on both discs (RE_LOG "BF1 does NOT ship sound
  # banks per era"): the first three letters of a map name are its planet, so
  # bf1_kas1 and bf1_kas2 share KAS.LVL. The game loads <mapdir>/sounds/
  # sounds.ron, so each map dir gets its own copy of its planet's bank.
  echo "== map sound banks =="
  # The game's CORE.LVL goes in too: a property's Inherit template (distances,
  # gain, rolloff) is there, not in the planet's LVL (card SNDT).
  bake_map_sounds() {  # <sound root> <map dir name> <map short name> <CORE.LVL>
    local root="$1" dir="$2" short="$3" core="$4"
    local planet
    planet=$(echo "${short:0:3}" | tr 'a-z' 'A-Z')
    if [ -f "$root/SOUND/$planet.LVL" ]; then
      clean "$OUT/$dir/sounds"
      "$BAKE" sounds "$root/SOUND/$planet.LVL" "$OUT/$dir" "$core"
    else
      echo "  (no SOUND/$planet.LVL for $dir — skipped)"
    fi
  }
  bake_map_sounds "$S1" tat1 tat1 "$B1/CORE.LVL"
  bake_map_sounds "$S2" tat2 tat2 "$B2/CORE.LVL"
  for p in $BF1_MAPS; do
    n=$(basename "$p" | tr 'A-Z' 'a-z'); bake_map_sounds "$S1" "bf1_$n" "$n" "$B1/CORE.LVL"
  done
  for p in $BF2_MAPS; do
    n=$(basename "$p" | tr 'A-Z' 'a-z'); bake_map_sounds "$S2" "bf2_$n" "$n" "$B2/CORE.LVL"
  done
  # The RESIDENT bank both games load and never unload: CORE.LVL's `global`
  # SampleBank + `snd_` — every shell/load sound, incl. shell_menu_accept
  # (analysis/re-sound-global.md §0.6; the per-map banks do not carry them).
  # Baked here rather than with the streams so that `sound-names` below sees
  # these two directories on a FRESH tree too.
  echo "== resident global banks (CORE.LVL) =="
  clean "$OUT/core_bf1/sounds" "$OUT/core_bf2/sounds"
  "$BAKE" sounds "$B1/CORE.LVL" "$OUT/core_bf1"
  "$BAKE" sounds "$B2/CORE.LVL" "$OUT/core_bf2"
  # Recover readable names for the hashed properties/samples so
  # scripts/audition.sh can lay a bank out under names a human can read.
  # Needs the baked classes, so it runs after them; harmless if they are
  # missing (the dictionary just gets smaller).
  echo "== sound names =="
  "$BAKE" sound-names "$OUT/classes" "$E/bf1/lua,$E/bf2/lua" \
    $(ls -d "$OUT"/*/sounds 2>/dev/null | xargs -r -n1 dirname)
fi
if want global-sound; then
  # The sound BOTH games keep outside the per-map banks (the maintainer's unified sound
  # plan, PLAN Log 2026-09-08 (b); analysis/re-sound-global.md §5):
  #  (i)  BF2 SOUND/GLOBAL.LVL — 29 mono VO streams. Its two stereo MUSIC
  #       streams (630 MB) are indexed, never decoded: music stays streamed.
  #  (ii) BF1 SOUND/CW.LVL + GCW.LVL — the four VO streams, including the
  #       nine-command tactical chatter the per-map banks never carried.
  # Only segments a shipped `snd_` script names are encoded, so every per-map
  # sound LVL goes in as the reference set (BF1 TEST.LVL is not shipped
  # content and is left out). Both write <id>.ogg keyed by SEGMENT ID — the
  # only name that data has.
  echo "== global VO streams =="
  clean "$OUT/audio/global_bf1" "$OUT/audio/global_bf2"
  REF1=$(ls "$S1"/SOUND/*.LVL | grep -vE '/(CW|GCW|TEST)\.LVL$' | paste -sd,)
  REF2=$(ls "$S2"/SOUND/*.LVL | grep -v '/GLOBAL\.LVL$' | paste -sd,)
  "$BAKE" global-sound "$S1/SOUND/CW.LVL,$S1/SOUND/GCW.LVL" "$OUT/audio/global_bf1" "$REF1"
  "$BAKE" global-sound "$S2/SOUND/GLOBAL.LVL" "$OUT/audio/global_bf2" "$REF2"
fi
if want load; then
  echo "== load screens =="
  "$BAKE" loadscreen "$B1/LOAD/TAT1.LVL"   "$OUT/tat1"
  "$BAKE" loadscreen "$B2/LOAD/TAT2.LVL"   "$OUT/tat2"
  "$BAKE" loadscreen "$B2/LOAD/COMMON.LVL" "$OUT/common_bf2"
fi

if want maps; then
  echo "== other ground maps =="
  for p in $BF1_MAPS; do
    n=$(basename "$p" | tr 'A-Z' 'a-z'); d="$OUT/bf1_$n"
    clean "$d/models" "$d/textures" "$d/collision" "$d/nav"
    "$BAKE" map "$B1/$p.LVL" "$d"
    [ -f "$B1/LOAD/$(basename "$p").LVL" ] && "$BAKE" loadscreen "$B1/LOAD/$(basename "$p").LVL" "$d"
  done
  for p in $BF2_MAPS; do
    n=$(basename "$p" | tr 'A-Z' 'a-z'); d="$OUT/bf2_$n"
    planet=$(dirname "$p" | tr 'A-Z' 'a-z')
    clean "$d/models" "$d/textures" "$d/collision" "$d/nav"
    pc_arg "$planet" "$n"
    "$BAKE" map "$B2/$p.LVL" "$d" "${PC_ARG[@]}"
    [ -f "$B2/LOAD/$(basename "$p").LVL" ] && "$BAKE" loadscreen "$B2/LOAD/$(basename "$p").LVL" "$d"
  done
fi
if want beep; then
  echo "== BF1 loading beep =="
  "$BAKE" beep "$S1/CORE.LVL" "$OUT/audio"
fi
echo "rebake done -> $OUT"
