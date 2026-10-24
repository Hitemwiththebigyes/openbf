#!/usr/bin/env python3
"""A3: diff the Lua script-callback binding lists of SWBF1 (Lua 4.0.1) and
SWBF2 (Lua 5.0.2), and classify every binding into an engine subsystem.

Inputs  (format: "hexaddr name" per line):
    analysis/bf1-bindings.txt
    analysis/bf2-bindings.txt

Usage:
    python3 scripts/bindings_diff.py            # summary + grouped listing
    python3 scripts/bindings_diff.py --md       # markdown tables (for a3-subsystem-map.md)

Classification is heuristic, by name.  Rules are ordered: the FIRST matching
rule wins, so put the most specific patterns first.  Keep this table in sync
with analysis/a3-subsystem-map.md.
"""
import argparse
import re
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# ---------------------------------------------------------------------------
# Subsystem classification rules: (subsystem, compiled-regex) — first hit wins.
# Case-sensitive on purpose: the binding names are CamelCase and case carries
# meaning (e.g. 'CP' vs 'cp').
# ---------------------------------------------------------------------------
RULES = [(k, re.compile(v)) for k, v in [
    # --- sound first so VoiceOver/VO/HeroMusic don't fall into net's Voice* --
    ("sound/music/vo",    r"Sound|Music|Audio|Voice[Oo]ver|VO$|VO[A-Z]|Stream"
                          r"|Speaker|Bass|Doppler|SFX|Mix|Snd|Volume|OutputMode"
                          r"|Audition"),
    # --- dropped regions (must not be shadowed by generic rules) -----------
    ("net/online",        r"DNAS|Gamespy|GamesSpy|OnlineService"),
    ("net/online",        r"Session|Lobby|Quickmatch|Friend|Join|NetJoin|Login|LogOn"
                          r"|Leader[Bb]oard|Stats|Clan|Logout|LoggedIn"
                          r"|ConnectSockets|Network|AutoNet|Host|Dedicated|Patch"
                          r"|Journal|MPMetagame|Spectator|Voice"
                          r"|PrevSessionId|Favorite|Filters$|ApplyFilters|Ping"
                          r"|MaxPlayers|Password|CdKey|NatNeg|Punkbuster"
                          r"|Leave|IPAddr|NetCable|NetHW|NetConfig|NetGame|NetWas"
                          r"|InNetGame|InMultiplayer|ConnectType|Vote|Server"
                          r"|BootInvite|Guest|BadNetworkConnection|Kick"),
    ("profile/save/memcard", r"Profile|MemCard|Memcard|MemoryCard|Card\b|OnCard"
                          r"|Save|Load(?!Display|ing)|Delete|Format(?!ion|Time)"
                          r"|SPState"
                          r"|Campaign|Metagame|IsFileExist"),
    ("stats/awards",      r"Award|Medal|Career|Rank\b|GetRank|PlayerIDAtRank"
                          r"|Unlockable"),
    # --- gameplay-side systems --------------------------------------------
    ("ai/hero",           r"AI(?![a-z])|Difficulty|Hero|Planning|Barrier|Hint"
                          r"|StationaryWaitTime|Snipe|Formation|AutoBalance"
                          r"|Aggressiveness|NumBots"),
    ("mission/objective", r"Objective|Mission(?!Hint)|Victory|Defeat"
                          r"|Historical|Timer\b|TimeLimit|ScoreLimit|GameRules"
                          r"|FlagCapture|AssaultDestroy|CriticalSystem|SpaceAssault"
                          r"|Flag(Gameplay|Carrier)|CanCharacterInteractWithFlag"
                          r"|IsGameOver|Bonus"),
    ("spawn/cp",          r"CommandPost|\bCP\b|CP$|Spawn|PathPoint|LandingRegion"
                          r"|DeathRegion|CaptureRegion"),
    ("team/reinforcements", r"Team|Reinforcement|Bleed|UnitCount|FriendlyFire"
                          r"|SwitchSides|PlayerSide"),
    ("vehicle",           r"Vehicle|Flyer|Walker|Turret|Hover"),
    ("soldier/unit",      r"Character|Soldier|Unit|Entity|Object(?!ive)|Class\b"
                          r"|ClassProperty|Prone|Stance|Crouch|Animation|Suicide"),
    ("camera",            r"Camera|CameraShot|View(?!Multiplier)|FirstPerson"
                          r"|Freecamera"),
    ("hud/interface",     r"IF[A-Z]|IFace|IFObj|IFModel|HUD|Map[A-Z]|MiniMap"
                          r"|Minimap|Popup|MessageText|Print3D|Marker|Reticule"
                          r"|Draw|ShowTimer|ShowTeamPoints|ScreenPosition"
                          r"|LoadDisplay|Timer(Name|Rate|Value)|Font"
                          r"|MultipageText|Texture"),
    ("shell/menu/movie",  r"Movie|Shell|Menu|Screen|Scene|Cursor|DownloadableContent"
                          r"|DCMap|Language|Platform|Keyboard|ustr|FormatTime"
                          r"|ToolTip|Demo|Attract|Legal|Training|Error|Console"
                          r"|Reboot|Quit|Unpause|Pause|localizestr|unicode"
                          r"|usprintf|UnicodeStrCat|SPProgress|Splitscreen"
                          r"|SplitScreen|NTGUI"),
    ("controller/input",  r"Controller|Input|Rumble|AutoAim|Button|Pad\b|Stick"
                          r"|ControlMode|ControlScale|YAxisFlip|AnalogId"
                          r"|ControlsToDefault|Leftstick"),
    # fallthrough
    ("misc",              r""),
]]

