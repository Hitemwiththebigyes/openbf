#!/usr/bin/env python3
"""pack.py -- your two PS2 discs in, one self-contained game file out (card PACK).

    python3 scripts/pack.py [--iso1 iso/SWBF1.iso] [--iso2 iso/SWBF2.iso] [--out dist/openbf]

Steps, each skipped when its output is already there (so a second run only
recompiles and re-packs):
  1. build the `bake` tool and the `game` binary (cargo, release)
  2. extract each disc's DATA/_LVL_PS2 tree     -> extracted/bf{1,2}/lvl/  (bake extract-iso)
  3. extract the Lua scripts of the LVLs        -> extracted/bf{1,2}/lua/  (extract_lua.py)
  4. build the missions from the disc's own bytecode -> engine/missions/   (gen_missions.py --ps2-only)
  5. bake every asset                           -> engine/assets_baked/    (rebake.sh)
  6. copy the game, strip the copy, append assets_baked/ + missions/  -> --out

The result plays from anywhere with nothing beside it. It holds the games' own
content, so it is for your machine only: never upload or share it. Needs
cargo, python3, bash and ffmpeg (the bake encodes speech to OGG); `strip` is
used when present. Tested on Linux and Windows (card WIN: Git for Windows' bash
runs the bake; the output is dist/openbf.exe).
"""

import argparse
import glob
import hashlib
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ENGINE = os.path.join(ROOT, "engine")
EXTRACTED = os.path.join(ROOT, "extracted")
BAKED = os.path.join(ENGINE, "assets_baked")
MISSIONS = os.path.join(ENGINE, "missions")
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ENGINE, "target")
WINDOWS = os.name == "nt"
EXE = ".exe" if WINDOWS else ""

# The LVLs whose scripts the bake and the missions read, per disc (subdir <- LVL).
LUA = {
    "bf1": {"mission": "MISSION.LVL", "common": "COMMON/ENG/COMMON.LVL", "shell": "SHELL/ENG/SHELL.LVL"},
    "bf2": {"mission": "MISSION.LVL", "common": "COMMON.LVL", "shell": "SHELL.LVL",
            "ingame": "INGAME.LVL", "inshell": "INSHELL.LVL"},
}


def step(msg):
    print(f"\n== {msg}", flush=True)


def run(cmd, cwd=ROOT):
    print("   $ " + " ".join(cmd), flush=True)
    r = subprocess.run(cmd, cwd=cwd)
    if r.returncode:
        sys.exit(f"pack: failed ({r.returncode}): {' '.join(cmd)}")


def lvl_root(game):
    """The disc's _LVL_PS2 directory: the dev layout's, else where extract-iso puts it."""
    return os.path.join(EXTRACTED, game, "lvl", "DATA", "_LVL_PS2")


def need(tool, why):
    if shutil.which(tool) is None:
        sys.exit(f"pack: `{tool}` is not on PATH ({why})")


# What a bake / the missions are made by: when any of it changes, the output is stale and is made again.
BAKE_INPUTS = ("engine/crates/bake", "engine/crates/mungefs", "engine/crates/manifest", "scripts/rebake.sh",
               "engine/crates/bake/src/effect_names.txt", "engine/Cargo.toml", "engine/Cargo.lock")
MISSION_INPUTS = ("scripts/gen_missions.py", "scripts/extract_lua.py")
# Kept in extracted/ (local, ignored by git): engine/missions/ is tracked in the dev tree.
BAKE_STAMP = os.path.join(EXTRACTED, "bake_inputs.sha256")
MISSION_STAMP = os.path.join(EXTRACTED, "mission_inputs.sha256")


