#!/usr/bin/env python3
"""pure_move_check.py — prove a file split MOVED code without changing it.

usage: pure_move_check.py <old> <new-file> [<new-file> ...]
  <old> is a plain path, or <git-rev>:<path> (read with `git show`), e.g.
    pure_move_check.py HEAD:engine/crates/core/src/sim.rs \
        engine/crates/core/src/sim.rs engine/crates/core/src/sim/*.rs
  (list the facade file too when it survives the split.)

Every non-blank line of the old file is normalised (whitespace stripped) and must
appear in the new files at least as many times. Ignored on both sides because a split
must change them: `use`/`pub use` (multi-line too), `mod x;`, `//!` and `#![..]`
lines, lone brace/paren lines, `impl .. {` headers, `#[cfg(test)]`, `mod tests {`.
A visibility change (`fn` -> `pub(super) fn`) is reported, not failed.

Exit 0: nothing lost or edited. Exit 1: some old line is missing from the new files
(printed) — a line was edited, dropped or duplicated wrongly. New-only lines are
printed as information: expect only new doc comments, impl headers and glue.
"""
import re
import subprocess
import sys
from collections import Counter

VIS = re.compile(r"^pub(\([^)]*\))?\s+")
LONE = {"{", "}", "};", "},", ")", ");", "]", "];", "],"}
IGNORE_ANY = [
    re.compile(r"^(pub(\([^)]*\))?\s+)?mod\s+\w+;$"),
    re.compile(r"^(pub(\([^)]*\))?\s+)?mod\s+\w+\s*\{$"),
    re.compile(r"^#\[cfg\(test\)\]$"),
    re.compile(r"^impl\b.*\{$"),
    re.compile(r"^#!\["),
    re.compile(r"^//!"),
]


def read_old(spec):
    try:
        return open(spec, encoding="utf-8").read()
    except OSError:
        return subprocess.run(["git", "show", spec], capture_output=True, text=True, check=True).stdout


def meaningful(text):
    out, in_use = [], False
    for raw in text.split("\n"):
        s = raw.strip()
        if not s:
            continue
        if in_use:
            in_use = not s.endswith(";")
            continue
        if re.match(r"^(pub(\([^)]*\))?\s+)?use\s", s):
            in_use = not s.endswith(";")
            continue
        if s in LONE or any(p.match(s) for p in IGNORE_ANY):
            continue
        out.append(s)
    return out


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    old_lines = meaningful(read_old(sys.argv[1]))
    new_lines = []
    for p in sys.argv[2:]:
        new_lines += meaningful(open(p, encoding="utf-8").read())
    old_c, new_c = Counter(old_lines), Counter(new_lines)
    missing, extra = old_c - new_c, new_c - old_c

    vis_changed = []
    extra_by_key = Counter()
    for line, n in extra.items():
        extra_by_key[VIS.sub("", line)] += n
    for line, n in list(missing.items()):
        key = VIS.sub("", line)
        take = min(n, extra_by_key.get(key, 0))
        if take:
            extra_by_key[key] -= take
            vis_changed.append((line, take))
            missing[line] -= take
    missing = +missing

    print(f"old meaningful lines: {len(old_lines)}   new: {len(new_lines)}")
    print(f"visibility-only changes: {sum(n for _, n in vis_changed)}")
    for line, n in vis_changed[:20]:
        print(f"    vis  x{n}  {line[:110]}")
    leftover_extra = sum(extra_by_key.values())
    print(f"new-only lines (docs, glue, moved-impl headers): {leftover_extra}")
    if missing:
        print(f"LOST OR EDITED: {sum(missing.values())} old line(s) absent from the new files:")
        for line, n in list(missing.items())[:60]:
            print(f"    x{n}  {line[:140]}")
        sys.exit(1)
    print("PURE MOVE: every old line is present in the new files.")


if __name__ == "__main__":
    main()
