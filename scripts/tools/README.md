# scripts/tools

Small standalone helpers that the analysis notes, cards and AGENTS.md cite. Each runs from any directory: paths
resolve from the repository root. The reverse-engineering tools read the PS2 executables from your own disc extract
(`extracted/bf1/SWBFRONT.ELF`, `extracted/bf2/SWBFRNT2.ELF`); nothing here ships game data.

| tool | what it does | usage |
|---|---|---|
| `mix.py` | renders the event sounds of a playtest black-box snapshot (or a SOUND_LOG csv) to a WAV and reports overloads; needs numpy, and ffmpeg for Ogg takes | `python3 scripts/tools/mix.py <snapshot.json> engine/assets_baked --wav out.wav` |
| `elfrd.py` | prints 32-bit words (hex and float) of an ELF at a virtual address | `python3 scripts/tools/elfrd.py extracted/bf1/SWBFRONT.ELF 534b10 8` |
| `refs.py` | every `jal` / `j` instruction and data word that references an address | `python3 scripts/tools/refs.py b1 22c7f0` |
| `eedis.py` | small R5900 disassembler, VU0 macro ops with their field masks | `python3 scripts/tools/eedis.py bf1 22ad3c 22ad80` |
| `eedis2.py` | `eedis.py` plus unaligned loads/stores, FPU/COP2 branches, likely-branches | `python3 scripts/tools/eedis2.py bf2 <lo> <hi>` |
| `vu1dis.py` | VU1 microprogram disassembler (`tags`, `find <regex>`, or a range) | `python3 scripts/tools/vu1dis.py bf2` |
| `pure_move_check.py` | proves a file split only moved lines (run it on every split) | `python3 scripts/tools/pure_move_check.py HEAD:<old.rs> <new files...>` |
| `win_run.sh` / `xkey.py` | the windowed game on a private Xvfb display with a scratch profile and no sound device, driven by a key script | header of `win_run.sh` |

`scripts/re_check.py` (`fn`, `callers`, `word`) and `scripts/re_calc.py` are the main RE checkers; see the
"RE and absence claims" rules in the loop's RULES.md.