def inputs_hash(paths):
    """One sha256 over the source files under `paths` (line endings folded, so a CRLF checkout hashes the same)."""
    h = hashlib.sha256()
    for p in paths:
        full = os.path.join(ROOT, p)
        files = [full] if os.path.isfile(full) else sorted(
            os.path.join(d, f) for d, _, fs in os.walk(full) for f in fs
            if f.endswith((".rs", ".toml")) and os.sep + "tests" not in d[len(full):])
        for f in files:
            h.update(os.path.relpath(f, ROOT).replace(os.sep, "/").encode())
            with open(f, "rb") as source:
                h.update(source.read().replace(b"\r\n", b"\n"))
    return h.hexdigest()


def stale(stamp, paths):
    """Why the output at `stamp` must be made again, or None when it is current."""
    if not os.path.isfile(stamp):
        return "made before pack.py recorded what made it"
    return None if open(stamp).read().strip() == inputs_hash(paths) else "its source changed since it was made"


def find_bash():
    """bash for rebake.sh. On Windows, Git for Windows' own: WSL's System32\\bash.exe runs Linux, which cannot run
    the Windows bake.exe or read C:/ paths."""
    if not WINDOWS:
        need("bash", "rebake.sh")
        return shutil.which("bash")
    roots = []
    git = shutil.which("git")
    if git:  # ...\Git\cmd\git.exe or ...\Git\mingw64\bin\git.exe
        d = os.path.dirname(os.path.realpath(git))
        for _ in range(3):
            roots.append(d)
            d = os.path.dirname(d)
    for env in ("ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"):
        if os.environ.get(env):
            roots.append(os.path.join(os.environ[env], "Git"))
    if os.environ.get("LOCALAPPDATA"):
        roots.append(os.path.join(os.environ["LOCALAPPDATA"], "Programs", "Git"))
    for r in roots:
        b = os.path.join(r, "bin", "bash.exe")
        if os.path.isfile(b):
            return b
    sys.exit("pack: Git for Windows' bash.exe was not found (install Git for Windows, https://git-scm.com)")