SUBSYSTEM_ORDER = list(OrderedDict.fromkeys(k for k, _ in RULES))


def classify(name: str) -> str:
    for subsystem, rx in RULES:
        if rx.search(name):
            return subsystem
    return "misc"


def load(path: Path) -> tuple:
    """Return ({name: addr}, [duplicate names]). Keeps the FIRST addr seen."""
    out, dups = {}, []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        addr, name = line.split(None, 1)
        if name in out:
            dups.append(f"{name} ({out[name]}, {addr})")
        else:
            out[name] = addr
    return out, dups


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--md", action="store_true", help="emit markdown tables")
    args = ap.parse_args()

    bf1, bf1_dups = load(ROOT / "analysis" / "bf1-bindings.txt")
    bf2, bf2_dups = load(ROOT / "analysis" / "bf2-bindings.txt")

    shared = sorted(bf1.keys() & bf2.keys())
    bf1_only = sorted(bf1.keys() - bf2.keys())
    bf2_only = sorted(bf2.keys() - bf1.keys())

    print(f"BF1 bindings : {len(bf1)} unique"
          + (f" (dups: {', '.join(bf1_dups)})" if bf1_dups else ""))
    print(f"BF2 bindings : {len(bf2)} unique"
          + (f" (dups: {', '.join(bf2_dups)})" if bf2_dups else ""))
    print(f"shared       : {len(shared)}")
    print(f"BF1-only     : {len(bf1_only)}")
    print(f"BF2-only     : {len(bf2_only)}")
    print()

    groups = OrderedDict((s, []) for s in SUBSYSTEM_ORDER)
    for name in sorted(bf1.keys() | bf2.keys()):
        groups[classify(name)].append(name)

    def status(n):
        if n in bf1 and n in bf2:
            return "both"
        return "BF1-only" if n in bf1 else "BF2-only"

    if args.md:
        for sub, names in groups.items():
            if not names:
                continue
            n_both = sum(1 for n in names if status(n) == "both")
            n_b1 = sum(1 for n in names if status(n) == "BF1-only")
            n_b2 = sum(1 for n in names if status(n) == "BF2-only")
            print(f"### {sub} ({len(names)}: {n_both} shared, "
                  f"{n_b1} BF1-only, {n_b2} BF2-only)\n")
            print("| binding | BF1 addr | BF2 addr |")
            print("|---|---|---|")
            for n in names:
                print(f"| {n} | {bf1.get(n, '—')} | {bf2.get(n, '—')} |")
            print()
    else:
        for sub, names in groups.items():
            if not names:
                continue
            print(f"== {sub} ({len(names)}) ==")
            for n in names:
                print(f"  {status(n):8s}  {bf1.get(n, '--------'):>8s}  "
                      f"{bf2.get(n, '--------'):>8s}  {n}")
            print()


if __name__ == "__main__":
    main()
