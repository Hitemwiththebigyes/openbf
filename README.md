# openbf

An open-source Rust engine that plays the PlayStation 2 versions of
Star Wars: Battlefront and Star Wars: Battlefront II.

**Unofficial fan project.** Not affiliated with or endorsed by Lucasfilm, Disney,
Electronic Arts, LucasArts or Pandemic Studios. This repository contains no game
assets: you need your own PS2 discs of both games. Everything the game needs is
extracted from your ISOs on your own machine.

Work in progress: playable, but far from finished.

## Building your copy

One command turns your two disc images into one game file:

```bash
git clone https://github.com/sprucey149/openbf.git
cd openbf
mkdir iso   # put your own disc images here: iso/SWBF1.iso and iso/SWBF2.iso
python3 scripts/pack.py
./dist/openbf
```

`pack.py` extracts the discs, bakes the assets, builds the game, and packs everything into
`dist/openbf` (about 3 GB). Run that file from anywhere; it keeps your profile and its bug-report snapshots in an
`openbf_data/` folder beside itself. Run the script again after pulling new code: it skips what is still current
and bakes again only when the code that bakes has changed (that can take a while).

**`dist/openbf` contains the games' own content, so never upload or share it.**
Everyone builds their own from their own discs.

Requirements on Linux:
- Rust (https://rustup.rs), Python 3, bash, ffmpeg
- ALSA and udev libraries (on Fedora: `alsa-lib-devel systemd-devel`; on Debian/Ubuntu: `libasound2-dev libudev-dev`)
- A Vulkan-capable GPU and a game controller; there is no keyboard and mouse play

### Windows

The same command builds `dist\openbf.exe`. You need:
- Rust (https://rustup.rs), with the Visual Studio C++ Build Tools that rustup offers to install
- Python 3 (https://python.org, or `winget install Python.Python.3.12`)
- Git for Windows (https://git-scm.com): the bake runs in its bash; `pack.py` finds it by itself
- ffmpeg on your `PATH` (`winget install Gyan.FFmpeg`, then open a new terminal)
- A DirectX 12 or Vulkan GPU and a game controller

In a new PowerShell or Command Prompt:
```
git clone https://github.com/sprucey149/openbf.git
cd openbf
mkdir iso
python scripts\pack.py
dist\openbf.exe
```
Your profile and snapshots are saved in `openbf_data\` beside `openbf.exe`.

Disc images: `--iso1 <path>` and `--iso2 <path>` point the script at images stored somewhere else.
Tested and working on Linux and Windows, with the NTSC-U PS2 releases (SLUS-20898 and SLUS-21240).

## How it was made

The engine is written in Rust, and nearly all of it was written by AI agents (Anthropic's Claude)
running in a loop. Each behaviour is checked against the original game, running in the PCSX2
emulator, by reading the game's memory and comparing rendered pixels.
That scaffolding is published too:
- `PLAN.md`: the goals and the locked feature set
- `engine/CRATES.md`: a map of the code
- `analysis/`: the reverse-engineering notes, with every behaviour traced to the original game
- `docs/loop/`: how the AI loop is run (its rules, its session prompt, the backlog and every work card)
- `scripts/`: the build, bake, test-gate and loop tools

No code from the original games is included; the decompilation is used only as a reference for
behaviour.

This repository is updated in batches; the work is tracked as cards (units of work) in
`docs/loop/cards/`.

## Join in

I started this project because I wanted a better Star Wars: Battlefront, and I still want that
game for myself. I'd much rather build it together with everyone who loves these games too.
**Pull requests, bug reports, feature requests, ideas, playtesting and feedback of any kind are
welcome and hoped for.**

- **Found a bug?** Open an issue: say what you did, what you saw, which map, and attach a
  screenshot if you can. Press **F9** in the game right after it happens: that saves a snapshot of every unit's state
  to `openbf_data/playtest/last.json`. Attach that file too; it usually shows the cause.
- **Want a feature?** Open an issue and describe it. The open cards in `docs/loop/cards/` show
  what is already planned.
- **Want to write code?** See [CONTRIBUTING.md](CONTRIBUTING.md).

It's MIT licensed, so you're also free to fork it and take it your own way. If you do, I'd love
to hear about it, and even more to see your improvements come back here.