def missing_music():
    """Music bodies a stream index names but the bake has not written (an assets_baked from before card PACK)."""
    out = []
    for idx in glob.glob(os.path.join(BAKED, "*/sounds/streams/streams.ron")) + \
            glob.glob(os.path.join(BAKED, "audio/global_*/global.ron")):
        text = open(idx, encoding="utf-8").read()
        d = os.path.dirname(idx)
        # source: "x.lvl", channels: N, block_size: N, data_offset: N, bytes: N, baked: false
        for m in re.finditer(r'source:\s*"([^"]+)",[^)]*?data_offset:\s*(\d+),\s*bytes:\s*\d+,\s*baked:\s*false', text):
            stem = m.group(1).rsplit(".", 1)[0].lower()
            body = os.path.join(d, "music", f"{stem}_{int(m.group(2)):x}.bin")
            if not os.path.isfile(body):
                out.append(body)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--iso1", default=os.path.join(ROOT, "iso", "SWBF1.iso"), help="Star Wars: Battlefront (PS2) disc image")
    ap.add_argument("--iso2", default=os.path.join(ROOT, "iso", "SWBF2.iso"), help="Star Wars: Battlefront II (PS2) disc image")
    ap.add_argument("--out", default=os.path.join(ROOT, "dist", "openbf" + EXE), help="the game file to write")
    ap.add_argument("--rebake", action="store_true", help="bake every asset again even when assets_baked/ exists")
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    if out.startswith(os.path.join(ENGINE, "")) or os.path.isdir(out):
        sys.exit(f"pack: --out {out} must be a file outside engine/")
    need("cargo", "the Rust toolchain, https://rustup.rs")

    step("1/6 build bake + game (release)")
    run(["cargo", "build", "--release", "-p", "bake", "-p", "game"], cwd=ENGINE)
    bake, game = os.path.join(TARGET, "release", "bake" + EXE), os.path.join(TARGET, "release", "game" + EXE)

    step("2/6 the discs")
    for g, iso in (("bf1", a.iso1), ("bf2", a.iso2)):
        if os.path.isfile(os.path.join(lvl_root(g), "CORE.LVL")):
            print(f"   {g}: already extracted ({lvl_root(g)})")
            continue
        if not os.path.isfile(iso):
            sys.exit(f"pack: no disc image at {iso} (pass --iso1 / --iso2)")
        run([bake, "extract-iso", iso, os.path.join(EXTRACTED, g, "lvl")])

    step("3/6 the Lua scripts")
    for g, lvls in LUA.items():
        for sub, lvl in lvls.items():
            d = os.path.join(EXTRACTED, g, "lua", sub)
            if os.path.isdir(d) and os.listdir(d):
                continue
            run([sys.executable, "-I", os.path.join(ROOT, "scripts", "extract_lua.py"),
                 os.path.join(lvl_root(g), lvl), "--out", d])

    step("4/6 the missions")
    # A checkout that tracks the missions in git (the dev tree) keeps its own; only a generated set goes stale.
    tracked = subprocess.run(["git", "-C", ROOT, "ls-files", "--", "engine/missions"], capture_output=True,
                             text=True).stdout.strip()
    have = glob.glob(os.path.join(MISSIONS, "*.ron"))
    why = stale(MISSION_STAMP, MISSION_INPUTS) if have and not tracked else None
    if have and (tracked or (not a.rebake and why is None)):
        print(f"   already there ({MISSIONS})")
    else:
        if why:
            print(f"   making the missions again: {why}")
        run([sys.executable, "-I", os.path.join(ROOT, "scripts", "gen_missions.py"), "--ps2-only", "--out", MISSIONS])
        with open(MISSION_STAMP, "w") as fh:
            fh.write(inputs_hash(MISSION_INPUTS) + "\n")

    step("5/6 the bake")
    bash = find_bash()
    baked = os.path.isfile(os.path.join(BAKED, "classes", "bf1_all.ron"))
    why = stale(BAKE_STAMP, BAKE_INPUTS) if baked else None
    if a.rebake or not baked or why:
        if why and not a.rebake:
            print(f"   baking again: the bake in {BAKED} was {why}")
        need("ffmpeg", "the bake encodes speech and effects to OGG")
        run([bash, os.path.join(ROOT, "scripts", "rebake.sh")])
        with open(BAKE_STAMP, "w") as fh:
            fh.write(inputs_hash(BAKE_INPUTS) + "\n")
    else:
        print(f"   already baked ({BAKED}); --rebake bakes again")
        n = len(missing_music())
        if n:
            print(f"   {n} music streams not copied out yet: rebaking the sound groups")
            need("ffmpeg", "the bake encodes speech and effects to OGG")
            run([bash, os.path.join(ROOT, "scripts", "rebake.sh"), "sounds", "global-sound"])
    left = missing_music()
    if left:
        sys.exit(f"pack: {len(left)} music streams still missing after the bake, e.g. {left[0]}")

    step("6/6 pack")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    tmp = os.path.join(os.path.dirname(out), "pack-tmp" + EXE)  # Windows runs only a name ending in .exe
    shutil.copyfile(game, tmp)
    os.chmod(tmp, 0o755)
    if WINDOWS:
        pass  # MSVC keeps the symbols in a .pdb beside the build, not in the .exe
    elif shutil.which("strip"):
        run(["strip", tmp])
    else:
        print("   (no `strip` on PATH: the game keeps its symbols, ~100 MB more)")
    # Written beside --out and swapped in: a game still running from --out keeps its old file.
    new = out + ".new"
    try:
        run([tmp, "--write-pack", new, f"assets_baked={BAKED}", f"missions={MISSIONS}"])
        try:
            os.replace(new, out)
        except PermissionError:
            sys.exit(f"pack: {out} is in use (Windows cannot replace a running game): close the game and run pack.py again")
    finally:
        os.remove(tmp)
        if os.path.exists(new):
            os.remove(new)
    size = os.path.getsize(out)
    print(f"\ndone: {out} ({size / 2**30:.2f} GiB). Run it from anywhere; nothing else is needed beside it.")
    print("It contains the games' own content: keep it on your machine, never upload or share it.")


if __name__ == "__main__":
    main()
